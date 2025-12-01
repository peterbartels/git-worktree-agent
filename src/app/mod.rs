//! Main application state and logic

mod actions;
mod handlers;
mod settings;
mod setup;
mod state;
mod views;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};

use crate::config::Config;
use crate::git::Repository;
use crate::ui::{AppStatus, BranchListState, HelpWidget, LogsState, Theme};
use crate::watcher::{Watcher, WatcherEvent};

pub use state::ViewMode;

use state::{SettingsState, SetupState};

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
    /// Directory to print after exit (for 'o' command)
    exit_to_directory: Option<std::path::PathBuf>,
}

impl App {
    /// Create a new application
    pub fn new(repo_path: &std::path::Path) -> Result<Self> {
        let repo = Repository::discover(repo_path)?;

        // Check if config file exists (first run detection)
        // Config is always stored in the main worktree (where .git directory is)
        let config_path = repo.main_root().join(crate::config::CONFIG_FILE_NAME);
        let is_first_run = !config_path.exists();

        let config = Config::load(repo.main_root())?;

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
            worktree_count: 0, // Will be updated on first branch list update
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
            exit_to_directory: None,
        };

        // Only update branch list if not in setup mode
        if !is_first_run {
            app.update_branch_list();
        }
        Ok(app)
    }

    /// Run the application's main loop
    /// Returns the directory to cd to after exit (if any)
    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<Option<std::path::PathBuf>> {
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
            self.config.save(self.repo.main_root())?;
        }

        Ok(self.exit_to_directory)
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
            ViewMode::CreateWorktree(state) => {
                let state = state.clone();
                self.render_main(frame, area);
                self.render_create_worktree(frame, area, &state);
            }
        }
    }
}
