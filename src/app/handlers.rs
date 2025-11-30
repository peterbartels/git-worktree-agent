//! Event handlers for keyboard and mouse input

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use super::state::ViewMode;
use super::App;

impl App {
    /// Handle key events
    pub(super) fn on_key_event(&mut self, key: KeyEvent) {
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
    pub(super) fn on_mouse_event(&mut self, mouse: MouseEvent) {
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

    /// Handle main view keys
    pub(super) fn handle_main_keys(&mut self, key: KeyEvent) {
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

    /// Handle logs view keys
    pub(super) fn handle_logs_keys(&mut self, key: KeyEvent) {
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
    pub(super) fn handle_error_keys(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => {
                self.running = false;
            }
            _ => {}
        }
    }

    /// Handle help view keys
    pub(super) fn handle_help_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.view_mode = ViewMode::Main;
            }
            _ => {}
        }
    }

    /// Handle keys in delete confirmation dialog
    pub(super) fn handle_delete_confirm_keys(&mut self, key: KeyEvent) {
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
}

