//! Application actions for worktree management

use tracing::{debug, error, info};

use super::App;
use super::state::ViewMode;
use crate::git::WorktreeManager;
use crate::ui::BranchStatus;
use crate::watcher::WatcherEvent;

impl App {
    /// Update the branch list from current state
    pub(super) fn update_branch_list(&mut self) {
        let manager = WorktreeManager::new(&self.repo);
        let worktrees = manager.list().unwrap_or_default();

        let mut items: Vec<crate::ui::BranchItem> = self
            .watcher
            .get_known_branches()
            .iter()
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
                } else if self.config.is_ignored(&branch.name) {
                    BranchStatus::Untracked
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
        let manager = WorktreeManager::new(&self.repo);
        self.status.worktree_count = manager.list().map(|w| w.len()).unwrap_or(0);
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
        if let Err(e) = self.config.save(self.repo.root()) {
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

        let manager = WorktreeManager::new(&self.repo);

        // Check if this is the main worktree (the one with .git)
        if let Ok(worktrees) = manager.list() {
            if let Some(wt) = worktrees
                .iter()
                .find(|w| w.branch.as_deref() == Some(&selected.name))
            {
                if wt.is_main {
                    self.status.last_error = Some("Cannot delete the main worktree".to_string());
                    return;
                }
            }
        }

        // Show confirmation dialog
        self.view_mode = ViewMode::DeleteConfirm {
            branch: selected.name.clone(),
            input: String::new(),
        };
    }

    /// Actually perform the worktree deletion after confirmation
    pub(super) fn do_delete_worktree(&mut self, branch: &str) {
        let manager = WorktreeManager::new(&self.repo);

        // Get worktree path
        if let Ok(Some(path)) = manager.get_worktree_path(branch) {
            if let Err(e) = manager.remove(&path, false) {
                error!("Failed to remove worktree: {}", e);
                self.status.last_error = Some(e.to_string());
            }
        }

        self.update_branch_list();
    }

    /// Toggle ignore for the selected branch
    pub(super) fn toggle_track_selected(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        if self.config.is_ignored(&selected.name) {
            self.config.unignore_branch(&selected.name);
        } else {
            self.config.ignore_branch(&selected.name);
        }

        if let Err(e) = self.config.save(self.repo.root()) {
            error!("Failed to save config: {}", e);
        }

        self.update_branch_list();
    }

    /// Toggle auto-create mode
    pub(super) fn toggle_auto_create(&mut self) {
        self.config.auto_create_worktrees = !self.config.auto_create_worktrees;
        self.status.auto_create_enabled = self.config.auto_create_worktrees;

        if let Err(e) = self.config.save(self.repo.root()) {
            error!("Failed to save config: {}", e);
        }
    }
}
