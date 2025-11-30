//! Git operations module using gitoxide
//!
//! Provides functionality for:
//! - Discovering and managing git repositories
//! - Fetching remote branches
//! - Creating and managing worktrees

mod repository;
mod worktree;

pub use repository::*;
pub use worktree::*;
