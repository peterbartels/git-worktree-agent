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
    /// Create new worktree dialog
    CreateWorktree(CreateWorktreeState),
}

/// State for the create new worktree dialog (2-step wizard)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorktreeState {
    /// Current step in the wizard
    pub step: CreateWorktreeStep,
    /// Available base branches to checkout from (all branches)
    pub base_branches: Vec<String>,
    /// Index of selected base branch in the filtered list
    pub selected_base_index: usize,
    /// Filter text for base branches
    pub base_branch_filter: String,
    /// The selected base branch (set after step 1)
    pub selected_base: Option<String>,
    /// New branch name input
    pub new_branch_name: String,
    /// Default branch name (for placeholder display)
    pub default_branch: Option<String>,
}

/// Which step of the create worktree wizard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateWorktreeStep {
    /// Step 1: Select base branch (with search)
    SelectBaseBranch,
    /// Step 2: Enter new branch name
    EnterBranchName,
}

impl CreateWorktreeState {
    pub fn new(base_branches: Vec<String>, default_branch: Option<&str>) -> Self {
        let selected_base_index = default_branch
            .and_then(|db| base_branches.iter().position(|b| b == db))
            .unwrap_or(0);

        Self {
            step: CreateWorktreeStep::SelectBaseBranch,
            base_branches,
            selected_base_index,
            base_branch_filter: String::new(),
            selected_base: None,
            new_branch_name: String::new(),
            default_branch: default_branch.map(|s| s.to_string()),
        }
    }

    /// Get filtered list of base branches
    pub fn filtered_branches(&self) -> Vec<&String> {
        if self.base_branch_filter.is_empty() {
            self.base_branches.iter().collect()
        } else {
            let filter_lower = self.base_branch_filter.to_lowercase();
            self.base_branches
                .iter()
                .filter(|b| b.to_lowercase().contains(&filter_lower))
                .collect()
        }
    }

    /// Get the currently highlighted base branch from the filtered list
    pub fn highlighted_base_branch(&self) -> Option<&str> {
        let filtered = self.filtered_branches();
        filtered.get(self.selected_base_index).map(|s| s.as_str())
    }

    /// Reset selection when filter changes
    pub fn on_filter_changed(&mut self) {
        self.selected_base_index = 0;
    }

    /// Move to the next step
    pub fn next_step(&mut self) -> bool {
        match self.step {
            CreateWorktreeStep::SelectBaseBranch => {
                if let Some(branch) = self.highlighted_base_branch() {
                    self.selected_base = Some(branch.to_string());
                    self.step = CreateWorktreeStep::EnterBranchName;
                    true
                } else {
                    false
                }
            }
            CreateWorktreeStep::EnterBranchName => {
                // This is the final step, Enter will create
                false
            }
        }
    }

    /// Move to the previous step
    pub fn prev_step(&mut self) {
        match self.step {
            CreateWorktreeStep::SelectBaseBranch => {
                // Already at first step
            }
            CreateWorktreeStep::EnterBranchName => {
                self.step = CreateWorktreeStep::SelectBaseBranch;
            }
        }
    }
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
