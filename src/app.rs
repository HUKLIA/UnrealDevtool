use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use eframe::egui;

use crate::audio::AudioPlayer;
use crate::config::{
    clear_engine_path, load_audio_config, load_custom_links, load_engine_path, load_media_config, load_project_config,
    load_project_path, load_ui_config, load_upload_config, save_audio_config, save_custom_links, save_engine_path,
    save_media_config, save_project_config, save_project_path, save_upload_config, AudioConfig, CustomLink,
    MediaConfig, UploadConfig,
};
use crate::engine::{build_init_status, detect_unreal_engine, is_valid_engine_dir};
use crate::gif::GifPlayer;
use crate::ops::{git as ops_git, package as ops_package, update as ops_update, vs as ops_vs};
use crate::ops::update::UpdateInfo;
use crate::theme::{apply_theme, install_fonts};
use crate::types::{BuildConfiguration, BuildOutcome, BuildTarget, ExtrasTab, GitState, GitTaskStatus,
                   IdeChoice, RunState, Sheet};
use crate::webview::{WebPanel, WebViewManager};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// How long the completed boot log stays on screen before it starts to fade.
const INTRO_HOLD_SECS: f32 = 0.55;
/// How long that fade takes.
const INTRO_FADE_SECS: f32 = 0.45;

// ── Application state ─────────────────────────────────────────────────────────

pub struct DevToolApp {
    pub engine_dir:           Option<PathBuf>,
    /// Manually-picked engine folder (persisted). When set and still valid,
    /// this wins over auto-detection — lets the user recover from
    /// "[ERROR] Engine not found." when the registry lookup can't find it.
    pub engine_override:      Option<PathBuf>,
    pub project_path:         Option<PathBuf>,
    pub project_path_input:   String,
    pub status_message:       Arc<Mutex<String>>,
    pub status_display:       String,
    pub is_working:           Arc<Mutex<bool>>,
    pub was_working:          bool,
    pub gif_player:           Option<GifPlayer>,
    pub audio_player:         Option<AudioPlayer>,
    pub audio_muted:          bool,
    pub audio_volume:         u32,
    pub busy_label:           String,
    pub cancel_flag:          Arc<AtomicBool>,
    pub progress:             Arc<Mutex<f32>>,

    // Package pre-flight
    pub pack_name_input:            String,
    pub exe_name_input:             String,
    pub next_version_preview:       u32,
    pub use_custom_version:         bool,
    pub version_override:           String,
    pub build_configuration:        BuildConfiguration,
    /// Platform this build targets. Persisted per project.
    pub build_target:               BuildTarget,
    pub editor_is_running:          bool,   // refreshed by `refresh_package_observed` (tab entry, periodic poll, task completion)
    /// `is_editor_running()` shells out to `tasklist` — set on a background
    /// thread by `refresh_package_observed`, drained into `editor_is_running`
    /// above on the next frame (see `update()`), same pattern as
    /// `pc_check_disk` below.
    pub editor_check_pending:       Arc<Mutex<Option<bool>>>,
    /// `find_next_version()` does a `read_dir` over the build folder — set on
    /// a background thread by `refresh_package_observed`, drained into
    /// `next_version_preview` above on the next frame (see `update()`), same
    /// pattern as `editor_check_pending` just above. Needed because
    /// `refresh_package_observed` is now also called from the Package tab's
    /// periodic poll (see `update()`'s tick block), not just on tab entry —
    /// a `read_dir` blocking the UI thread once every couple of seconds
    /// would be a regression of the exact class of bug this app already did
    /// dedicated work to remove.
    pub version_check_pending:      Arc<Mutex<Option<u32>>>,

    // VS-rebuild pre-flight
    pub show_vs_config: bool,
    pub ide_choice:     IdeChoice,

    // PC / environment pre-flight checks (engine & project path validity,
    // space-in-path UAT workaround, disk space)
    pub use_space_free_link: bool,
    pub pc_check_items:      Vec<crate::ops::preflight::CheckItem>,
    /// `None` while the background disk-space check is still running.
    pub pc_check_disk:       Arc<Mutex<Option<crate::ops::preflight::CheckItem>>>,
    pub build_log_path:      Option<PathBuf>,
    pub build_log_diagnosis: Vec<crate::ops::diagnostics::Diagnosis>,

    // App self-check (the DevTool's own install/config/update health, as
    // opposed to Check PC Setup which is about the Unreal project/engine)
    pub app_check_items:     Vec<crate::ops::preflight::CheckItem>,
    pub app_check_github:    Arc<Mutex<Option<crate::ops::preflight::CheckItem>>>,

    // Post-package upload panel
    pub pending_zip:        Arc<Mutex<Option<PathBuf>>>,
    pub show_upload_panel:  bool,
    pub upload_zip_path:    PathBuf,
    pub upload_use_local:   bool,
    pub upload_use_gdrive:  bool,
    pub upload_local_path:  String,
    pub upload_rclone_dest: String,   // e.g. "gdrive:/Builds/MyGame"
    pub gdrive_remote_status: Option<bool>, // None = not checked yet, Some(found?)
    pub gdrive_upload_failed:      Arc<Mutex<bool>>, // set by the upload background task
    pub show_upload_fallback_panel: bool,

    // Git state machine
    pub git_state:               GitState,
    pub git_next_state:          GitState,
    pub git_result:              Arc<Mutex<Option<GitTaskStatus>>>,
    pub git_current_branch:      String,
    pub git_status:              ops_git::GitStatusSummary,
    /// `git_current_branch`/`git_status_summary` shell out to `git` up to
    /// 7 times combined (status, log, rev-list, the 14-day activity log,
    /// its `git var` timezone lookup, the working-tree diffstat) — set on a
    /// background thread by `refresh_git_status_async` (called from
    /// `open_git_menu`, after a git task finishes, and by the Git tab's
    /// periodic poll in `update()` — see that method's doc comment), drained
    /// into the two fields above on the next frame, same pattern as
    /// `pc_check_disk` below.
    pub git_refresh_pending:     Arc<Mutex<Option<(String, ops_git::GitStatusSummary)>>>,
    pub git_merged_from:         String,
    pub git_commit_msg:          String,
    pub git_new_branch_name:     String,
    pub git_package_after_merge: bool,

    // Post-package: open folder prompt
    pub show_open_folder_panel:    bool,
    pub pending_open_folder_path:  std::path::PathBuf,

    // Extras
    pub dm_target_name:        String,
    pub dm_message_presets:    Vec<String>,
    pub dm_custom_message:     String,
    pub dm_image_path:         String,

    // Quick Links: user-editable label+URL buttons (Extras tab)
    pub custom_links:    Vec<CustomLink>,
    pub links_edit_mode: bool,

    // Miku view mode: false = 2D gif (default), true = 3D web
    pub miku_mode_3d: bool,

    // Embedded WebView2 panels (3D Miku, Cookie Clicker, Sponder Bird)
    pub webview_manager:  WebViewManager,
    pub active_web_panel: Option<WebPanel>,
    pub pending_webview:  Option<(WebPanel, egui::Rect)>,

    // Self-update
    pub update_info:        Arc<Mutex<Option<UpdateInfo>>>,
    pub show_update_banner: bool,
    /// Reuse the previous cook instead of re-cooking everything. Off by
    /// default: a stale iterative cook can hide a content problem a clean cook
    /// would surface, so the build you ship should be a full one.
    pub iterate_cook: bool,
    /// `-compressed`: smaller paks, slower to build and to load.
    pub compress_pak: bool,
    /// How the project is packaged (see `types::PackageMethod`).
    pub package_method: crate::types::PackageMethod,
    /// The most a package should weigh, in MB (0 = no limit), and the text box for it.
    pub size_budget_mb: u32,
    pub size_budget_input: String,
    /// State of the Unreal tools sheet.
    pub tools: crate::ui::tools::ToolsState,
    /// Command palette (Ctrl+K).
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_sel: usize,
    /// A build set to start unattended at a time of day: when, and the
    /// "HH:MM" it was set for.
    pub schedule: Option<(Instant, String)>,
    pub schedule_input: String,
    /// Result of `ops::doctor::check_project`, refreshed on the slow poll.
    pub doctor_items: Vec<crate::ops::preflight::CheckItem>,
    /// One-click fixes for what `doctor_items` found. Computed when the checks
    /// sheet opens, not every poll: finding maps walks the Content folder.
    pub doctor_fixes: Vec<crate::ops::doctor::Fix>,
    /// Live project monitor; exists only while its sheet is open.
    pub monitor: Option<crate::ops::monitor::MonitorHandle>,
    pub monitor_filter: crate::types::LogFilter,
    /// Show only this log category (`LogCook`…) in the monitor's log.
    pub monitor_category: Option<String>,
    /// The log as it was when Pause was pressed, so it can be read while the
    /// editor keeps writing.
    pub monitor_frozen: Option<Vec<crate::ops::monitor::LogEntry>>,
    /// Extra UAT arguments typed by the user (validated before use).
    pub extra_uat_args: String,

    // ── Clean project ───────────────────────────────────────────────────
    /// Removable folders found under the project, with their sizes.
    pub clean_targets: Vec<crate::ops::clean::Target>,
    /// Which of them are ticked.
    pub clean_selected: Vec<bool>,
    /// Second click arms the delete, same as the updater.
    pub clean_confirm: bool,

    // ── Frame pacing ────────────────────────────────────────────────────
    /// Rolling frame-time samples in milliseconds, for the debug meter and
    /// for deciding how much motion the machine can actually afford.
    pub frame_ms: std::collections::VecDeque<f32>,
    /// Smoothed frame interval in seconds. Animations are scaled against the
    /// real display cadence rather than an assumed 60 Hz.
    pub frame_dt: f32,
    /// Frames that followed an idle gap, counted separately so they cannot be
    /// mistaken for dropped frames.
    pub wake_frames: u32,

    /// Arms the second click of the update action — see the update notice in
    /// `ui/mod.rs` for why replacing the running exe is not a one-click thing.
    pub update_confirm: bool,
    /// Set by the first click on Cancel; a second click within a few seconds
    /// confirms. A 30-minute build should not die to one stray click.
    pub cancel_armed: Option<Instant>,
    pub last_update_check:  Instant,

    // Start time for the current task. The completed-build surface snapshots
    // its elapsed duration when the task finishes.
    pub task_started_at:    Option<Instant>,

    // Custom media (2D image/GIF + looping sound)
    pub custom_gif_path:   Option<PathBuf>,
    pub custom_sound_path: Option<PathBuf>,

    // Dev-assistant chat (local LLM via Ollama / LM Studio)
    pub chat_history:    Vec<crate::ops::llm::ChatMessage>,
    pub chat_input:      String,
    pub chat_providers:  Arc<Mutex<crate::ops::llm::ChatProviders>>,
    pub chat_detecting:  Arc<Mutex<bool>>,
    pub chat_provider:   Option<crate::ops::llm::LlmProvider>,
    pub chat_model:      String,
    /// Accumulates the in-flight assistant reply while streaming; moved into
    /// `chat_history` once `chat_busy` flips back to false (see `update()`,
    /// same was_working/just_finished pattern used for background tasks).
    pub chat_streaming:  Arc<Mutex<String>>,
    pub chat_busy:       Arc<Mutex<bool>>,
    pub was_chat_busy:   bool,
    pub chat_cancel:     Arc<AtomicBool>,

    // Stored so background tasks can call request_repaint() on completion
    pub egui_ctx: egui::Context,

    /// Set once `center_window_on_startup` has actually centered the window.
    /// After that it never touches window position again — recentering on
    /// every resize/move fought the user's own drags (window would snap
    /// back to center and flicker), so this only runs once, at startup.
    pub has_centered_window: bool,

    // Tabbed main layout
    /// Which secondary surface is open over the run surface, if any.
    pub sheet: Option<Sheet>,
    /// Current step of the interactive guide.  Anchors are captured while
    /// the underlying surface is laid out, then the guide paints its
    /// spotlight and callout over that live control.
    /// The manual is its own overlay rather than a sheet.
    ///
    /// As a `Sheet` it was mutually exclusive with every other sheet, so the
    /// tour could only ever describe the build screen — it could not open the
    /// Browser or Settings and point at something on them.
    pub guide_active: bool,
    pub guide_step: usize,
    pub guide_target: Option<egui::Rect>,
    /// True while the multi-step git flow owns the main surface.
    pub show_git_sheet: bool,

    // ── The run ─────────────────────────────────────────────────────────
    /// Live stage timings and log tail, written by the packaging thread.
    pub run: Arc<Mutex<crate::ops::run::RunProgress>>,
    /// Result of the last finished build. `Some` puts the surface in `Done`.
    pub last_build: Option<BuildOutcome>,
    /// Previous builds read off disk, newest first.
    pub builds: Vec<crate::ops::history::BuildRecord>,
    /// Filled by the background scan in `refresh_builds`, drained on the next
    /// frame — walking the build tree stats thousands of files, so it can
    /// never run on the UI thread.
    pub builds_pending: Arc<Mutex<Option<Vec<crate::ops::history::BuildRecord>>>>,
    /// Filled by `refresh_clean_targets`, drained on the next frame.
    pub clean_pending: Arc<Mutex<Option<Vec<crate::ops::clean::Target>>>>,
    /// Preflight checks plus the build-log scan, computed off the UI thread.
    ///
    /// The log scan reads and pattern-matches a whole UAT log, which runs to
    /// tens of megabytes. It used to happen inline on the render thread every
    /// three seconds, which is exactly the periodic hitch that made the app
    /// feel like it was dropping frames.
    #[allow(clippy::type_complexity)]
    pub pc_check_pending: Arc<Mutex<Option<(
        Vec<crate::ops::preflight::CheckItem>,
        Option<PathBuf>,
        Vec<crate::ops::diagnostics::Diagnosis>,
    )>>>,
    pub last_builds_poll: Instant,

    // ── Built-in browser ────────────────────────────────────────────────
    /// Address-bar contents. Separate from `browser_current` so typing in the
    /// field does not count as having navigated anywhere.
    pub browser_url_input: String,
    /// Last URL we told the webview to load. Drives which shortcut renders as
    /// active; in-page navigation (clicking a link) does not update it, since
    /// wry gives us no URL-changed callback to hook.
    pub browser_current:   String,
    pub extras_tab: ExtrasTab,

    /// Last time the active tab's cheap "keep it live" poll ran (see
    /// `update()`'s tick block). A single shared timer rather than
    /// per-tab: `switch_tab` already does a full one-time refresh on
    /// entry and resets this, so the only thing this timer paces is
    /// "how long has the user been sitting on the current tab" —
    /// switching tabs never needs to remember an older tab's countdown.
    pub last_tab_poll:  Instant,
    /// Separate, much longer-interval timer gating just the disk-space
    /// check within the Dashboard tab's poll — see
    /// `refresh_pc_check_disk_async`'s doc comment for why disk space
    /// can't share the same cadence as the rest of that tab's checks.
    pub last_disk_poll: Instant,
    /// Same idea as `last_disk_poll`, for the Package tab's editor-running
    /// check — see `refresh_editor_check_async`'s doc comment for why the
    /// `tasklist` spawn can't share the version preview's 2s cadence.
    pub last_editor_poll: Instant,

    // Boot/intro splash screen — shown once at launch, before the main UI.
    pub show_intro:       bool,
    pub intro_started_at: Option<Instant>,
    pub intro_log:        Vec<String>,
    pub intro_revealed:   usize,
    pub intro_done:       bool,
    /// When the boot log finished. Drives the auto-advance hold and fade —
    /// see `tick_intro` / `intro_fade`.
    pub intro_done_at:    Option<Instant>,

    // Dashboard tab: paste-a-log-segment fallback alongside the
    // auto-scanned-from-disk build log (mirrors the reference mockup, which
    // only supports paste — auto-scan-from-disk is our own addition on top).
    pub pasted_log_input:      String,
    pub pasted_log_diagnosis:  Vec<crate::ops::diagnostics::Diagnosis>,
}

impl DevToolApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ui_cfg = load_ui_config();
        if let Some((r, g, b)) = ui_cfg.accent_rgb {
            crate::theme::set_accent_value(egui::Color32::from_rgb(r, g, b));
        }
        install_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let project_path    = load_project_path();
        let engine_override = load_engine_path().filter(|p| is_valid_engine_dir(p));
        let engine_dir       = engine_override.clone()
            .or_else(|| detect_unreal_engine(project_path.as_deref()));
        let init_status  = build_init_status(&engine_dir, &project_path);
        let project_path_input = project_path.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let media_cfg = load_media_config();
        let custom_gif_path = (!media_cfg.gif_path.is_empty())
            .then(|| PathBuf::from(&media_cfg.gif_path))
            .filter(|p| p.exists());
        let custom_sound_path = (!media_cfg.sound_path.is_empty())
            .then(|| PathBuf::from(&media_cfg.sound_path))
            .filter(|p| p.exists());

        let gif_player = custom_gif_path.as_ref()
            .and_then(|p| GifPlayer::from_file(p))
            .or_else(|| GifPlayer::from_bytes(include_bytes!("../Image/miku-hatsune.gif")));
        let raw_window  = cc.window_handle().expect("no window handle").as_raw();
        let raw_display = cc.display_handle().expect("no display handle").as_raw();
        let webview_manager = WebViewManager::new(raw_window, raw_display);
        let upload_cfg  = load_upload_config();
        let audio_cfg   = load_audio_config();
        let audio_bytes = custom_sound_path.as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_else(|| include_bytes!("../Sound/Ievan Polkka.mp3").to_vec());
        let audio_player = AudioPlayer::new(audio_bytes, audio_cfg.muted, audio_cfg.volume);
        let mut app = Self {
            engine_dir,
            engine_override,
            project_path,
            project_path_input,
            status_message: Arc::new(Mutex::new(init_status.clone())),
            status_display: init_status,
            is_working:  Arc::new(Mutex::new(false)),
            was_working: false,
            gif_player,
            audio_player,
            audio_muted:  audio_cfg.muted,
            audio_volume: audio_cfg.volume,
            busy_label:  String::new(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            progress:    Arc::new(Mutex::new(0.0_f32)),
            pack_name_input:             String::new(),
            exe_name_input:              String::new(),
            next_version_preview:        1,
            use_custom_version:          false,
            version_override:            String::new(),
            build_configuration:         BuildConfiguration::Development,
            build_target: BuildTarget::Win64,
            editor_is_running:           false,
            editor_check_pending:        Arc::new(Mutex::new(None)),
            version_check_pending:       Arc::new(Mutex::new(None)),
            show_vs_config:       false,
            ide_choice:           IdeChoice::Rider,
            use_space_free_link:  false,
            pc_check_items:       Vec::new(),
            pc_check_disk:        Arc::new(Mutex::new(None)),
            build_log_path:       None,
            build_log_diagnosis:  Vec::new(),
            app_check_items:      Vec::new(),
            app_check_github:     Arc::new(Mutex::new(None)),
            pending_zip:        Arc::new(Mutex::new(None)),
            show_upload_panel:  false,
            upload_zip_path:    PathBuf::new(),
            upload_use_local:   false,
            upload_use_gdrive:  false,
            upload_local_path:  upload_cfg.local_path,
            upload_rclone_dest: upload_cfg.rclone_dest,
            gdrive_remote_status: None,
            gdrive_upload_failed: Arc::new(Mutex::new(false)),
            show_upload_fallback_panel: false,
            git_state:               GitState::Idle,
            git_next_state:          GitState::Idle,
            git_result:              Arc::new(Mutex::new(None)),
            git_current_branch:      String::new(),
            git_status:              ops_git::GitStatusSummary::default(),
            git_refresh_pending:     Arc::new(Mutex::new(None)),
            git_merged_from:         String::new(),
            git_commit_msg:          String::new(),
            git_new_branch_name:     String::new(),
            git_package_after_merge: false,
            show_open_folder_panel:    false,
            pending_open_folder_path:  std::path::PathBuf::new(),
            dm_target_name:            "gonkindroid".to_string(),
            dm_message_presets:        vec!["Hey!".to_string(),
                                        "You up?".to_string(),
                                         "Help!!".to_string(),],
            dm_custom_message:         String::new(),
            dm_image_path:             String::new(),
            custom_links:              load_custom_links(),
            links_edit_mode:           false,
            miku_mode_3d:              false,
            webview_manager,
            active_web_panel: None,
            pending_webview:  None,
            update_info:        Arc::new(Mutex::new(None)),
            show_update_banner: true,
            update_confirm: false,
            cancel_armed: None,
            frame_ms: std::collections::VecDeque::new(),
            frame_dt: 1.0 / 60.0,
            wake_frames: 0,
            iterate_cook: false,
            compress_pak: false,
            package_method: crate::types::PackageMethod::Full,
            size_budget_mb: 0,
            size_budget_input: String::new(),
            tools: Default::default(),
            palette_open: false,
            palette_query: String::new(),
            palette_sel: 0,
            schedule: None,
            schedule_input: String::new(),
            doctor_items: Vec::new(),
            doctor_fixes: Vec::new(),
            monitor: None,
            monitor_filter: crate::types::LogFilter::All,
            monitor_category: None,
            monitor_frozen: None,
            extra_uat_args: String::new(),
            clean_targets: Vec::new(),
            clean_selected: Vec::new(),
            clean_confirm: false,
            last_update_check:  Instant::now(),
            task_started_at:    None,
            custom_gif_path,
            custom_sound_path,
            chat_history:    Vec::new(),
            chat_input:      String::new(),
            chat_providers:  Arc::new(Mutex::new(Vec::new())),
            chat_detecting:  Arc::new(Mutex::new(false)),
            chat_provider:   None,
            chat_model:      String::new(),
            chat_streaming:  Arc::new(Mutex::new(String::new())),
            chat_busy:       Arc::new(Mutex::new(false)),
            was_chat_busy:   false,
            chat_cancel:     Arc::new(AtomicBool::new(false)),
            egui_ctx: cc.egui_ctx.clone(),
            has_centered_window: false,
            sheet: None,
            guide_active: false,
            guide_step: 0,
            guide_target: None,
            show_git_sheet: false,
            run: Arc::new(Mutex::new(crate::ops::run::RunProgress::default())),
            last_build: None,
            builds: Vec::new(),
            builds_pending: Arc::new(Mutex::new(None)),
            clean_pending: Arc::new(Mutex::new(None)),
            pc_check_pending: Arc::new(Mutex::new(None)),
            last_builds_poll: Instant::now(),
            browser_url_input: crate::ui::browser::HOME_URL.to_string(),
            browser_current:   crate::ui::browser::HOME_URL.to_string(),
            extras_tab: ExtrasTab::Miku,
            last_tab_poll:  Instant::now(),
            last_disk_poll: Instant::now(),
            last_editor_poll: Instant::now(),
            show_intro:       true,
            intro_started_at: None,
            intro_log:        Vec::new(),
            intro_revealed:   0,
            intro_done:       false,
            intro_done_at:    None,
            pasted_log_input:     String::new(),
            pasted_log_diagnosis: Vec::new(),
        };
        app.intro_log = app.build_intro_log();
        // Dashboard is the default tab, so it never goes through
        // `switch_tab`'s "populate pc_check_items on entry" path — populate
        // it directly here instead, otherwise Preflight Diagnostics starts
        // empty until the user manually switches tabs and back.
        app.refresh_pc_check();
        // Load the package config and scan past builds for whatever project
        // was already set. These used to happen on first entry to the Package
        // tab; with one surface there is no such entry point, so the run
        // surface would otherwise open with empty names and an empty history.
        if app.project_path.is_some() {
            app.open_package_config();
            app.refresh_builds();
        }
        ops_update::cleanup_old_binary();
        app.check_for_updates(cc.egui_ctx.clone());
        #[cfg(debug_assertions)]
        app.demo_outcome();
        app
    }

    /// Debug builds only: `UDT_DEMO=ok|failed` opens straight onto a finished
    /// build, so the result surfaces can be looked at without a 30-minute run.
    #[cfg(debug_assertions)]
    fn demo_outcome(&mut self) {
        let Ok(mode) = std::env::var("UDT_DEMO") else { return };
        self.show_intro = false;
        // Screens that are not a build result: open straight onto them.
        match mode.as_str() {
            "ready" => return,
            "monitor" => { self.open_sheet(crate::types::Sheet::Monitor); return; }
            "checks"  => { self.open_sheet(crate::types::Sheet::Diagnostics); return; }
            "extras"  => { self.open_sheet(crate::types::Sheet::Extras); return; }
            "palette" => { self.palette_open = true; return; }
            "tools" | "tools-launch" | "tools-plugins" | "tools-content" | "tools-engines" | "tools-compare" | "tools-run" | "tools-cheat" => {
                use crate::ui::tools::ToolsTab;
                self.tools.tab = match mode.as_str() {
                    "tools-launch" => ToolsTab::Launch,
                    "tools-plugins" => ToolsTab::Plugins,
                    "tools-content" => ToolsTab::Content,
                    "tools-engines" => ToolsTab::Engines,
                    "tools-cheat" => ToolsTab::Cheatsheet,
                    _ => ToolsTab::Commandlets,
                };
                if mode == "tools-run"
                    && let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()) {
                    let log = dir.join("Saved").join("Logs").join("DevTool_Validate_data.log");
                    if log.is_file() {
                        self.tools.run = Some(std::sync::Arc::new(std::sync::Mutex::new(
                            crate::ops::tools::ToolRun::finished_from_log("Validate data", log, 1))));
                    }
                }
                if mode == "tools-compare" {
                    self.tools.tab = ToolsTab::Content;
                    if let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()) {
                        let recs = crate::ops::history::scan(dir);
                        if let (Some(n), Some(o)) = (recs.first(), recs.get(1)) {
                            let pack = self.pack_name_input.trim();
                            let root = |b: &crate::ops::history::BuildRecord| {
                                let d = b.dir.join(pack);
                                if d.is_dir() { d } else { b.dir.clone() }
                            };
                            let c = crate::ops::insights::compare(&root(o), &root(n), crate::ops::insights::DEFAULT_BUDGET);
                            self.tools.comparison = Some((o.version.clone(), n.version.clone(), c));
                        }
                    }
                }
                if mode == "tools-content"
                    && let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()) {
                    self.tools.insights = Some(crate::ops::insights::scan(
                        &dir.join("Content"), crate::ops::insights::DEFAULT_BUDGET));
                }
                self.open_sheet(crate::types::Sheet::Tools);
                return;
            }
            "guide"   => { self.open_guide(); self.guide_step = crate::ui::guide::step::CONFIGURE; return; }
            _ => {}
        }
        if mode == "running" {
            use crate::ops::run::{Level, LogLine, Stage};
            *self.is_working.lock().unwrap() = true;
            self.was_working = true;
            self.task_started_at = Some(Instant::now() - std::time::Duration::from_secs(754));
            let mut r = self.run.lock().unwrap();
            let now = Instant::now();
            r.current = Some(Stage::Cook);
            r.started = [Some(now - std::time::Duration::from_secs(754)),
                         Some(now - std::time::Duration::from_secs(600)), None, None];
            r.elapsed[0] = Some(std::time::Duration::from_secs(150));
            r.warnings = 12;
            r.errors = 1;
            for (i, (t, l)) in [
                ("LogCook: Display: Cooked packages 4210 Packages Remain 1730", Level::Normal),
                ("LogShaderCompilers: Warning: Shader compile slow on worker 3", Level::Warn),
                ("LogCook: Error: Failed to cook /Game/Old/Unused.uasset", Level::Error),
                ("LogCook: Display: Cooked packages 4300 Packages Remain 1640", Level::Normal),
            ].into_iter().enumerate() {
                let _ = i;
                r.lines.push_back(LogLine { text: t.into(), level: l });
            }
            return;
        }
        let ok = mode == "ok" || mode == "android";
        self.last_build = Some(crate::types::BuildOutcome {
            version: "v0.0.11".into(),
            zip: ok.then(|| PathBuf::from("Q:/Demo/build/v0.0.11/Demo_v0.0.11.zip")),
            bytes: 668 * 1024 * 1024,
            duration: std::time::Duration::from_secs(692),
            stages: [Some(std::time::Duration::from_secs(120)), Some(std::time::Duration::from_secs(400)),
                     Some(std::time::Duration::from_secs(60)), Some(std::time::Duration::from_secs(112))],
            warnings: 4,
            errors: if ok { 0 } else { 3 },
            ok,
            platform: if mode == "android" { BuildTarget::Android } else { self.build_target },
            config: self.build_configuration,
            log: None,
            error_samples: if ok { Vec::new() } else { vec![
                "LogInit: Error: Failed to load /Game/Maps/Main.umap".into(),
                "fatal error C1083: Cannot open include file: 'MobiusFish.h'".into(),
            ] },
        });
    }

    /// Boot-log lines for the intro splash — built from what was actually
    /// detected at startup (not placeholder text), so the "boot sequence"
    /// reflects your real project/engine/git state.
    fn build_intro_log(&self) -> Vec<String> {
        let mut log = vec!["Initializing Unreal DevTool...".to_string()];

        match &self.project_path {
            Some(p) => log.push(format!(
                "Found project: '{}'",
                p.file_name().unwrap_or_default().to_string_lossy()
            )),
            None => log.push("No project set yet — pick one from the Dashboard.".to_string()),
        }

        match &self.engine_dir {
            Some(e) => {
                log.push(format!("Engine located: {}", e.display()));
                if crate::ops::preflight::has_space(e) {
                    log.push("WARNING: space detected in engine path — space-free fix available.".to_string());
                }
            }
            None => log.push("WARNING: engine not auto-detected — set it manually from the Dashboard.".to_string()),
        }

        log.push("Checking local disk space...".to_string());
        log.push("Ready for Dev Assistant — checking for Ollama / LM Studio...".to_string());
        log.push("Unreal DevTool loaded successfully. Welcome back.".to_string());
        log
    }

    /// Advances the intro's line-by-line reveal. Called every frame while
    /// `show_intro` is true; paces itself off wall-clock time rather than
    /// frame count so it's consistent regardless of frame rate.
    pub fn tick_intro(&mut self, ctx: &egui::Context) {
        let started = *self.intro_started_at.get_or_insert_with(Instant::now);
        const LINE_INTERVAL_MS: u128 = 220;
        let elapsed_ms = started.elapsed().as_millis();
        let target = ((elapsed_ms / LINE_INTERVAL_MS) as usize + 1).min(self.intro_log.len());
        if target > self.intro_revealed {
            self.intro_revealed = target;
        }
        if self.intro_revealed >= self.intro_log.len() {
            // Record *when* the log finished, so the hand-off below can hold
            // the completed state briefly instead of cutting away the instant
            // the last line lands.
            if self.intro_done_at.is_none() {
                self.intro_done_at = Some(Instant::now());
            }
            self.intro_done = true;
        }

        // Auto-advance. There used to be an "OPEN DEVTOOL" button here, which
        // made the splash a dead end waiting on a click that only ever had one
        // possible answer. It now dismisses itself: a short hold so the final
        // line is readable, then a fade the intro screen itself renders (see
        // `intro_fade`), and the main UI takes over.
        if let Some(done_at) = self.intro_done_at {
            let held = done_at.elapsed().as_secs_f32();
            if held >= INTRO_HOLD_SECS + INTRO_FADE_SECS {
                self.show_intro = false;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    /// Opacity for the whole intro screen: 1.0 until the log finishes and the
    /// hold elapses, then eased down to 0 over `INTRO_FADE_SECS`.
    pub fn intro_fade(&self) -> f32 {
        let Some(done_at) = self.intro_done_at else { return 1.0 };
        let held = done_at.elapsed().as_secs_f32();
        if held <= INTRO_HOLD_SECS {
            return 1.0;
        }
        let t = ((held - INTRO_HOLD_SECS) / INTRO_FADE_SECS).clamp(0.0, 1.0);
        // Ease-in-cubic on the way out — it lingers, then leaves quickly,
        // which reads as a deliberate hand-off rather than a dropped frame.
        1.0 - t * t * t
    }

    // ── Shared helpers ────────────────────────────────────────────────────────

    pub fn try_apply_typed_path(&mut self) {
        let trimmed = self.project_path_input.trim().to_string();
        if trimmed.is_empty() { return; }
        let p = std::path::PathBuf::from(&trimmed);
        if p.exists() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("uproject")) {
            save_project_path(&p);
            self.project_path = Some(p);
            self.redetect_engine();
        } else {
            self.set_status("[ERROR] Select an existing .uproject file.".into());
            if let Some(current) = &self.project_path {
                self.project_path_input = current.to_string_lossy().to_string();
            }
        }
    }

    pub fn set_status(&self, msg: String) {
        *self.status_message.lock().unwrap_or_else(|e| e.into_inner()) = msg;
    }

    pub fn refresh_status(&self) {
        self.set_status(build_init_status(&self.engine_dir, &self.project_path));
    }

    /// Re-runs engine detection against the current project and updates
    /// `engine_dir`. Call whenever the project path changes. A valid manual
    /// override always wins over auto-detection.
    pub fn redetect_engine(&mut self) {
        self.engine_dir = self.engine_override.clone()
            .filter(|p| is_valid_engine_dir(p))
            .or_else(|| detect_unreal_engine(self.project_path.as_deref()));
        self.refresh_status();
        // Every caller of this function just changed which engine/project
        // the app is pointed at (typing a new project path, Browse…, Clear
        // override, Auto-detect) — without this, the Dashboard's PREFLIGHT
        // DIAGNOSTICS card kept showing check results for whatever
        // engine/project was active BEFORE the change until the user
        // manually flipped tabs away and back to Dashboard. `refresh_pc_check`
        // is already fully backgrounded for its one slow part (the
        // disk-space PowerShell spawn), and every call site of
        // `redetect_engine` is a discrete user action (a button click or a
        // typed-path commit), never per-frame, so there's no spam risk.
        self.refresh_pc_check();
    }

    /// Accepts `path` as the active project: persists it, records it in the
    /// recents list, re-detects the engine and refreshes everything derived
    /// from it. One entry point, so no caller has to remember the full set.
    pub fn apply_project_path(&mut self, path: PathBuf) {
        if !path.is_file() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("uproject")) {
            self.set_status("[ERROR] Select an existing .uproject file.".into());
            return;
        }
        save_project_path(&path);
        crate::config::push_recent_project(&path);
        self.project_path_input = path.to_string_lossy().to_string();
        self.project_path       = Some(path);
        self.redetect_engine();
        self.open_package_config();
        self.refresh_pc_check();
        self.refresh_builds();
        if let Some(dir) = self.git_project_dir() {
            self.refresh_git_status_async(dir);
        }
    }

    /// Native picker for a `.uproject`.
    pub fn choose_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Unreal Project", &["uproject"])
            .set_title("Select your .uproject file")
            .pick_file()
        else { return };
        self.apply_project_path(path);
    }

    /// Lets the user manually point at their Unreal Engine install folder —
    /// the escape hatch for "[ERROR] Engine not found." when auto-detection
    /// (registry / EngineAssociation) can't locate it, e.g. a source build or
    /// a non-standard install path. Validates the folder before accepting it.
    pub fn choose_engine_dir(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select your Unreal Engine installation folder (e.g. .../UE_5.4)")
            .pick_folder()
        else { return };

        if !is_valid_engine_dir(&path) {
            self.set_status(format!(
                "[ERROR] Not a valid engine folder — expected to find Engine\\Build\\BatchFiles\\RunUAT.bat under {}",
                path.display()
            ));
            return;
        }

        save_engine_path(&path);
        self.engine_override = Some(path);
        self.redetect_engine();
        self.set_status(format!("[OK] Engine set: {}", self.engine_dir.as_ref().unwrap().display()));
    }

    /// Drops the manual engine override and falls back to auto-detection.
    pub fn clear_engine_override(&mut self) {
        clear_engine_path();
        self.engine_override = None;
        self.redetect_engine();
    }

    // ── App self-check ───────────────────────────────────────────────────────

    pub fn refresh_app_check(&mut self) {
        self.app_check_items = crate::ops::selfcheck::run_checks();

        *self.app_check_github.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let slot = Arc::clone(&self.app_check_github);
        let ctx  = self.egui_ctx.clone();
        thread::spawn(move || {
            let item = crate::ops::selfcheck::github_reachable();
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(item);
            ctx.request_repaint();
        });
    }

    /// Manual fallback for the "Leftover update file" warning — automatic
    /// cleanup (`ops::update::cleanup_old_binary`) already retries this on
    /// every startup, but this lets the user clear it immediately without
    /// restarting if it's still showing up.
    pub fn cleanup_leftover_binary_now(&mut self) {
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent() {
                let _ = std::fs::remove_file(dir.join("unreal_devtool_old.exe"));
            }
        self.refresh_app_check();
    }

    // ── Custom quick links ────────────────────────────────────────────────────

    pub fn save_links(&mut self) {
        save_custom_links(&self.custom_links);
    }

    pub fn add_custom_link(&mut self) {
        self.custom_links.push(CustomLink { label: "New Link".into(), url: String::new() });
        self.save_links();
    }

    pub fn remove_custom_link(&mut self, index: usize) {
        if index < self.custom_links.len() {
            self.custom_links.remove(index);
            self.save_links();
        }
    }

    /// Switches the active tab, running whatever one-time setup that tab's
    /// content needs (mirrors what the old button-triggered `open_*` methods
    /// did before there were tabs to switch between instead).
    /// Which state the main surface is in. Derived, never set directly — the
    /// surface follows the work rather than a selection.
    pub fn run_state(&self) -> RunState {
        if *self.is_working.lock().unwrap_or_else(|e| e.into_inner()) {
            RunState::Running
        } else if self.project_path.is_none() {
            RunState::Setup
        } else if self.last_build.is_some() {
            RunState::Done
        } else {
            RunState::Ready
        }
    }

    /// Re-reads the project's config for packaging-readiness problems. A few
    /// small files, so it runs inline.
    pub fn refresh_doctor(&mut self) {
        self.doctor_items = match &self.project_path {
            Some(p) => crate::ops::doctor::check_project(p, self.build_target),
            None    => Vec::new(),
        };
    }

    /// Recomputes the one-click fixes (see `doctor_fixes`).
    pub fn refresh_doctor_fixes(&mut self) {
        self.doctor_fixes = match &self.project_path {
            Some(p) => crate::ops::doctor::fixes(p),
            None    => Vec::new(),
        };
    }

    /// Opens a sheet, doing whatever one-time refresh it needs on entry.
    pub fn open_sheet(&mut self, sheet: Sheet) {
        self.sheet = Some(sheet);
        match sheet {
            Sheet::Chat        => self.open_chat_panel(),
            Sheet::Diagnostics => {
                self.refresh_doctor();
                self.refresh_doctor_fixes();
                self.refresh_pc_check();
                self.refresh_clean_targets();
            }
            Sheet::Extras      => {
                if self.extras_tab == ExtrasTab::SelfCheck { self.refresh_app_check(); }
            }
            Sheet::Browser | Sheet::Settings | Sheet::Monitor | Sheet::Tools => {}
        }
    }

    pub fn close_sheet(&mut self) {
        self.sheet = None;
    }

    /// Measures the removable folders under the project.
    ///
    /// Blocking on the caller's thread would stat tens of thousands of files,
    /// so it runs in the background like every other disk walk here.
    pub fn refresh_clean_targets(&mut self) {
        let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf())
        else { return };
        let out = Arc::clone(&self.clean_pending);
        let ctx = self.egui_ctx.clone();
        thread::spawn(move || {
            let found = crate::ops::clean::scan(&dir);
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
            ctx.request_repaint();
        });
    }

    /// Deletes the ticked folders.
    pub fn start_clean(&mut self) {
        let paths: Vec<std::path::PathBuf> = self.clean_targets.iter()
            .zip(self.clean_selected.iter())
            .filter(|(t, on)| **on && t.exists)
            .map(|(t, _)| t.path.clone())
            .collect();
        if paths.is_empty() { return; }
        self.clean_confirm = false;
        self.busy_label = "Cleaning project…".into();
        self.run_background_task("[INFO] Removing generated folders…", move || {
            crate::ops::clean::remove(&paths)
        });
    }

    /// Rescans `<project>/build/` on a background thread.
    pub fn refresh_builds(&mut self) {
        let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf())
        else { return };
        let out = Arc::clone(&self.builds_pending);
        let ctx = self.egui_ctx.clone();
        thread::spawn(move || {
            let found = crate::ops::history::scan(&dir);
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
            ctx.request_repaint();
        });
    }

    /// Clears the finished-build result, returning the surface to `Ready`.
    pub fn dismiss_build(&mut self) {
        self.last_build = None;
        self.run.lock().unwrap_or_else(|e| e.into_inner()).reset();
    }


    /// Scans a pasted log excerpt (Dashboard tab) against the same known-error
    /// table as the on-disk build-log scanner — for when the relevant log
    /// isn't the most recent one on disk, or came from somewhere else.
    pub fn scan_pasted_log(&mut self) {
        self.pasted_log_diagnosis = crate::ops::diagnostics::scan_build_log_text(&self.pasted_log_input);
    }

    // ── PC / environment pre-flight ──────────────────────────────────────────

    pub fn refresh_pc_check(&mut self) {
        self.refresh_pc_check_cheap();
        self.refresh_pc_check_disk_async();
    }

    /// Re-runs the preflight checks and the build-log scan on a worker.
    ///
    /// The name is a misnomer kept for its callers: only `run_checks` is
    /// cheap. The build-log scan opens the newest UAT log — routinely tens of
    /// megabytes — and matches every line against the known-error table. That
    /// used to run inline here, and the periodic poll called it every three
    /// seconds, so the render thread stalled for the length of the read over
    /// and over for as long as the app stayed open. It is now off-thread and
    /// the result is drained in `pump_background_state`.
    ///
    /// Still split from `refresh_pc_check_disk_async`, which needs a much
    /// longer leash because it spawns PowerShell.
    pub fn refresh_pc_check_cheap(&mut self) {
        let engine  = self.engine_dir.clone();
        let project = self.project_path.clone();
        let out     = Arc::clone(&self.pc_check_pending);
        let ctx     = self.egui_ctx.clone();
        thread::spawn(move || {
            let items = crate::ops::preflight::run_checks(&engine, &project);
            let (log, diags) = match project.as_ref()
                .and_then(|p| crate::ops::diagnostics::latest_build_log(p))
            {
                Some(log) => {
                    let d = crate::ops::diagnostics::scan_build_log(&log);
                    (Some(log), d)
                }
                None => (None, Vec::new()),
            };
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some((items, log, diags));
            ctx.request_repaint();
        });
    }

    /// The disk-space half of `refresh_pc_check`. Needs a PowerShell spawn
    /// (slow, cold-start overhead) — running that on the UI thread would
    /// freeze the window until it returns, so it goes on a background
    /// thread like every other slow operation in this app.
    ///
    /// Kept separate from `refresh_pc_check_cheap` so callers that only
    /// need the cheap checks refreshed (the periodic Dashboard poll) don't
    /// also pay for a fresh PowerShell spawn on every tick — free disk
    /// space essentially never changes meaningfully within a few seconds,
    /// so spawning PowerShell that often would be pure waste. The poll
    /// instead calls this on its own, much longer interval, gated by
    /// `last_disk_poll`; that timer is reset *here* (not at the poll call
    /// site) so it stays correct no matter which caller — tab entry, the
    /// manual "Refresh" button, or the periodic poll — triggered this
    /// particular check.
    pub fn refresh_pc_check_disk_async(&mut self) {
        self.last_disk_poll = Instant::now();
        *self.pc_check_disk.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf()) {
            let slot = Arc::clone(&self.pc_check_disk);
            let ctx  = self.egui_ctx.clone();
            thread::spawn(move || {
                let item = crate::ops::preflight::disk_space_check_item(&dir);
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(item.unwrap_or(crate::ops::preflight::CheckItem {
                    status: crate::ops::preflight::CheckStatus::Warn,
                    label:  "Disk space".into(),
                    detail: "Could not determine free disk space.".into(),
                }));
                ctx.request_repaint();
            });
        }
    }

    /// One-click fix for the "UAT breaks on spaces in paths" issue: aliases
    /// the engine and/or project folder to a space-free directory junction
    /// and remembers to route packaging through it from now on. Junctions
    /// don't touch the real files — they're just an alternate, space-free
    /// path to the same folder.
    pub fn apply_space_free_fix(&mut self) {
        if let Some(engine) = self.engine_dir.clone()
            && crate::ops::preflight::has_space(&engine)
            && let Err(e) = crate::ops::preflight::ensure_space_free_alias(&engine)
        {
            self.set_status(format!("[ERROR] {e}"));
            return;
        }
        if let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf())
            && crate::ops::preflight::has_space(&dir)
            && let Err(e) = crate::ops::preflight::ensure_space_free_alias(&dir)
        {
            self.set_status(format!("[ERROR] {e}"));
            return;
        }
        self.use_space_free_link = true;
        self.set_status("[OK] Space-free link ready — packaging will route through it from now on.".into());
    }

    pub fn git_project_dir(&self) -> Option<PathBuf> {
        self.project_path.as_ref()?.parent().map(|p| p.to_path_buf())
    }

    /// Centers the window on its monitor — once, at startup only. Windows
    /// doesn't center new windows by default (it cascades them, or eframe
    /// restores whatever position was persisted from the last run). This
    /// intentionally does NOT run again on later resizes or moves: an
    /// earlier version re-centered on every size change, which fought the
    /// user's own attempts to drag the window (it would snap back and
    /// flicker) — so after this first call, the window is fully free to be
    /// moved and resized without any interference.
    pub fn center_window_on_startup(&mut self, ctx: &egui::Context) {
        if self.has_centered_window { return; }
        let (outer_rect, monitor_size) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.outer_rect, vp.monitor_size)
        });
        let (Some(outer_rect), Some(monitor_size)) = (outer_rect, monitor_size) else { return };
        self.has_centered_window = true;
        let size = outer_rect.size();
        let pos = egui::pos2(
            ((monitor_size.x - size.x) / 2.0).max(0.0),
            ((monitor_size.y - size.y) / 2.0).max(0.0),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
    }

    // ── Self-update ───────────────────────────────────────────────────────────

    /// Ask GitHub for the latest release in the background; updates `update_info`
    /// (read by the UI) if a newer build is available, then requests a repaint
    /// so the banner appears immediately without waiting for user input.
    pub fn check_for_updates(&mut self, ctx: egui::Context) {
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let update_info = Arc::clone(&self.update_info);
        self.last_update_check = Instant::now();
        thread::spawn(move || {
            if let Ok(Some(info)) = ops_update::check_for_update(&current_version) {
                *update_info.lock().unwrap() = Some(info);
                ctx.request_repaint();
            }
        });
    }

    /// Download the latest release exe, swap it in for the running one, and
    /// relaunch. On success the app exits; on failure the error is reported
    /// in the status area.
    pub fn start_update_install(&mut self, info: UpdateInfo) {
        self.show_update_banner = false;
        self.busy_label = "[ DOWNLOADING UPDATE ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status   = Arc::clone(&self.status_message);
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        // Takes the whole `UpdateInfo` rather than just the download URL so
        // the checksum sidecar published next to the artifact travels with it
        // — see `ops::update::download_and_install` for why it is verified.
        self.run_background_task("Downloading update…", move || {
            match ops_update::download_and_install(
                &info.download_url,
                info.checksum_url.as_deref(),
                &status, &cancel, &progress,
            ) {
                Ok(())   => std::process::exit(0),
                Err(e)   => format!("[ERROR] Update failed: {e}"),
            }
        });
    }

    // ── Packaging-sound controls ──────────────────────────────────────────────

    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_muted = muted;
        if let Some(a) = &mut self.audio_player { a.set_muted(muted); }
        save_audio_config(&AudioConfig { muted: self.audio_muted, volume: self.audio_volume });
    }

    pub fn set_audio_volume(&mut self, volume: u32) {
        self.audio_volume = volume;
        if let Some(a) = &mut self.audio_player { a.set_volume(volume); }
        save_audio_config(&AudioConfig { muted: self.audio_muted, volume: self.audio_volume });
    }

    // ── Custom media (2D image/GIF + looping sound) ───────────────────────────

    fn current_media_config(&self) -> MediaConfig {
        MediaConfig {
            gif_path:   self.custom_gif_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            sound_path: self.custom_sound_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        }
    }

    pub fn choose_custom_gif(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image / GIF", &["gif", "png", "jpg", "jpeg", "bmp", "webp"])
            .set_title("Select a 2D image or GIF")
            .pick_file()
        else { return };

        match GifPlayer::from_file(&path) {
            Some(player) => {
                self.gif_player = Some(player);
                self.custom_gif_path = Some(path);
                save_media_config(&self.current_media_config());
                self.set_status("[OK] Custom image/GIF loaded.".into());
            }
            None => self.set_status("[ERROR] Could not load that image/GIF.".into()),
        }
    }

    pub fn reset_gif_to_default(&mut self) {
        self.gif_player = GifPlayer::from_bytes(include_bytes!("../Image/miku-hatsune.gif"));
        self.custom_gif_path = None;
        save_media_config(&self.current_media_config());
        self.set_status("[OK] Restored default Miku GIF.".into());
    }

    pub fn choose_custom_sound(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["mp3", "wav"])
            .set_title("Select a looping sound")
            .pick_file()
        else { return };

        match std::fs::read(&path) {
            Ok(bytes) => {
                if let Some(a) = &mut self.audio_player { a.set_source(bytes); }
                self.custom_sound_path = Some(path);
                save_media_config(&self.current_media_config());
                self.set_status("[OK] Custom sound loaded.".into());
            }
            Err(e) => self.set_status(format!("[ERROR] Could not read sound file: {e}")),
        }
    }

    pub fn reset_sound_to_default(&mut self) {
        if let Some(a) = &mut self.audio_player {
            a.set_source(include_bytes!("../Sound/Ievan Polkka.mp3").to_vec());
        }
        self.custom_sound_path = None;
        save_media_config(&self.current_media_config());
        self.set_status("[OK] Restored default sound.".into());
    }

    // ── Package actions ───────────────────────────────────────────────────────

    pub fn open_package_config(&mut self) {
        let project_path = match &self.project_path { Some(p) => p.clone(), None => return };
        // User-owned fields: loaded fresh from disk on tab entry. This is a
        // *load*, not a background refresh — it only ever runs here (and on
        // the post-merge auto-open), never on a timer — so it can never
        // clobber an in-progress edit the way calling this whole function
        // on a timer would. See `refresh_package_observed` below for the
        // half of this that's safe to re-run periodically.
        let (pack, exe, configuration, target) = load_project_config(&project_path);
        self.pack_name_input = pack;
        self.exe_name_input  = exe;
        self.build_configuration = configuration;
        self.build_target = target;
        let (compress, extra, method) = crate::config::load_uat_options(&project_path);
        self.compress_pak = compress;
        self.extra_uat_args = extra;
        self.package_method = method;
        self.size_budget_mb = crate::config::load_size_budget(&project_path);
        self.size_budget_input = if self.size_budget_mb > 0 { self.size_budget_mb.to_string() } else { String::new() };

        self.refresh_package_observed();
        // Default the editable version field to the next auto-incremented
        // version; the user can tick "Custom" to keep/change it. Seeded
        // from whatever `next_version_preview` holds *right now* — which
        // `refresh_package_observed` just kicked off a background
        // recompute of, so this can momentarily be one frame stale (same
        // tradeoff already accepted throughout this file, e.g.
        // `editor_is_running` below). The visible auto-version label
        // itself doesn't have this problem — it re-derives from
        // `next_version_preview` fresh every frame (see
        // `show_package_config_panel`), so it corrects on its own the
        // instant the background check lands.
        self.version_override   = ops_package::format_version(self.next_version_preview);
        self.use_custom_version  = false;
        self.show_vs_config      = false;
        self.git_state           = GitState::Idle;
    }

    /// Recomputes ONLY the *observed* half of the Package tab's state — the
    /// auto-incremented version preview and whether the editor is currently
    /// running. Deliberately split out of `open_package_config`, which also
    /// (re)loads the *user-owned* fields (pack/exe name, build
    /// configuration) from disk: calling `open_package_config` on a timer
    /// would silently overwrite whatever the user is mid-typing every time
    /// it fired. Calling this instead is safe on a timer because it never
    /// touches `pack_name_input`, `exe_name_input`, `version_override`,
    /// `use_custom_version`, or `build_configuration`.
    ///
    /// This is also the actual fix for "packaging a second time fails with
    /// a cryptic rename error": `next_version_preview` used to be set only
    /// when the Package tab was *entered* (inside `open_package_config`),
    /// so a build that finished while the user stayed on the tab kept
    /// showing the version that was JUST built. Starting a second package
    /// run then reused that same `build/v0.0.X/` folder, and
    /// `fs::rename(&uat_windows, &target)` in `ops::package::package_game`
    /// failed because the target already existed — only discovered after
    /// the user waited out a full ~12 minute build. Called from `update()`'s
    /// `just_finished` handling (so the version bumps the instant a package
    /// completes) and from the Package tab's periodic poll while the tab
    /// stays open (see `update()`'s tick block), in addition to
    /// `open_package_config` above.
    pub fn refresh_package_observed(&mut self) {
        self.refresh_package_observed_version_only();
        self.refresh_editor_check_async();
    }

    /// Just the version-preview half of [`refresh_package_observed`]. The
    /// periodic Package-tab poll calls this every tick and throttles the
    /// editor check separately, since only this half is cheap enough to run
    /// at that cadence (see `refresh_editor_check_async`'s doc comment).
    pub fn refresh_package_observed_version_only(&mut self) {
        let Some(project_path) = self.project_path.clone() else { return };
        let build_dir = project_path.parent()
            .map(|p| p.join("build"))
            .unwrap_or_default();

        // `find_next_version` does a `read_dir` over the build folder.
        // Cheap in isolation, but this method is now also called from a
        // timer tick (every ~2s while the Package tab is open) — running
        // it synchronously on the UI thread would stall the window for
        // that long on *every* tick instead of just once on tab entry.
        // Same background-thread-plus-pending-slot pattern as
        // `editor_check_pending` right below (and documented on
        // `version_check_pending`'s field comment): computed off-thread,
        // drained into `next_version_preview` at the top of `update()`.
        *self.version_check_pending.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let version_slot = Arc::clone(&self.version_check_pending);
        let version_ctx  = self.egui_ctx.clone();
        thread::spawn(move || {
            let next = ops_package::find_next_version(&build_dir);
            *version_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(next);
            version_ctx.request_repaint();
        });
    }

    /// The "is the Unreal Editor open?" half of the Package tab's observed
    /// state, split out of [`refresh_package_observed`] for the same reason
    /// [`refresh_pc_check_disk_async`] is split out of the Dashboard's
    /// checks: it is drastically more expensive than what it sits next to,
    /// and needs nothing like the same refresh rate.
    ///
    /// `is_editor_running()` shells out to `tasklist` — and it checks two
    /// executable names with a short-circuiting `any()`, so in the common
    /// case (no editor open, which is exactly when the user is sitting here
    /// configuring a build) it spawns *two* processes, not one. On the
    /// Package tab's 2s poll that worked out to ~60 process spawns a
    /// minute, every one of them scanned by real-time AV — the same kind of
    /// spawn contention this app has already been bitten by elsewhere.
    ///
    /// Opening or closing the editor is a rare, deliberate user action, so
    /// a 10s floor is imperceptible where a 2s one was wasteful. As with
    /// `last_disk_poll`, the timer is reset *here* rather than at the poll
    /// call site, so it stays correct regardless of which caller (tab
    /// entry, task completion, or the periodic poll) triggered it.
    ///
    /// Runs in the background either way: spawning a process on the UI
    /// thread stalls the whole window until it returns, and
    /// `editor_is_running` simply keeps its previous value until the result
    /// lands (drained in `update()`).
    pub fn refresh_editor_check_async(&mut self) {
        self.last_editor_poll = Instant::now();
        *self.editor_check_pending.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let editor_slot = Arc::clone(&self.editor_check_pending);
        let editor_ctx  = self.egui_ctx.clone();
        thread::spawn(move || {
            *editor_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(ops_package::is_editor_running());
            editor_ctx.request_repaint();
        });
    }

    pub fn start_packaging(&mut self) {
        let project_path = match self.project_path.clone() { Some(p) => p, None => return };
        if !project_path.is_file() {
            self.set_status(format!("[ERROR] Project file not found: {}", project_path.display()));
            return;
        }
        let engine_dir   = match self.engine_dir.clone() {
            Some(e) => e,
            None    => {
                self.set_status("[ERROR] Engine not found.".into());
                return;
            }
        };
        if !is_valid_engine_dir(&engine_dir) {
            self.set_status(format!("[ERROR] Invalid Unreal Engine folder: {}", engine_dir.display()));
            return;
        }
        let pack_name = self.pack_name_input.trim().to_string();
        let exe_name  = self.exe_name_input.trim().to_string();
        if let Err(e) = ops_package::validate_leaf_name(&self.pack_name_input, "Package name") {
            self.set_status(format!("[ERROR] {e}"));
            return;
        }
        if let Err(e) = ops_package::validate_leaf_name(&self.exe_name_input, "Executable name") {
            self.set_status(format!("[ERROR] {e}"));
            return;
        }
        let version_str = if self.use_custom_version {
            self.version_override.trim().to_string()
        } else {
            ops_package::format_version(self.next_version_preview)
        };
        if let Err(e) = ops_package::validate_leaf_name(&version_str, "Version") {
            self.set_status(format!("[ERROR] {e}"));
            return;
        }
        let build_configuration = self.build_configuration;
        let build_target        = self.build_target;
        save_project_config(&project_path, &pack_name, &exe_name, build_configuration, build_target);
        self.task_started_at    = Some(Instant::now());
        self.busy_label = "[ PACKAGING IN PROGRESS ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        if let Some(a) = &mut self.audio_player { a.play_looping(); }
        let status_clone  = Arc::clone(&self.status_message);
        let pending_clone = Arc::clone(&self.pending_zip);
        let cancel        = Arc::clone(&self.cancel_flag);
        let progress      = Arc::clone(&self.progress);
        let run           = Arc::clone(&self.run);
        let iterate       = self.iterate_cook;
        let package_method = self.package_method;
        let mut extra_args = match ops_package::parse_extra_uat_args(&self.extra_uat_args) {
            Ok(v)  => v,
            Err(e) => {
                self.set_status(format!("[ERROR] Extra UAT arguments: {e}"));
                return;
            }
        };
        if self.compress_pak && !extra_args.iter().any(|a| a == "-compressed") {
            extra_args.insert(0, "-compressed".into());
        }
        // A fresh run starts from zero stages and an empty log, and the
        // previous result stops being the thing on screen.
        {
            let mut r = run.lock().unwrap_or_else(|e| e.into_inner());
            r.reset();
            if let Some(secs) = crate::ops::history::recent_stage_secs(&self.builds) {
                r.seed_typical(secs);
            }
        }
        self.last_build = None;
        let use_space_free_link = self.use_space_free_link;
        self.run_background_task("Starting UAT pipeline…", move || {
            ops_package::package_game(project_path, engine_dir, pack_name, exe_name, version_str, build_configuration, build_target, status_clone, pending_clone, cancel, progress, run, iterate, use_space_free_link, extra_args, package_method)
        });
    }

    pub fn start_upload(&mut self) {
        let zip = self.upload_zip_path.clone();
        if !zip.exists() {
            self.set_status(format!("[ERROR] Zip not found: {}", zip.display()));
            self.show_upload_panel = false;
            return;
        }

        save_upload_config(&UploadConfig {
            local_path:  self.upload_local_path.clone(),
            rclone_dest: self.upload_rclone_dest.clone(),
        });

        let use_local   = self.upload_use_local;
        let use_gdrive  = self.upload_use_gdrive;
        let local_path  = self.upload_local_path.clone();
        let rclone_dest = self.upload_rclone_dest.clone();
        let status      = Arc::clone(&self.status_message);
        let cancel      = Arc::clone(&self.cancel_flag);
        let progress    = Arc::clone(&self.progress);

        *self.gdrive_upload_failed.lock().unwrap_or_else(|e| e.into_inner()) = false;
        let gdrive_failed = Arc::clone(&self.gdrive_upload_failed);

        self.show_upload_panel = false;
        self.busy_label = "[ UPLOADING BUILD ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }

        self.run_background_task("Starting upload…", move || {
            let mut parts = Vec::new();
            if use_local {
                if cancel.load(Ordering::Relaxed) { return "[CANCELLED]".to_string(); }
                *progress.lock().unwrap() = 0.2;
                parts.push(ops_package::copy_to_local(&zip, &local_path));
                *progress.lock().unwrap() = if use_gdrive { 0.5 } else { 1.0 };
            }
            if use_gdrive {
                if cancel.load(Ordering::Relaxed) { return "[CANCELLED]".to_string(); }
                *progress.lock().unwrap() = if use_local { 0.5 } else { 0.1 };
                let result = ops_package::upload_via_rclone(&zip, &rclone_dest, &status, &cancel);
                if result.starts_with("[ERROR]") {
                    *gdrive_failed.lock().unwrap_or_else(|e| e.into_inner()) = true;
                }
                parts.push(result);
                *progress.lock().unwrap() = 1.0;
            }
            if parts.is_empty() { return "[DONE] No destination selected — nothing uploaded.".to_string(); }
            parts.join("\n")
        });
    }

    // ── VS-rebuild actions ────────────────────────────────────────────────────

    pub fn open_vs_config(&mut self) {
        self.show_vs_config      = true;
        self.sheet               = None;
        self.show_git_sheet      = false;
        self.git_state           = GitState::Idle;
    }

    pub fn start_vs_rebuild(&mut self) {
        let project_path = match self.project_path.clone() { Some(p) => p, None => return };
        if !project_path.is_file() {
            self.set_status(format!("[ERROR] Project file not found: {}", project_path.display()));
            self.show_vs_config = false;
            return;
        }
        let engine_dir   = match self.engine_dir.clone() {
            Some(e) => e,
            None    => {
                self.set_status("[ERROR] Engine not found.".into());
                self.show_vs_config = false;
                return;
            }
        };
        if !is_valid_engine_dir(&engine_dir) {
            self.set_status(format!("[ERROR] Invalid Unreal Engine folder: {}", engine_dir.display()));
            self.show_vs_config = false;
            return;
        }
        let ide = self.ide_choice;
        self.show_vs_config = false;
        self.busy_label = "[ GENERATING PROJECT FILES ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status_clone = Arc::clone(&self.status_message);
        let cancel       = Arc::clone(&self.cancel_flag);
        let progress     = Arc::clone(&self.progress);
        let use_space_free_link = self.use_space_free_link;
        self.run_background_task("Preparing to regenerate project files…", move || {
            ops_vs::rebuild_vs_files(project_path, engine_dir, ide, status_clone, cancel, progress, use_space_free_link)
        });
    }

    // ── Git actions ───────────────────────────────────────────────────────────

    pub fn open_git_menu(&mut self) {
        self.show_vs_config      = false;
        self.git_commit_msg.clear();
        self.git_new_branch_name.clear();
        match self.git_project_dir() {
            Some(d) => self.refresh_git_status_async(d),
            None => {
                self.git_current_branch = "unknown".into();
                self.git_status         = ops_git::GitStatusSummary::default();
            }
        }
        self.git_state = GitState::Menu;
    }

    /// Kicks off `git_current_branch` + `git_status_summary` on a
    /// background thread; the result is picked up and applied to
    /// `git_current_branch`/`git_status` on the next frame (see `update()`).
    /// Between now and then those two fields keep showing whatever they last
    /// held — stale by at most a frame or two, which is a fine tradeoff for
    /// not blocking the UI thread on up to 7 sequential `git` subprocess
    /// spawns (see `git_refresh_pending`'s doc comment for the full list).
    /// Called from `open_git_menu`, after a git task finishes (`update()`'s
    /// `just_finished` handling), and from the Git tab's periodic poll
    /// (`update()`'s tick block, every ~5s while that tab is open and the
    /// app is idle) — all three just call this the same way, so there's
    /// only one place that actually spawns the refresh.
    pub fn refresh_git_status_async(&mut self, dir: PathBuf) {
        let slot = Arc::clone(&self.git_refresh_pending);
        let ctx  = self.egui_ctx.clone();
        thread::spawn(move || {
            let branch = ops_git::git_current_branch(&dir);
            let status = ops_git::git_status_summary(&dir);
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some((branch, status));
            ctx.request_repaint();
        });
    }

    pub fn git_start_commit_push(&mut self) {
        let dir    = match self.git_project_dir() { Some(d) => d, None => return };
        let msg    = self.git_commit_msg.trim().to_string();
        let branch = self.git_current_branch.clone();
        let status = Arc::clone(&self.status_message);
        let result = Arc::clone(&self.git_result);
        self.git_next_state = GitState::AfterPush;
        self.busy_label     = "[ COMMITTING & PUSHING ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        self.run_background_task("Staging changes…", move || {
            ops_git::task_git_commit_push(dir, msg, branch, status, result, cancel, progress)
        });
    }

    pub fn git_start_sync(&mut self) {
        let dir      = match self.git_project_dir() { Some(d) => d, None => return };
        let status   = Arc::clone(&self.status_message);
        let result   = Arc::clone(&self.git_result);
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "[ SYNCING WITH MAIN ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Fetching origin/main…", move || {
            ops_git::task_git_sync(dir, status, result, cancel, progress)
        });
    }

    pub fn start_merge_and_package(&mut self) {
        self.git_package_after_merge = true;
        self.git_start_merge();
    }

    pub fn git_start_merge(&mut self) {
        let dir         = match self.git_project_dir() { Some(d) => d, None => return };
        let from_branch = self.git_current_branch.clone();
        let status      = Arc::clone(&self.status_message);
        let result      = Arc::clone(&self.git_result);
        let cancel      = Arc::clone(&self.cancel_flag);
        let progress    = Arc::clone(&self.progress);
        self.git_merged_from = from_branch.clone();
        self.git_next_state  = GitState::AfterMerge;
        self.busy_label      = "[ MERGING TO MAIN ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Switching to main…", move || {
            ops_git::task_git_merge_to_main(dir, from_branch, status, result, cancel, progress)
        });
    }

    pub fn git_start_checkout(&mut self, branch: String) {
        let dir      = match self.git_project_dir() { Some(d) => d, None => return };
        let status   = Arc::clone(&self.status_message);
        let result   = Arc::clone(&self.git_result);
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "[ SWITCHING BRANCH ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Switching branch…", move || {
            ops_git::task_git_checkout(dir, branch, status, result, cancel, progress)
        });
    }

    pub fn git_start_new_branch(&mut self, name: String) {
        let dir      = match self.git_project_dir() { Some(d) => d, None => return };
        let status   = Arc::clone(&self.status_message);
        let result   = Arc::clone(&self.git_result);
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "[ CREATING BRANCH ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Creating branch…", move || {
            ops_git::task_git_create_branch(dir, name, status, result, cancel, progress)
        });
    }

    // ── Background task runner ────────────────────────────────────────────────

    pub fn run_background_task<F>(&mut self, start_msg: &str, task: F)
    where
        F: FnOnce() -> String + Send + 'static,
    {
        self.cancel_flag.store(false, Ordering::Relaxed);
        *self.progress.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
        *self.is_working.lock().unwrap_or_else(|e| e.into_inner()) = true;
        *self.status_message.lock().unwrap_or_else(|e| e.into_inner()) = start_msg.to_string();
        let status  = Arc::clone(&self.status_message);
        let working = Arc::clone(&self.is_working);
        let ctx     = self.egui_ctx.clone();
        thread::spawn(move || {
            // catch_unwind prevents a panic inside the task from propagating out
            // of the thread and poisoning the shared Mutexes — a poisoned Mutex
            // would cause every subsequent .lock().unwrap() on the UI thread to
            // panic and crash the whole app.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task))
                .unwrap_or_else(|_| {
                    "[ERROR] Packaging crashed unexpectedly — please try again. \
                     If this keeps happening, check available disk space and that \
                     the engine path is correct.".to_string()
                });
            // Use unwrap_or_else so we can still write through a poisoned mutex
            // (which would happen if we panicked while the lock was held above).
            *status.lock().unwrap_or_else(|e| e.into_inner())  = result;
            *working.lock().unwrap_or_else(|e| e.into_inner()) = false;
            // Wake the UI immediately — without this the busy screen stays up
            // until the user moves the mouse (egui is event-driven / reactive).
            ctx.request_repaint();
        });
    }

    // ── Dev-assistant chat (local LLM) ───────────────────────────────────────

    pub fn open_chat_panel(&mut self) {
        self.show_vs_config = false;
        // Re-scan every time the Chat tab is (re-)entered, not just the
        // first time this session. `chat_providers` is OBSERVED state — a
        // local LLM server can be started or stopped by the user at any
        // point outside this app — but this used to only auto-detect
        // "if empty", so once any provider was found once, a server that
        // was later closed kept showing as available (with a now-dead
        // model list) until the user happened to notice and click the
        // sidebar's manual ↻ button. `detect_chat_providers` already
        // de-dupes overlapping calls via `chat_detecting` and uses a
        // fail-fast 400ms-connect/2s-total timeout on each probe (see
        // `probe_agent`), so re-running it on every entry is cheap and
        // never blocks the UI thread — and since the sidebar keeps
        // rendering the previous list until the refresh lands (see
        // `show_chat_panel_ui`'s `providers.is_empty()` branch), this
        // doesn't cause a loading flicker either.
        self.detect_chat_providers();
    }

    /// Probes Ollama/LM Studio for reachability + available models on a
    /// background thread — even a localhost HTTP call shouldn't block the
    /// UI thread (the common case is "nothing is running", and DNS/socket
    /// setup still costs real time).
    pub fn detect_chat_providers(&mut self) {
        let mut detecting = self.chat_detecting.lock().unwrap_or_else(|e| e.into_inner());
        if *detecting { return; }
        *detecting = true;
        drop(detecting);

        let providers = Arc::clone(&self.chat_providers);
        let detecting = Arc::clone(&self.chat_detecting);
        let ctx       = self.egui_ctx.clone();
        thread::spawn(move || {
            let found = crate::ops::llm::detect_providers();
            *providers.lock().unwrap_or_else(|e| e.into_inner()) = found;
            *detecting.lock().unwrap_or_else(|e| e.into_inner()) = false;
            ctx.request_repaint();
        });
    }

    /// Context the assistant gets on every turn so it can actually help
    /// troubleshoot this project, not just chat in a vacuum.
    fn chat_system_context(&self) -> String {
        format!(
            "You are a helpful assistant embedded in \"Unreal DevTool\", a Windows GUI for \
             packaging and managing an Unreal Engine project. Current context:\n\
             - Engine: {}\n\
             - Project: {}\n\
             - Git branch: {}\n\
             - Last status: {}\n\
             Use this context to help troubleshoot build/packaging/git issues when it's \
             relevant to the question. Keep answers concise.",
            self.engine_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not found".into()),
            self.project_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not set".into()),
            if self.git_current_branch.is_empty() { "unknown" } else { &self.git_current_branch },
            self.status_display.replace('\n', " "),
        )
    }

    pub fn send_chat_message(&mut self) {
        let text = self.chat_input.trim().to_string();
        if text.is_empty() { return; }
        if *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner()) { return; }
        let Some(provider) = self.chat_provider else { return };
        if self.chat_model.is_empty() { return; }

        self.chat_history.push(crate::ops::llm::ChatMessage { role: "user".into(), content: text });
        self.chat_input.clear();

        let mut messages = vec![crate::ops::llm::ChatMessage {
            role: "system".into(), content: self.chat_system_context(),
        }];
        messages.extend(self.chat_history.iter().cloned());

        self.chat_cancel.store(false, Ordering::Relaxed);
        *self.chat_streaming.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
        *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner())      = true;

        let model     = self.chat_model.clone();
        let streaming = Arc::clone(&self.chat_streaming);
        let busy      = Arc::clone(&self.chat_busy);
        let cancel    = Arc::clone(&self.chat_cancel);
        let ctx       = self.egui_ctx.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::ops::llm::stream_chat(provider, &model, &messages, &cancel, |tok| {
                    streaming.lock().unwrap_or_else(|e| e.into_inner()).push_str(tok);
                    ctx.request_repaint();
                })
            }));
            match result {
                Ok(Err(e)) => {
                    let mut s = streaming.lock().unwrap_or_else(|e| e.into_inner());
                    if s.is_empty() { s.push_str(&format!("[ERROR] {e}")); }
                }
                Err(_) => {
                    streaming.lock().unwrap_or_else(|e| e.into_inner())
                        .push_str("\n[ERROR] Chat request crashed unexpectedly.");
                }
                Ok(Ok(())) => {}
            }
            *busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
            ctx.request_repaint();
        });
    }

    pub fn cancel_chat_message(&mut self) {
        self.chat_cancel.store(true, Ordering::Relaxed);
    }
}
