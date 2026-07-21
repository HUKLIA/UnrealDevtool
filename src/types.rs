/// IDE to open after generating project files.
#[derive(Clone, Copy, PartialEq)]
pub enum IdeChoice {
    VisualStudio,
    Rider,
    SkipOpen,
}

/// Top-level tab. Drives the main tab bar and which tab's content
/// `show_idle_view` routes to.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppTab {
    Dashboard,
    Package,
    Git,
    Chat,
    Extras,
}

/// Sub-navigation within the Extras tab.
#[derive(Clone, Copy, PartialEq)]
pub enum ExtrasTab {
    Miku,
    Games,
    SelfCheck,
    Discord,
    Customize,
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

/// Returned from `show_upload_panel_ui()` to tell the caller what to do.
pub enum UploadAction {
    None,
    Upload,
    Skip,
}

/// Returned from `show_git_panel()` to tell the caller which background task to launch.
pub enum GitAction {
    None,
    StartCommitPush,
    StartSync,
    StartMerge,
    StartMergeAndPackage,
    StartCheckout { branch: String },
    StartNewBranch { name: String },
}
