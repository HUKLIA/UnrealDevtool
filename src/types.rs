use std::path::PathBuf;
use std::time::Duration;

/// IDE to open after generating project files.
#[derive(Clone, Copy, PartialEq)]
pub enum IdeChoice {
    VisualStudio,
    Rider,
    SkipOpen,
}

/// Unreal build configuration used by the packaging pipeline.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuildConfiguration {
    Development,
    Shipping,
}

impl BuildConfiguration {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "Development",
            Self::Shipping => "Shipping",
        }
    }
}

/// Which lines the Monitor's log pane shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Warnings,
    Errors,
}

/// How the project is turned into a package.
///
/// All three end in the same place — a staged, paked folder that is zipped —
/// but they get there differently, and each is the right tool for a different
/// moment in a project's life.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PackageMethod {
    /// One `RunUAT BuildCookRun` does everything: compile, cook, stage, pak,
    /// archive. What Unreal's own Project Launcher does, and the safe default.
    Full,
    /// Compile first with UnrealBuildTool directly (editor target, then game
    /// target), then hand UAT a finished build with `-skipbuild`. A compile
    /// error fails in a couple of minutes and shows compiler output, instead
    /// of surfacing 20 minutes into a run buried in UAT's log.
    Stepwise,
    /// Skip compiling and cooking and re-stage what is already in
    /// `Saved/Cooked`. Minutes instead of a full run, for when only packaging
    /// settings or staged files changed.
    Restage,
}

impl PackageMethod {
    pub const ALL: [PackageMethod; 3] = [Self::Full, Self::Stepwise, Self::Restage];

    pub fn label(self) -> &'static str {
        match self {
            Self::Full     => "Full package",
            Self::Stepwise => "Compile, then package",
            Self::Restage  => "Restage last cook",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Full     => "One UAT BuildCookRun does everything. The default, and what Unreal's Project Launcher runs.",
            Self::Stepwise => "Compiles with UnrealBuildTool first (editor, then game), then cooks and packages. Compile errors fail fast with compiler output. Blueprint-only projects skip the compile.",
            Self::Restage  => "Reuses the cook already in Saved/Cooked and only stages, paks and archives. Fast — but the content is whatever was cooked last.",
        }
    }

    pub fn as_key(self) -> &'static str {
        match self { Self::Full => "full", Self::Stepwise => "stepwise", Self::Restage => "restage" }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_key() == k.trim())
    }
}

/// Platform a build targets.
///
/// One per build rather than a multi-select queue: the progress, stage-timing
/// and log model are all built around a single run, and a queue would be a
/// much larger rework than the value it adds here.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    Win64,
    Android,
    Linux,
    Mac,
}

impl BuildTarget {
    pub const ALL: [BuildTarget; 4] =
        [BuildTarget::Win64, BuildTarget::Android, BuildTarget::Linux, BuildTarget::Mac];

    /// What UAT expects after `-platform=`.
    pub fn uat_name(self) -> &'static str {
        match self {
            BuildTarget::Win64   => "Win64",
            BuildTarget::Android => "Android",
            BuildTarget::Linux   => "Linux",
            BuildTarget::Mac     => "Mac",
        }
    }

    /// Short label for the chip row.
    /// The platform name UnrealBuildTool takes on its command line.
    pub fn ubt_platform(self) -> &'static str {
        match self {
            Self::Win64   => "Win64",
            Self::Android => "Android",
            Self::Linux   => "Linux",
            Self::Mac     => "Mac",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BuildTarget::Win64   => "Windows",
            BuildTarget::Android => "Android",
            BuildTarget::Linux   => "Linux",
            BuildTarget::Mac     => "Mac",
        }
    }

    /// Stable id for the saved project config.
    pub fn as_key(self) -> &'static str {
        self.uat_name()
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.uat_name().eq_ignore_ascii_case(k))
    }

    /// Prefix of the folder UAT archives into.
    ///
    /// Win64 archives to `Windows`, not `Win64`. Android appends the texture
    /// format (`Android_ASTC`, `Android_DXT`, …), so this is matched as a
    /// prefix rather than an exact name.
    pub fn output_prefix(self) -> &'static str {
        match self {
            BuildTarget::Win64   => "Windows",
            BuildTarget::Android => "Android",
            BuildTarget::Linux   => "Linux",
            BuildTarget::Mac     => "Mac",
        }
    }

    /// Only Windows produces a single renameable `.exe`. Android emits an
    /// apk/aab, Linux a bare ELF, Mac an `.app` bundle — renaming any of those
    /// to `<name>.exe` would corrupt the output rather than tidy it.
    pub fn renames_executable(self) -> bool {
        matches!(self, BuildTarget::Win64)
    }

    /// True when this target can be built from a Windows host at all.
    ///
    /// Mac packaging requires macOS; the others are cross-compilable from
    /// Windows given the right toolchain, which is checked separately.
    pub fn buildable_on_windows(self) -> bool {
        !matches!(self, BuildTarget::Mac)
    }
}

/// What the main surface is showing.
///
/// This replaced a five-way `AppTab`. The distinction matters: a tab is a
/// place the user chooses to be, whereas these are states the *work* is in —
/// the app derives which one applies (see `DevToolApp::run_state`) rather than
/// the user selecting it. That is the whole point of the task-first layout:
/// there is one surface, and it follows the job.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// No project set yet — nothing else in the app can do anything useful.
    Setup,
    /// A project is set and a build can be started.
    Ready,
    /// A background task owns the screen.
    Running,
    /// A build just finished and has not been dismissed.
    Done,
}

/// Secondary surfaces, shown as a sheet over the run surface.
///
/// None of these are destinations you sit in while working, which is why they
/// are overlays rather than peers of the build surface — they used to each
/// cost a permanent slot in a sidebar that was always on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    Chat,
    Browser,
    Extras,
    Diagnostics,
    Monitor,
    Tools,
    Settings,
}

impl Sheet {
    pub fn title(self) -> &'static str {
        match self {
            Sheet::Chat        => "Dev Assistant",
            Sheet::Browser     => "Browser",
            Sheet::Extras      => "Extras",
            Sheet::Diagnostics => "Project setup & checks",
            Sheet::Monitor     => "Project monitor",
            Sheet::Tools       => "Unreal tools",
            Sheet::Settings    => "Settings",
        }
    }
}

/// Sub-navigation within the Extras sheet.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExtrasTab {
    Miku,
    Games,
    SelfCheck,
    Discord,
    QuickLinks,
    Customize,
}

/// What a finished run produced. Drives the `Done` surface.
///
/// Held separately from the live `RunProgress` so the result survives the next
/// run being prepared, and so the numbers shown are a snapshot rather than
/// timings that keep ticking after the build has stopped.
#[derive(Clone)]
pub struct BuildOutcome {
    pub version:   String,
    pub message:   String,
    pub zip:       Option<PathBuf>,
    pub outcome:   PackageOutcome,
    /// Whether this run requested a distributable ZIP as a required stage.
    pub archive_requested: bool,
    pub archive_creation: ArchiveCreation,
    pub archive_verification: ArchiveVerification,
    pub bytes:     u64,
    pub duration:  Duration,
    /// Final per-stage durations, indexed by `ops::run::Stage::index`.
    pub stages:    [Option<Duration>; 4],
    pub warnings:  u32,
    pub errors:    u32,
    /// What it was built for, so the result does not have to assume Windows.
    pub platform:  BuildTarget,
    pub config:    BuildConfiguration,
    /// UAT's full log for this run.
    pub log:       Option<PathBuf>,
    /// The first few error lines, for the failure summary.
    pub error_samples: Vec<String>,
}

/// Terminal state reported by the packaging operation itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl PackageOutcome {
    pub fn is_success(self) -> bool { self == Self::Succeeded }

    pub fn as_str(self) -> &'static str {
        match self { Self::Succeeded => "succeeded", Self::Failed => "failed", Self::Cancelled => "cancelled" }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value { "succeeded" => Some(Self::Succeeded), "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled), _ => None }
    }
}

/// ZIP creation and verification are separate because UAT can succeed even
/// when no usable distributable archive was produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveCreation {
    NotAttempted,
    Created,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveVerification {
    NotAttempted,
    Verified,
    Rejected,
}

/// Typed completion from the package pipeline. UI behavior must use these
/// fields rather than parse the human-readable message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageTaskResult {
    pub outcome: PackageOutcome,
    pub archive_requested: bool,
    pub archive_creation: ArchiveCreation,
    pub archive_verification: ArchiveVerification,
    pub message: String,
    pub output_dir: Option<PathBuf>,
    pub log: Option<PathBuf>,
    /// Present only when the archive passed full entry/checksum verification.
    pub verified_zip: Option<PathBuf>,
}

impl PackageTaskResult {
    pub fn succeeded(message: String, output_dir: Option<PathBuf>, log: Option<PathBuf>, zip: PathBuf) -> Self {
        Self { outcome: PackageOutcome::Succeeded, archive_requested: true, archive_creation: ArchiveCreation::Created,
            archive_verification: ArchiveVerification::Verified, message, output_dir, log,
            verified_zip: Some(zip) }
    }

    /// A successful package when the caller requested the output folder but no ZIP stage.
    pub fn succeeded_without_archive(message: String, output_dir: Option<PathBuf>, log: Option<PathBuf>) -> Self {
        Self { outcome: PackageOutcome::Succeeded, archive_requested: false,
            archive_creation: ArchiveCreation::NotAttempted,
            archive_verification: ArchiveVerification::NotAttempted, message, output_dir, log,
            verified_zip: None }
    }

    pub fn failed(message: String, archive_requested: bool, archive_creation: ArchiveCreation,
                  archive_verification: ArchiveVerification, output_dir: Option<PathBuf>,
                  log: Option<PathBuf>) -> Self {
        Self { outcome: PackageOutcome::Failed, archive_requested, archive_creation, archive_verification,
            message, output_dir, log, verified_zip: None }
    }

    pub fn cancelled(message: String, archive_requested: bool, archive_creation: ArchiveCreation,
                     output_dir: Option<PathBuf>, log: Option<PathBuf>) -> Self {
        Self { outcome: PackageOutcome::Cancelled, archive_requested, archive_creation,
            archive_verification: ArchiveVerification::NotAttempted,
            message, output_dir, log, verified_zip: None }
    }

    pub fn is_success(&self) -> bool {
        self.outcome.is_success() && (!self.archive_requested
            || (self.archive_creation == ArchiveCreation::Created
                && self.archive_verification == ArchiveVerification::Verified
                && self.verified_zip.is_some()))
    }
}

impl ArchiveCreation {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotAttempted => "not attempted",
            Self::Created => "created",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl ArchiveVerification {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotAttempted => "not attempted",
            Self::Verified => "verified",
            Self::Rejected => "rejected",
        }
    }
}

impl BuildOutcome {
    pub fn is_success(&self) -> bool {
        self.outcome.is_success() && (!self.archive_requested
            || (self.archive_creation == ArchiveCreation::Created
                && self.archive_verification == ArchiveVerification::Verified
                && self.zip.is_some()))
    }

    pub fn status_label(&self) -> &'static str {
        if self.is_success() { "BUILD COMPLETE" }
        else if self.outcome == PackageOutcome::Cancelled { "BUILD CANCELLED" }
        else { "BUILD FAILED" }
    }
}

#[cfg(test)]
mod package_result_tests {
    use super::*;

    #[test]
    fn error_text_cannot_turn_a_failed_task_into_success() {
        let result = PackageTaskResult::failed(
            "[DONE] misleading status followed by [ERROR] archive failed".into(),
            true, ArchiveCreation::Created,
            ArchiveVerification::Rejected,
            None,
            None,
        );
        assert_eq!(result.outcome, PackageOutcome::Failed);
        assert_eq!(result.archive_verification, ArchiveVerification::Rejected);
        assert!(!result.is_success());
        assert!(result.verified_zip.is_none());
    }

    #[test]
    fn cancellation_is_a_distinct_terminal_state() {
        let result = PackageTaskResult::cancelled(
            "[CANCELLED] stopped during packaging".into(),
            true, ArchiveCreation::Cancelled,
            None,
            None,
        );
        assert_eq!(result.outcome, PackageOutcome::Cancelled);
        assert_eq!(result.archive_creation, ArchiveCreation::Cancelled);
        assert_eq!(result.archive_verification, ArchiveVerification::NotAttempted);
        assert!(!result.is_success());
    }

    #[test]
    fn requested_output_folder_succeeds_without_an_unrequested_archive() {
        let result = PackageTaskResult::succeeded_without_archive(
            "[DONE] Package output is ready.".into(), None, None);
        assert!(result.is_success());
        assert!(!result.archive_requested);
        assert_eq!(result.archive_creation, ArchiveCreation::NotAttempted);
        assert_eq!(result.archive_verification, ArchiveVerification::NotAttempted);
        assert!(result.verified_zip.is_none());
    }

    #[test]
    fn failure_to_create_a_requested_archive_fails_the_requested_workflow() {
        let result = PackageTaskResult::failed(
            "ZIP creation failed".into(), true, ArchiveCreation::Failed,
            ArchiveVerification::NotAttempted, None, None);
        assert!(result.archive_requested);
        assert!(!result.is_success());
    }

    #[test]
    fn an_unverified_archive_cannot_be_successful() {
        let result = PackageTaskResult {
            outcome: PackageOutcome::Succeeded,
            archive_requested: true,
            archive_creation: ArchiveCreation::Created,
            archive_verification: ArchiveVerification::Rejected,
            message: "[DONE] misleading legacy text".into(),
            output_dir: None,
            log: None,
            verified_zip: None,
        };
        assert!(!result.is_success());
    }
}

/// Every step in the git flow. Drives which panel the git sheet shows.
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
