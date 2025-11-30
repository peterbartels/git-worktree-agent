//! Application state types and enums

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

impl Default for SetupState {
    fn default() -> Self {
        Self::new()
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
    pub fn all() -> &'static [SettingsField] {
        &[
            SettingsField::Remote,
            SettingsField::PollInterval,
            SettingsField::WorktreeBaseDir,
            SettingsField::BaseBranch,
            SettingsField::PostCreateCommand,
            SettingsField::AutoCreate,
        ]
    }

    pub fn index(&self) -> usize {
        match self {
            SettingsField::Remote => 0,
            SettingsField::PollInterval => 1,
            SettingsField::WorktreeBaseDir => 2,
            SettingsField::BaseBranch => 3,
            SettingsField::PostCreateCommand => 4,
            SettingsField::AutoCreate => 5,
        }
    }

    pub fn from_index(index: usize) -> Self {
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

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}
