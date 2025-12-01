# Development Guide

This guide is for developers working on git-worktree-agent itself.

## Prerequisites

- **Rust**: 1.70 or later (install via [rustup](https://rustup.rs/))
- **Git**: For version control and testing
- **Node.js**: 16+ (for npm publishing)
- **Docker**: For cross-compilation (optional)

## Getting Started

### Clone and Build

```bash
git clone https://github.com/peterbartels/git-worktree-agent
cd git-worktree-agent

# Install dependencies and build
cargo build

# Run in development
cargo run

# Run with debug logging
cargo run -- --debug
```

### Development Workflow

1. **Make changes** to the code
2. **Test locally**:
   ```bash
   cargo run
   # or with debug output
   RUST_LOG=debug cargo run
   ```
3. **Run tests**:
   ```bash
   cargo test
   ```
4. **Check formatting**:
   ```bash
   cargo fmt --check
   ```
5. **Run lints**:
   ```bash
   cargo clippy
   ```

## Project Architecture

### Module Overview

```
src/
├── main.rs          # CLI entry point, argument parsing
├── app.rs           # Main TUI application logic
├── config.rs        # JSON configuration handling
├── executor.rs      # Command execution (post-create hooks)
├── watcher.rs       # Remote branch polling logic
├── git/
│   ├── mod.rs
│   ├── repository.rs    # gitoxide-based repo operations
│   └── worktree.rs      # git worktree management
└── ui/
    ├── mod.rs
    ├── branch_list.rs   # Branch list widget
    ├── status.rs        # Status bar widget
    ├── logs.rs          # Command logs widget
    └── help.rs          # Help overlay
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `crossterm` | Cross-platform terminal handling |
| `gix` | Git operations (gitoxide) |
| `serde` + `serde_json` | Configuration serialization |
| `tokio` | Async runtime (for future use) |
| `clap` | Command-line argument parsing |
| `tracing` | Logging and diagnostics |

### Data Flow

1. **Startup**:
   - Parse CLI arguments
   - Discover git repository
   - Load or create configuration
   - Initialize watcher with known branches

2. **Main Loop**:
   - Poll for terminal events (keyboard input)
   - Check if polling interval has elapsed
   - Process watcher events (fetch results, new branches)
   - Render UI

3. **Branch Polling**:
   - Fetch from remote (using gitoxide)
   - Compare remote refs with known branches
   - Detect new branches
   - Optionally create worktrees

4. **Worktree Creation**:
   - Use git CLI for worktree operations
   - Update configuration state
   - Run post-create hooks

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_sanitize_branch_name

# With output
cargo test -- --nocapture
```

### Manual Testing

For testing the TUI, you'll need a git repository with a remote:

```bash
# Create a test repository
mkdir /tmp/test-repo
cd /tmp/test-repo
git init
git remote add origin https://github.com/some/repo

# Run git-worktree-agent
cargo run -- --path /tmp/test-repo --debug
```

### Testing Different Scenarios

1. **First run** (no config):
   - Should show initial branch selection screen
   
2. **Polling**:
   - Wait for poll interval to see fetch activity
   
3. **Worktree creation**:
   - Select a branch and press Enter
   
4. **Hook execution**:
   - Set a post-create command and create a worktree
   
5. **Error handling**:
   - Test with invalid remotes, network issues, etc.

## Building for Release

### Local Release Build

```bash
cargo build --release
# Binary at: target/release/gwa
```

### Cross-Platform Builds

We use [cross](https://github.com/cross-rs/cross) for cross-compilation:

```bash
# Install cross
cargo install cross

# Build for all platforms
./scripts/build-release.sh
```

### Supported Platforms

| Platform | Target |
|----------|--------|
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |
| macOS x64 | `x86_64-apple-darwin` |
| macOS ARM64 | `aarch64-apple-darwin` |
| Windows x64 | `x86_64-pc-windows-gnu` |

## Publishing

### Cargo (crates.io)

```bash
# Login (first time)
cargo login

# Publish
cargo publish
```

### npm

1. Build binaries for all platforms:
   ```bash
   ./scripts/build-release.sh
   ```

2. Publish platform packages:
   ```bash
   cd dist/npm/linux-x64 && npm publish --access public
   cd dist/npm/linux-arm64 && npm publish --access public
   # ... etc for each platform
   ```

3. Publish main package:
   ```bash
   npm publish --access public
   ```

## Debugging

### Enable Debug Logging

```bash
# Via CLI flag
cargo run -- --debug

# Via environment variable
RUST_LOG=debug cargo run
RUST_LOG=gwa=debug cargo run  # Just this crate
```

### Common Issues

1. **Terminal rendering issues**:
   - Try a different terminal emulator
   - Check terminal supports Unicode

2. **Git authentication**:
   - Ensure SSH keys are configured
   - Check git credential helpers

3. **Cross-compilation failures**:
   - Ensure Docker is running
   - Check cross configuration

## Code Style

- Follow Rust conventions
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Add doc comments for public APIs
- Write tests for new functionality

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and lints
5. Submit a pull request

## Version Bumping

1. Update `Cargo.toml` version
2. Update `package.json` version
3. Update CHANGELOG.md
4. Create git tag: `git tag v0.x.x`
5. Push tag: `git push --tags`

