# Deployment Guide

This document describes how to create and publish releases for `git-worktree-agent`.

## Overview

This project uses [Changesets](https://github.com/changesets/changesets) for version management and automated releases. The release process is fully automated via GitHub Actions.

## Architecture

The npm package bundles **all platform binaries** in a single package:

```
git-worktree-agent/
├── bin/
│   ├── gwa           # JS wrapper (entry point)
│   └── install.js    # Postinstall script
└── binaries/
    ├── gwa-linux-x64
    ├── gwa-linux-arm64
    ├── gwa-darwin-x64
    ├── gwa-darwin-arm64
    └── gwa-win32-x64.exe
```

When users install, the postinstall script copies the correct binary to `bin/gwa-binary`.

## Prerequisites

### NPM Trusted Publisher (OIDC)

This project uses npm's Trusted Publisher feature with OpenID Connect (OIDC) for secure, tokenless publishing.

**Configure on npm:**

1. Go to [npmjs.com](https://www.npmjs.com/) and log in
2. Navigate to your package → **Settings** → **Trusted Publisher**
3. Select **GitHub Actions**
4. Enter:
   - **Repository owner**: `peterbartels`
   - **Repository name**: `git-worktree-agent`
   - **Workflow filename**: `release.yml`
   - **Environment**: (leave blank)

### GitHub Token

The `GITHUB_TOKEN` is automatically provided by GitHub Actions with these permissions:

```yaml
permissions:
  contents: write       # Create releases
  pull-requests: write  # Create version PRs
  id-token: write       # OIDC for npm
```

## How to Create a Release

### Step 1: Create a Changeset

When you make changes that should be released:

```bash
npx changeset
```

You'll be prompted to:

1. **Select packages**: Choose `git-worktree-agent`
2. **Select bump type**:
   - `patch` - Bug fixes (0.1.0 → 0.1.1)
   - `minor` - New features (0.1.0 → 0.2.0)
   - `major` - Breaking changes (0.1.0 → 1.0.0)
3. **Write a summary**: This becomes the changelog entry

### Step 2: Commit and Push

```bash
git add .changeset/*.md
git commit -m "feat: your feature description"
git push
```

### Step 3: What Happens Next

1. **GitHub Actions detects changesets** → builds binaries for all 5 platforms
2. **Creates "Version Packages" PR** with version bump and changelog
3. **You merge the PR** → triggers publish
4. **Package published to npm** with all binaries
5. **GitHub Release created** with standalone binaries

## Release Workflow

```
Push with changeset
       ↓
┌──────────────────────────────────────────┐
│  Build Binaries (parallel)               │
│  ├── linux-x64    (ubuntu-latest)        │
│  ├── linux-arm64  (ubuntu + cross)       │
│  ├── darwin-x64   (macos-latest)         │
│  ├── darwin-arm64 (macos-latest)         │
│  └── win32-x64    (windows-latest)       │
└──────────────────────────────────────────┘
       ↓
┌──────────────────────────────────────────┐
│  Release Job                             │
│  ├── Download all binaries               │
│  ├── Bundle into npm package             │
│  ├── Publish to npm (with provenance)    │
│  └── Create GitHub Release               │
└──────────────────────────────────────────┘
```

## Manual Release (Emergency)

If you need to release manually:

### 1. Build All Binaries

You'll need access to all platforms or use cross-compilation:

```bash
# Linux (native)
cargo build --release
cp target/release/gwa binaries/gwa-linux-x64

# Linux ARM64 (using cross)
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/gwa binaries/gwa-linux-arm64

# macOS/Windows - need native machines or CI
```

### 2. Update Version

```bash
npm version patch  # or minor/major
# Also update Cargo.toml to match
```

### 3. Publish

```bash
npm publish --access public
```

### 4. Create GitHub Release

```bash
gh release create v0.x.x binaries/* --generate-notes
```

## Troubleshooting

### "npm ERR! 403 Forbidden"

- Ensure Trusted Publisher is configured on npm
- Verify workflow filename matches (`release.yml`)
- For new packages, first publish manually then configure OIDC

### Build Fails for Platform

- macOS builds require `macos-latest` runner
- Linux ARM64 uses `cross` for cross-compilation
- Check Rust target is available: `rustup target list`

### Provenance Issues

Ensure package.json has:

```json
"publishConfig": {
  "access": "public",
  "provenance": true
}
```

And workflow has:

```yaml
permissions:
  id-token: write
env:
  NPM_CONFIG_PROVENANCE: true
```

## Version Strategy

We follow [Semantic Versioning](https://semver.org/):

- **MAJOR**: Breaking changes to CLI or config format
- **MINOR**: New features, backwards-compatible
- **PATCH**: Bug fixes, docs, performance

During initial development (0.x.x), minor versions may contain breaking changes.
