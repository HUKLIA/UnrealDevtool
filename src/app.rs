use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::{load_project_config, load_project_path, save_project_config, save_project_path};
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

    // Package pre-flight
    pub show_package_config:  bool,
    pub pack_name_input:      String,
    pub exe_name_input:       String,
    pub next_version_preview: u32,

    // VS-rebuild pre-flight
    pub show_vs_config: bool,
    pub ide_choice:     IdeChoice,

    // Git state machine
    pub git_state:            GitState,
    pub git_next_state:       GitState,
    pub git_result:           Arc<Mutex<Option<GitTaskStatus>>>,
    pub git_current_branch:   String,
    pub git_merged_from:      String,
    pub git_commit_msg:       String,
    pub git_new_branch_name:  String,
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
        // GIF is embedded at compile time — no file needed at runtime
        let gif_player = GifPlayer::from_bytes(include_bytes!("../Image/miku-hatsune.gif"));
        Self {
            engine_dir,
            project_path,
            project_path_input,
            status_message: Arc::new(Mutex::new(init_status.clone())),
            status_display: init_status,
            is_working:  Arc::new(Mutex::new(false)),
            was_working: false,
            gif_player,
            busy_label:           String::new(),
            show_package_config:  false,
            pack_name_input:      String::new(),
            exe_name_input:       String::new(),
            next_version_preview: 1,
            show_vs_config:       false,
            ide_choice:           IdeChoice::Rider,
            git_state:            GitState::Idle,
            git_next_state:       GitState::Idle,
            git_result:           Arc::new(Mutex::new(None)),
            git_current_branch:   String::new(),
            git_merged_from:      String::new(),
            git_commit_msg:       String::new(),
            git_new_branch_name:  String::new(),
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
        let status_clone = Arc::clone(&self.status_message);
        self.run_background_task("Starting UAT pipeline…", move || {
            ops_package::package_game(project_path, engine_dir, pack_name, exe_name, status_clone)
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
        self.busy_label = "◈  GENERATING PROJECT FILES  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        let status_clone = Arc::clone(&self.status_message);
        self.run_background_task("Preparing to regenerate project files…", move || {
            ops_vs::rebuild_vs_files(project_path, engine_dir, ide, status_clone)
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
        self.run_background_task("Staging changes…", move || {
            ops_git::task_git_commit_push(dir, msg, branch, status, result)
        });
    }

    pub fn git_start_sync(&mut self) {
        let dir    = match self.git_project_dir() { Some(d) => d, None => return };
        let status = Arc::clone(&self.status_message);
        let result = Arc::clone(&self.git_result);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "◈  SYNCING WITH MAIN  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Fetching origin/main…", move || {
            ops_git::task_git_sync(dir, status, result)
        });
    }

    pub fn git_start_merge(&mut self) {
        let dir         = match self.git_project_dir() { Some(d) => d, None => return };
        let from_branch = self.git_current_branch.clone();
        let status      = Arc::clone(&self.status_message);
        let result      = Arc::clone(&self.git_result);
        self.git_merged_from = from_branch.clone();
        self.git_next_state  = GitState::AfterMerge;
        self.busy_label      = "◈  MERGING TO MAIN  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Switching to main…", move || {
            ops_git::task_git_merge_to_main(dir, from_branch, status, result)
        });
    }

    pub fn git_start_checkout(&mut self, branch: String) {
        let dir    = match self.git_project_dir() { Some(d) => d, None => return };
        let status = Arc::clone(&self.status_message);
        let result = Arc::clone(&self.git_result);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "◈  SWITCHING BRANCH  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Switching branch…", move || {
            ops_git::task_git_checkout(dir, branch, status, result)
        });
    }

    pub fn git_start_new_branch(&mut self, name: String) {
        let dir    = match self.git_project_dir() { Some(d) => d, None => return };
        let status = Arc::clone(&self.status_message);
        let result = Arc::clone(&self.git_result);
        self.git_next_state = GitState::Idle;
        self.busy_label     = "◈  CREATING BRANCH  ◈".into();
        if let Some(g) = &mut self.gif_player { g.reset(); }
        self.run_background_task("Creating branch…", move || {
            ops_git::task_git_create_branch(dir, name, status, result)
        });
    }

    // ── Background task runner ────────────────────────────────────────────────

    pub fn run_background_task<F>(&mut self, start_msg: &str, task: F)
    where
        F: FnOnce() -> String + Send + 'static,
    {
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
