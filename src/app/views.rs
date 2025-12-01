//! View rendering functions

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use super::App;
use super::state::{CreateWorktreeState, CreateWorktreeStep};
use crate::ui::{BranchListWidget, BranchLogWidget, ScrollableLogsWidget, StatusWidget};

impl App {
    /// Render the main view
    pub(super) fn render_main(&mut self, frame: &mut Frame, area: Rect) {
        // Main vertical layout: status, content, logs, hint
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Status bar
                Constraint::Min(10),   // Split view (branches + command log)
                Constraint::Length(6), // Bottom logs (reduced)
                Constraint::Length(1), // Keybindings hint
            ])
            .split(area);

        // Status bar
        frame.render_widget(StatusWidget::new(&self.status, &self.theme), main_chunks[0]);

        // Split view: branches (25%) | command log (75%)
        let split_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25), // Branch list
                Constraint::Percentage(75), // Branch command log
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
            ScrollableLogsWidget::new(
                &self.watcher.command_logs,
                &self.theme,
                &mut self.logs_state,
            ),
            main_chunks[2],
        );

        // Keybindings hint
        let hint = Line::from(vec![
            Span::styled(" ↑/↓", Style::default().fg(self.theme.primary)),
            Span::styled(" nav ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter", Style::default().fg(self.theme.primary)),
            Span::styled(" checkout ", Style::default().fg(self.theme.muted)),
            Span::styled("c", Style::default().fg(self.theme.primary)),
            Span::styled(" new ", Style::default().fg(self.theme.muted)),
            Span::styled("o", Style::default().fg(self.theme.primary)),
            Span::styled(" cd ", Style::default().fg(self.theme.muted)),
            Span::styled("d", Style::default().fg(self.theme.primary)),
            Span::styled(" del ", Style::default().fg(self.theme.muted)),
            Span::styled("u", Style::default().fg(self.theme.primary)),
            Span::styled(" hide ", Style::default().fg(self.theme.muted)),
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
            ScrollableLogsWidget::new(
                &self.watcher.command_logs,
                &self.theme,
                &mut self.logs_state,
            )
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
            Span::styled(
                "q",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" or ", Style::default().fg(self.theme.muted)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to exit", Style::default().fg(self.theme.muted)),
        ]));

        frame.render_widget(block, popup_area);
        frame.render_widget(Paragraph::new(lines), inner);
    }

    /// Render delete confirmation dialog
    pub(super) fn render_delete_confirm(
        &self,
        frame: &mut Frame,
        area: Rect,
        branch: String,
        input: String,
    ) {
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
                Span::styled(
                    &branch,
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
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
                Span::styled(
                    "yes",
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" to confirm: "),
                Span::styled(
                    &input,
                    Style::default()
                        .fg(self.theme.fg)
                        .add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled("█", Style::default().fg(self.theme.primary)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(paragraph, inner);
    }

    /// Render create worktree dialog (2-step wizard)
    pub(super) fn render_create_worktree(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &CreateWorktreeState,
    ) {
        match state.step {
            CreateWorktreeStep::SelectBaseBranch => {
                self.render_create_worktree_step1(frame, area, state);
            }
            CreateWorktreeStep::EnterBranchName => {
                self.render_create_worktree_step2(frame, area, state);
            }
        }
    }

    /// Step 1: Select base branch (with search)
    fn render_create_worktree_step1(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &CreateWorktreeState,
    ) {
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 18.min(area.height.saturating_sub(4));

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.secondary))
            .title(Span::styled(
                " Create Worktree (1/2) ",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Title
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Filter input
                Constraint::Length(1), // Spacing
                Constraint::Min(5),    // Branch list
                Constraint::Length(2), // Instructions
            ])
            .split(inner);

        // Title
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Select base branch to checkout from:",
                Style::default().fg(self.theme.fg),
            ))),
            chunks[0],
        );

        // Filter input with count and placeholder
        let filtered_branches = state.filtered_branches();
        let filter_count = if state.base_branch_filter.is_empty() {
            format!("{} branches", filtered_branches.len())
        } else {
            format!(
                "{}/{} matching",
                filtered_branches.len(),
                state.base_branches.len()
            )
        };

        // Show placeholder when filter is empty
        let filter_display = if state.base_branch_filter.is_empty() {
            if let Some(ref default) = state.default_branch {
                format!("default ({})", default)
            } else {
                String::new()
            }
        } else {
            state.base_branch_filter.clone()
        };

        let filter_style = if state.base_branch_filter.is_empty() {
            Style::default().fg(self.theme.muted)
        } else {
            Style::default().fg(self.theme.fg)
        };

        let filter_line = Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(self.theme.primary)),
            Span::styled(filter_display, filter_style),
            Span::styled("█", Style::default().fg(self.theme.primary)),
            Span::styled(
                format!("  ({})", filter_count),
                Style::default().fg(self.theme.muted),
            ),
        ]);
        frame.render_widget(Paragraph::new(filter_line), chunks[2]);

        // Branch list
        let visible_items: usize = chunks[4].height as usize;
        let start_idx = if state.selected_base_index >= visible_items {
            state.selected_base_index - visible_items + 1
        } else {
            0
        };

        let items: Vec<ListItem> = filtered_branches
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(visible_items)
            .map(|(i, branch)| {
                let is_selected = i == state.selected_base_index;
                let style = if is_selected {
                    Style::default()
                        .fg(self.theme.primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.muted)
                };

                let prefix = if is_selected { "▸ " } else { "  " };
                ListItem::new(Line::from(Span::styled(
                    format!("{}{}", prefix, branch),
                    style,
                )))
            })
            .collect();

        if items.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  No matching branches",
                    Style::default().fg(self.theme.warning),
                ))),
                chunks[4],
            );
        } else {
            frame.render_widget(List::new(items), chunks[4]);
        }

        // Instructions
        let instructions = Line::from(vec![
            Span::styled("Type", Style::default().fg(self.theme.primary)),
            Span::styled(" filter  ", Style::default().fg(self.theme.muted)),
            Span::styled("↑↓", Style::default().fg(self.theme.primary)),
            Span::styled(" select  ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter", Style::default().fg(self.theme.primary)),
            Span::styled(" next  ", Style::default().fg(self.theme.muted)),
            Span::styled("Esc", Style::default().fg(self.theme.primary)),
            Span::styled(" cancel", Style::default().fg(self.theme.muted)),
        ]);
        frame.render_widget(
            Paragraph::new(instructions).alignment(ratatui::layout::Alignment::Center),
            chunks[5],
        );
    }

    /// Step 2: Enter new branch name
    fn render_create_worktree_step2(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &CreateWorktreeState,
    ) {
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 10.min(area.height.saturating_sub(4));

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.secondary))
            .title(Span::styled(
                " Create Worktree (2/2) ",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Base branch info
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Label
                Constraint::Length(1), // Input
                Constraint::Min(1),    // Spacing
                Constraint::Length(2), // Instructions
            ])
            .split(inner);

        // Show selected base branch
        let base_branch = state.selected_base.as_deref().unwrap_or("(none)");
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("From: ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    base_branch,
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            chunks[0],
        );

        // Label
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Enter new branch name:",
                Style::default().fg(self.theme.fg),
            ))),
            chunks[2],
        );

        // Input
        let input_line = Line::from(vec![
            Span::styled(&state.new_branch_name, Style::default().fg(self.theme.fg)),
            Span::styled("█", Style::default().fg(self.theme.primary)),
        ]);
        frame.render_widget(Paragraph::new(input_line), chunks[3]);

        // Instructions
        let instructions = Line::from(vec![
            Span::styled("Enter", Style::default().fg(self.theme.primary)),
            Span::styled(" create  ", Style::default().fg(self.theme.muted)),
            Span::styled("Esc/Backspace", Style::default().fg(self.theme.primary)),
            Span::styled(" back", Style::default().fg(self.theme.muted)),
        ]);
        frame.render_widget(
            Paragraph::new(instructions).alignment(ratatui::layout::Alignment::Center),
            chunks[5],
        );
    }
}
