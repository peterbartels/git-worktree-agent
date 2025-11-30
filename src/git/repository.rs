//! Repository discovery and remote operations using git CLI

use color_eyre::eyre::{Context, Result, eyre};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, warn};

/// Information about a remote branch
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBranch {
    /// Full reference name (e.g., "origin/feature/my-branch")
    pub full_ref: String,
    /// Short branch name (e.g., "feature/my-branch")
    pub name: String,
    /// Remote name (e.g., "origin")
    pub remote: String,
    /// Commit hash the branch points to
    pub commit: String,
}

/// Wrapper around a git repository (uses git CLI)
pub struct Repository {
    root: PathBuf,
}

impl Repository {
    /// Discover and open a git repository from the given path
    pub fn discover(path: &Path) -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to run git in: {}", path.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!(
                "Could not find a git repository in '{}' or in any of its parents.\n{}",
                path.display(),
                stderr.trim()
            ));
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let root = PathBuf::from(root);

        debug!("Discovered git repository at: {}", root.display());

        Ok(Self { root })
    }

    /// Get the repository root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check if a remote exists
    pub fn remote_exists(&self, remote_name: &str) -> bool {
        Command::new("git")
            .args(["remote", "get-url", remote_name])
            .current_dir(&self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Validate that a remote exists and return an error message if not
    pub fn validate_remote(&self, remote_name: &str) -> Result<(), String> {
        if !self.remote_exists(remote_name) {
            return Err(format!(
                "Remote '{}' not found.\n\n\
                 Please add the remote first:\n\
                 git remote add {} <url>\n\n\
                 Or update your configuration to use an existing remote.",
                remote_name, remote_name
            ));
        }
        Ok(())
    }

    /// Get list of configured remotes
    pub fn get_remotes(&self) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["remote"])
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git remote")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let remotes: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(remotes)
    }

    /// Get all remote branches for a specific remote
    pub fn get_remote_branches(&self, remote_name: &str) -> Result<Vec<RemoteBranch>> {
        let output = Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(refname:short) %(objectname:short)",
                &format!("refs/remotes/{}", remote_name),
            ])
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| "Failed to run git for-each-ref")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("git for-each-ref failed: {}", stderr);
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let prefix = format!("{}/", remote_name);
        let mut branches = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let full_ref = parts[0];
                let commit = parts[1];

                // Skip HEAD
                if full_ref.ends_with("/HEAD") {
                    continue;
                }

                // Extract branch name (remove remote prefix)
                let name = full_ref
                    .strip_prefix(&prefix)
                    .unwrap_or(full_ref)
                    .to_string();

                branches.push(RemoteBranch {
                    full_ref: full_ref.to_string(),
                    name,
                    remote: remote_name.to_string(),
                    commit: commit.to_string(),
                });
            }
        }

        debug!(
            "Found {} remote branches for {}",
            branches.len(),
            remote_name
        );
        Ok(branches)
    }

    /// Get the default branch for a remote (e.g., origin/HEAD -> origin/main)
    pub fn get_default_branch(&self, remote_name: &str) -> Option<String> {
        // Try to get the remote's HEAD reference
        let output = Command::new("git")
            .args([
                "symbolic-ref",
                &format!("refs/remotes/{}/HEAD", remote_name),
            ])
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .ok()?;

        if output.status.success() {
            let ref_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // refs/remotes/origin/main -> main
            if let Some(branch) = ref_name.strip_prefix(&format!("refs/remotes/{}/", remote_name)) {
                return Some(branch.to_string());
            }
        }

        // Fallback: try common default branch names
        let output = Command::new("git")
            .args(["branch", "-r"])
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .ok()?;

        if output.status.success() {
            let branches = String::from_utf8_lossy(&output.stdout);
            let common_defaults = ["main", "master", "develop", "dev"];

            for default in common_defaults {
                let pattern = format!("{}/{}", remote_name, default);
                if branches
                    .lines()
                    .any(|line| line.trim().trim_start_matches("* ") == pattern)
                {
                    return Some(default.to_string());
                }
            }
        }

        // Last fallback: get the current branch
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .ok()?;

        if output.status.success() {
            let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !current.is_empty() {
                return Some(current);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_discover_repo() {
        let current_dir = env::current_dir().unwrap();
        let result = Repository::discover(&current_dir);
        if let Ok(repo) = result {
            assert!(repo.root().exists());
        }
    }
}
