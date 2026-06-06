use std::fs;
use std::path::{Path, PathBuf};

/// Base config directory: `%APPDATA%\UnrealDevTool\`
pub fn config_dir() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|v| PathBuf::from(v).join("UnrealDevTool"))
}

// ── Project path persistence ──────────────────────────────────────────────────

pub fn load_project_path() -> Option<PathBuf> {
    let content = fs::read_to_string(config_dir()?.join("project_path.txt")).ok()?;
    let p = PathBuf::from(content.trim());
    if p.exists() { Some(p) } else { None }
}

pub fn save_project_path(path: &Path) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("project_path.txt"), path.to_string_lossy().as_bytes());
    }
}

pub fn clear_project_path() {
    if let Some(dir) = config_dir() {
        let _ = fs::remove_file(dir.join("project_path.txt"));
    }
}

// ── Per-project build config (`{stem}_build.cfg`) ────────────────────────────
// Line 1: pack_name   (folder name + zip prefix)
// Line 2: exe_name    (game exe is renamed to this)

pub fn project_config_file(project_path: &Path) -> Option<PathBuf> {
    let stem = project_path.file_stem()?.to_string_lossy().to_string();
    config_dir().map(|d| d.join(format!("{}_build.cfg", stem)))
}

pub fn load_project_config(project_path: &Path) -> (String, String) {
    let default = project_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(cfg) = project_config_file(project_path) {
        if let Ok(content) = fs::read_to_string(&cfg) {
            let mut lines = content.lines();
            let pack = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            let exe  = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            return (pack, exe);
        }
    }
    (default.clone(), default)
}

pub fn save_project_config(project_path: &Path, pack_name: &str, exe_name: &str) {
    if let Some(cfg) = project_config_file(project_path) {
        if let Some(dir) = config_dir() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(cfg, format!("{}\n{}", pack_name, exe_name));
    }
}

// ── Upload destination config (`upload.cfg`) ─────────────────────────────────
// Line 1: local_copy_path
// Line 2: rclone_dest  (e.g. "gdrive:/Builds/MyGame")

pub struct UploadConfig {
    pub local_path:  String,
    pub rclone_dest: String,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self { local_path: String::new(), rclone_dest: String::new() }
    }
}

pub fn load_upload_config() -> UploadConfig {
    let path = match config_dir() { Some(d) => d.join("upload.cfg"), None => return UploadConfig::default() };
    let content = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return UploadConfig::default() };
    let mut lines = content.lines();
    UploadConfig {
        local_path:  lines.next().unwrap_or("").to_string(),
        rclone_dest: lines.next().unwrap_or("").to_string(),
    }
}

pub fn save_upload_config(cfg: &UploadConfig) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("upload.cfg"), format!("{}\n{}", cfg.local_path, cfg.rclone_dest));
    }
}
