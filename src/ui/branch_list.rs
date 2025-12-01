//! Branch list widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget},
};

use super::Theme;

/// Status of a branch
#[derive(Debug, Clone, PartialEq)]
pub enum BranchStatus {
    /// Remote branch with no local worktree
    Remote,
    /// Has local worktree, active
    LocalActive,
    /// Has local worktree but prunable
    LocalPrunable,
    /// Queued for worktree creation
    Queued,
    /// Creating worktree
    Creating,
    /// Running hook
    RunningHook,
}

/// A branch item for display
#[derive(Debug, Clone)]
pub struct BranchItem {
    pub name: String,
    pub status: BranchStatus,
    pub is_default: bool,
}

/// Branch list widget state
pub struct BranchListState {
    pub list_state: ListState,
    pub items: Vec<BranchItem>,
}

impl BranchListState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            items: Vec::new(),
        }
    }

    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn select_previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn selected(&self) -> Option<&BranchItem> {
        self.list_state.selected().and_then(|i| self.items.get(i))
    }

    pub fn selected_branch(&self) -> Option<String> {
        self.selected().map(|item| item.name.clone())
    }

    pub fn select_by_name(&mut self, name: &str) {
        if let Some(idx) = self.items.iter().position(|item| item.name == name) {
            self.list_state.select(Some(idx));
        }
    }

    pub fn update_items(&mut self, items: Vec<BranchItem>) {
        // Remember the selected branch name before updating
        let selected_branch_name = self.selected_branch();
        self.items = items;

        // Try to re-select the same branch by name
        if let Some(name) = selected_branch_name {
            if let Some(idx) = self.items.iter().position(|item| item.name == name) {
                self.list_state.select(Some(idx));
                return;
            }
        }

        // Fallback: select first item if nothing selected
        if self.list_state.selected().is_none() && !self.items.is_empty() {
            self.list_state.select(Some(0));
        }
    }
}

impl Default for BranchListState {
    fn default() -> Self {
        Self::new()
    }
}

/// The branch list widget
pub struct BranchListWidget<'a> {
    title: &'a str,
    theme: &'a Theme,
}

impl<'a> BranchListWidget<'a> {
    pub fn new(title: &'a str, theme: &'a Theme) -> Self {
        Self { title, theme }
    }

    fn status_indicator(&self, status: &BranchStatus) -> (&str, Style) {
        match status {
            BranchStatus::Remote => ("○", Style::default().fg(self.theme.muted)),
            BranchStatus::LocalActive => ("●", Style::default().fg(self.theme.success)),
            BranchStatus::LocalPrunable => ("◐", Style::default().fg(self.theme.warning)),
            BranchStatus::Queued => ("◷", Style::default().fg(self.theme.warning)),
            BranchStatus::Creating => ("◔", Style::default().fg(self.theme.primary)),
            BranchStatus::RunningHook => ("⟳", Style::default().fg(self.theme.secondary)),
        }
    }
}

impl StatefulWidget for BranchListWidget<'_> {
    type State = BranchListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let items: Vec<ListItem> = state
            .items
            .iter()
            .map(|item| {
                let (indicator, indicator_style) = self.status_indicator(&item.status);

                let name_style = if item.is_default {
                    Style::default()
                        .fg(self.theme.highlight)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };

                let suffix = if item.is_default {
                    Span::styled(" (default)", Style::default().fg(self.theme.muted))
                } else {
                    Span::raw("")
                };

                let status_label = match item.status {
                    BranchStatus::Queued => {
                        Span::styled(" queued", Style::default().fg(self.theme.warning))
                    }
                    BranchStatus::Creating => {
                        Span::styled(" ⟳ creating...", Style::default().fg(self.theme.primary))
                    }
                    BranchStatus::RunningHook => Span::styled(
                        " ⟳ running hook...",
                        Style::default().fg(self.theme.secondary),
                    ),
                    BranchStatus::LocalPrunable => {
                        Span::styled(" (prunable)", Style::default().fg(self.theme.warning))
                    }
                    _ => Span::raw(""),
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", indicator), indicator_style),
                    Span::styled(&item.name, name_style),
                    suffix,
                    status_label,
                ]))
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.primary))
            .title(Span::styled(
                self.title,
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(self.theme.primary)
                    .fg(self.theme.bg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        StatefulWidget::render(list, area, buf, &mut state.list_state);
    }
}
