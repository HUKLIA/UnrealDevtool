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

// ── Manual engine-folder override persistence ────────────────────────────────
// Used when auto-detection (registry / EngineAssociation) can't find the
// engine — e.g. a source build, a non-standard install location, or a
// machine where the Epic Games Launcher registry keys are missing.

pub fn load_engine_path() -> Option<PathBuf> {
    let content = fs::read_to_string(config_dir()?.join("engine_path.txt")).ok()?;
    let p = PathBuf::from(content.trim());
    if p.as_os_str().is_empty() { None } else { Some(p) }
}

pub fn save_engine_path(path: &Path) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("engine_path.txt"), path.to_string_lossy().as_bytes());
    }
}

pub fn clear_engine_path() {
    if let Some(dir) = config_dir() {
        let _ = fs::remove_file(dir.join("engine_path.txt"));
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
    if let Some(cfg) = project_config_file(project_path)
        && let Ok(content) = fs::read_to_string(&cfg) {
            let mut lines = content.lines();
            let pack = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            let exe  = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            return (pack, exe);
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

#[derive(Default)]
pub struct UploadConfig {
    pub local_path:  String,
    pub rclone_dest: String,
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

// ── Packaging sound config (`audio.cfg`) ─────────────────────────────────────
// Line 1: muted    ("1" or "0")
// Line 2: volume   (0-100)

pub struct AudioConfig {
    pub muted:  bool,
    pub volume: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { muted: false, volume: 50 }
    }
}

pub fn load_audio_config() -> AudioConfig {
    let path = match config_dir() { Some(d) => d.join("audio.cfg"), None => return AudioConfig::default() };
    let content = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return AudioConfig::default() };
    let mut lines = content.lines();
    let muted  = lines.next().map(|s| s.trim() == "1").unwrap_or(false);
    let volume = lines.next()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|v| v.min(100))
        .unwrap_or(50);
    AudioConfig { muted, volume }
}

pub fn save_audio_config(cfg: &AudioConfig) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("audio.cfg"), format!("{}\n{}", if cfg.muted { 1 } else { 0 }, cfg.volume));
    }
}

// ── Custom media config (`media.cfg`) ────────────────────────────────────────
// Line 1: gif_path   (custom 2D image/GIF; empty = use the built-in Miku gif)
// Line 2: sound_path (custom looping sound; empty = use the built-in track)

#[derive(Default)]
pub struct MediaConfig {
    pub gif_path:   String,
    pub sound_path: String,
}


pub fn load_media_config() -> MediaConfig {
    let path = match config_dir() { Some(d) => d.join("media.cfg"), None => return MediaConfig::default() };
    let content = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return MediaConfig::default() };
    let mut lines = content.lines();
    MediaConfig {
        gif_path:   lines.next().unwrap_or("").to_string(),
        sound_path: lines.next().unwrap_or("").to_string(),
    }
}

pub fn save_media_config(cfg: &MediaConfig) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("media.cfg"), format!("{}\n{}", cfg.gif_path, cfg.sound_path));
    }
}

// ── UI accent color config (`ui.cfg`) ────────────────────────────────────────
// Line 1: "r,g,b" (0-255 each). Missing/unparsable = None = default teal.

#[derive(Default)]
pub struct UiConfig {
    pub accent_rgb: Option<(u8, u8, u8)>,
}

pub fn load_ui_config() -> UiConfig {
    let path = match config_dir() { Some(d) => d.join("ui.cfg"), None => return UiConfig::default() };
    let content = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return UiConfig::default() };
    let accent_rgb = content.lines().next().and_then(|line| {
        let mut p = line.split(',');
        Some((p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?))
    });
    UiConfig { accent_rgb }
}

pub fn save_ui_config(cfg: &UiConfig) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let line = cfg.accent_rgb.map(|(r, g, b)| format!("{r},{g},{b}")).unwrap_or_default();
        let _ = fs::write(dir.join("ui.cfg"), line);
    }
}

// ── Custom quick links (`links.json`) ────────────────────────────────────────
// User-editable label+URL buttons shown in the Extras tab. Seeded with the
// original hardcoded set plus project-management links the user asked for —
// those have no sensible universal default (a Trello board / Jira project is
// specific to one team), so they start with an empty URL: clicking one with
// no URL set opens it for editing instead of navigating nowhere.

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomLink {
    pub label: String,
    pub url:   String,
}

pub fn default_custom_links() -> Vec<CustomLink> {
    vec![
        CustomLink { label: "Claude".into(),            url: "https://claude.ai/new".into() },
        CustomLink { label: "ChatGPT".into(),           url: "https://chatgpt.com/".into() },
        CustomLink { label: "Gemini".into(),            url: "https://gemini.google.com/app".into() },
        CustomLink { label: "Epic Games".into(),        url: "https://www.epicgames.com/".into() },
        CustomLink { label: "Unreal Docs".into(),       url: "https://dev.epicgames.com/community/assistant/unreal-engine".into() },
        CustomLink { label: "Trello".into(),            url: String::new() },
        CustomLink { label: "Jira".into(),              url: String::new() },
        CustomLink { label: "Task List".into(),         url: String::new() },
        CustomLink { label: "Requirement Check".into(), url: String::new() },
    ]
}

pub fn load_custom_links() -> Vec<CustomLink> {
    let Some(dir) = config_dir() else { return default_custom_links() };
    let path = dir.join("links.json");
    match fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Vec<CustomLink>>(&s).ok()) {
        Some(links) if !links.is_empty() => links,
        _ => default_custom_links(),
    }
}

pub fn save_custom_links(links: &[CustomLink]) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(links) {
            let _ = fs::write(dir.join("links.json"), json);
        }
    }
}
