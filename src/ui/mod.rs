//! TUI components for git-worktree-agent

mod branch_list;
mod help;
mod logs;
mod status;

pub use branch_list::{BranchItem, BranchListState, BranchListWidget, BranchStatus};
pub use help::HelpWidget;
pub use logs::{BranchLogWidget, LogsState, ScrollableLogsWidget};
pub use status::{AppStatus, StatusWidget};

use ratatui::style::Color;

/// Color scheme for the application
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
    pub highlight: Color,
    pub bg: Color,
    pub fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: Color::Rgb(138, 180, 248),    // Soft blue
            secondary: Color::Rgb(187, 154, 247),  // Lavender
            success: Color::Rgb(166, 218, 149),    // Soft green
            warning: Color::Rgb(238, 190, 111),    // Amber
            error: Color::Rgb(237, 135, 150),      // Coral red
            muted: Color::Rgb(108, 112, 134),      // Gray
            highlight: Color::Rgb(245, 224, 220),  // Cream
            bg: Color::Rgb(30, 30, 46),            // Dark base
            fg: Color::Rgb(205, 214, 244),         // Light text
        }
    }
}

