//! Configuration management for git-worktree-agent
//!
//! Stores user preferences and tracked/untracked branches in a JSON file
//! that is gitignored (since it's per-user different).

use chrono::{DateTime, Utc};
use color_eyre::eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The name of the config file stored in the git repository root
pub const CONFIG_FILE_NAME: &str = ".gwa-config.json";

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Version of the config file format
    #[serde(default = "default_version")]
    pub version: u32,

    /// Polling interval in seconds for checking remote branches
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,

    /// Command to run after a worktree is created (e.g., "npm install")
    #[serde(default)]
    pub post_create_command: Option<String>,

    /// Working directory relative to worktree root for running commands
    #[serde(default)]
    pub command_working_dir: Option<String>,

    /// Patterns for branches to automatically ignore (glob patterns)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,

    /// Branches that are explicitly tracked (worktrees will be created)
    #[serde(default)]
    pub tracked_branches: HashSet<String>,

    /// Branches that are explicitly untracked (will not create worktrees)
    #[serde(default)]
    pub untracked_branches: HashSet<String>,

    /// Whether to auto-create worktrees for new branches
    #[serde(default = "default_auto_create")]
    pub auto_create_worktrees: bool,

    /// Base directory for worktrees (relative to repo root, default: "../")
    #[serde(default = "default_worktree_base")]
    pub worktree_base_dir: String,

    /// The base/main branch to watch from (e.g., "main" or "master")
    #[serde(default)]
    pub base_branch: Option<String>,

    /// Remote name to watch (default: "origin")
    #[serde(default = "default_remote")]
    pub remote_name: String,

    /// Worktree states (persisted between runs)
    #[serde(default)]
    pub worktrees: Vec<WorktreeState>,

    /// Last time we fetched from remote
    #[serde(default)]
    pub last_fetch: Option<DateTime<Utc>>,
}

fn default_version() -> u32 {
    1
}

fn default_poll_interval() -> u64 {
    10
}

fn default_auto_create() -> bool {
    false
}

fn default_worktree_base() -> String {
    "..".to_string()
}

fn default_remote() -> String {
    "origin".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: default_version(),
            poll_interval_secs: default_poll_interval(),
            post_create_command: None,
            command_working_dir: None,
            ignore_patterns: vec![
                "dependabot/*".to_string(),
                "renovate/*".to_string(),
            ],
            tracked_branches: HashSet::new(),
            untracked_branches: HashSet::new(),
            auto_create_worktrees: default_auto_create(),
            worktree_base_dir: default_worktree_base(),
            base_branch: None,
            remote_name: default_remote(),
            worktrees: vec![],
            last_fetch: None,
        }
    }
}

/// State of a worktree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorktreeState {
    /// Branch name
    pub branch: String,

    /// Path to the worktree directory
    pub path: PathBuf,

    /// When the worktree was created
    pub created_at: DateTime<Utc>,

    /// Status of the post-create command
    pub hook_status: HookStatus,

    /// Whether this worktree is currently active
    pub is_active: bool,
}

/// Status of the post-create hook command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum HookStatus {
    /// No hook configured or not yet run
    #[default]
    None,
    /// Hook is currently running
    Running,
    /// Hook completed successfully
    Success,
    /// Hook failed with an error message
    Failed(String),
    /// Hook was skipped
    Skipped,
}

impl Config {
    /// Load config from a file, or create default if it doesn't exist
    pub fn load(repo_root: &Path) -> Result<Self> {
        let config_path = repo_root.join(CONFIG_FILE_NAME);

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

            let config: Config = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save config to file
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let config_path = repo_root.join(CONFIG_FILE_NAME);

        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;

        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    /// Check if a branch should be ignored based on patterns
    pub fn should_ignore_branch(&self, branch: &str) -> bool {
        // Check explicit untracked list
        if self.untracked_branches.contains(branch) {
            return true;
        }

        // Check ignore patterns
        for pattern in &self.ignore_patterns {
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                if glob_pattern.matches(branch) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a branch is explicitly tracked
    pub fn is_tracked(&self, branch: &str) -> bool {
        self.tracked_branches.contains(branch)
    }

    /// Track a branch
    pub fn track_branch(&mut self, branch: &str) {
        self.untracked_branches.remove(branch);
        self.tracked_branches.insert(branch.to_string());
    }

    /// Untrack a branch
    pub fn untrack_branch(&mut self, branch: &str) {
        self.tracked_branches.remove(branch);
        self.untracked_branches.insert(branch.to_string());
    }

    /// Add a worktree state
    pub fn add_worktree(&mut self, worktree: WorktreeState) {
        // Remove any existing entry for this branch
        self.worktrees.retain(|w| w.branch != worktree.branch);
        self.worktrees.push(worktree);
    }

    /// Remove a worktree state
    pub fn remove_worktree(&mut self, branch: &str) {
        self.worktrees.retain(|w| w.branch != branch);
    }

    /// Get worktree state by branch name
    pub fn get_worktree(&self, branch: &str) -> Option<&WorktreeState> {
        self.worktrees.iter().find(|w| w.branch == branch)
    }

    /// Get mutable worktree state by branch name
    pub fn get_worktree_mut(&mut self, branch: &str) -> Option<&mut WorktreeState> {
        self.worktrees.iter_mut().find(|w| w.branch == branch)
    }

    /// Get the worktree directory path for a branch
    pub fn get_worktree_path(&self, repo_root: &Path, branch: &str) -> PathBuf {
        let sanitized_branch = sanitize_branch_name(branch);
        repo_root
            .join(&self.worktree_base_dir)
            .join(&sanitized_branch)
    }
}

/// Sanitize a branch name for use as a directory name
pub fn sanitize_branch_name(branch: &str) -> String {
    branch
        .replace('/', "-")
        .replace('\\', "-")
        .replace(':', "-")
        .replace('*', "-")
        .replace('?', "-")
        .replace('"', "-")
        .replace('<', "-")
        .replace('>', "-")
        .replace('|', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_branch_name() {
        assert_eq!(sanitize_branch_name("feature/my-branch"), "feature-my-branch");
        assert_eq!(sanitize_branch_name("fix:bug"), "fix-bug");
    }

    #[test]
    fn test_should_ignore_branch() {
        let mut config = Config::default();
        config.ignore_patterns.push("feature/*".to_string());
        config.untracked_branches.insert("main".to_string());

        assert!(config.should_ignore_branch("feature/test"));
        assert!(config.should_ignore_branch("main"));
        assert!(!config.should_ignore_branch("develop"));
    }
}

