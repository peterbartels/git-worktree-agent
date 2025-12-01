# git-worktree-agent

## 0.2.1

### Patch Changes

- 24a82c2: Fix npm publish by adding provenance config and bin/gwa wrapper

## 0.2.0

### Minor Changes

- a4c0c30: Initial release of git-worktree-agent (gwa)

  - TUI for managing git worktrees from remote branches
  - Automatic branch watching with configurable poll interval
  - Smart worktree creation for remote branches
  - Post-create hooks (e.g., run `npm install` automatically)
  - Track/untrack branches with fine-grained control
  - Pattern-based branch filtering with glob patterns
  - Persistent JSON configuration per repository
  - Beautiful terminal UI built with ratatui
