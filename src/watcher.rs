//! Remote branch watcher
//!
//! Polls the remote repository for new branches and triggers worktree creation

use chrono::Utc;

use crate::config::Config;
use crate::executor::{CommandExecutor, CommandLog, CommandOutput, RunningCommand};
use crate::git::{RemoteBranch, Repository, WorktreeAgent};
use color_eyre::eyre::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use tracing::{debug, error};

/// Events that can occur during watching
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// Fetch started
    FetchStarted,
    /// Fetch completed successfully (with optional output messages)
    FetchCompleted(Option<String>),
    /// Fetch failed
    FetchFailed(String),
    /// New remote branches discovered
    NewBranchesFound(Vec<String>),
    /// Worktree creation started
    WorktreeCreating(String),
    /// Worktree created successfully
    WorktreeCreated(String, PathBuf),
    /// Worktree creation failed
    WorktreeCreateFailed(String, String),
    /// Hook started
    HookStarted(String),
    /// Hook output received
    HookOutput(String, CommandOutput),
    /// Hook completed
    HookCompleted(String, i32),
}

/// Background watcher state
pub struct Watcher {
    /// Known remote branches
    known_branches: HashMap<String, RemoteBranch>,
    /// Running hook commands
    running_hooks: HashMap<String, RunningCommand>,
    /// Command logs
    pub command_logs: Vec<CommandLog>,
    /// Is a fetch currently in progress?
    fetch_in_progress: bool,
    /// Queue of branches pending worktree creation (processed sequentially)
    pending_branches: Vec<String>,
    /// Currently processing a worktree (branch name if any)
    current_processing: Option<String>,
}

impl Watcher {
    /// Create a new watcher
    pub fn new() -> Self {
        Self {
            known_branches: HashMap::new(),
            running_hooks: HashMap::new(),
            command_logs: Vec::new(),
            fetch_in_progress: false,
            pending_branches: Vec::new(),
            current_processing: None,
        }
    }

    /// Initialize with current remote branches
    pub fn init(&mut self, repo: &Repository, config: &Config) -> Result<()> {
        // Get remote branches
        let remote_branches = repo.get_remote_branches(&config.remote_name)?;
        for branch in remote_branches {
            self.known_branches.insert(branch.name.clone(), branch);
        }

        // Get local branches
        let local_branches = repo.get_local_branches()?;
        for branch in local_branches {
            // Only add if not already present (remote branch takes precedence)
            if !self.known_branches.contains_key(&branch.name) {
                self.known_branches.insert(branch.name.clone(), branch);
            }
        }

        debug!(
            "Initialized watcher with {} known branches",
            self.known_branches.len()
        );
        Ok(())
    }

    /// Check if fetch is currently in progress
    pub fn is_fetching(&self) -> bool {
        self.fetch_in_progress
    }

    /// Start a background fetch (non-blocking)
    pub fn start_fetch(
        &mut self,
        repo_root: PathBuf,
        remote_name: String,
        event_tx: mpsc::Sender<WatcherEvent>,
    ) {
        if self.fetch_in_progress {
            return;
        }

        self.fetch_in_progress = true;
        let _ = event_tx.send(WatcherEvent::FetchStarted);

        // Spawn background thread for fetch
        thread::spawn(move || {
            let result = std::process::Command::new("git")
                .args(["fetch", "--prune", &remote_name])
                .current_dir(&repo_root)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output();

            match result {
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    // Collect any output messages (warnings, info, etc.)
                    let mut messages = Vec::new();
                    for line in stderr.lines().chain(stdout.lines()) {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            messages.push(trimmed.to_string());
                        }
                    }

                    if output.status.success() {
                        let output_msg = if messages.is_empty() {
                            None
                        } else {
                            Some(messages.join("\n"))
                        };
                        let _ = event_tx.send(WatcherEvent::FetchCompleted(output_msg));
                    } else {
                        // Check if it's a real error or just a warning
                        let is_real_error = stderr.lines().any(|line| {
                            let line = line.trim().to_lowercase();
                            !line.is_empty()
                                && !line.starts_with("warning")
                                && !line.contains("post-quantum")
                                && !line.starts_with("hint:")
                                && !line.starts_with("from ")
                        });

                        if is_real_error {
                            let _ = event_tx.send(WatcherEvent::FetchFailed(stderr.to_string()));
                        } else {
                            // It's just warnings, treat as success
                            let output_msg = if messages.is_empty() {
                                None
                            } else {
                                Some(messages.join("\n"))
                            };
                            let _ = event_tx.send(WatcherEvent::FetchCompleted(output_msg));
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(WatcherEvent::FetchFailed(e.to_string()));
                }
            }
        });
    }

    /// Called when fetch completes - update branch list
    pub fn on_fetch_complete(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) {
        self.fetch_in_progress = false;
        config.last_fetch = Some(Utc::now());

        // Get current remote branches
        let remote_branches = match repo.get_remote_branches(&config.remote_name) {
            Ok(branches) => branches,
            Err(e) => {
                error!("Failed to get remote branches: {}", e);
                return;
            }
        };

        // Get current local branches
        let local_branches = repo.get_local_branches().unwrap_or_default();

        let mut new_branches = Vec::new();

        // Find new remote branches
        for branch in &remote_branches {
            if !self.known_branches.contains_key(&branch.name) {
                // This is a new branch
                if !config.should_ignore_branch(&branch.name) {
                    new_branches.push(branch.name.clone());
                }
                self.known_branches
                    .insert(branch.name.clone(), branch.clone());
            }
        }

        // Add local branches (only if not already tracked as remote)
        for branch in &local_branches {
            if !self.known_branches.contains_key(&branch.name) {
                self.known_branches
                    .insert(branch.name.clone(), branch.clone());
            }
        }

        // Build set of all current branch names (remote + local)
        let mut current_names: std::collections::HashSet<String> =
            remote_branches.iter().map(|b| b.name.clone()).collect();
        for branch in &local_branches {
            current_names.insert(branch.name.clone());
        }

        // Remove branches that no longer exist
        self.known_branches
            .retain(|name, _| current_names.contains(name));

        if !new_branches.is_empty() {
            let _ = event_tx.send(WatcherEvent::NewBranchesFound(new_branches.clone()));

            // Auto-create worktrees if enabled - queue them for sequential processing
            if config.auto_create_worktrees {
                let worktree_agent = WorktreeAgent::new(repo);

                for branch in &new_branches {
                    // Skip if already tracked or untracked
                    if config.should_ignore_branch(branch) {
                        continue;
                    }

                    // Check if worktree already exists
                    if worktree_agent.has_worktree_for_branch(branch).unwrap_or(false) {
                        continue;
                    }

                    // Queue the branch for processing (instead of creating immediately)
                    if !self.pending_branches.contains(branch) {
                        self.pending_branches.push(branch.clone());
                    }
                }

                // Start processing if not already doing so
                if self.current_processing.is_none() && !self.pending_branches.is_empty() {
                    self.process_next_branch(repo, config, event_tx);
                }
            }
        }
    }

    /// Process the next branch in the queue (creates worktree + starts hook)
    fn process_next_branch(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) {
        // Get the next branch from the queue
        let Some(branch) = self.pending_branches.first().cloned() else {
            self.current_processing = None;
            return;
        };

        self.pending_branches.remove(0);
        self.current_processing = Some(branch.clone());

        let worktree_agent = WorktreeAgent::new(repo);
        let _ = self.create_worktree(repo, config, &branch, &worktree_agent, event_tx);
    }

    /// Check if we're currently processing branches or have pending ones
    pub fn is_processing(&self) -> bool {
        self.current_processing.is_some() || !self.pending_branches.is_empty()
    }

    /// Get count of pending branches
    pub fn pending_count(&self) -> usize {
        self.pending_branches.len()
            + if self.current_processing.is_some() {
                1
            } else {
                0
            }
    }

    /// Check if a branch is in the pending queue
    pub fn is_pending(&self, branch: &str) -> bool {
        self.pending_branches.contains(&branch.to_string())
    }

    /// Check if a branch is currently being processed
    pub fn is_current(&self, branch: &str) -> bool {
        self.current_processing
            .as_ref()
            .map(|b| b == branch)
            .unwrap_or(false)
    }

    /// Check if a branch has a running hook
    pub fn has_running_hook(&self, branch: &str) -> bool {
        self.running_hooks.contains_key(branch)
    }

    /// Queue a branch for worktree creation (used for manual creation)
    pub fn queue_branch(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        branch: &str,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) {
        // Don't queue if already queued or being processed
        if self.is_pending(branch) || self.is_current(branch) {
            return;
        }

        // Add to queue
        self.pending_branches.push(branch.to_string());

        // Start processing if not already doing so
        if self.current_processing.is_none() {
            self.process_next_branch(repo, config, event_tx);
        }
    }

    /// Called when fetch fails
    pub fn on_fetch_failed(&mut self) {
        self.fetch_in_progress = false;
    }

    /// Check for running hooks output (call this frequently)
    pub fn check_running_hooks(&mut self, event_tx: &mpsc::Sender<WatcherEvent>) {
        let mut completed = Vec::new();

        for (branch, running) in &self.running_hooks {
            // Drain available output
            loop {
                match running.output_rx.try_recv() {
                    Ok(output) => {
                        // Find the log for this branch
                        if let Some(log) = self
                            .command_logs
                            .iter_mut()
                            .rev()
                            .find(|l| l.branch == *branch)
                        {
                            log.add_output(output.clone());
                        }

                        match &output {
                            CommandOutput::Exit(code) => {
                                let _ = event_tx
                                    .send(WatcherEvent::HookCompleted(branch.clone(), *code));
                                completed.push(branch.clone());
                            }
                            CommandOutput::Error(_) => {
                                let _ =
                                    event_tx.send(WatcherEvent::HookCompleted(branch.clone(), -1));
                                completed.push(branch.clone());
                            }
                            _ => {
                                let _ =
                                    event_tx.send(WatcherEvent::HookOutput(branch.clone(), output));
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        completed.push(branch.clone());
                        break;
                    }
                }
            }
        }

        // Remove completed hooks and clear current_processing if done
        for branch in completed {
            self.running_hooks.remove(&branch);

            // If this was the branch we were processing, mark as done
            if self.current_processing.as_ref() == Some(&branch) {
                self.current_processing = None;
            }
        }
    }

    /// Try to process the next pending branch (call after hook completes)
    pub fn try_process_next(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) {
        // Only process next if we're not currently processing anything
        if self.current_processing.is_none() && !self.pending_branches.is_empty() {
            self.process_next_branch(repo, config, event_tx);
        }
    }

    /// Create a worktree for a branch
    pub fn create_worktree(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        branch: &str,
        worktree_agent: &WorktreeAgent,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) -> Result<()> {
        let worktree_path = config.get_worktree_path(repo.root(), branch);

        let _ = event_tx.send(WatcherEvent::WorktreeCreating(branch.to_string()));

        match worktree_agent.create(branch, &worktree_path, &config.remote_name) {
            Ok(log_messages) => {
                // Log all the git output
                self.add_worktree_log(branch, &log_messages);

                let _ = event_tx.send(WatcherEvent::WorktreeCreated(
                    branch.to_string(),
                    worktree_path.clone(),
                ));
            }
            Err(e) => {
                error!("Failed to create worktree for {}: {}", branch, e);
                let _ = event_tx.send(WatcherEvent::WorktreeCreateFailed(
                    branch.to_string(),
                    e.to_string(),
                ));
                // Clear current_processing so next branch can proceed
                self.current_processing = None;
                return Ok(());
            }
        }

        // Run post-create hook if configured
        let mut hook_started = false;
        if let Some(cmd) = config.post_create_command.clone() {
            self.run_hook(branch, &cmd, &worktree_path, event_tx)?;
            hook_started = true;
        }

        // If no hook was started, clear current_processing so next branch can proceed
        if !hook_started {
            self.current_processing = None;
        }

        Ok(())
    }

    /// Run a hook command for a branch
    fn run_hook(
        &mut self,
        branch: &str,
        command: &str,
        worktree_path: &PathBuf,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) -> Result<()> {
        let _ = event_tx.send(WatcherEvent::HookStarted(branch.to_string()));

        // Create command log
        let log = CommandLog::new(branch.to_string(), command.to_string());
        self.command_logs.push(log);

        // Start async command in the worktree directory
        let running = CommandExecutor::run_async(command.to_string(), worktree_path)?;
        self.running_hooks.insert(branch.to_string(), running);

        Ok(())
    }

    /// Start a hook for a manually created worktree (not through the normal queue)
    pub fn start_hook(
        &mut self,
        branch: String,
        command: String,
        worktree_path: std::path::PathBuf,
        event_tx: mpsc::Sender<WatcherEvent>,
    ) {
        if let Err(e) = self.run_hook(&branch, &command, &worktree_path, &event_tx) {
            tracing::error!("Failed to start hook for {}: {}", branch, e);
        }
    }

    /// Get all known branches (both local and remote)
    pub fn get_known_branches(&self) -> Vec<&RemoteBranch> {
        self.known_branches.values().collect()
    }

    /// Get a specific branch by name
    pub fn get_branch_by_name(&self, name: &str) -> Option<&RemoteBranch> {
        self.known_branches.get(name)
    }

    /// Add a new local branch to the known branches list
    pub fn add_local_branch(&mut self, name: &str) {
        if !self.known_branches.contains_key(name) {
            self.known_branches.insert(
                name.to_string(),
                RemoteBranch {
                    full_ref: name.to_string(),
                    name: name.to_string(),
                    remote: String::new(),
                    commit: String::new(), // We don't have the commit hash readily available
                    is_local: true,
                },
            );
        }
    }

    /// Add a log entry for fetch output (warnings/errors)
    pub fn add_fetch_log(&mut self, remote_name: &str, output: &str) {
        let mut log = CommandLog::new_system(
            format!("fetch:{}", remote_name),
            format!("git fetch --prune {}", remote_name),
        );

        for line in output.lines() {
            log.add_output(CommandOutput::Stdout(line.to_string()));
        }
        log.add_output(CommandOutput::Exit(0));

        self.command_logs.push(log);
    }

    /// Add a simple success log for fetch
    pub fn add_fetch_success_log(&mut self, remote_name: &str) {
        let mut log = CommandLog::new_system(
            format!("fetch:{}", remote_name),
            format!("git fetch --prune {}", remote_name),
        );

        log.add_output(CommandOutput::Stdout("Fetch successful".to_string()));
        log.add_output(CommandOutput::Exit(0));

        self.command_logs.push(log);
    }

    /// Add a log entry for worktree creation
    pub fn add_worktree_log(&mut self, branch: &str, messages: &[String]) {
        let mut log = CommandLog::new(branch.to_string(), format!("git worktree add ({})", branch));

        for line in messages {
            if line.starts_with("ERROR:") {
                log.add_output(CommandOutput::Stderr(line.clone()));
            } else {
                log.add_output(CommandOutput::Stdout(line.clone()));
            }
        }
        log.add_output(CommandOutput::Exit(0));

        self.command_logs.push(log);
    }

    /// Add a single log message for a branch (just the command header, no output)
    pub fn add_command_log(&mut self, branch: &str, message: &str) {
        let log = CommandLog::new(branch.to_string(), message.to_string());
        self.command_logs.push(log);
    }
}

impl Default for Watcher {
    fn default() -> Self {
        Self::new()
    }
}
