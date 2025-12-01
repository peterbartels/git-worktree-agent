# git-worktree-manager (gwm)

A terminal UI for managing git worktrees from remote branches with automatic polling and configurable hooks.

![Demo](docs/demo.gif)

## Features

- 🔄 **Automatic Branch Watching**: Polls remote for new branches every 10 seconds (configurable)
- 🌳 **Smart Worktree Creation**: Automatically creates local worktrees for remote branches
- ⚡ **Post-Create Hooks**: Run commands like `npm install` automatically when worktrees are created
- 📋 **Track/Untrack Branches**: Fine-grained control over which branches to manage
- 🎯 **Pattern-Based Filtering**: Ignore branches matching glob patterns
- 💾 **Persistent Configuration**: JSON config file (gitignored for per-user settings)
- 🖥️ **Beautiful TUI**: Built with ratatui for a modern terminal experience

## Installation

### Via npm (recommended)

```bash
npm install -g git-worktree-manager
```

### Via Cargo

```bash
cargo install git-worktree-manager
```

### From Source

```bash
git clone https://github.com/peterbartels/git-worktree-manager
cd git-worktree-manager
cargo install --path .
```

## Quick Start

1. Navigate to your git repository:
   ```bash
   cd /path/to/your/repo
   ```

2. Initialize the configuration:
   ```bash
   gwm --init
   ```

3. Start the TUI:
   ```bash
   gwm
   ```

4. On first run, you'll be prompted to select which branches to create worktrees for.

## Usage

### TUI Mode (default)

```bash
gwm                    # Start in current directory
gwm --path /path/to/repo  # Start in specific directory
gwm --debug            # Enable debug logging
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Create worktree for selected branch |
| `d` | Delete/untrack worktree |
| `t` | Toggle track/untrack branch |
| `r` | Refresh (fetch from remote) |
| `a` | Toggle auto-create mode |
| `l` | View full command logs |
| `?` | Show help |
| `q` / `Esc` | Quit |

### Command Line Options

```bash
# Show current configuration
gwm --show-config

# Set post-create command
gwm --set-command "npm install"

# Set poll interval (in seconds)
gwm --set-poll-interval 30

# Enable auto-create mode
gwm --auto-create

# Initialize configuration interactively
gwm --init
```

## Configuration

The configuration is stored in `.gwm-config.json` in your repository root. This file is automatically added to `.gitignore` since settings are typically per-developer.

### Example Configuration

```json
{
  "version": 1,
  "poll_interval_secs": 10,
  "post_create_command": "npm install",
  "command_working_dir": null,
  "ignore_patterns": [
    "dependabot/*",
    "renovate/*"
  ],
  "tracked_branches": [
    "feature/my-feature",
    "fix/important-bug"
  ],
  "untracked_branches": [
    "main",
    "develop"
  ],
  "auto_create_worktrees": false,
  "worktree_base_dir": "..",
  "remote_name": "origin",
  "worktrees": []
}
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `poll_interval_secs` | number | `10` | How often to check for new branches (seconds) |
| `post_create_command` | string | `null` | Command to run after creating a worktree |
| `command_working_dir` | string | `null` | Subdirectory to run commands in (relative to worktree root) |
| `ignore_patterns` | array | `[]` | Glob patterns for branches to ignore |
| `tracked_branches` | array | `[]` | Branches to explicitly track |
| `untracked_branches` | array | `[]` | Branches to explicitly ignore |
| `auto_create_worktrees` | boolean | `false` | Automatically create worktrees for new branches |
| `worktree_base_dir` | string | `".."` | Where to create worktrees (relative to repo root) |
| `remote_name` | string | `"origin"` | Remote to watch |

## How It Works

1. **Discovery**: GWA discovers the git repository from your current directory
2. **Polling**: Every N seconds, it fetches from the configured remote
3. **Detection**: New branches are detected by comparing with known branches
4. **Creation**: Worktrees are created in the configured base directory
5. **Hooks**: If configured, post-create commands are run automatically

### Worktree Layout

By default, worktrees are created in the parent directory of your repository:

```
projects/
├── my-repo/              # Main repository (where you run gwm)
│   ├── .git/
│   ├── .gwm-config.json
│   └── ...
├── feature-my-feature/   # Worktree for feature/my-feature
├── fix-important-bug/    # Worktree for fix/important-bug
└── ...
```

## Development

### Prerequisites

- Rust 1.70 or later
- Git

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run directly
cargo run

# Run with debug logging
cargo run -- --debug
```

### Testing

```bash
# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

### Project Structure

```
src/
├── main.rs        # Entry point and CLI handling
├── app.rs         # Main application state and TUI logic
├── config.rs      # Configuration management
├── executor.rs    # Command execution for hooks
├── watcher.rs     # Remote branch polling
├── git/
│   ├── mod.rs
│   ├── repository.rs  # Git repository operations
│   └── worktree.rs    # Worktree management
└── ui/
    ├── mod.rs
    ├── branch_list.rs # Branch list widget
    ├── status.rs      # Status bar widget
    ├── logs.rs        # Command logs widget
    └── help.rs        # Help overlay widget
```

### Architecture

- **gitoxide (gix)**: Used for git operations (fetching, reading refs)
- **git CLI**: Used for worktree operations (create, remove, list)
- **ratatui**: TUI framework for the terminal interface
- **crossterm**: Terminal handling (events, rendering)

### Building for Release

The project includes scripts for building release binaries:

```bash
# Install cross for cross-compilation
cargo install cross

# Build for all platforms
./scripts/build-release.sh
```

## Troubleshooting

### "Failed to discover git repository"

Make sure you're running `gwm` from within a git repository or specify the path:

```bash
gwm --path /path/to/repo
```

### "Fetch failed"

Check your network connection and ensure you have access to the remote repository:

```bash
git fetch origin
```

### Worktree creation fails

Ensure the target directory doesn't exist and you have write permissions:

```bash
ls -la ../  # Check parent directory
```

### Hooks not running

Check the command is valid and the working directory exists:

```bash
gwm --show-config  # Verify configuration
```

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Related Projects

- [git-worktree](https://git-scm.com/docs/git-worktree) - Git's built-in worktree command
- [gitoxide](https://github.com/Byron/gitoxide) - Rust implementation of Git
- [ratatui](https://github.com/ratatui/ratatui) - Rust TUI library
