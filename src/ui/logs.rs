//! Command logs widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Widget,
    },
};

use crate::executor::{CommandLog, CommandOutput};

use super::Theme;

/// Logs widget state
#[derive(Default)]
pub struct LogsState {
    pub scroll: u16,
    pub max_scroll: u16,
}

impl LogsState {
    pub fn scroll_down(&mut self) {
        if self.scroll < self.max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.max_scroll;
    }
}

/// Scrollable logs widget
pub struct ScrollableLogsWidget<'a> {
    logs: &'a [CommandLog],
    theme: &'a Theme,
    state: &'a mut LogsState,
    /// If true, only show system logs; if false, show all logs
    system_only: bool,
}

impl<'a> ScrollableLogsWidget<'a> {
    pub fn new(logs: &'a [CommandLog], theme: &'a Theme, state: &'a mut LogsState) -> Self {
        Self {
            logs,
            theme,
            state,
            system_only: true,
        }
    }

    pub fn show_all(mut self) -> Self {
        self.system_only = false;
        self
    }

    fn render_log_owned(&self, log: &CommandLog, _inner_width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header line
        let status_icon = if log.is_running {
            Span::styled("⟳ ", Style::default().fg(self.theme.primary))
        } else if log.succeeded() {
            Span::styled("✓ ", Style::default().fg(self.theme.success))
        } else {
            Span::styled("✗ ", Style::default().fg(self.theme.error))
        };

        lines.push(Line::from(vec![
            status_icon,
            Span::styled(
                log.branch.clone(),
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" $ ", Style::default().fg(self.theme.muted)),
            Span::styled(log.command.clone(), Style::default().fg(self.theme.fg)),
        ]));

        for output in &log.output {
            match output {
                CommandOutput::Stdout(line) => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line.clone(), Style::default().fg(self.theme.fg)),
                    ]));
                }
                CommandOutput::Stderr(line) => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line.clone(), Style::default().fg(self.theme.warning)),
                    ]));
                }
                CommandOutput::Exit(code) => {
                    let style = if *code == 0 {
                        Style::default().fg(self.theme.success)
                    } else {
                        Style::default().fg(self.theme.error)
                    };
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("Exit code: {}", code), style),
                    ]));
                }
                CommandOutput::Error(msg) => {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("Error: {}", msg),
                            Style::default().fg(self.theme.error),
                        ),
                    ]));
                }
            }
        }

        lines
    }
}

impl Widget for ScrollableLogsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.secondary))
            .title(Span::styled(
                " Logs ",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner_area = block.inner(area);

        // Collect lines (optionally filtering to system logs only)
        let mut all_lines: Vec<Line> = Vec::new();

        let logs_iter = self
            .logs
            .iter()
            .rev()
            .filter(|l| !self.system_only || l.is_system_log);
        for log in logs_iter {
            if !all_lines.is_empty() {
                all_lines.push(Line::raw("─".repeat(inner_area.width as usize)));
            }
            all_lines.extend(self.render_log_owned(log, inner_area.width));
        }

        // Update max scroll
        let content_height = all_lines.len() as u16;
        self.state.max_scroll = content_height.saturating_sub(inner_area.height);

        // Apply scroll
        let visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(self.state.scroll as usize)
            .take(inner_area.height as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines).block(block);
        paragraph.render(area, buf);

        // Render scrollbar
        if self.state.max_scroll > 0 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(self.state.max_scroll as usize)
                .position(self.state.scroll as usize);

            StatefulWidget::render(
                scrollbar,
                area.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                buf,
                &mut scrollbar_state,
            );
        }
    }
}

/// Branch command log widget - shows detailed output for a specific branch
pub struct BranchLogWidget<'a> {
    logs: &'a [CommandLog],
    branch: Option<&'a str>,
    theme: &'a Theme,
    state: &'a mut LogsState,
}

impl<'a> BranchLogWidget<'a> {
    pub fn new(
        logs: &'a [CommandLog],
        branch: Option<&'a str>,
        theme: &'a Theme,
        state: &'a mut LogsState,
    ) -> Self {
        Self {
            logs,
            branch,
            theme,
            state,
        }
    }

    fn render_log_detail(&self, log: &CommandLog) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header with command
        let status_icon = if log.is_running {
            Span::styled("⟳ ", Style::default().fg(self.theme.primary))
        } else if log.succeeded() {
            Span::styled("✓ ", Style::default().fg(self.theme.success))
        } else {
            Span::styled("✗ ", Style::default().fg(self.theme.error))
        };

        lines.push(Line::from(vec![
            status_icon,
            Span::styled("$ ", Style::default().fg(self.theme.muted)),
            Span::styled(
                log.command.clone(),
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // All output lines
        for output in &log.output {
            match output {
                CommandOutput::Stdout(line) => {
                    lines.push(Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(self.theme.fg),
                    )));
                }
                CommandOutput::Stderr(line) => {
                    lines.push(Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(self.theme.warning),
                    )));
                }
                CommandOutput::Exit(code) => {
                    let style = if *code == 0 {
                        Style::default().fg(self.theme.success)
                    } else {
                        Style::default().fg(self.theme.error)
                    };
                    lines.push(Line::from(Span::styled(format!("Exit: {}", code), style)));
                }
                CommandOutput::Error(msg) => {
                    lines.push(Line::from(Span::styled(
                        format!("Error: {}", msg),
                        Style::default().fg(self.theme.error),
                    )));
                }
            }
        }

        lines
    }
}

impl Widget for BranchLogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.branch {
            Some(branch) => format!(" {} ", branch),
            None => " Select a branch ".to_string(),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.muted))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner_area = block.inner(area);

        // Find logs for this branch
        let branch_logs: Vec<&CommandLog> = if let Some(branch) = self.branch {
            self.logs.iter().filter(|l| l.branch == branch).collect()
        } else {
            Vec::new()
        };

        if branch_logs.is_empty() {
            let message = if self.branch.is_some() {
                "No commands run yet for this branch"
            } else {
                "Select a branch to view command output"
            };
            let paragraph = Paragraph::new(Line::from(Span::styled(
                message,
                Style::default().fg(self.theme.muted),
            )))
            .block(block);
            paragraph.render(area, buf);
            return;
        }

        // Collect all lines from all logs for this branch
        let mut all_lines: Vec<Line> = Vec::new();

        for (i, log) in branch_logs.iter().enumerate() {
            if i > 0 {
                all_lines.push(Line::raw(""));
                all_lines.push(Line::from(Span::styled(
                    "─".repeat(inner_area.width.saturating_sub(2) as usize),
                    Style::default().fg(self.theme.muted),
                )));
                all_lines.push(Line::raw(""));
            }
            all_lines.extend(self.render_log_detail(log));
        }

        // Update max scroll
        let content_height = all_lines.len() as u16;
        self.state.max_scroll = content_height.saturating_sub(inner_area.height);

        // Apply scroll
        let visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(self.state.scroll as usize)
            .take(inner_area.height as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines).block(block);
        paragraph.render(area, buf);

        // Render scrollbar if needed
        if self.state.max_scroll > 0 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(self.state.max_scroll as usize)
                .position(self.state.scroll as usize);

            StatefulWidget::render(
                scrollbar,
                area.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                buf,
                &mut scrollbar_state,
            );
        }
    }
}
