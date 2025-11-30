//! View rendering functions

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::App;
use crate::ui::{
    BranchListWidget, BranchLogWidget, ScrollableLogsWidget, StatusWidget,
};

impl App {
    /// Render the main view
    pub(super) fn render_main(&mut self, frame: &mut Frame, area: Rect) {
        // Main vertical layout: status, content, logs, hint
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Status bar
                Constraint::Min(10),    // Split view (branches + command log)
                Constraint::Length(6),  // Bottom logs (reduced)
                Constraint::Length(1),  // Keybindings hint
            ])
            .split(area);

        // Status bar
        frame.render_widget(StatusWidget::new(&self.status, &self.theme), main_chunks[0]);

        // Split view: branches (25%) | command log (75%)
        let split_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),  // Branch list
                Constraint::Percentage(75),  // Branch command log
            ])
            .split(main_chunks[1]);

        // Branch list (left side)
        frame.render_stateful_widget(
            BranchListWidget::new(" Branches ", &self.theme),
            split_chunks[0],
            &mut self.branch_list_state,
        );

        // Branch command log (right side)
        let selected_branch = self.branch_list_state.selected_branch();
        frame.render_widget(
            BranchLogWidget::new(
                &self.watcher.command_logs,
                selected_branch.as_deref(),
                &self.theme,
                &mut self.branch_logs_state,
            ),
            split_chunks[1],
        );

        // Bottom logs (general git output)
        frame.render_widget(
            ScrollableLogsWidget::new(&self.watcher.command_logs, &self.theme, &mut self.logs_state),
            main_chunks[2],
        );

        // Keybindings hint
        let hint = Line::from(vec![
            Span::styled(" ↑/↓", Style::default().fg(self.theme.primary)),
            Span::styled(" nav ", Style::default().fg(self.theme.muted)),
            Span::styled("j/k", Style::default().fg(self.theme.primary)),
            Span::styled(" scroll ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter", Style::default().fg(self.theme.primary)),
            Span::styled(" create ", Style::default().fg(self.theme.muted)),
            Span::styled("d", Style::default().fg(self.theme.primary)),
            Span::styled(" del ", Style::default().fg(self.theme.muted)),
            Span::styled("t", Style::default().fg(self.theme.primary)),
            Span::styled(" track ", Style::default().fg(self.theme.muted)),
            Span::styled("s", Style::default().fg(self.theme.primary)),
            Span::styled(" settings ", Style::default().fg(self.theme.muted)),
            Span::styled("?", Style::default().fg(self.theme.primary)),
            Span::styled(" help ", Style::default().fg(self.theme.muted)),
            Span::styled("q", Style::default().fg(self.theme.primary)),
            Span::styled(" quit", Style::default().fg(self.theme.muted)),
        ]);
        frame.render_widget(Paragraph::new(hint), main_chunks[3]);
    }

    /// Render full-screen logs view
    pub(super) fn render_logs_fullscreen(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            ScrollableLogsWidget::new(&self.watcher.command_logs, &self.theme, &mut self.logs_state)
                .show_all(),
            area,
        );
    }

    /// Render error view
    pub(super) fn render_error(&self, frame: &mut Frame, area: Rect, error_msg: String) {
        // Calculate popup size
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 15.min(area.height.saturating_sub(4));
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Error block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.error))
            .title(Span::styled(
                " ⚠ Error ",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);

        // Split error message into lines
        let mut lines: Vec<Line> = vec![Line::raw("")];
        
        for line in error_msg.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(self.theme.fg),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("Press ", Style::default().fg(self.theme.muted)),
            Span::styled("q", Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" or ", Style::default().fg(self.theme.muted)),
            Span::styled("Esc", Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(" to exit", Style::default().fg(self.theme.muted)),
        ]));

        frame.render_widget(block, popup_area);
        frame.render_widget(Paragraph::new(lines), inner);
    }

    /// Render delete confirmation dialog
    pub(super) fn render_delete_confirm(&self, frame: &mut Frame, area: Rect, branch: String, input: String) {
        // Center the popup
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 10;

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.error))
            .title(Span::styled(
                " ⚠ Delete Worktree ",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::raw("Delete worktree for branch "),
                Span::styled(&branch, Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD)),
                Span::raw("?"),
            ]),
            Line::raw(""),
            Line::styled(
                "This will remove the worktree directory!",
                Style::default().fg(self.theme.warning),
            ),
            Line::raw(""),
            Line::from(vec![
                Span::raw("Type "),
                Span::styled("yes", Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
                Span::raw(" to confirm: "),
                Span::styled(&input, Style::default().fg(self.theme.fg).add_modifier(Modifier::UNDERLINED)),
                Span::styled("█", Style::default().fg(self.theme.primary)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(paragraph, inner);
    }
}

