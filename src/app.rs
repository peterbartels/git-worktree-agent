//! Main application state and logic

use std::sync::mpsc;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    DefaultTerminal, Frame,
};
use tracing::{debug, error, info};

use crate::config::{Config, HookStatus};
use crate::git::{Repository, WorktreeManager};
use crate::ui::{
    BranchItem, BranchListState, BranchListWidget, BranchLogWidget, BranchStatus, HelpWidget,
    ScrollableLogsWidget, LogsState, StatusWidget, AppStatus, Theme,
};
use crate::watcher::{Watcher, WatcherEvent};

/// Current view mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    /// Main branch list view
    Main,
    /// Full-screen logs view
    Logs,
    /// Help overlay
    Help,
    /// Fatal error - must exit
    Error(String),
    /// Initial setup wizard
    Setup,
    /// Settings screen
    Settings,
    /// Delete confirmation dialog
    DeleteConfirm {
        /// Branch name to delete
        branch: String,
        /// User input (must be "yes" to proceed)
        input: String,
    },
}

/// Setup wizard step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    Remote,
    PollInterval,
    WorktreeBaseDir,
    BaseBranch,
    PostCreateCommand,
    AutoCreate,
    Confirm,
}

/// Setup wizard state
#[derive(Debug, Clone)]
pub struct SetupState {
    pub step: SetupStep,
    pub remotes: Vec<String>,
    pub branches: Vec<String>,
    pub selected_index: usize,
    pub remote_name: String,
    pub poll_interval: u64,
    pub worktree_base_dir: String,
    pub base_branch: Option<String>,
    pub post_create_command: Option<String>,
    pub auto_create: bool,
}

impl SetupState {
    pub fn new() -> Self {
        Self {
            step: SetupStep::Remote,
            remotes: Vec::new(),
            branches: Vec::new(),
            selected_index: 0,
            remote_name: "origin".to_string(),
            poll_interval: 10,
            worktree_base_dir: "..".to_string(),
            base_branch: None,
            post_create_command: None,
            auto_create: false,
        }
    }
}

/// Settings field being edited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Remote,
    PollInterval,
    WorktreeBaseDir,
    BaseBranch,
    PostCreateCommand,
    AutoCreate,
}

impl SettingsField {
    fn all() -> &'static [SettingsField] {
        &[
            SettingsField::Remote,
            SettingsField::PollInterval,
            SettingsField::WorktreeBaseDir,
            SettingsField::BaseBranch,
            SettingsField::PostCreateCommand,
            SettingsField::AutoCreate,
        ]
    }

    fn index(&self) -> usize {
        match self {
            SettingsField::Remote => 0,
            SettingsField::PollInterval => 1,
            SettingsField::WorktreeBaseDir => 2,
            SettingsField::BaseBranch => 3,
            SettingsField::PostCreateCommand => 4,
            SettingsField::AutoCreate => 5,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            0 => SettingsField::Remote,
            1 => SettingsField::PollInterval,
            2 => SettingsField::WorktreeBaseDir,
            3 => SettingsField::BaseBranch,
            4 => SettingsField::PostCreateCommand,
            5 => SettingsField::AutoCreate,
            _ => SettingsField::Remote,
        }
    }
}

/// Settings screen state
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub selected_field: SettingsField,
    pub editing: bool,
    pub edit_value: String,
    pub remotes: Vec<String>,
    pub branches: Vec<String>,
    pub list_index: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            selected_field: SettingsField::Remote,
            editing: false,
            edit_value: String::new(),
            remotes: Vec::new(),
            branches: Vec::new(),
            list_index: 0,
        }
    }
}

/// Main application state
pub struct App {
    /// Is the application running?
    running: bool,
    /// Current view mode
    view_mode: ViewMode,
    /// Git repository
    repo: Repository,
    /// Configuration
    config: Config,
    /// Branch watcher
    watcher: Watcher,
    /// Event receiver from watcher
    event_rx: mpsc::Receiver<WatcherEvent>,
    /// Event sender for watcher
    event_tx: mpsc::Sender<WatcherEvent>,
    /// Branch list state
    branch_list_state: BranchListState,
    /// Logs state (for scrolling bottom logs)
    logs_state: LogsState,
    /// Branch log state (for scrolling right panel)
    branch_logs_state: LogsState,
    /// Application status
    status: AppStatus,
    /// Last poll time
    last_poll: Instant,
    /// Theme
    theme: Theme,
    /// Setup wizard state (only used during first run)
    setup_state: Option<SetupState>,
    /// Settings screen state
    settings_state: Option<SettingsState>,
}

impl App {
    /// Create a new application
    pub fn new(repo_path: &std::path::Path) -> Result<Self> {
        let repo = Repository::discover(repo_path)?;
        
        // Check if config file exists (first run detection)
        let config_path = repo.root().join(crate::config::CONFIG_FILE_NAME);
        let is_first_run = !config_path.exists();
        
        let config = Config::load(repo.root())?;

        let (event_tx, event_rx) = mpsc::channel();

        let mut watcher = Watcher::new();
        // Only init watcher if we have a valid remote
        if repo.remote_exists(&config.remote_name) {
            let _ = watcher.init(&repo, &config);
        }

        let status = AppStatus {
            is_fetching: false,
            last_fetch: config.last_fetch,
            remote_branch_count: watcher.get_known_branches().len(),
            worktree_count: config.worktrees.len(),
            running_hooks: 0,
            last_error: None,
            auto_create_enabled: config.auto_create_worktrees,
            poll_interval: config.poll_interval_secs,
            remote_name: config.remote_name.clone(),
        };

        // Determine initial view mode and setup state
        let (initial_view_mode, setup_state) = if is_first_run {
            // First run - start setup wizard
            let mut setup = SetupState::new();
            
            // Get available remotes
            setup.remotes = repo.get_remotes().unwrap_or_default();
            if !setup.remotes.is_empty() {
                // Default to first remote or "origin" if available
                if let Some(idx) = setup.remotes.iter().position(|r| r == "origin") {
                    setup.selected_index = idx;
                    setup.remote_name = "origin".to_string();
                } else {
                    setup.remote_name = setup.remotes[0].clone();
                }
            }
            
            (ViewMode::Setup, Some(setup))
        } else if let Err(err_msg) = repo.validate_remote(&config.remote_name) {
            // Remote doesn't exist - show error
            (ViewMode::Error(err_msg), None)
        } else {
            (ViewMode::Main, None)
        };

        let mut app = Self {
            running: false,
            view_mode: initial_view_mode,
            repo,
            config,
            watcher,
            event_rx,
            event_tx,
            branch_list_state: BranchListState::new(),
            logs_state: LogsState::default(),
            branch_logs_state: LogsState::default(),
            status,
            last_poll: Instant::now() - Duration::from_secs(999), // Force initial poll
            theme: Theme::default(),
            setup_state,
            settings_state: None,
        };

        // Only sync if not in setup mode
        if !is_first_run {
            app.sync_worktrees_with_git();
            app.update_branch_list();
        }
        Ok(app)
    }

    /// Sync config worktrees with actual git worktree state
    /// Removes worktrees from config that no longer exist in git
    fn sync_worktrees_with_git(&mut self) {
        let manager = WorktreeManager::new(&self.repo);
        
        if let Ok(git_worktrees) = manager.list() {
            // Get the set of branches that have actual worktrees
            let existing_branches: std::collections::HashSet<String> = git_worktrees
                .iter()
                .filter_map(|wt| wt.branch.clone())
                .collect();
            
            // Remove worktrees from config that don't exist in git anymore
            let removed: Vec<String> = self.config.worktrees
                .iter()
                .filter(|wt| !existing_branches.contains(&wt.branch))
                .map(|wt| wt.branch.clone())
                .collect();
            
            for branch in &removed {
                self.config.remove_worktree(branch);
            }
            
            // Save if we removed anything
            if !removed.is_empty() {
                let _ = self.config.save(self.repo.root());
            }
        }
    }

    /// Run the application's main loop
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;

        // Check if we're starting in error mode
        let started_with_error = matches!(self.view_mode, ViewMode::Error(_));

        while self.running {
            // Only process events and poll if not in error or setup mode
            if !matches!(self.view_mode, ViewMode::Error(_) | ViewMode::Setup) {
                // Process any pending watcher events
                self.process_watcher_events();

                // Check if we need to poll
                let poll_interval = Duration::from_secs(self.config.poll_interval_secs);
                if self.last_poll.elapsed() >= poll_interval {
                    self.do_poll();
                }
            }

            // Render
            terminal.draw(|frame| self.render(frame))?;

            // Handle events with timeout
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.on_key_event(key);
                    }
                    Event::Mouse(mouse) => {
                        self.on_mouse_event(mouse);
                    }
                    _ => {}
                }
            }
        }

        // Only save config if we didn't start in error mode
        if !started_with_error {
            self.config.save(self.repo.root())?;
        }

        Ok(())
    }

    /// Render the application
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        match &self.view_mode {
            ViewMode::Main => self.render_main(frame, area),
            ViewMode::Logs => self.render_logs_fullscreen(frame, area),
            ViewMode::Help => {
                self.render_main(frame, area);
                frame.render_widget(HelpWidget::new(&self.theme), area);
            }
            ViewMode::Error(msg) => self.render_error(frame, area, msg.clone()),
            ViewMode::Setup => self.render_setup(frame, area),
            ViewMode::Settings => self.render_settings(frame, area),
            ViewMode::DeleteConfirm { branch, input } => {
                let branch = branch.clone();
                let input = input.clone();
                self.render_main(frame, area);
                self.render_delete_confirm(frame, area, branch, input);
            }
        }
    }

    /// Render delete confirmation dialog
    fn render_delete_confirm(&self, frame: &mut Frame, area: Rect, branch: String, input: String) {
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

    /// Render setup wizard
    fn render_setup(&self, frame: &mut Frame, area: Rect) {
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

    /// Render error view
    fn render_error(&self, frame: &mut Frame, area: Rect, error_msg: String) {
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
        frame.render_widget(ratatui::widgets::Clear, popup_area);

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

    /// Render the main view
    fn render_main(&mut self, frame: &mut Frame, area: Rect) {
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
    fn render_logs_fullscreen(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            ScrollableLogsWidget::new(&self.watcher.command_logs, &self.theme, &mut self.logs_state),
            area,
        );
    }

    /// Handle key events
    fn on_key_event(&mut self, key: KeyEvent) {
        match &self.view_mode {
            ViewMode::Main => self.handle_main_keys(key),
            ViewMode::Logs => self.handle_logs_keys(key),
            ViewMode::Help => self.handle_help_keys(key),
            ViewMode::Error(_) => self.handle_error_keys(key),
            ViewMode::Setup => self.handle_setup_keys(key),
            ViewMode::Settings => self.handle_settings_keys(key),
            ViewMode::DeleteConfirm { .. } => self.handle_delete_confirm_keys(key),
        }
    }

    /// Handle mouse events (for scrolling)
    fn on_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                match &self.view_mode {
                    ViewMode::Main => {
                        // Scroll the branch log panel (or bottom logs based on position)
                        self.branch_logs_state.scroll_up();
                    }
                    ViewMode::Logs => {
                        self.logs_state.scroll_up();
                    }
                    _ => {}
                }
            }
            MouseEventKind::ScrollDown => {
                match &self.view_mode {
                    ViewMode::Main => {
                        self.branch_logs_state.scroll_down();
                    }
                    ViewMode::Logs => {
                        self.logs_state.scroll_down();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Handle setup wizard keys
    fn handle_setup_keys(&mut self, key: KeyEvent) {
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

    fn setup_next_step(&mut self) {
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

    fn setup_prev_step(&mut self) {
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

    fn finish_setup(&mut self) {
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
        self.view_mode = ViewMode::Main;
        self.update_branch_list();
    }

    fn finish_setup_with_defaults(&mut self) {
        self.setup_state = None;

        // Just save the default config
        let _ = self.config.save(self.repo.root());

        // Try to initialize watcher
        if self.repo.remote_exists(&self.config.remote_name) {
            let _ = self.watcher.init(&self.repo, &self.config);
        }

        self.view_mode = ViewMode::Main;
        self.update_branch_list();
    }

    /// Handle main view keys
    fn handle_main_keys(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                self.running = false;
            }
            (_, KeyCode::Char('?')) => {
                self.view_mode = ViewMode::Help;
            }
            (_, KeyCode::Char('l')) => {
                self.view_mode = ViewMode::Logs;
                self.logs_state.scroll_to_bottom();
            }
            // Arrow keys navigate branch list
            (_, KeyCode::Down) => {
                self.branch_list_state.select_next();
                self.branch_logs_state.scroll = 0; // Reset scroll on selection change
            }
            (_, KeyCode::Up) => {
                self.branch_list_state.select_previous();
                self.branch_logs_state.scroll = 0; // Reset scroll on selection change
            }
            // j/k scroll branch command log (right panel)
            (_, KeyCode::Char('j')) => {
                self.branch_logs_state.scroll_down();
            }
            (_, KeyCode::Char('k')) => {
                self.branch_logs_state.scroll_up();
            }
            (_, KeyCode::PageDown) => {
                for _ in 0..5 { self.branch_logs_state.scroll_down(); }
            }
            (_, KeyCode::PageUp) => {
                for _ in 0..5 { self.branch_logs_state.scroll_up(); }
            }
            (_, KeyCode::Enter) => {
                self.create_selected_worktree();
            }
            (_, KeyCode::Char('d')) => {
                self.delete_selected_worktree();
            }
            (_, KeyCode::Char('t')) => {
                self.toggle_track_selected();
            }
            (_, KeyCode::Char('r')) => {
                self.do_poll();
            }
            (_, KeyCode::Char('a')) => {
                self.toggle_auto_create();
            }
            (_, KeyCode::Char('s')) => {
                self.open_settings();
            }
            _ => {}
        }
    }

    /// Open settings screen
    fn open_settings(&mut self) {
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
    fn handle_settings_keys(&mut self, key: KeyEvent) {
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

    fn apply_settings_edit(&mut self) {
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
    fn render_settings(&self, frame: &mut Frame, area: Rect) {
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

    /// Handle logs view keys
    fn handle_logs_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => {
                self.view_mode = ViewMode::Main;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.logs_state.scroll_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.logs_state.scroll_up();
            }
            KeyCode::Char('G') => {
                self.logs_state.scroll_to_bottom();
            }
            KeyCode::Char('g') => {
                self.logs_state.scroll = 0;
            }
            _ => {}
        }
    }

    /// Handle error view keys
    fn handle_error_keys(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                self.running = false;
            }
            _ => {}
        }
    }

    /// Handle help view keys
    fn handle_help_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.view_mode = ViewMode::Main;
            }
            _ => {}
        }
    }

    /// Update the branch list from current state
    fn update_branch_list(&mut self) {
        // First sync config with actual git state
        self.sync_worktrees_with_git();
        
        let manager = WorktreeManager::new(&self.repo);
        let worktrees = manager.list().unwrap_or_default();

        let mut items: Vec<BranchItem> = self
            .watcher
            .get_known_branches()
            .iter()
            .map(|branch| {
                let existing_worktree = worktrees
                    .iter()
                    .find(|w| w.branch.as_deref() == Some(&branch.name));

                let status = if self.config.untracked_branches.contains(&branch.name) {
                    BranchStatus::Untracked
                } else if let Some(wt) = existing_worktree {
                    if wt.is_prunable {
                        BranchStatus::LocalPrunable
                    } else {
                        BranchStatus::LocalActive
                    }
                } else {
                    BranchStatus::Remote
                };

                let is_default = self
                    .config
                    .base_branch
                    .as_ref()
                    .map(|b| b == &branch.name)
                    .unwrap_or(false);

                BranchItem {
                    name: branch.name.clone(),
                    status,
                    is_default,
                }
            })
            .collect();

        // Sort: active worktrees first, then by name
        items.sort_by(|a, b| {
            let a_active = matches!(a.status, BranchStatus::LocalActive | BranchStatus::LocalPrunable);
            let b_active = matches!(b.status, BranchStatus::LocalActive | BranchStatus::LocalPrunable);

            match (a_active, b_active) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        self.branch_list_state.update_items(items);
    }

    /// Poll for remote changes (starts background fetch)
    fn do_poll(&mut self) {
        // Don't start a new fetch if one is already in progress
        if self.watcher.is_fetching() {
            return;
        }

        self.last_poll = Instant::now();
        debug!("Starting background fetch");

        // Start non-blocking fetch
        self.watcher.start_fetch(
            self.repo.root().to_path_buf(),
            self.config.remote_name.clone(),
            self.event_tx.clone(),
        );

        self.update_status();
    }

    /// Process pending watcher events
    fn process_watcher_events(&mut self) {
        // Always check for hook output
        self.watcher.check_running_hooks(&self.event_tx);

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                WatcherEvent::FetchStarted => {
                    self.status.is_fetching = true;
                }
                WatcherEvent::FetchCompleted(output) => {
                    // Log any fetch output (warnings, etc.) to command logs
                    if let Some(msg) = output {
                        self.watcher.add_fetch_log(&self.config.remote_name, &msg);
                    }
                    // Process the completed fetch - update branches
                    self.watcher.on_fetch_complete(&self.repo, &mut self.config, &self.event_tx);
                    self.status.is_fetching = false;
                    self.status.last_fetch = self.config.last_fetch;
                    self.status.last_error = None;
                    self.update_branch_list();
                    self.update_status();
                }
                WatcherEvent::FetchFailed(msg) => {
                    self.watcher.on_fetch_failed();
                    self.watcher.add_fetch_log(&self.config.remote_name, &format!("Error: {}", msg));
                    self.status.is_fetching = false;
                    self.status.last_error = Some(msg);
                }
                WatcherEvent::NewBranchesFound(branches) => {
                    info!("New branches found: {:?}", branches);
                    self.update_branch_list();
                }
                WatcherEvent::WorktreeCreating(branch) => {
                    // Update UI to show creating status
                    if let Some(item) = self
                        .branch_list_state
                        .items
                        .iter_mut()
                        .find(|i| i.name == branch)
                    {
                        item.status = BranchStatus::Creating;
                    }
                }
                WatcherEvent::WorktreeCreated(branch, _path) => {
                    info!("Worktree created for: {}", branch);
                    self.update_branch_list();
                    self.update_status();
                }
                WatcherEvent::WorktreeCreateFailed(branch, msg) => {
                    error!("Worktree creation failed for {}: {}", branch, msg);
                    self.status.last_error = Some(format!("{}: {}", branch, msg));
                    self.update_branch_list();
                }
                WatcherEvent::HookStarted(branch) => {
                    self.status.running_hooks += 1;
                    if let Some(item) = self
                        .branch_list_state
                        .items
                        .iter_mut()
                        .find(|i| i.name == branch)
                    {
                        item.status = BranchStatus::RunningHook;
                    }
                }
                WatcherEvent::HookOutput(_, _) => {
                    // Output is already captured in watcher
                }
                WatcherEvent::HookCompleted(branch, exit_code) => {
                    self.status.running_hooks = self.status.running_hooks.saturating_sub(1);

                    // Update worktree hook status
                    if let Some(wt) = self.config.get_worktree_mut(&branch) {
                        wt.hook_status = if exit_code == 0 {
                            HookStatus::Success
                        } else {
                            HookStatus::Failed(format!("Exit code: {}", exit_code))
                        };
                    }

                    self.update_branch_list();
                }
            }
        }
    }

    /// Update status from current state
    fn update_status(&mut self) {
        self.status.remote_branch_count = self.watcher.get_known_branches().len();
        self.status.worktree_count = self.config.worktrees.len();
        self.status.auto_create_enabled = self.config.auto_create_worktrees;
        self.status.poll_interval = self.config.poll_interval_secs;
    }

    /// Create worktree for the selected branch
    fn create_selected_worktree(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Don't create if already has worktree
        if matches!(
            selected.status,
            BranchStatus::LocalActive | BranchStatus::Creating | BranchStatus::RunningHook
        ) {
            return;
        }

        let manager = WorktreeManager::new(&self.repo);

        if let Err(e) = self.watcher.create_worktree(
            &self.repo,
            &mut self.config,
            &selected.name,
            &manager,
            &self.event_tx,
        ) {
            error!("Failed to create worktree: {}", e);
            self.status.last_error = Some(e.to_string());
        }

        self.update_branch_list();
        if let Err(e) = self.config.save(self.repo.root()) {
            error!("Failed to save config: {}", e);
        }
    }

    /// Delete worktree for the selected branch
    fn delete_selected_worktree(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        // Only delete if has worktree
        if !matches!(
            selected.status,
            BranchStatus::LocalActive | BranchStatus::LocalPrunable
        ) {
            return;
        }

        let manager = WorktreeManager::new(&self.repo);

        // Check if this is the main worktree (the one with .git)
        if let Ok(worktrees) = manager.list() {
            if let Some(wt) = worktrees.iter().find(|w| w.branch.as_deref() == Some(&selected.name)) {
                if wt.is_main {
                    self.status.last_error = Some("Cannot delete the main worktree (contains .git)".to_string());
                    return;
                }
            }
        }

        // Show confirmation dialog
        self.view_mode = ViewMode::DeleteConfirm {
            branch: selected.name.clone(),
            input: String::new(),
        };
    }

    /// Actually perform the worktree deletion after confirmation
    fn do_delete_worktree(&mut self, branch: &str) {
        let manager = WorktreeManager::new(&self.repo);

        // Get worktree path
        if let Ok(Some(path)) = manager.get_worktree_path(branch) {
            if let Err(e) = manager.remove(&path, false) {
                error!("Failed to remove worktree: {}", e);
                self.status.last_error = Some(e.to_string());
            } else {
                // Update config
                self.config.remove_worktree(branch);
                self.config.untrack_branch(branch);

                if let Err(e) = self.config.save(self.repo.root()) {
                    error!("Failed to save config: {}", e);
                }
            }
        }

        self.update_branch_list();
    }

    /// Handle keys in delete confirmation dialog
    fn handle_delete_confirm_keys(&mut self, key: KeyEvent) {
        // Extract branch and input from view mode
        let (branch, mut input) = match &self.view_mode {
            ViewMode::DeleteConfirm { branch, input } => (branch.clone(), input.clone()),
            _ => return,
        };

        match key.code {
            KeyCode::Esc => {
                self.view_mode = ViewMode::Main;
            }
            KeyCode::Enter => {
                if input.to_lowercase() == "yes" {
                    self.view_mode = ViewMode::Main;
                    self.do_delete_worktree(&branch);
                }
                // If not "yes", do nothing - user must type exactly "yes"
            }
            KeyCode::Backspace => {
                input.pop();
                self.view_mode = ViewMode::DeleteConfirm { branch, input };
            }
            KeyCode::Char(c) => {
                input.push(c);
                self.view_mode = ViewMode::DeleteConfirm { branch, input };
            }
            _ => {}
        }
    }

    /// Toggle tracking for the selected branch
    fn toggle_track_selected(&mut self) {
        let Some(selected) = self.branch_list_state.selected().cloned() else {
            return;
        };

        if self.config.untracked_branches.contains(&selected.name) {
            self.config.track_branch(&selected.name);
        } else {
            self.config.untrack_branch(&selected.name);
        }

        if let Err(e) = self.config.save(self.repo.root()) {
            error!("Failed to save config: {}", e);
        }

        self.update_branch_list();
    }

    /// Toggle auto-create mode
    fn toggle_auto_create(&mut self) {
        self.config.auto_create_worktrees = !self.config.auto_create_worktrees;
        self.status.auto_create_enabled = self.config.auto_create_worktrees;

        if let Err(e) = self.config.save(self.repo.root()) {
            error!("Failed to save config: {}", e);
        }
    }
}

