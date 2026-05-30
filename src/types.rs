/// IDE to open after generating project files.
#[derive(Clone, Copy, PartialEq)]
pub enum IdeChoice {
    VisualStudio,
    Rider,
    SkipOpen,
}

/// Every step in the git flow. Stored in [`crate::app::DevToolApp`]; drives which panel shows.
#[derive(Clone, PartialEq)]
pub enum GitState {
    Idle,
    Menu,
    CommitMsg,
    SyncConfirm,
    MergeConfirm,
    AfterPush,
    AfterMerge,
    NewBranchAfterPush,
    NewBranchAfterMerge,
}

/// Result written by a git background task; drives the state transition in `update()`.
#[derive(Clone, PartialEq)]
pub enum GitTaskStatus {
    Ok,
    Conflict,
    Error,
}

/// Returned from `show_git_panel()` to tell the caller which background task to launch.
pub enum GitAction {
    None,
    StartCommitPush,
    StartSync,
    StartMerge,
    StartCheckout { branch: String },
    StartNewBranch { name: String },
}
