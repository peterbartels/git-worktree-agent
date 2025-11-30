//! Remote branch watcher
//!
//! Polls the remote repository for new branches and triggers worktree creation

use crate::config::{Config, HookStatus, WorktreeState};
use crate::executor::{CommandExecutor, CommandLog, CommandOutput, RunningCommand};
use crate::git::{RemoteBranch, Repository, WorktreeManager};
use chrono::Utc;
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
}

impl Watcher {
    /// Create a new watcher
    pub fn new() -> Self {
        Self {
            known_branches: HashMap::new(),
            running_hooks: HashMap::new(),
            command_logs: Vec::new(),
            fetch_in_progress: false,
        }
    }

    /// Initialize with current remote branches
    pub fn init(&mut self, repo: &Repository, config: &Config) -> Result<()> {
        let branches = repo.get_remote_branches(&config.remote_name)?;
        for branch in branches {
            self.known_branches.insert(branch.name.clone(), branch);
        }
        debug!("Initialized watcher with {} known branches", self.known_branches.len());
        Ok(())
    }

    /// Check if fetch is currently in progress
    pub fn is_fetching(&self) -> bool {
        self.fetch_in_progress
    }

    /// Start a background fetch (non-blocking)
    pub fn start_fetch(&mut self, repo_root: PathBuf, remote_name: String, event_tx: mpsc::Sender<WatcherEvent>) {
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
    pub fn on_fetch_complete(&mut self, repo: &Repository, config: &mut Config, event_tx: &mpsc::Sender<WatcherEvent>) {
        self.fetch_in_progress = false;
        config.last_fetch = Some(Utc::now());

        // Get current remote branches
        let current_branches = match repo.get_remote_branches(&config.remote_name) {
            Ok(branches) => branches,
            Err(e) => {
                error!("Failed to get remote branches: {}", e);
                return;
            }
        };

        let mut new_branches = Vec::new();

        // Find new branches
        for branch in &current_branches {
            if !self.known_branches.contains_key(&branch.name) {
                // This is a new branch
                if !config.should_ignore_branch(&branch.name) {
                    new_branches.push(branch.name.clone());
                }
                self.known_branches.insert(branch.name.clone(), branch.clone());
            }
        }

        // Remove branches that no longer exist
        let current_names: std::collections::HashSet<_> =
            current_branches.iter().map(|b| &b.name).collect();
        self.known_branches.retain(|name, _| current_names.contains(name));

        if !new_branches.is_empty() {
            let _ = event_tx.send(WatcherEvent::NewBranchesFound(new_branches.clone()));

            // Auto-create worktrees if enabled
            if config.auto_create_worktrees {
                let manager = WorktreeManager::new(repo);

                for branch in &new_branches {
                    // Skip if already tracked or untracked
                    if config.should_ignore_branch(branch) {
                        continue;
                    }

                    // Check if worktree already exists
                    if manager.has_worktree_for_branch(branch).unwrap_or(false) {
                        continue;
                    }

                    // Create the worktree
                    let _ = self.create_worktree(repo, config, branch, &manager, event_tx);
                }
            }
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
                                let _ = event_tx.send(WatcherEvent::HookCompleted(
                                    branch.clone(),
                                    *code,
                                ));
                                completed.push(branch.clone());
                            }
                            CommandOutput::Error(_) => {
                                let _ = event_tx.send(WatcherEvent::HookCompleted(
                                    branch.clone(),
                                    -1,
                                ));
                                completed.push(branch.clone());
                            }
                            _ => {
                                let _ = event_tx.send(WatcherEvent::HookOutput(
                                    branch.clone(),
                                    output,
                                ));
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

        // Remove completed hooks
        for branch in completed {
            self.running_hooks.remove(&branch);
        }
    }

    /// Create a worktree for a branch
    pub fn create_worktree(
        &mut self,
        repo: &Repository,
        config: &mut Config,
        branch: &str,
        manager: &WorktreeManager,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) -> Result<()> {
        let worktree_path = config.get_worktree_path(repo.root(), branch);

        let _ = event_tx.send(WatcherEvent::WorktreeCreating(branch.to_string()));

        match manager.create(branch, &worktree_path, &config.remote_name) {
            Ok(log_messages) => {
                // Log all the git output
                self.add_worktree_log(branch, &log_messages);

                let _ = event_tx.send(WatcherEvent::WorktreeCreated(
                    branch.to_string(),
                    worktree_path.clone(),
                ));

                // Track the branch
                config.track_branch(branch);

                // Add worktree state
                let worktree_state = WorktreeState {
                    branch: branch.to_string(),
                    path: worktree_path.clone(),
                    created_at: Utc::now(),
                    hook_status: HookStatus::None,
                    is_active: true,
                };
                config.add_worktree(worktree_state);
            }
            Err(e) => {
                error!("Failed to create worktree for {}: {}", branch, e);
                let _ = event_tx.send(WatcherEvent::WorktreeCreateFailed(
                    branch.to_string(),
                    e.to_string(),
                ));
            }
        }

        // Run post-create hook if configured
        if config.get_worktree(branch).is_some() {
            if let Some(cmd) = config.post_create_command.clone() {
                self.run_hook(config, branch, &cmd, &worktree_path, event_tx)?;
            }
        }

        Ok(())
    }

    /// Run a hook command for a branch
    fn run_hook(
        &mut self,
        config: &mut Config,
        branch: &str,
        command: &str,
        worktree_path: &PathBuf,
        event_tx: &mpsc::Sender<WatcherEvent>,
    ) -> Result<()> {
        let _ = event_tx.send(WatcherEvent::HookStarted(branch.to_string()));

        // Determine working directory
        let working_dir = if let Some(ref subdir) = config.command_working_dir {
            worktree_path.join(subdir)
        } else {
            worktree_path.clone()
        };

        // Update hook status
        if let Some(wt) = config.get_worktree_mut(branch) {
            wt.hook_status = HookStatus::Running;
        }

        // Create command log
        let log = CommandLog::new(branch.to_string(), command.to_string());
        self.command_logs.push(log);

        // Start async command
        let running = CommandExecutor::run_async(command.to_string(), &working_dir)?;
        self.running_hooks.insert(branch.to_string(), running);

        Ok(())
    }

    /// Get all known remote branches
    pub fn get_known_branches(&self) -> Vec<&RemoteBranch> {
        self.known_branches.values().collect()
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
        let mut log = CommandLog::new(
            branch.to_string(),
            format!("git worktree add ({})", branch),
        );
        
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
}

impl Default for Watcher {
    fn default() -> Self {
        Self::new()
    }
}
