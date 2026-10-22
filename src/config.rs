use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PlaytestPreset {
    pub name: String,
    pub log_window: bool,
    pub windowed: bool,
    pub no_sound: bool,
    pub resolution: usize,
    pub extra_args: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetSaveResult { Added, Replaced, ReplacementNeedsConfirmation }

pub fn upsert_playtest_preset(presets: &mut Vec<PlaytestPreset>, preset: PlaytestPreset, confirm_replace: bool) -> PresetSaveResult {
    if let Some(index) = presets.iter().position(|p| p.name.eq_ignore_ascii_case(&preset.name)) {
        if !confirm_replace { return PresetSaveResult::ReplacementNeedsConfirmation; }
        presets[index] = preset;
        PresetSaveResult::Replaced
    } else {
        presets.push(preset);
        PresetSaveResult::Added
    }
}

use crate::types::{BuildConfiguration, BuildTarget};

/// Base config directory: `%APPDATA%\UnrealDevTool\`
pub fn config_dir() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|v| PathBuf::from(v).join("UnrealDevTool"))
}

/// Disable automatic network probes for local-only testing. This is an
/// environment switch so it does not alter or rewrite the user's settings.
pub fn offline_mode() -> bool {
    std::env::var("UDT_OFFLINE").ok().is_some_and(|v| offline_value_enabled(&v))
}

fn offline_value_enabled(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
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

/// Recently-opened projects, newest first.
///
/// The app already persisted *the* project path; it never kept a history, so
/// switching between two projects meant browsing for the other one every time.
/// The setup surface lists these.
pub fn recents_file() -> Option<PathBuf> {
    Some(config_dir()?.join("recent_projects.txt"))
}

pub fn load_recent_projects() -> Vec<PathBuf> {
    let Some(f) = recents_file() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(f) else { return Vec::new() };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .take(8)
        .collect()
}

/// Moves `path` to the front, de-duplicated, capped at 8.
pub fn push_recent_project(path: &Path) {
    let Some(f) = recents_file() else { return };
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut list = load_recent_projects();
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(8);
    let body: Vec<String> = list.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let _ = std::fs::write(f, body.join("\n"));
}

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
// Line 1: pack_name          (folder name + zip prefix)
// Line 2: exe_name           (game exe is renamed to this)
// Line 3: configuration      (Development or Shipping; missing = Development)

fn project_identity(project_path: &Path) -> Option<(String, String)> {
    let stem = project_path.file_stem()?.to_string_lossy().to_lowercase();
    let stable_path = project_path.canonicalize().unwrap_or_else(|_| project_path.to_path_buf());
    let normalized = stable_path.to_string_lossy().replace('/', "\\").to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    let suffix = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Some((stem, suffix))
}

fn project_settings_file(project_path: &Path, kind: &str) -> Option<PathBuf> {
    project_settings_file_in(&config_dir()?, project_path, kind)
}

fn project_settings_file_in(dir: &Path, project_path: &Path, kind: &str) -> Option<PathBuf> {
    let (stem, suffix) = project_identity(project_path)?;
    Some(dir.join(format!("{stem}-{suffix}_{kind}.cfg")))
}

fn legacy_project_settings_file_in(dir: &Path, project_path: &Path, kind: &str) -> Option<PathBuf> {
    let stem = project_path.file_stem()?.to_string_lossy();
    Some(dir.join(format!("{stem}_{kind}.cfg")))
}

/// Resolve a project-specific file, migrating a legacy stem-only file once.
/// When two projects had the same stem, the first one opened claims the old
/// preferences; subsequent projects get their own defaults and files.
fn project_settings_file_for_load(project_path: &Path, kind: &str) -> Option<PathBuf> {
    project_settings_file_for_load_in(&config_dir()?, project_path, kind)
}

fn project_settings_file_for_load_in(dir: &Path, project_path: &Path, kind: &str) -> Option<PathBuf> {
    let current = project_settings_file_in(dir, project_path, kind)?;
    if current.is_file() { return Some(current); }
    if let Some(legacy) = legacy_project_settings_file_in(dir, project_path, kind).filter(|p| p.is_file())
        && fs::rename(&legacy, &current).is_ok() {
            return Some(current);
        }
    // A failed migration must never fall back to a stem-only file: another
    // project with the same name could otherwise read the first project's
    // settings.
    Some(current)
}

pub fn project_config_file(project_path: &Path) -> Option<PathBuf> {
    project_settings_file(project_path, "build")
}

/// Reads the per-project build settings.
///
/// The target is a fourth line appended after the existing three. A file
/// written by an older build simply has no fourth line, so it reads back as
/// Win64 — which is what those builds could produce anyway.
pub fn load_project_config(project_path: &Path) -> (String, String, BuildConfiguration, BuildTarget) {
    let default = project_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(cfg) = project_settings_file_for_load(project_path, "build")
        && let Ok(content) = fs::read_to_string(&cfg) {
            let mut lines = content.lines();
            let pack = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            let exe  = lines.next().filter(|s| !s.trim().is_empty())
                .map(str::to_string).unwrap_or_else(|| default.clone());
            let configuration = match lines.next().map(str::trim) {
                Some("Shipping") => BuildConfiguration::Shipping,
                _ => BuildConfiguration::Development,
            };
            let target = lines.next().map(str::trim)
                .and_then(BuildTarget::from_key)
                .unwrap_or(BuildTarget::Win64);
            return (pack, exe, configuration, target);
        }
    (default.clone(), default, BuildConfiguration::Development, BuildTarget::Win64)
}

pub fn save_project_config(
    project_path: &Path,
    pack_name: &str,
    exe_name: &str,
    configuration: BuildConfiguration,
    target: BuildTarget,
) {
    if let Some(cfg) = project_config_file(project_path) {
        if let Some(dir) = config_dir() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(cfg, format!(
            "{}\n{}\n{}\n{}",
            pack_name, exe_name, configuration.as_str(), target.as_key(),
        ));
    }
}

// ── Per-project advanced UAT options (`{stem}_uat.cfg`) ──────────────────────
// Line 1: "1" when pak compression is on
// Line 2: extra UAT arguments, verbatim (validated again before every build)
// Line 3: packaging method key (full / stepwise / restage)
//
// Kept in its own file rather than as lines five and six of the build config:
// that format is positional, and every added line risked shifting what an
// older build would read back as the target or configuration.

pub fn uat_options_file(project_path: &Path) -> Option<PathBuf> {
    project_settings_file(project_path, "uat")
}

pub fn load_uat_options(project_path: &Path) -> (bool, String, crate::types::PackageMethod) {
    use crate::types::PackageMethod;
    let none = (false, String::new(), PackageMethod::Full);
    let Some(f) = project_settings_file_for_load(project_path, "uat") else { return none };
    let Ok(content) = fs::read_to_string(f) else { return none };
    let mut lines = content.lines();
    let compress = lines.next().is_some_and(|l| l.trim() == "1");
    let extra = lines.next().unwrap_or("").trim().to_string();
    let method = lines.next().and_then(PackageMethod::from_key).unwrap_or(PackageMethod::Full);
    (compress, extra, method)
}

pub fn save_uat_options(project_path: &Path, compress: bool, extra: &str, method: crate::types::PackageMethod) {
    if let Some(f) = uat_options_file(project_path) {
        if let Some(dir) = config_dir() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(f, format!("{}
{}
{}",
            if compress { "1" } else { "0" }, extra.trim(), method.as_key()));
    }
}

// ── Per-project package size budget (`{stem}_budget.cfg`) ─────────────────────
// One line: the most a package should weigh, in megabytes. 0 or missing = none.

fn budget_file(project_path: &Path) -> Option<PathBuf> {
    project_settings_file(project_path, "budget")
}

fn playtest_presets_file(project_path: &Path) -> Option<PathBuf> {
    project_settings_file(project_path, "playtests").map(|p| p.with_extension("json"))
}

fn playtest_report_file(project_path: &Path) -> Option<PathBuf> {
    project_settings_file(project_path, "last_playtest").map(|p| p.with_extension("txt"))
}

pub fn load_playtest_presets(project_path: &Path) -> Vec<PlaytestPreset> {
    playtest_presets_file(project_path)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_playtest_presets(project_path: &Path, presets: &[PlaytestPreset]) -> Result<(), String> {
    let path = playtest_presets_file(project_path).ok_or("Could not identify this project")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create settings folder: {e}"))?;
    }
    let json = serde_json::to_string_pretty(presets).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| format!("Could not save playtest presets: {e}"))
}

pub fn load_playtest_report(project_path: &Path) -> String {
    playtest_report_file(project_path)
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| crate::ops::tools::sanitize_playtest_receipt(&text))
        .unwrap_or_default()
}

pub fn save_playtest_report(project_path: &Path, report: &str) -> Result<(), String> {
    let path = playtest_report_file(project_path).ok_or("Could not identify this project")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create settings folder: {e}"))?;
    }
    fs::write(path, report).map_err(|e| format!("Could not save playtest report: {e}"))
}

pub fn load_size_budget(project_path: &Path) -> u32 {
    project_settings_file_for_load(project_path, "budget")
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

pub fn save_size_budget(project_path: &Path, megabytes: u32) {
    if let Some(f) = budget_file(project_path) {
        if let Some(dir) = config_dir() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(f, megabytes.to_string());
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
    pub theme_mode: Option<String>,
}

pub fn load_ui_config() -> UiConfig {
    let path = match config_dir() { Some(d) => d.join("ui.cfg"), None => return UiConfig::default() };
    let content = match fs::read_to_string(&path) { Ok(s) => s, Err(_) => return UiConfig::default() };
    let accent_rgb = content.lines().next().and_then(|line| {
        let mut p = line.split(',');
        Some((p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?))
    });
    let theme_mode = content.lines().nth(1).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    UiConfig { accent_rgb, theme_mode }
}

pub fn save_ui_config(cfg: &UiConfig) {
    if let Some(dir) = config_dir() {
        let _ = fs::create_dir_all(&dir);
        let line = cfg.accent_rgb.map(|(r, g, b)| format!("{r},{g},{b}")).unwrap_or_default();
        let mode = cfg.theme_mode.as_deref().unwrap_or("dark");
        let _ = fs::write(dir.join("ui.cfg"), format!("{line}\n{mode}"));
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

/// Closing preferences are kept separate from visual settings so changing the
/// accent cannot reset how the window closes.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CloseBehavior {
    #[default]
    Ask,
    Minimize,
    Close,
}

pub fn load_close_behavior() -> CloseBehavior {
    config_dir().and_then(|d| fs::read_to_string(d.join("window.json")).ok())
        .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_close_behavior(value: CloseBehavior) -> Result<(), String> {
    let dir = config_dir().ok_or("Settings directory unavailable")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("window.json"), serde_json::to_string(&value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_named_projects_get_distinct_preferences() {
        let a = Path::new(r"C:\Studio\ProjectA\Game.uproject");
        let b = Path::new(r"D:\Archive\Game.uproject");
        assert_ne!(project_config_file(a), project_config_file(b));
        assert_ne!(uat_options_file(a), uat_options_file(b));
        assert_ne!(budget_file(a), budget_file(b));
    }

    #[test]
    fn project_path_matching_is_case_insensitive_on_windows() {
        let a = Path::new(r"C:\Studio\Game.uproject");
        let b = Path::new(r"c:\studio\game.UPROJECT");
        assert_eq!(project_config_file(a), project_config_file(b));
    }

    #[test]
    fn playtest_presets_round_trip_as_local_profile_data() {
        let preset = PlaytestPreset {
            name: "Level review".into(), log_window: true, windowed: true,
            no_sound: false, resolution: 1, extra_args: "-ExecCmds=stat fps".into(),
        };
        let encoded = serde_json::to_string(&vec![preset.clone()]).unwrap();
        let decoded: Vec<PlaytestPreset> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, vec![preset]);
    }

    #[test]
    fn same_named_preset_is_preserved_until_replacement_is_confirmed() {
        let original = PlaytestPreset { name: "Level review".into(), log_window: true,
            windowed: true, no_sound: false, resolution: 1, extra_args: String::new() };
        let replacement = PlaytestPreset { extra_args: "-nosplash".into(), ..original.clone() };
        let mut presets = vec![original.clone()];
        assert_eq!(upsert_playtest_preset(&mut presets, replacement.clone(), false), PresetSaveResult::ReplacementNeedsConfirmation);
        assert_eq!(presets, vec![original]);
        assert_eq!(upsert_playtest_preset(&mut presets, replacement.clone(), true), PresetSaveResult::Replaced);
        assert_eq!(presets, vec![replacement]);
    }

    #[test]
    fn failed_legacy_migration_never_returns_shared_settings() {
        let dir = std::env::temp_dir().join(format!("udt-config-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let project = Path::new(r"C:\Studio\Game.uproject");
        let current = project_settings_file_for_load_in(&dir, project, "build").unwrap();
        // Force rename to fail on Windows by placing a directory at the target.
        fs::create_dir(&current).unwrap();
        let legacy = dir.join("Game_build.cfg");
        fs::write(&legacy, "shared preferences").unwrap();

        let resolved = project_settings_file_for_load_in(&dir, project, "build").unwrap();
        assert_eq!(resolved, current);
        assert!(legacy.is_file(), "failed migration should leave legacy data intact");
        assert!(fs::read_to_string(&resolved).is_err(), "must not read the shared legacy file");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn first_same_named_project_claims_legacy_settings_without_sharing_them() {
        let dir = std::env::current_dir().unwrap().join("target").join(format!("udt-config-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let a_dir = dir.join("studio");
        let b_dir = dir.join("archive");
        fs::create_dir_all(&a_dir).unwrap();
        fs::create_dir_all(&b_dir).unwrap();
        let a = a_dir.join("Game.uproject");
        let b = b_dir.join("Game.uproject");
        let legacy = legacy_project_settings_file_in(&dir, &a, "build").unwrap();
        fs::write(&legacy, "legacy Game settings").unwrap();

        let migrated = project_settings_file_for_load_in(&dir, &a, "build").unwrap();
        assert_eq!(fs::read_to_string(&migrated).unwrap(), "legacy Game settings");
        assert!(!legacy.exists());

        let second = project_settings_file_for_load_in(&dir, &b, "build").unwrap();
        assert_ne!(migrated, second);
        assert!(!second.exists(), "a second same-name project must not inherit the legacy contents");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_switch_accepts_explicit_truthy_values() {
        for value in ["1", "true", "YES", " on "] {
            assert!(offline_value_enabled(value), "{value}");
        }
        for value in ["", "0", "false", "no"] {
            assert!(!offline_value_enabled(value), "{value}");
        }
    }
}
