use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::config::{
    clear_gdrive_auth, gdrive_token_path,
    load_gdrive_user, load_project_config, load_project_path, load_upload_config,
    save_project_config, save_project_path, save_upload_config, UploadConfig,
};
use crate::engine::{build_init_status, detect_unreal_engine};
use crate::gif::GifPlayer;
use crate::ops::{git as ops_git, package as ops_package, vs as ops_vs};
use crate::theme::apply_miku_theme;
use crate::types::{GitState, GitTaskStatus, IdeChoice};

// ── Application state ─────────────────────────────────────────────────────────

pub struct DevToolApp {
    pub engine_dir:           Option<PathBuf>,
    pub project_path:         Option<PathBuf>,
    pub project_path_input:   String,
    pub status_message:       Arc<Mutex<String>>,
    pub status_display:       String,
    pub is_working:           Arc<Mutex<bool>>,
    pub was_working:          bool,
    pub gif_player:           Option<GifPlayer>,
    pub busy_label:           String,
    pub cancel_flag:          Arc<AtomicBool>,
    pub progress:             Arc<Mutex<f32>>,

    // Package pre-flight
    pub show_package_config:  bool,
    pub pack_name_input:      String,
    pub exe_name_input:       String,
    pub next_version_preview: u32,

    // VS-rebuild pre-flight
    pub show_vs_config: bool,
    pub ide_choice:     IdeChoice,

    // Post-package upload panel
    pub pending_zip:                 Arc<Mutex<Option<PathBuf>>>,
    pub show_upload_panel:           bool,
    pub upload_zip_path:             PathBuf,
    pub upload_use_local:            bool,
    pub upload_use_gdrive:           bool,
    pub upload_local_path:           String,
    pub upload_gdrive_folder_id:   String,
    pub upload_gdrive_secret_path: String,   // path to client_secret.json
    pub upload_gdrive_user_email:  String,   // populated after first OAuth sign-in

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
}

impl DevToolApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_miku_theme(&cc.egui_ctx);
        let engine_dir   = detect_unreal_engine();
        let project_path = load_project_path();
        let init_status  = build_init_status(&engine_dir, &project_path);
        let project_path_input = project_path.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let gif_player  = GifPlayer::from_bytes(include_bytes!("../Image/miku-hatsune.gif"));
        let upload_cfg  = load_upload_config();
        Self {
            engine_dir,
            project_path,
            project_path_input,
            status_message: Arc::new(Mutex::new(init_status.clone())),
            status_display: init_status,
            is_working:  Arc::new(Mutex::new(false)),
            was_working: false,
            gif_player,
            busy_label:  String::new(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            progress:    Arc::new(Mutex::new(0.0_f32)),
            show_package_config:  false,
            pack_name_input:      String::new(),
            exe_name_input:       String::new(),
            next_version_preview: 1,
            show_vs_config:       false,
            ide_choice:           IdeChoice::Rider,
            pending_zip:                 Arc::new(Mutex::new(None)),
            show_upload_panel:           false,
            upload_zip_path:             PathBuf::new(),
            upload_use_local:            false,
            upload_use_gdrive:           false,
            upload_local_path:           upload_cfg.local_path,
            upload_gdrive_folder_id:     upload_cfg.gdrive_folder_id,
            upload_gdrive_secret_path:   upload_cfg.gdrive_secret_path,
            upload_gdrive_user_email:    load_gdrive_user(),
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
        }
    }

    // ── Shared helpers ────────────────────────────────────────────────────────

    pub fn try_apply_typed_path(&mut self) {
        let trimmed = self.project_path_input.trim().to_string();
        if trimmed.is_empty() { return; }
        let p = std::path::PathBuf::from(&trimmed);
        if p.exists() && p.extension().map_or(false, |e| e.eq_ignore_ascii_case("uproject")) {
            save_project_path(&p);
            self.project_path = Some(p);
            self.refresh_status();
        }
    }

    pub fn set_status(&self, msg: String) {
        *self.status_message.lock().unwrap() = msg;
    }

    pub fn refresh_status(&self) {
        self.set_status(build_init_status(&self.engine_dir, &self.project_path));
    }

    pub fn git_project_dir(&self) -> Option<PathBuf> {
        self.project_path.as_ref()?.parent().map(|p| p.to_path_buf())
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
        save_project_config(&project_path, &pack_name, &exe_name);
        self.show_package_config = false;
        self.busy_label = "◈  PACKAGING IN PROGRESS  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status_clone  = Arc::clone(&self.status_message);
        let pending_clone = Arc::clone(&self.pending_zip);
        let cancel        = Arc::clone(&self.cancel_flag);
        let progress      = Arc::clone(&self.progress);
        self.run_background_task("Starting UAT pipeline…", move || {
            ops_package::package_game(project_path, engine_dir, pack_name, exe_name, status_clone, pending_clone, cancel, progress)
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
            local_path:         self.upload_local_path.clone(),
            gdrive_folder_id:   self.upload_gdrive_folder_id.clone(),
            gdrive_secret_path: self.upload_gdrive_secret_path.clone(),
        });

        let use_local   = self.upload_use_local;
        let use_gdrive  = self.upload_use_gdrive;
        let local_path  = self.upload_local_path.clone();
        let folder_id   = self.upload_gdrive_folder_id.clone();
        let secret_path = std::path::PathBuf::from(&self.upload_gdrive_secret_path);
        let token_path  = gdrive_token_path().unwrap_or_default();
        let status      = Arc::clone(&self.status_message);
        let cancel      = Arc::clone(&self.cancel_flag);
        let progress    = Arc::clone(&self.progress);

        self.show_upload_panel = false;
        self.busy_label = "◈  UPLOADING BUILD  ◈".into();
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
                parts.push(ops_package::upload_to_gdrive_oauth(
                    &zip, &folder_id, &secret_path, &token_path, &status,
                ));
                *progress.lock().unwrap() = 1.0;
            }
            if parts.is_empty() { return "[DONE] No destination selected — nothing uploaded.".to_string(); }
            parts.join("\n")
        });
    }

    pub fn reload_gdrive_user(&mut self) {
        self.upload_gdrive_user_email = load_gdrive_user();
    }

    pub fn gdrive_sign_out(&mut self) {
        clear_gdrive_auth();
        self.upload_gdrive_user_email.clear();
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
        self.busy_label = "◈  GENERATING PROJECT FILES  ◈".into();
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
        self.busy_label     = "◈  COMMITTING & PUSHING  ◈".into();
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
        self.busy_label     = "◈  SYNCING WITH MAIN  ◈".into();
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
        self.busy_label      = "◈  MERGING TO MAIN  ◈".into();
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
        self.busy_label     = "◈  SWITCHING BRANCH  ◈".into();
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
        self.busy_label     = "◈  CREATING BRANCH  ◈".into();
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
        *self.progress.lock().unwrap()       = 0.0;
        *self.is_working.lock().unwrap()     = true;
        *self.status_message.lock().unwrap() = start_msg.to_string();
        let status  = Arc::clone(&self.status_message);
        let working = Arc::clone(&self.is_working);
        thread::spawn(move || {
            let result = task();
            *status.lock().unwrap()  = result;
            *working.lock().unwrap() = false;
        });
    }
}
