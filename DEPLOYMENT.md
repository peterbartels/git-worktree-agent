# Deployment Guide

This document describes how to create and publish releases for `git-worktree-agent`.

## Overview

This project uses [Changesets](https://github.com/changesets/changesets) for version management and automated releases. The release process is fully automated via GitHub Actions.

## Prerequisites

Before you can publish releases, ensure the following are configured:

### 1. NPM Trusted Publisher (OIDC)

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

**Repeat for each platform package** (`@gwa/linux-x64`, `@gwa/darwin-arm64`, etc.)

> **Why OIDC?** Unlike long-lived tokens, OIDC provides short-lived credentials that can't be leaked. npm can verify exactly which repository and workflow is publishing.

### 2. GitHub Token

The `GITHUB_TOKEN` is automatically provided by GitHub Actions. The workflow has these permissions:

```yaml
permissions:
  contents: write       # Create releases
  pull-requests: write  # Create version PRs
  id-token: write       # OIDC for npm
```

## How to Create a Release

### Step 1: Create a Changeset

When you make changes that should be released, create a changeset:

```bash
npx changeset
```

You'll be prompted to:

1. **Select packages**: Choose `git-worktree-agent`
2. **Select bump type**:
   - `patch` - Bug fixes, minor updates (0.1.0 → 0.1.1)
   - `minor` - New features, backwards compatible (0.1.0 → 0.2.0)
   - `major` - Breaking changes (0.1.0 → 1.0.0)
3. **Write a summary**: Describe the change (this becomes the changelog entry)

This creates a markdown file in `.changeset/` directory.

### Step 2: Commit the Changeset

Commit the changeset file with your code changes:

```bash
git add .changeset/*.md
git commit -m "feat: add new feature"
git push
```

### Step 3: Merge to Main

When your PR is merged to `main`, the GitHub Action will:

1. Detect changesets in the merge
2. Create or update a "Version Packages" PR
3. This PR will:
   - Bump versions based on changesets
   - Update `CHANGELOG.md`
   - Remove processed changeset files

### Step 4: Publish the Release

When you merge the "Version Packages" PR:

1. The release workflow triggers
2. Builds binaries for all platforms:
   - Linux x64 & arm64
   - macOS x64 & Apple Silicon (arm64)
   - Windows x64
3. Publishes platform-specific npm packages (`@gwa/linux-x64`, etc.)
4. Publishes the main `git-worktree-agent` package to npm
5. Creates a GitHub Release with:
   - Pre-built binaries
   - Auto-generated release notes
   - SHA256 checksums

## Package Structure

The npm distribution consists of multiple packages:

### Main Package

```
git-worktree-agent
├── bin/gwa          # Shell script that runs the binary
├── npm/install.js   # Post-install script that downloads the binary
└── package.json
```

### Platform Packages

Each platform has its own npm package containing just the binary:

- `@gwa/linux-x64`
- `@gwa/linux-arm64`
- `@gwa/darwin-x64`
- `@gwa/darwin-arm64`
- `@gwa/win32-x64`

When users run `npm install -g git-worktree-agent`:

1. npm installs the main package
2. npm installs the appropriate platform package via `optionalDependencies`
3. The `postinstall` script copies the binary to the correct location

## Manual Release (Emergency)

If you need to release without changesets (requires npm login locally):

### 1. Update Version

```bash
# Update package.json version
npm version patch  # or minor/major

# Update Cargo.toml version to match
```

### 2. Build Binaries Locally

```bash
./scripts/build-release.sh
```

### 3. Publish Platform Packages

```bash
cd dist/npm/linux-x64
npm publish --access public

cd ../linux-arm64
npm publish --access public

# Repeat for all platforms...
```

### 4. Publish Main Package

```bash
cd /path/to/project
npm publish --access public
```

### 5. Create GitHub Release

```bash
gh release create v0.x.x dist/* --generate-notes
```

## Troubleshooting

### "npm ERR! 403 Forbidden"

- Ensure Trusted Publisher is configured for the package on npm
- Verify the workflow filename matches exactly (`release.yml`)
- Check that the repository owner/name match
- For new packages, you may need to publish manually first, then configure Trusted Publisher

### Build Fails for Platform

- Check if the Rust target is installed
- For cross-compilation, ensure Docker is working (required by `cross`)
- macOS builds require a macOS runner

### Version Mismatch

If npm and Cargo versions get out of sync:

1. Manually update both `package.json` and `Cargo.toml` to the same version
2. Commit and push
3. The next release will use the corrected version

### Changeset Not Detected

- Ensure the changeset file is in `.changeset/` directory
- File should be a `.md` file with valid frontmatter
- Check that the package name in the changeset matches `package.json`

## CI/CD Workflows

### `.github/workflows/ci.yml`

Runs on every push and PR:
- Builds the project
- Runs tests
- Checks formatting (`cargo fmt`)
- Runs clippy lints

### `.github/workflows/release.yml`

Runs on push to `main`:
- Creates/updates Version Packages PR (via changesets)
- On version merge: builds binaries, publishes to npm, creates GitHub release

## Version Strategy

We follow [Semantic Versioning](https://semver.org/):

- **MAJOR**: Breaking changes to CLI interface or config format
- **MINOR**: New features, new CLI flags, backwards-compatible changes
- **PATCH**: Bug fixes, performance improvements, documentation updates

During initial development (0.x.x), minor versions may contain breaking changes.

