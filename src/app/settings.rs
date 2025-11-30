//! Settings screen functionality

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::state::{SettingsField, SettingsState, ViewMode};
use super::App;

impl App {
    /// Open settings screen
    pub(super) fn open_settings(&mut self) {
        let mut state = SettingsState::new();
        
        // Load current values
        state.remotes = self.repo.get_remotes().unwrap_or_default();
        if let Ok(branches) = self.repo.get_remote_branches(&self.config.remote_name) {
            state.branches = branches.iter().map(|b| b.name.clone()).collect();
        }
        
        self.settings_state = Some(state);
        self.view_mode = ViewMode::Settings;
    }

    /// Handle settings keys
    pub(super) fn handle_settings_keys(&mut self, key: crossterm::event::KeyEvent) {
        let Some(settings) = self.settings_state.as_mut() else { return };

        if settings.editing {
            // Text input mode
            match key.code {
                KeyCode::Esc => {
                    settings.editing = false;
                    settings.edit_value.clear();
                }
                KeyCode::Enter => {
                    // Apply the edit
                    self.apply_settings_edit();
                }
                KeyCode::Char(c) => {
                    settings.edit_value.push(c);
                }
                KeyCode::Backspace => {
                    settings.edit_value.pop();
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => {
                self.settings_state = None;
                self.view_mode = ViewMode::Main;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let idx = settings.selected_field.index();
                if idx < SettingsField::all().len() - 1 {
                    settings.selected_field = SettingsField::from_index(idx + 1);
                    settings.list_index = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let idx = settings.selected_field.index();
                if idx > 0 {
                    settings.selected_field = SettingsField::from_index(idx - 1);
                    settings.list_index = 0;
                }
            }
            KeyCode::Enter => {
                // Enter edit mode or toggle for certain fields
                match settings.selected_field {
                    SettingsField::AutoCreate => {
                        self.config.auto_create_worktrees = !self.config.auto_create_worktrees;
                        self.status.auto_create_enabled = self.config.auto_create_worktrees;
                        let _ = self.config.save(self.repo.root());
                    }
                    SettingsField::Remote => {
                        // Cycle through remotes
                        let remotes = &settings.remotes;
                        if !remotes.is_empty() {
                            let current_idx = remotes.iter().position(|r| r == &self.config.remote_name).unwrap_or(0);
                            let next_idx = (current_idx + 1) % remotes.len();
                            self.config.remote_name = remotes[next_idx].clone();
                            self.status.remote_name = self.config.remote_name.clone();
                            // Reload branches for new remote
                            if let Ok(branches) = self.repo.get_remote_branches(&self.config.remote_name) {
                                settings.branches = branches.iter().map(|b| b.name.clone()).collect();
                            }
                            let _ = self.watcher.init(&self.repo, &self.config);
                            let _ = self.config.save(self.repo.root());
                        }
                    }
                    SettingsField::BaseBranch => {
                        // Cycle through branches
                        let branches = &settings.branches;
                        if self.config.base_branch.is_none() {
                            if !branches.is_empty() {
                                self.config.base_branch = Some(branches[0].clone());
                            }
                        } else {
                            let current = self.config.base_branch.as_ref().unwrap();
                            let current_idx = branches.iter().position(|b| b == current);
                            if let Some(idx) = current_idx {
                                if idx + 1 < branches.len() {
                                    self.config.base_branch = Some(branches[idx + 1].clone());
                                } else {
                                    self.config.base_branch = None;
                                }
                            } else {
                                self.config.base_branch = None;
                            }
                        }
                        let _ = self.config.save(self.repo.root());
                    }
                    SettingsField::PollInterval => {
                        settings.editing = true;
                        settings.edit_value = self.config.poll_interval_secs.to_string();
                    }
                    SettingsField::WorktreeBaseDir => {
                        settings.editing = true;
                        settings.edit_value = self.config.worktree_base_dir.clone();
                    }
                    SettingsField::PostCreateCommand => {
                        settings.editing = true;
                        settings.edit_value = self.config.post_create_command.clone().unwrap_or_default();
                    }
                }
            }
            KeyCode::Left => {
                // Decrease numeric values
                if settings.selected_field == SettingsField::PollInterval {
                    if self.config.poll_interval_secs > 5 {
                        self.config.poll_interval_secs -= 5;
                        self.status.poll_interval = self.config.poll_interval_secs;
                        let _ = self.config.save(self.repo.root());
                    }
                }
            }
            KeyCode::Right => {
                // Increase numeric values
                if settings.selected_field == SettingsField::PollInterval {
                    if self.config.poll_interval_secs < 300 {
                        self.config.poll_interval_secs += 5;
                        self.status.poll_interval = self.config.poll_interval_secs;
                        let _ = self.config.save(self.repo.root());
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn apply_settings_edit(&mut self) {
        let Some(settings) = self.settings_state.as_mut() else { return };
        
        match settings.selected_field {
            SettingsField::PollInterval => {
                if let Ok(val) = settings.edit_value.parse::<u64>() {
                    self.config.poll_interval_secs = val.max(1).min(3600);
                    self.status.poll_interval = self.config.poll_interval_secs;
                }
            }
            SettingsField::WorktreeBaseDir => {
                self.config.worktree_base_dir = settings.edit_value.clone();
            }
            SettingsField::PostCreateCommand => {
                if settings.edit_value.is_empty() {
                    self.config.post_create_command = None;
                } else {
                    self.config.post_create_command = Some(settings.edit_value.clone());
                }
            }
            _ => {}
        }
        
        settings.editing = false;
        settings.edit_value.clear();
        let _ = self.config.save(self.repo.root());
    }

    /// Render settings screen
    pub(super) fn render_settings(&self, frame: &mut Frame, area: Rect) {
        let Some(settings) = &self.settings_state else { return };

        // Fill background
        frame.render_widget(
            Block::default().style(Style::default().bg(self.theme.bg)),
            area,
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Content
                Constraint::Length(2),  // Navigation
            ])
            .split(area);

        // Title
        let title = Paragraph::new(Line::from(vec![
            Span::styled("⚙ Settings", Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
        ]))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(self.theme.muted)));
        frame.render_widget(title, chunks[0]);

        // Settings list
        let mut lines = Vec::new();
        
        for field in SettingsField::all() {
            let is_selected = *field == settings.selected_field;
            let arrow = if is_selected { "▶ " } else { "  " };
            
            let (label, value) = match field {
                SettingsField::Remote => (
                    "Remote",
                    self.config.remote_name.clone(),
                ),
                SettingsField::PollInterval => (
                    "Poll Interval",
                    if settings.editing && is_selected {
                        format!("{}_ (editing)", settings.edit_value)
                    } else {
                        format!("{}s  ← → to adjust", self.config.poll_interval_secs)
                    },
                ),
                SettingsField::WorktreeBaseDir => (
                    "Worktree Directory",
                    if settings.editing && is_selected {
                        format!("{}_ (editing)", settings.edit_value)
                    } else {
                        self.config.worktree_base_dir.clone()
                    },
                ),
                SettingsField::BaseBranch => (
                    "Base Branch",
                    self.config.base_branch.clone().unwrap_or_else(|| "(auto)".to_string()),
                ),
                SettingsField::PostCreateCommand => (
                    "Post-Create Command",
                    if settings.editing && is_selected {
                        format!("{}_ (editing)", settings.edit_value)
                    } else {
                        self.config.post_create_command.clone().unwrap_or_else(|| "(none)".to_string())
                    },
                ),
                SettingsField::AutoCreate => (
                    "Auto-Create Worktrees",
                    if self.config.auto_create_worktrees { "Yes".to_string() } else { "No".to_string() },
                ),
            };

            let line_style = if is_selected {
                Style::default().fg(self.theme.bg).bg(self.theme.primary)
            } else {
                Style::default().fg(self.theme.fg)
            };

            lines.push(Line::from(vec![
                Span::styled(arrow, Style::default().fg(self.theme.primary)),
                Span::styled(format!("{:<22}", label), line_style.add_modifier(Modifier::BOLD)),
                Span::styled(value, if is_selected { line_style } else { Style::default().fg(self.theme.secondary) }),
            ]));
            lines.push(Line::raw("")); // Spacing
        }

        let content = Paragraph::new(lines);
        frame.render_widget(content, chunks[1]);

        // Navigation hint
        let nav = Line::from(vec![
            Span::styled(" j/k ", Style::default().fg(self.theme.primary)),
            Span::styled("navigate ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter ", Style::default().fg(self.theme.primary)),
            Span::styled("edit/toggle ", Style::default().fg(self.theme.muted)),
            Span::styled("←/→ ", Style::default().fg(self.theme.primary)),
            Span::styled("adjust ", Style::default().fg(self.theme.muted)),
            Span::styled("Esc/s ", Style::default().fg(self.theme.primary)),
            Span::styled("back", Style::default().fg(self.theme.muted)),
        ]);
        frame.render_widget(Paragraph::new(nav), chunks[2]);
    }
}

