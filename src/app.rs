use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use eframe::egui;

use crate::audio::AudioPlayer;
use crate::config::{
    clear_engine_path, load_audio_config, load_engine_path, load_media_config, load_project_config, load_project_path,
    load_ui_config, load_upload_config, save_audio_config, save_engine_path, save_media_config, save_project_config,
    save_project_path, save_upload_config, AudioConfig, MediaConfig, UploadConfig,
};
use crate::engine::{build_init_status, detect_unreal_engine, is_valid_engine_dir};
use crate::gif::GifPlayer;
use crate::ops::{git as ops_git, package as ops_package, update as ops_update, vs as ops_vs};
use crate::ops::update::UpdateInfo;
use crate::theme::apply_miku_theme;
use crate::types::{GitState, GitTaskStatus, IdeChoice};
use crate::webview::{WebPanel, WebViewManager};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

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
    pub show_package_config:        bool,
    pub pack_name_input:            String,
    pub exe_name_input:             String,
    pub next_version_preview:       u32,
    pub use_custom_version:         bool,
    pub version_override:           String,
    pub editor_is_running:          bool,   // snapshotted when config panel opens
    pub close_editor_before_package: bool,  // user toggle; default true (safe)

    // VS-rebuild pre-flight
    pub show_vs_config: bool,
    pub ide_choice:     IdeChoice,

    // PC / environment pre-flight checks (engine & project path validity,
    // space-in-path UAT workaround, disk space)
    pub show_pc_check:       bool,
    pub use_space_free_link: bool,
    pub pc_check_items:      Vec<crate::ops::preflight::CheckItem>,

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
    pub git_merged_from:         String,
    pub git_commit_msg:          String,
    pub git_new_branch_name:     String,
    pub git_package_after_merge: bool,

    // Post-package: open folder prompt
    pub show_open_folder_panel:    bool,
    pub pending_open_folder_path:  std::path::PathBuf,

    // Extras
    pub show_dm_spencer_panel: bool,
    pub dm_target_name:        String,
    pub dm_message_presets:    Vec<String>,
    pub dm_custom_message:     String,
    pub dm_image_path:         String,

    // Miku view mode: false = 2D gif (default), true = 3D web
    pub miku_mode_3d: bool,

    // Embedded WebView2 panels (3D Miku, Cookie Clicker, Sponder Bird)
    pub webview_manager:  WebViewManager,
    pub active_web_panel: Option<WebPanel>,
    pub pending_webview:  Option<(WebPanel, egui::Rect)>,

    // Self-update
    pub update_info:        Arc<Mutex<Option<UpdateInfo>>>,
    pub show_update_banner: bool,
    pub last_update_check:  Instant,

    // Fast-package progress animation
    pub fast_package_mode:  bool,
    pub task_started_at:    Option<Instant>,

    // TACHYON packaging ad: plays once (video + its own baked-in audio) in
    // place of the normal gif/3D preview, then reverts automatically.
    pub ad_playing:    bool,
    pub ad_started_at: Option<Instant>,

    // Custom media (2D image/GIF + looping sound)
    pub show_media_config: bool,
    pub custom_gif_path:   Option<PathBuf>,
    pub custom_sound_path: Option<PathBuf>,

    // Stored so background tasks can call request_repaint() on completion
    pub egui_ctx: egui::Context,
}

impl DevToolApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ui_cfg = load_ui_config();
        if let Some((r, g, b)) = ui_cfg.accent_rgb {
            crate::theme::set_accent_value(egui::Color32::from_rgb(r, g, b));
        }
        apply_miku_theme(&cc.egui_ctx);
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
            show_package_config:         false,
            pack_name_input:             String::new(),
            exe_name_input:              String::new(),
            next_version_preview:        1,
            use_custom_version:          false,
            version_override:            String::new(),
            editor_is_running:           false,
            close_editor_before_package: true,
            show_vs_config:       false,
            ide_choice:           IdeChoice::Rider,
            show_pc_check:        false,
            use_space_free_link:  false,
            pc_check_items:       Vec::new(),
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
            git_merged_from:         String::new(),
            git_commit_msg:          String::new(),
            git_new_branch_name:     String::new(),
            git_package_after_merge: false,
            show_open_folder_panel:    false,
            pending_open_folder_path:  std::path::PathBuf::new(),
            show_dm_spencer_panel:     false,
            dm_target_name:            "gonkindroid".to_string(),
            dm_message_presets:        vec!["Hey!".to_string(),
                                        "You up?".to_string(),
                                         "Help!!".to_string(),],
            dm_custom_message:         String::new(),
            dm_image_path:             String::new(),
            miku_mode_3d:              false,
            webview_manager,
            active_web_panel: None,
            pending_webview:  None,
            update_info:        Arc::new(Mutex::new(None)),
            show_update_banner: true,
            last_update_check:  Instant::now(),
            fast_package_mode:  false,
            task_started_at:    None,
            ad_playing:         false,
            ad_started_at:      None,
            show_media_config:  false,
            custom_gif_path,
            custom_sound_path,
            egui_ctx: cc.egui_ctx.clone(),
        };
        ops_update::cleanup_old_binary();
        app.check_for_updates(cc.egui_ctx.clone());
        app
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

    // ── PC / environment pre-flight ──────────────────────────────────────────

    pub fn open_pc_check(&mut self) {
        self.show_package_config = false;
        self.show_vs_config      = false;
        self.git_state            = GitState::Idle;
        self.show_pc_check       = true;
        self.refresh_pc_check();
    }

    pub fn refresh_pc_check(&mut self) {
        self.pc_check_items = crate::ops::preflight::run_checks(&self.engine_dir, &self.project_path);
    }

    /// One-click fix for the "UAT breaks on spaces in paths" issue: aliases
    /// the engine and/or project folder to a space-free directory junction
    /// and remembers to route packaging through it from now on. Junctions
    /// don't touch the real files — they're just an alternate, space-free
    /// path to the same folder.
    pub fn apply_space_free_fix(&mut self) {
        if let Some(engine) = self.engine_dir.clone()
            && crate::ops::preflight::has_space(&engine)
        {
            if let Err(e) = crate::ops::preflight::ensure_space_free_alias(&engine) {
                self.set_status(format!("[ERROR] {e}"));
                return;
            }
        }
        if let Some(dir) = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf())
            && crate::ops::preflight::has_space(&dir)
        {
            if let Err(e) = crate::ops::preflight::ensure_space_free_alias(&dir) {
                self.set_status(format!("[ERROR] {e}"));
                return;
            }
        }
        self.use_space_free_link = true;
        self.set_status("[OK] Space-free link ready — packaging will route through it from now on.".into());
    }

    pub fn git_project_dir(&self) -> Option<PathBuf> {
        self.project_path.as_ref()?.parent().map(|p| p.to_path_buf())
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
    pub fn start_update_install(&mut self, download_url: String) {
        self.show_update_banner = false;
        self.busy_label = "[ DOWNLOADING UPDATE ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status   = Arc::clone(&self.status_message);
        let cancel   = Arc::clone(&self.cancel_flag);
        let progress = Arc::clone(&self.progress);
        self.run_background_task("Downloading update…", move || {
            match ops_update::download_and_install(&download_url, &status, &cancel, &progress) {
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

    pub fn open_media_config(&mut self) {
        self.show_package_config = false;
        self.show_vs_config      = false;
        self.git_state            = GitState::Idle;
        self.show_media_config   = true;
    }

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

    // ── TACHYON packaging ad ──────────────────────────────────────────────────

    /// Ends the TACHYON ad (natural `ended` event, safety-timeout, or webview
    /// load failure) and resumes whatever the user would normally see: gif
    /// reset to frame 0, looping background music restarted at the correct
    /// speed. A no-op if the ad isn't currently playing.
    pub fn end_ad(&mut self) {
        if !self.ad_playing { return; }
        self.ad_playing    = false;
        self.ad_started_at = None;
        if let Some(g) = &mut self.gif_player { g.reset(); }
        if let Some(a) = &mut self.audio_player {
            // `set_speed` only auto-restarts playback when already marked
            // playing, so calling it before the explicit `play_looping()`
            // just primes the speed for that call — no stop-then-append
            // race here since audio was never started for the ad path.
            a.set_speed(if self.fast_package_mode { 2.5 } else { 1.0 });
            a.play_looping();
        }
    }

    // ── Package actions ───────────────────────────────────────────────────────

    pub fn open_package_config(&mut self) {
        let project_path = match &self.project_path { Some(p) => p.clone(), None => return };
        let (pack, exe) = load_project_config(&project_path);
        self.pack_name_input = pack;
        self.exe_name_input  = exe;
        let build_dir = project_path.parent()
            .map(|p| p.join("build"))
            .unwrap_or_default();
        self.next_version_preview = ops_package::find_next_version(&build_dir);
        // Default the editable version field to the next auto-incremented
        // version; the user can tick "Custom" to keep/change it.
        self.version_override   = ops_package::format_version(self.next_version_preview);
        self.use_custom_version  = false;
        // Snapshot whether the editor is running right now so the config panel
        // can show the appropriate warning without calling tasklist every frame.
        self.editor_is_running = ops_package::is_editor_running();
        self.show_package_config = true;
        self.show_vs_config      = false;
        self.git_state           = GitState::Idle;
    }

    pub fn start_packaging(&mut self) {
        let project_path = match self.project_path.clone() { Some(p) => p, None => return };
        let engine_dir   = match self.engine_dir.clone() {
            Some(e) => e,
            None    => {
                self.set_status("[ERROR] Engine not found.".into());
                self.show_package_config = false;
                return;
            }
        };
        let pack_name = self.pack_name_input.trim().to_string();
        let exe_name  = self.exe_name_input.trim().to_string();
        if pack_name.is_empty() || exe_name.is_empty() {
            self.set_status("[ERROR] Names cannot be empty.".into());
            return;
        }
        let version_str = if self.use_custom_version {
            self.version_override.trim().to_string()
        } else {
            ops_package::format_version(self.next_version_preview)
        };
        if version_str.is_empty() || version_str.chars().any(|c| "\\/:*?\"<>|".contains(c)) {
            self.set_status("[ERROR] Invalid version — cannot be empty or contain \\ / : * ? \" < > |".into());
            return;
        }
        save_project_config(&project_path, &pack_name, &exe_name);
        self.show_package_config = false;
        self.fast_package_mode  = false;
        self.task_started_at    = Some(Instant::now());
        self.busy_label = "[ PACKAGING IN PROGRESS ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.ad_playing = pack_name.eq_ignore_ascii_case("TACHYON") && crate::ops::ads::ad_video_url().is_some();
        if self.ad_playing {
            self.ad_started_at = Some(Instant::now());
            self.webview_manager.restart_ad();
            // The ad has its own baked-in audio — skip play_looping() so it
            // doesn't compete with the ad's soundtrack.
        } else {
            self.ad_started_at = None;
            if let Some(a) = &mut self.audio_player { a.play_looping(); }
        }
        let status_clone  = Arc::clone(&self.status_message);
        let pending_clone = Arc::clone(&self.pending_zip);
        let cancel        = Arc::clone(&self.cancel_flag);
        let progress      = Arc::clone(&self.progress);
        let close_editor  = self.close_editor_before_package;
        let use_space_free_link = self.use_space_free_link;
        self.run_background_task("Starting UAT pipeline…", move || {
            ops_package::package_game(project_path, engine_dir, pack_name, exe_name, version_str, status_clone, pending_clone, cancel, progress, close_editor, use_space_free_link)
        });
    }

    pub fn start_fast_packaging(&mut self) {
        let project_path = match self.project_path.clone() { Some(p) => p, None => return };
        let engine_dir   = match self.engine_dir.clone() {
            Some(e) => e,
            None    => {
                self.set_status("[ERROR] Engine not found.".into());
                self.show_package_config = false;
                return;
            }
        };
        let pack_name = self.pack_name_input.trim().to_string();
        let exe_name  = self.exe_name_input.trim().to_string();
        if pack_name.is_empty() || exe_name.is_empty() {
            self.set_status("[ERROR] Names cannot be empty.".into());
            return;
        }
        let version_str = if self.use_custom_version {
            self.version_override.trim().to_string()
        } else {
            ops_package::format_version(self.next_version_preview)
        };
        if version_str.is_empty() || version_str.chars().any(|c| "\\/:*?\"<>|".contains(c)) {
            self.set_status("[ERROR] Invalid version — cannot be empty or contain \\ / : * ? \" < > |".into());
            return;
        }
        save_project_config(&project_path, &pack_name, &exe_name);
        self.show_package_config = false;
        self.fast_package_mode  = true;
        self.task_started_at    = Some(Instant::now());
        self.busy_label = "[ ⚡ FAST PACKAGING ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.ad_playing = pack_name.eq_ignore_ascii_case("TACHYON") && crate::ops::ads::ad_video_url().is_some();
        if self.ad_playing {
            self.ad_started_at = Some(Instant::now());
            self.webview_manager.restart_ad();
        } else {
            self.ad_started_at = None;
            if let Some(a) = &mut self.audio_player { a.set_speed(2.5); a.play_looping(); }
        }
        let status_clone  = Arc::clone(&self.status_message);
        let pending_clone = Arc::clone(&self.pending_zip);
        let cancel        = Arc::clone(&self.cancel_flag);
        let progress      = Arc::clone(&self.progress);
        let close_editor  = self.close_editor_before_package;
        let use_space_free_link = self.use_space_free_link;
        self.run_background_task("Starting fast UAT pipeline…", move || {
            ops_package::package_game(project_path, engine_dir, pack_name, exe_name, version_str, status_clone, pending_clone, cancel, progress, close_editor, use_space_free_link)
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
        self.show_package_config = false;
        self.git_state           = GitState::Idle;
    }

    pub fn start_vs_rebuild(&mut self) {
        let project_path = match self.project_path.clone() { Some(p) => p, None => return };
        let engine_dir   = match self.engine_dir.clone() {
            Some(e) => e,
            None    => {
                self.set_status("[ERROR] Engine not found.".into());
                self.show_vs_config = false;
                return;
            }
        };
        let ide = self.ide_choice;
        self.show_vs_config = false;
        self.busy_label = "[ GENERATING PROJECT FILES ]".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status_clone = Arc::clone(&self.status_message);
        let cancel       = Arc::clone(&self.cancel_flag);
        let progress     = Arc::clone(&self.progress);
        self.run_background_task("Preparing to regenerate project files…", move || {
            ops_vs::rebuild_vs_files(project_path, engine_dir, ide, status_clone, cancel, progress)
        });
    }

    // ── Git actions ───────────────────────────────────────────────────────────

    pub fn open_git_menu(&mut self) {
        self.show_package_config = false;
        self.show_vs_config      = false;
        self.git_commit_msg.clear();
        self.git_new_branch_name.clear();
        self.git_current_branch = self.git_project_dir()
            .map(|d| ops_git::git_current_branch(&d))
            .unwrap_or_else(|| "unknown".into());
        self.git_state = GitState::Menu;
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
}
