//! Status bar widget

use chrono::{DateTime, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::Theme;

/// Application status
#[derive(Debug, Clone, Default)]
pub struct AppStatus {
    /// Whether we're currently fetching
    pub is_fetching: bool,
    /// Last fetch time
    pub last_fetch: Option<DateTime<Utc>>,
    /// Number of remote branches
    pub remote_branch_count: usize,
    /// Number of local worktrees
    pub worktree_count: usize,
    /// Number of running hooks
    pub running_hooks: usize,
    /// Last error message
    pub last_error: Option<String>,
    /// Auto-create enabled
    pub auto_create_enabled: bool,
    /// Poll interval in seconds
    pub poll_interval: u64,
    /// Current remote name
    pub remote_name: String,
}

/// Status widget
pub struct StatusWidget<'a> {
    status: &'a AppStatus,
    theme: &'a Theme,
}

impl<'a> StatusWidget<'a> {
    pub fn new(status: &'a AppStatus, theme: &'a Theme) -> Self {
        Self { status, theme }
    }

    fn format_last_fetch(&self) -> String {
        match self.status.last_fetch {
            Some(dt) => {
                let now = Utc::now();
                let duration = now.signed_duration_since(dt);

                if duration.num_seconds() < 60 {
                    format!("{}s ago", duration.num_seconds())
                } else if duration.num_minutes() < 60 {
                    format!("{}m ago", duration.num_minutes())
                } else {
                    format!("{}h ago", duration.num_hours())
                }
            }
            None => "never".to_string(),
        }
    }
}

impl Widget for StatusWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let fetch_status = if self.status.is_fetching {
            Span::styled(
                "⟳ Fetching... ",
                Style::default().fg(self.theme.primary),
            )
        } else {
            Span::styled(
                format!("◉ Last fetch: {} ", self.format_last_fetch()),
                Style::default().fg(self.theme.muted),
            )
        };

        let auto_create = if self.status.auto_create_enabled {
            Span::styled(
                "│ Auto: ON ",
                Style::default().fg(self.theme.success),
            )
        } else {
            Span::styled(
                "│ Auto: OFF ",
                Style::default().fg(self.theme.muted),
            )
        };

        let poll_info = Span::styled(
            format!("│ Poll: {}s ", self.status.poll_interval),
            Style::default().fg(self.theme.muted),
        );

        let branch_count = Span::styled(
            format!(
                "│ Remote: {} │ Worktrees: {} ",
                self.status.remote_branch_count, self.status.worktree_count
            ),
            Style::default().fg(self.theme.secondary),
        );

        let hook_status = if self.status.running_hooks > 0 {
            Span::styled(
                format!("│ Hooks: {} running ", self.status.running_hooks),
                Style::default().fg(self.theme.warning),
            )
        } else {
            Span::raw("")
        };

        let error_status = if let Some(ref err) = self.status.last_error {
            Span::styled(
                format!("│ ⚠ {} ", truncate_str(err, 40)),
                Style::default().fg(self.theme.error),
            )
        } else {
            Span::raw("")
        };

        let line = Line::from(vec![
            fetch_status,
            auto_create,
            poll_info,
            branch_count,
            hook_status,
            error_status,
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.muted))
            .title(Span::styled(
                format!(" {} ", self.status.remote_name),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(line).block(block);
        paragraph.render(area, buf);
    }
}

fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len.saturating_sub(3)]
    }
}

