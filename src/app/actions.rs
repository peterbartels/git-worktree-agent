//! Application actions for worktree management

use tracing::{debug, error, info};

use super::App;
use super::state::{CreateWorktreeState, ViewMode};
use crate::git::WorktreeAgent;
use crate::ui::BranchStatus;
use crate::watcher::WatcherEvent;

impl App {
    /// Update the branch list from current state
    pub(super) fn update_branch_list(&mut self) {
        let worktree_agent = WorktreeAgent::new(&self.repo);
        let worktrees = worktree_agent.list().unwrap_or_default();

        let mut items: Vec<crate::ui::BranchItem> = self
            .watcher
            .get_known_branches()
            .iter()
            // Filter out ignored branches - they don't appear in the list
            .filter(|branch| !self.config.should_ignore_branch(&branch.name))
            .map(|branch| {
                let existing_worktree = worktrees
                    .iter()
                    .find(|w| w.branch.as_deref() == Some(&branch.name));

                // Check queue/processing status first
                let status = if self.watcher.is_current(&branch.name) {
                    // Currently being processed - check if hook is running
                    if self.watcher.has_running_hook(&branch.name) {
                        BranchStatus::RunningHook
                    } else {
                        BranchStatus::Creating
                    }
                } else if self.watcher.is_pending(&branch.name) {
                    BranchStatus::Queued
                } else if let Some(wt) = existing_worktree {
                    // Check if hook is running for this worktree
                    if self.watcher.has_running_hook(&branch.name) {
                        BranchStatus::RunningHook
                    } else if wt.is_prunable {
                        BranchStatus::LocalPrunable
                    } else {
                        BranchStatus::LocalActive
                    }
                } else {
                    BranchStatus::Remote
                };

                let is_default = self
                    .config
                    .base_branch
                    .as_ref()
                    .map(|b| b == &branch.name)
                    .unwrap_or(false);

                crate::ui::BranchItem {
                    name: branch.name.clone(),
                    status,
                    is_default,
                }
            })
            .collect();

        // Sort: active worktrees first, then by name
        items.sort_by(|a, b| {
            let a_active = matches!(
                a.status,
                BranchStatus::LocalActive | BranchStatus::LocalPrunable
            );
            let b_active = matches!(
                b.status,
                BranchStatus::LocalActive | BranchStatus::LocalPrunable
            );

            match (a_active, b_active) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        self.branch_list_state.update_items(items);
    }

    /// Poll for remote changes (starts background fetch)
    pub(super) fn do_poll(&mut self) {
        // Don't start a new fetch if one is already in progress
        if self.watcher.is_fetching() {
            return;
        }

        self.last_poll = std::time::Instant::now();
        debug!("Starting background fetch");

        // Start non-blocking fetch
        self.watcher.start_fetch(
            self.repo.root().to_path_buf(),
            self.config.remote_name.clone(),
            self.event_tx.clone(),
        );

        self.update_status();
    }

    /// Process pending watcher events
    pub(super) fn process_watcher_events(&mut self) {
        // Always check for hook output
        self.watcher.check_running_hooks(&self.event_tx);

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                WatcherEvent::FetchStarted => {
                    self.status.is_fetching = true;
                }
                WatcherEvent::FetchCompleted(output) => {
                    // Log fetch result to command logs
                    if let Some(msg) = output {
                        self.watcher.add_fetch_log(&self.config.remote_name, &msg);
                    } else {
                        // No warnings/errors - log simple success
                        self.watcher.add_fetch_success_log(&self.config.remote_name);
                    }
                    // Process the completed fetch - update branches
                    self.watcher
                        .on_fetch_complete(&self.repo, &mut self.config, &self.event_tx);
                    self.status.is_fetching = false;
                    self.status.last_fetch = self.config.last_fetch;
                    self.status.last_error = None;
                    self.update_branch_list();
                    self.update_status();
                }
                WatcherEvent::FetchFailed(msg) => {
                    self.watcher.on_fetch_failed();
                    self.watcher
                        .add_fetch_log(&self.config.remote_name, &format!("Error: {}", msg));
                    self.status.is_fetching = false;
                    self.status.last_error = Some(msg);
                }
                WatcherEvent::NewBranchesFound(branches) => {
                    info!("New branches found: {:?}", branches);
                    self.update_branch_list();
                }
                WatcherEvent::WorktreeCreating(branch) => {
                    // Update UI to show creating status
                    if let Some(item) = self
                        .branch_list_state
                        .items
                        .iter_mut()
                        .find(|i| i.name == branch)
                    {
                        item.status = BranchStatus::Creating;
                    }
                }
                WatcherEvent::WorktreeCreated(branch, _path) => {
                    info!("Worktree created for: {}", branch);
                    self.update_branch_list();
                    self.update_status();

                    // If no hook is configured, process next pending branch
                    if self.config.post_create_command.is_none() {
                        self.watcher
                            .try_process_next(&self.repo, &mut self.config, &self.event_tx);
                    }
                }
                WatcherEvent::WorktreeCreateFailed(branch, msg) => {
                    error!("Worktree creation failed for {}: {}", branch, msg);
                    self.status.last_error = Some(format!("{}: {}", branch, msg));
                    self.update_branch_list();

                    // Process next pending branch even if this one failed
                    self.watcher
                        .try_process_next(&self.repo, &mut self.config, &self.event_tx);
                }
                WatcherEvent::HookStarted(branch) => {
                    self.status.running_hooks += 1;
                    if let Some(item) = self
                        .branch_list_state
                        .items
                        .iter_mut()
                        .find(|i| i.name == branch)
                    {
                        item.status = BranchStatus::RunningHook;
                    }
                }
                WatcherEvent::HookOutput(_, _) => {
                    // Output is already captured in watcher
                }
                WatcherEvent::HookCompleted(branch, exit_code) => {
                    self.status.running_hooks = self.status.running_hooks.saturating_sub(1);

                    if exit_code != 0 {
                        self.status.last_error = Some(format!(
                            "Hook failed for {}: exit code {}",
                            branch, exit_code
                        ));
                    }

                    self.update_branch_list();

                    // Process next pending branch (sequential worktree creation)
                    self.watcher
                        .try_process_next(&self.repo, &mut self.config, &self.event_tx);
                }
            }
        }
    }

    /// Update status from current state
    pub(super) fn update_status(&mut self) {
        self.status.remote_branch_count = self.watcher.get_known_branches().len();
        // Count worktrees from git directly
        let worktree_agent = WorktreeAgent::new(&self.repo);
        self.status.worktree_count = worktree_agent.list().map(|w| w.len()).unwrap_or(0);
        self.status.auto_create_enabled = self.config.auto_create_worktrees;
        self.status.poll_interval = self.config.poll_interval_secs;
    }

    /// Create worktree for the selected branch (queued for sequential processing)
    pub(super) fn create_selected_worktree(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Don't create if already has worktree or already queued/processing
        if matches!(
            selected.status,
            BranchStatus::LocalActive
                | BranchStatus::Creating
                | BranchStatus::RunningHook
                | BranchStatus::Queued
        ) {
            return;
        }

        // Queue the branch for sequential processing
        self.watcher
            .queue_branch(&self.repo, &mut self.config, &selected.name, &self.event_tx);

        self.update_branch_list();
        if let Err(e) = self.config.save(self.repo.main_root()) {
            error!("Failed to save config: {}", e);
        }
    }

    /// Delete worktree for the selected branch
    pub(super) fn delete_selected_worktree(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Only delete if has worktree
        if !matches!(
            selected.status,
            BranchStatus::LocalActive | BranchStatus::LocalPrunable
        ) {
            return;
        }

        let worktree_agent = WorktreeAgent::new(&self.repo);

        // Check if this is the main worktree (the one with .git)
        if let Ok(worktrees) = worktree_agent.list()
            && let Some(wt) = worktrees
                .iter()
                .find(|w| w.branch.as_deref() == Some(&selected.name))
            && wt.is_main
        {
            self.status.last_error = Some("Cannot delete the main worktree".to_string());
            return;
        }

        // Show confirmation dialog
        self.view_mode = ViewMode::DeleteConfirm {
            branch: selected.name.clone(),
            input: String::new(),
        };
    }

    /// Actually perform the worktree deletion after confirmation
    pub(super) fn do_delete_worktree(&mut self, branch: &str) {
        let worktree_agent = WorktreeAgent::new(&self.repo);

        // Get worktree path
        if let Ok(Some(path)) = worktree_agent.get_worktree_path(branch)
            && let Err(e) = worktree_agent.remove(&path, false)
        {
            error!("Failed to remove worktree: {}", e);
            self.status.last_error = Some(e.to_string());
        }

        self.update_branch_list();
    }

    /// Untrack (ignore) the selected branch - removes it from the list
    pub(super) fn untrack_selected(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Add to ignore list
        self.config.ignore_branch(&selected.name);

        if let Err(e) = self.config.save(self.repo.main_root()) {
            error!("Failed to save config: {}", e);
        }

        self.update_branch_list();
    }

    /// Toggle auto-create mode
    pub(super) fn toggle_auto_create(&mut self) {
        self.config.auto_create_worktrees = !self.config.auto_create_worktrees;
        self.status.auto_create_enabled = self.config.auto_create_worktrees;

        if let Err(e) = self.config.save(self.repo.main_root()) {
            error!("Failed to save config: {}", e);
        }
    }

    /// Open the selected worktree directory and exit
    /// After exiting, the path will be printed so user can cd to it
    pub(super) fn open_selected_worktree(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Only works for local worktrees
        if !matches!(
            selected.status,
            BranchStatus::LocalActive | BranchStatus::LocalPrunable
        ) {
            self.status.last_error = Some("No worktree exists for this branch".to_string());
            return;
        }

        let worktree_agent = WorktreeAgent::new(&self.repo);

        // Get worktree path
        if let Ok(Some(path)) = worktree_agent.get_worktree_path(&selected.name) {
            // Store the path to print after exit
            self.exit_to_directory = Some(path);
            // Exit the application
            self.running = false;
        } else {
            self.status.last_error = Some("Could not find worktree path".to_string());
        }
    }

    /// Open the create new worktree dialog
    pub(super) fn open_create_worktree(&mut self) {
        // Get list of branches that can be used as base
        let mut base_branches = Vec::new();
        let default_branch = self.config.base_branch.clone();

        // Add default branch first if it exists
        if let Some(ref default) = default_branch {
            base_branches.push(default.clone());
        }

        // Add all other known branches (both local and remote)
        for branch in self.watcher.get_known_branches() {
            // Skip if already added as default
            if Some(&branch.name) != default_branch.as_ref() {
                base_branches.push(branch.name.clone());
            }
        }

        // If no branches found, can't create
        if base_branches.is_empty() {
            self.status.last_error = Some("No base branches available".to_string());
            return;
        }

        // Create state with default branch info for placeholder
        let state = CreateWorktreeState::new(base_branches, default_branch.as_deref());
        self.view_mode = ViewMode::CreateWorktree(state);
    }

    /// Create a new worktree with a new branch
    pub(super) fn do_create_new_worktree(&mut self, new_branch: &str, base_branch: &str) {
        let worktree_agent = WorktreeAgent::new(&self.repo);

        // Get the worktree path using the same logic as existing worktrees
        let worktree_path = self
            .config
            .get_worktree_path(self.repo.main_root(), new_branch);

        // Determine the full ref for the base branch
        // If it's a remote branch, use origin/branch, otherwise use just the branch name
        let base_ref = if let Some(branch_info) = self.watcher.get_branch_by_name(base_branch) {
            if branch_info.is_local {
                base_branch.to_string()
            } else {
                branch_info.full_ref.clone()
            }
        } else {
            // Default to remote ref if not found
            format!("{}/{}", self.config.remote_name, base_branch)
        };

        info!(
            "Creating new worktree: {} from {} at {:?}",
            new_branch, base_ref, worktree_path
        );

        // Add to command logs
        self.watcher.add_command_log(
            new_branch,
            &format!("Creating new branch '{}' from '{}'", new_branch, base_ref),
        );

        match worktree_agent.create_new_branch(new_branch, &base_ref, &worktree_path) {
            Ok(log_messages) => {
                for msg in log_messages {
                    self.watcher.add_command_log(new_branch, &msg);
                }

                // Add the new branch to known branches so it shows up immediately
                self.watcher.add_local_branch(new_branch);

                // Update the branch list first, then select the new branch
                self.update_branch_list();
                self.branch_list_state.select_by_name(new_branch);
                self.update_status();

                // Run post-create command if configured
                if let Some(ref command) = self.config.post_create_command {
                    self.watcher.add_command_log(
                        new_branch,
                        &format!("Running post-create hook: {}", command),
                    );
                    self.watcher.start_hook(
                        new_branch.to_string(),
                        command.clone(),
                        worktree_path,
                        self.event_tx.clone(),
                    );
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to create worktree: {}", e);
                self.watcher.add_command_log(new_branch, &error_msg);
                self.status.last_error = Some(error_msg);
            }
        }
    }
}
