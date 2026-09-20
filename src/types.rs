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
    pub zip:       Option<PathBuf>,
    pub bytes:     u64,
    pub duration:  Duration,
    /// Final per-stage durations, indexed by `ops::run::Stage::index`.
    pub stages:    [Option<Duration>; 4],
    pub warnings:  u32,
    pub errors:    u32,
    pub ok:        bool,
    /// What it was built for, so the result does not have to assume Windows.
    pub platform:  BuildTarget,
    pub config:    BuildConfiguration,
    /// UAT's full log for this run.
    pub log:       Option<PathBuf>,
    /// The first few error lines, for the failure summary.
    pub error_samples: Vec<String>,
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
