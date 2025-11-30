//! Help overlay widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use super::Theme;

/// Help overlay widget
pub struct HelpWidget<'a> {
    theme: &'a Theme,
}

impl<'a> HelpWidget<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

    fn render_keybinding(&self, key: &'static str, desc: &'static str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{:>12} ", key),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc, Style::default().fg(self.theme.fg)),
        ])
    }
}

impl Widget for HelpWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the help popup
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 24.min(area.height.saturating_sub(4));

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: area.x + popup_x,
            y: area.y + popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.secondary))
            .title(Span::styled(
                " Keyboard Shortcuts ",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let lines = vec![
            Line::from(Span::styled(" Navigation", Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD))),
            self.render_keybinding("↑ / ↓", "Navigate branches"),
            self.render_keybinding("j / k", "Scroll command log"),
            self.render_keybinding("Mouse wheel", "Scroll command log"),
            Line::raw(""),
            Line::from(Span::styled(" Actions", Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD))),
            self.render_keybinding("Enter", "Create worktree for branch"),
            self.render_keybinding("d", "Delete/untrack worktree"),
            self.render_keybinding("t", "Toggle track/untrack branch"),
            self.render_keybinding("r", "Refresh (fetch from remote)"),
            self.render_keybinding("a", "Toggle auto-create mode"),
            Line::raw(""),
            Line::from(Span::styled(" Views", Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD))),
            self.render_keybinding("l", "Full-screen logs"),
            self.render_keybinding("s", "Settings"),
            self.render_keybinding("?", "Toggle this help"),
            self.render_keybinding("q / Esc", "Quit"),
            Line::raw(""),
            Line::from(Span::styled(" Mouse", Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD))),
            self.render_keybinding("Shift+drag", "Select text"),
        ];

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

