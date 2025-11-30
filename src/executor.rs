//! Command execution for post-worktree hooks
//!
//! Runs configured commands after a worktree is created (e.g., npm install)

use color_eyre::eyre::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use tracing::{debug, error, info};

/// Output from a running command
#[derive(Debug, Clone)]
pub enum CommandOutput {
    /// Standard output line
    Stdout(String),
    /// Standard error line
    Stderr(String),
    /// Command completed with exit code
    Exit(i32),
    /// Command failed to start
    Error(String),
}

/// A running command process
pub struct RunningCommand {
    /// Receiver for command output
    pub output_rx: mpsc::Receiver<CommandOutput>,
    /// Handle to the background thread
    _handle: thread::JoinHandle<()>,
}

/// Execute a command in a worktree directory
pub struct CommandExecutor;

impl CommandExecutor {
    /// Run a command synchronously and return the result
    pub fn run_sync(command: &str, working_dir: &Path) -> Result<(i32, String, String)> {
        info!("Running command: {} in {}", command, working_dir.display());

        let output = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", command])
                .current_dir(working_dir)
                .output()
        } else {
            Command::new("sh")
                .args(["-c", command])
                .current_dir(working_dir)
                .output()
        }
        .with_context(|| format!("Failed to execute command: {}", command))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        debug!("Command exited with code: {}", exit_code);

        Ok((exit_code, stdout, stderr))
    }

    /// Run a command asynchronously with streaming output
    pub fn run_async(command: String, working_dir: &Path) -> Result<RunningCommand> {
        info!(
            "Starting async command: {} in {}",
            command,
            working_dir.display()
        );

        let (tx, rx) = mpsc::channel();
        let working_dir = working_dir.to_path_buf();

        let handle = thread::spawn(move || {
            let result = Self::run_command_with_output(&command, &working_dir, tx.clone());

            if let Err(e) = result {
                let _ = tx.send(CommandOutput::Error(e.to_string()));
            }
        });

        Ok(RunningCommand {
            output_rx: rx,
            _handle: handle,
        })
    }

    /// Internal helper to run command with output streaming
    fn run_command_with_output(
        command: &str,
        working_dir: &Path,
        tx: mpsc::Sender<CommandOutput>,
    ) -> Result<()> {
        use std::io::{BufRead, BufReader};

        let mut child = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", command])
                .current_dir(working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            Command::new("sh")
                .args(["-c", command])
                .current_dir(working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        }
        .with_context(|| format!("Failed to spawn command: {}", command))?;

        // Read stdout in a thread
        let stdout = child.stdout.take();
        let tx_stdout = tx.clone();
        let stdout_handle = thread::spawn(move || {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stdout.send(CommandOutput::Stdout(line)).is_err() {
                        break;
                    }
                }
            }
        });

        // Read stderr in a thread
        let stderr = child.stderr.take();
        let tx_stderr = tx.clone();
        let stderr_handle = thread::spawn(move || {
            if let Some(stderr) = stderr {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if tx_stderr.send(CommandOutput::Stderr(line)).is_err() {
                        break;
                    }
                }
            }
        });

        // Wait for the command to complete
        let status = child.wait().with_context(|| "Failed to wait for command")?;

        // Wait for output threads
        let _ = stdout_handle.join();
        let _ = stderr_handle.join();

        // Send exit status
        let exit_code = status.code().unwrap_or(-1);
        let _ = tx.send(CommandOutput::Exit(exit_code));

        if exit_code != 0 {
            error!("Command failed with exit code: {}", exit_code);
        }

        Ok(())
    }
}

/// Log entry for command execution
#[derive(Debug, Clone)]
pub struct CommandLog {
    /// Branch name this command was run for
    pub branch: String,
    /// The command that was run
    pub command: String,
    /// Output lines
    pub output: Vec<CommandOutput>,
    /// Whether the command is still running
    pub is_running: bool,
    /// Final exit code (if completed)
    pub exit_code: Option<i32>,
    /// Whether this is a system log (fetch, etc.) vs branch-specific command
    pub is_system_log: bool,
}

impl CommandLog {
    /// Create a new command log (for branch-specific commands)
    pub fn new(branch: String, command: String) -> Self {
        Self {
            branch,
            command,
            output: Vec::new(),
            is_running: true,
            exit_code: None,
            is_system_log: false,
        }
    }

    /// Create a new system log (for fetch, etc.)
    pub fn new_system(name: String, command: String) -> Self {
        Self {
            branch: name,
            command,
            output: Vec::new(),
            is_running: true,
            exit_code: None,
            is_system_log: true,
        }
    }

    /// Add output to the log
    pub fn add_output(&mut self, output: CommandOutput) {
        match &output {
            CommandOutput::Exit(code) => {
                self.is_running = false;
                self.exit_code = Some(*code);
            }
            CommandOutput::Error(_) => {
                self.is_running = false;
                self.exit_code = Some(-1);
            }
            _ => {}
        }
        self.output.push(output);
    }

    /// Check if command succeeded
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Get a summary of the output
    pub fn summary(&self) -> String {
        if self.is_running {
            format!("Running: {}", self.command)
        } else if self.succeeded() {
            format!("✓ {}", self.command)
        } else {
            format!(
                "✗ {} (exit code: {})",
                self.command,
                self.exit_code.unwrap_or(-1)
            )
        }
    }
}
