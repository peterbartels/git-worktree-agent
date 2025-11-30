//! Git worktree operations

use color_eyre::eyre::{Context, Result, eyre};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, info, warn};

use super::Repository;

/// Information about an existing worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Path to the worktree
    pub path: PathBuf,
    /// Branch name (if any)
    pub branch: Option<String>,
    /// HEAD commit
    pub head: String,
    /// Whether this is the main worktree
    pub is_main: bool,
    /// Whether the worktree is locked
    pub is_locked: bool,
    /// Whether the worktree is prunable (directory missing)
    pub is_prunable: bool,
}

/// Manager for git worktree operations
pub struct WorktreeManager<'a> {
    repo: &'a Repository,
}

impl<'a> WorktreeManager<'a> {
    /// Create a new worktree manager
    pub fn new(repo: &'a Repository) -> Self {
        Self { repo }
    }

    /// List all worktrees
    pub fn list(&self) -> Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(self.repo.root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git worktree list")?;

        if !output.status.success() {
            return Err(eyre!(
                "git worktree list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_worktree_list(&stdout)
    }

    /// Parse the porcelain output of git worktree list
    fn parse_worktree_list(&self, output: &str) -> Result<Vec<WorktreeInfo>> {
        let mut worktrees = Vec::new();
        let mut current: Option<WorktreeInfo> = None;

        for line in output.lines() {
            if line.is_empty() {
                if let Some(wt) = current.take() {
                    worktrees.push(wt);
                }
                continue;
            }

            if let Some(path) = line.strip_prefix("worktree ") {
                if let Some(wt) = current.take() {
                    worktrees.push(wt);
                }
                current = Some(WorktreeInfo {
                    path: PathBuf::from(path),
                    branch: None,
                    head: String::new(),
                    is_main: false,
                    is_locked: false,
                    is_prunable: false,
                });
            } else if let Some(current) = current.as_mut() {
                if let Some(head) = line.strip_prefix("HEAD ") {
                    current.head = head.to_string();
                } else if let Some(branch) = line.strip_prefix("branch ") {
                    // Strip refs/heads/ prefix
                    let branch_name = branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch);
                    current.branch = Some(branch_name.to_string());
                } else if line == "bare" {
                    current.is_main = true;
                } else if line == "locked" {
                    current.is_locked = true;
                } else if line == "prunable" {
                    current.is_prunable = true;
                }
            }
        }

        if let Some(wt) = current.take() {
            worktrees.push(wt);
        }

        // The first worktree is typically the main one
        if let Some(first) = worktrees.first_mut() {
            first.is_main = true;
        }

        Ok(worktrees)
    }

    /// Create a new worktree for a branch
    /// Returns (success, output_messages) where output_messages contains git output for logging
    pub fn create(&self, branch: &str, path: &Path, remote: &str) -> Result<Vec<String>> {
        info!("Creating worktree for branch '{}' at: {}", branch, path.display());

        let mut log_messages = Vec::new();
        log_messages.push(format!("Creating worktree at: {}", path.display()));

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory: {}", parent.display()))?;
        }

        // Check if worktree already exists
        if path.exists() {
            return Err(eyre!("Worktree path already exists: {}", path.display()));
        }

        // Create the worktree tracking the remote branch
        let remote_ref = format!("{}/{}", remote, branch);
        log_messages.push(format!("$ git worktree add --track -b {} {} {}", branch, path.display(), remote_ref));
        
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "--track",
                "-b",
                branch,
                path.to_string_lossy().as_ref(),
                &remote_ref,
            ])
            .current_dir(self.repo.root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git worktree add")?;

        // Capture all output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        for line in stdout.lines().chain(stderr.lines()) {
            if !line.trim().is_empty() {
                log_messages.push(line.to_string());
            }
        }

        if !output.status.success() {
            // If branch already exists locally, try without -b
            if stderr.contains("already exists") {
                debug!("Branch already exists locally, trying without -b flag");
                log_messages.push(format!("Branch exists locally, retrying: git worktree add {} {}", path.display(), branch));
                
                let output = Command::new("git")
                    .args([
                        "worktree",
                        "add",
                        path.to_string_lossy().as_ref(),
                        branch,
                    ])
                    .current_dir(self.repo.root())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .with_context(|| "Failed to run git worktree add (retry)")?;

                let stdout2 = String::from_utf8_lossy(&output.stdout);
                let stderr2 = String::from_utf8_lossy(&output.stderr);
                
                for line in stdout2.lines().chain(stderr2.lines()) {
                    if !line.trim().is_empty() {
                        log_messages.push(line.to_string());
                    }
                }

                if !output.status.success() {
                    log_messages.push(format!("ERROR: git worktree add failed (exit code: {})", output.status.code().unwrap_or(-1)));
                    return Err(eyre!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            } else {
                log_messages.push(format!("ERROR: git worktree add failed (exit code: {})", output.status.code().unwrap_or(-1)));
                return Err(eyre!("git worktree add failed: {}", stderr));
            }
        }

        // Verify the directory was actually created
        if !path.exists() {
            log_messages.push(format!("ERROR: Directory was not created at {}", path.display()));
            return Err(eyre!("Worktree directory was not created: {}", path.display()));
        }

        log_messages.push(format!("✓ Worktree created successfully at: {}", path.display()));
        info!("Successfully created worktree at: {}", path.display());
        Ok(log_messages)
    }

    /// Remove a worktree
    pub fn remove(&self, path: &Path, force: bool) -> Result<()> {
        info!("Removing worktree at: {}", path.display());

        let path_str = path.to_string_lossy();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path_str);

        let output = Command::new("git")
            .args(&args)
            .current_dir(self.repo.root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git worktree remove")?;

        if !output.status.success() {
            return Err(eyre!(
                "git worktree remove failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        info!("Successfully removed worktree");
        Ok(())
    }

    /// Prune stale worktree references
    pub fn prune(&self) -> Result<()> {
        debug!("Pruning stale worktree references");

        let output = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(self.repo.root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git worktree prune")?;

        if !output.status.success() {
            warn!(
                "git worktree prune failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Check if a worktree exists for a branch
    pub fn has_worktree_for_branch(&self, branch: &str) -> Result<bool> {
        let worktrees = self.list()?;
        Ok(worktrees.iter().any(|w| w.branch.as_deref() == Some(branch)))
    }

    /// Get worktree path for a branch (if exists)
    pub fn get_worktree_path(&self, branch: &str) -> Result<Option<PathBuf>> {
        let worktrees = self.list()?;
        Ok(worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some(branch))
            .map(|w| w.path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list() {
        let output = r#"worktree /path/to/main
HEAD abc123
branch refs/heads/main

worktree /path/to/feature
HEAD def456
branch refs/heads/feature/my-feature
"#;

        let repo = Repository::discover(&std::env::current_dir().unwrap());
        if let Ok(repo) = repo {
            let manager = WorktreeManager::new(&repo);
            let worktrees = manager.parse_worktree_list(output).unwrap();
            
            assert_eq!(worktrees.len(), 2);
            assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
            assert_eq!(worktrees[1].branch.as_deref(), Some("feature/my-feature"));
        }
    }
}

