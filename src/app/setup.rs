//! Setup wizard functionality

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::state::{SetupState, SetupStep};
use super::App;

impl App {
    /// Render setup wizard
    pub(super) fn render_setup(&self, frame: &mut Frame, area: Rect) {
        let Some(setup) = &self.setup_state else { return };

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
                Constraint::Length(3),  // Navigation
            ])
            .split(area);

        // Title
        let step_name = match setup.step {
            SetupStep::Remote => "Select Remote",
            SetupStep::PollInterval => "Poll Interval",
            SetupStep::WorktreeBaseDir => "Worktree Directory",
            SetupStep::BaseBranch => "Base Branch",
            SetupStep::PostCreateCommand => "Post-Create Command",
            SetupStep::AutoCreate => "Auto-Create Worktrees",
            SetupStep::Confirm => "Confirm Settings",
        };

        let title = Paragraph::new(Line::from(vec![
            Span::styled("Setup Wizard - ", Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(step_name, Style::default().fg(self.theme.secondary).add_modifier(Modifier::BOLD)),
        ]))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(self.theme.muted)));
        frame.render_widget(title, chunks[0]);

        // Content based on step
        match setup.step {
            SetupStep::Remote => self.render_setup_remote(frame, chunks[1], setup),
            SetupStep::PollInterval => self.render_setup_poll_interval(frame, chunks[1], setup),
            SetupStep::WorktreeBaseDir => self.render_setup_worktree_dir(frame, chunks[1], setup),
            SetupStep::BaseBranch => self.render_setup_base_branch(frame, chunks[1], setup),
            SetupStep::PostCreateCommand => self.render_setup_post_command(frame, chunks[1], setup),
            SetupStep::AutoCreate => self.render_setup_auto_create(frame, chunks[1], setup),
            SetupStep::Confirm => self.render_setup_confirm(frame, chunks[1], setup),
        }

        // Navigation
        let nav = Line::from(vec![
            Span::styled(" ↑/↓ ", Style::default().fg(self.theme.primary)),
            Span::styled("navigate ", Style::default().fg(self.theme.muted)),
            Span::styled("Enter ", Style::default().fg(self.theme.primary)),
            Span::styled("confirm ", Style::default().fg(self.theme.muted)),
            Span::styled("Tab ", Style::default().fg(self.theme.primary)),
            Span::styled("next ", Style::default().fg(self.theme.muted)),
            Span::styled("Shift+Tab ", Style::default().fg(self.theme.primary)),
            Span::styled("back ", Style::default().fg(self.theme.muted)),
            Span::styled("Esc ", Style::default().fg(self.theme.primary)),
            Span::styled("skip setup", Style::default().fg(self.theme.muted)),
        ]);
        frame.render_widget(
            Paragraph::new(nav).block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(self.theme.muted))),
            chunks[2],
        );
    }

    fn render_setup_remote(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let mut lines = vec![
            Line::from(Span::styled("Select the remote repository to watch:", Style::default().fg(self.theme.fg))),
            Line::raw(""),
        ];

        if setup.remotes.is_empty() {
            lines.push(Line::from(Span::styled(
                "No remotes found! Add a remote first: git remote add origin <url>",
                Style::default().fg(self.theme.error),
            )));
        } else {
            for (i, remote) in setup.remotes.iter().enumerate() {
                let style = if i == setup.selected_index {
                    Style::default().fg(self.theme.bg).bg(self.theme.primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };
                let prefix = if i == setup.selected_index { "▶ " } else { "  " };
                lines.push(Line::from(Span::styled(format!("{}{}", prefix, remote), style)));
            }
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_poll_interval(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let lines = vec![
            Line::from(Span::styled("How often should we check for new branches? (seconds)", Style::default().fg(self.theme.fg))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Current: ", Style::default().fg(self.theme.muted)),
                Span::styled(format!("{}", setup.poll_interval), Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled(" seconds", Style::default().fg(self.theme.muted)),
            ]),
            Line::raw(""),
            Line::from(Span::styled("Use ↑/↓ to adjust (5-300 seconds)", Style::default().fg(self.theme.muted))),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_worktree_dir(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let lines = vec![
            Line::from(Span::styled("Where should worktrees be created?", Style::default().fg(self.theme.fg))),
            Line::from(Span::styled("(relative to repository root)", Style::default().fg(self.theme.muted))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Directory: ", Style::default().fg(self.theme.muted)),
                Span::styled(&setup.worktree_base_dir, Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("_", Style::default().fg(self.theme.primary).add_modifier(Modifier::SLOW_BLINK)),
            ]),
            Line::raw(""),
            Line::from(Span::styled("Common options: '..' (parent dir), './worktrees'", Style::default().fg(self.theme.muted))),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_base_branch(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let mut lines = vec![
            Line::from(Span::styled("Select the base/main branch (optional):", Style::default().fg(self.theme.fg))),
            Line::raw(""),
        ];

        // Option for "Auto" (auto-detect default branch)
        let auto_style = if setup.selected_index == 0 {
            Style::default().fg(self.theme.bg).bg(self.theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.muted)
        };
        lines.push(Line::from(Span::styled(
            if setup.selected_index == 0 { "▶ (auto)" } else { "  (auto)" },
            auto_style,
        )));

        for (i, branch) in setup.branches.iter().enumerate() {
            let style = if i + 1 == setup.selected_index {
                Style::default().fg(self.theme.bg).bg(self.theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.fg)
            };
            let prefix = if i + 1 == setup.selected_index { "▶ " } else { "  " };
            lines.push(Line::from(Span::styled(format!("{}{}", prefix, branch), style)));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_post_command(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let cmd_display = setup.post_create_command.as_deref().unwrap_or("");
        let lines = vec![
            Line::from(Span::styled("Command to run after creating a worktree:", Style::default().fg(self.theme.fg))),
            Line::from(Span::styled("(e.g., 'npm install', 'yarn', 'make setup')", Style::default().fg(self.theme.muted))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("$ ", Style::default().fg(self.theme.muted)),
                Span::styled(cmd_display, Style::default().fg(self.theme.primary).add_modifier(Modifier::BOLD)),
                Span::styled("_", Style::default().fg(self.theme.primary).add_modifier(Modifier::SLOW_BLINK)),
            ]),
            Line::raw(""),
            Line::from(Span::styled("Leave empty to skip post-create commands", Style::default().fg(self.theme.muted))),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_auto_create(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let lines = vec![
            Line::from(Span::styled("Automatically create worktrees for new branches?", Style::default().fg(self.theme.fg))),
            Line::raw(""),
            Line::from(vec![
                Span::styled(if setup.selected_index == 0 { "▶ " } else { "  " }, Style::default().fg(self.theme.primary)),
                Span::styled(
                    "Yes - automatically create worktrees (can be changed later with 'a')",
                    if setup.selected_index == 0 {
                        Style::default().fg(self.theme.bg).bg(self.theme.primary).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.fg)
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled(if setup.selected_index == 1 { "▶ " } else { "  " }, Style::default().fg(self.theme.primary)),
                Span::styled(
                    "No - manually select branches (recommended)",
                    if setup.selected_index == 1 {
                        Style::default().fg(self.theme.bg).bg(self.theme.primary).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.fg)
                    },
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_setup_confirm(&self, frame: &mut Frame, area: Rect, setup: &SetupState) {
        let lines = vec![
            Line::from(Span::styled("Review your settings:", Style::default().fg(self.theme.fg).add_modifier(Modifier::BOLD))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Remote:           ", Style::default().fg(self.theme.muted)),
                Span::styled(&setup.remote_name, Style::default().fg(self.theme.secondary)),
            ]),
            Line::from(vec![
                Span::styled("  Poll interval:    ", Style::default().fg(self.theme.muted)),
                Span::styled(format!("{}s", setup.poll_interval), Style::default().fg(self.theme.secondary)),
            ]),
            Line::from(vec![
                Span::styled("  Worktree dir:     ", Style::default().fg(self.theme.muted)),
                Span::styled(&setup.worktree_base_dir, Style::default().fg(self.theme.secondary)),
            ]),
            Line::from(vec![
                Span::styled("  Base branch:      ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    setup.base_branch.as_deref().unwrap_or("(auto)"),
                    Style::default().fg(self.theme.secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Post-create cmd:  ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    setup.post_create_command.as_deref().unwrap_or("(none)"),
                    Style::default().fg(self.theme.secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Auto-create:      ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    if setup.auto_create { "Yes" } else { "No" },
                    Style::default().fg(self.theme.secondary),
                ),
            ]),
            Line::raw(""),
            Line::from(Span::styled("Press Enter to save and continue, or go back to modify.", Style::default().fg(self.theme.muted))),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// Handle setup wizard keys
    pub(super) fn handle_setup_keys(&mut self, key: crossterm::event::KeyEvent) {
        let Some(setup) = self.setup_state.as_mut() else { return };

        match key.code {
            KeyCode::Esc => {
                // Skip setup, use defaults
                self.finish_setup_with_defaults();
            }
            KeyCode::Tab => {
                // Next step
                self.setup_next_step();
            }
            KeyCode::BackTab => {
                // Previous step
                self.setup_prev_step();
            }
            KeyCode::Enter => {
                if matches!(setup.step, SetupStep::Confirm) {
                    self.finish_setup();
                } else {
                    self.setup_next_step();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match setup.step {
                    SetupStep::Remote => {
                        if setup.selected_index > 0 {
                            setup.selected_index -= 1;
                        }
                    }
                    SetupStep::PollInterval => {
                        if setup.poll_interval < 300 {
                            setup.poll_interval += 5;
                        }
                    }
                    SetupStep::BaseBranch => {
                        if setup.selected_index > 0 {
                            setup.selected_index -= 1;
                        }
                    }
                    SetupStep::AutoCreate => {
                        setup.selected_index = if setup.selected_index == 0 { 1 } else { 0 };
                    }
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match setup.step {
                    SetupStep::Remote => {
                        if setup.selected_index < setup.remotes.len().saturating_sub(1) {
                            setup.selected_index += 1;
                        }
                    }
                    SetupStep::PollInterval => {
                        if setup.poll_interval > 5 {
                            setup.poll_interval -= 5;
                        }
                    }
                    SetupStep::BaseBranch => {
                        let max = setup.branches.len(); // +1 for "auto" option but we start at 0
                        if setup.selected_index < max {
                            setup.selected_index += 1;
                        }
                    }
                    SetupStep::AutoCreate => {
                        setup.selected_index = if setup.selected_index == 0 { 1 } else { 0 };
                    }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                match setup.step {
                    SetupStep::WorktreeBaseDir => {
                        setup.worktree_base_dir.push(c);
                    }
                    SetupStep::PostCreateCommand => {
                        let cmd = setup.post_create_command.get_or_insert_with(String::new);
                        cmd.push(c);
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match setup.step {
                    SetupStep::WorktreeBaseDir => {
                        setup.worktree_base_dir.pop();
                    }
                    SetupStep::PostCreateCommand => {
                        if let Some(ref mut cmd) = setup.post_create_command {
                            cmd.pop();
                            if cmd.is_empty() {
                                setup.post_create_command = None;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(super) fn setup_next_step(&mut self) {
        let Some(setup) = self.setup_state.as_mut() else { return };

        match setup.step {
            SetupStep::Remote => {
                // Save remote selection
                if !setup.remotes.is_empty() {
                    setup.remote_name = setup.remotes[setup.selected_index].clone();
                }
                // Load branches for the selected remote
                if let Ok(branches) = self.repo.get_remote_branches(&setup.remote_name) {
                    setup.branches = branches.iter().map(|b| b.name.clone()).collect();
                }
                setup.selected_index = 0;
                setup.step = SetupStep::PollInterval;
            }
            SetupStep::PollInterval => {
                setup.step = SetupStep::WorktreeBaseDir;
            }
            SetupStep::WorktreeBaseDir => {
                // Pre-select the default branch if available
                if let Some(default_branch) = self.repo.get_default_branch(&setup.remote_name) {
                    // Find index: 0 is "(auto)", 1+ are branches
                    if let Some(idx) = setup.branches.iter().position(|b| b == &default_branch) {
                        setup.selected_index = idx + 1; // +1 because 0 is "(auto)"
                    } else {
                        setup.selected_index = 0;
                    }
                } else {
                    setup.selected_index = 0;
                }
                setup.step = SetupStep::BaseBranch;
            }
            SetupStep::BaseBranch => {
                // Save base branch selection
                if setup.selected_index == 0 {
                    setup.base_branch = None;
                } else {
                    setup.base_branch = setup.branches.get(setup.selected_index - 1).cloned();
                }
                setup.selected_index = 1; // Default to "No" for auto-create
                setup.step = SetupStep::PostCreateCommand;
            }
            SetupStep::PostCreateCommand => {
                setup.step = SetupStep::AutoCreate;
            }
            SetupStep::AutoCreate => {
                setup.auto_create = setup.selected_index == 0;
                setup.step = SetupStep::Confirm;
            }
            SetupStep::Confirm => {
                self.finish_setup();
            }
        }
    }

    pub(super) fn setup_prev_step(&mut self) {
        let Some(setup) = self.setup_state.as_mut() else { return };

        match setup.step {
            SetupStep::Remote => {} // Can't go back
            SetupStep::PollInterval => {
                setup.selected_index = setup.remotes.iter().position(|r| r == &setup.remote_name).unwrap_or(0);
                setup.step = SetupStep::Remote;
            }
            SetupStep::WorktreeBaseDir => {
                setup.step = SetupStep::PollInterval;
            }
            SetupStep::BaseBranch => {
                setup.step = SetupStep::WorktreeBaseDir;
            }
            SetupStep::PostCreateCommand => {
                setup.selected_index = if setup.base_branch.is_none() { 0 } else {
                    setup.branches.iter().position(|b| Some(b) == setup.base_branch.as_ref()).map(|i| i + 1).unwrap_or(0)
                };
                setup.step = SetupStep::BaseBranch;
            }
            SetupStep::AutoCreate => {
                setup.step = SetupStep::PostCreateCommand;
            }
            SetupStep::Confirm => {
                setup.selected_index = if setup.auto_create { 0 } else { 1 };
                setup.step = SetupStep::AutoCreate;
            }
        }
    }

    pub(super) fn finish_setup(&mut self) {
        let Some(setup) = self.setup_state.take() else { return };

        // Apply settings to config
        self.config.remote_name = setup.remote_name;
        self.config.poll_interval_secs = setup.poll_interval;
        self.config.worktree_base_dir = setup.worktree_base_dir;
        self.config.base_branch = setup.base_branch;
        self.config.post_create_command = setup.post_create_command;
        self.config.auto_create_worktrees = setup.auto_create;

        // Save config
        let _ = self.config.save(self.repo.root());

        // Re-initialize watcher with new config
        let _ = self.watcher.init(&self.repo, &self.config);

        // Update status
        self.status.remote_name = self.config.remote_name.clone();
        self.status.poll_interval = self.config.poll_interval_secs;
        self.status.auto_create_enabled = self.config.auto_create_worktrees;

        // Switch to main view
        self.view_mode = super::state::ViewMode::Main;
        self.update_branch_list();
    }

    pub(super) fn finish_setup_with_defaults(&mut self) {
        self.setup_state = None;

        // Just save the default config
        let _ = self.config.save(self.repo.root());

        // Try to initialize watcher
        if self.repo.remote_exists(&self.config.remote_name) {
            let _ = self.watcher.init(&self.repo, &self.config);
        }

        self.view_mode = super::state::ViewMode::Main;
        self.update_branch_list();
    }
}

