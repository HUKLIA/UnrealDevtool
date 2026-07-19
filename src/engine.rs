use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER};
use winreg::RegKey;

/// Reads the `EngineAssociation` field from a `.uproject` JSON file without
/// pulling in a JSON parser dependency.
fn read_engine_association(uproject: &Path) -> Option<String> {
    let content = std::fs::read_to_string(uproject).ok()?;
    let key = "\"EngineAssociation\"";
    let after_key = &content[content.find(key)? + key.len()..];
    let after_colon = after_key[after_key.find(':')? + 1..].trim_start();
    if !after_colon.starts_with('"') { return None; }
    let inner = &after_colon[1..];
    let value = inner[..inner.find('"')?].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

/// Resolves an `EngineAssociation` value to an engine directory:
/// - `"5.4"` / `"4.27"` → `HKLM\SOFTWARE\EpicGames\Unreal Engine\<ver>`
/// - `"{GUID}"` → `HKCU\Software\Epic Games\Unreal Engine\Builds\<guid>`
fn find_engine_by_association(assoc: &str) -> Option<PathBuf> {
    let assoc = assoc.trim();
    let dir = if assoc.starts_with('{') {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let builds = hkcu.open_subkey("Software\\Epic Games\\Unreal Engine\\Builds").ok()?;
        builds.get_value::<String, _>(assoc).ok()?
    } else {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let k = hklm.open_subkey(format!("SOFTWARE\\EpicGames\\Unreal Engine\\{}", assoc)).ok()?;
        k.get_value::<String, _>("InstalledDirectory").ok()?
    };
    let path = PathBuf::from(&dir);
    if is_valid_engine_dir(&path) { Some(path) } else { None }
}

/// True if `dir` looks like the root of an Unreal Engine install — i.e. it
/// contains `Engine\Build\BatchFiles\RunUAT.bat`. Used both by auto-detection
/// and to validate a folder the user picks manually.
pub fn is_valid_engine_dir(dir: &Path) -> bool {
    dir.join("Engine\\Build\\BatchFiles\\RunUAT.bat").exists()
}

/// Finds the Unreal Engine installation that matches `uproject` (if provided),
/// then falls back to scanning HKLM for the newest UE 5.x install, then HKCU
/// custom builds. Using the project's `EngineAssociation` field avoids picking
/// the wrong version when multiple engine versions are installed side-by-side.
pub fn detect_unreal_engine(uproject: Option<&Path>) -> Option<PathBuf> {
    if let Some(proj) = uproject
        && let Some(assoc) = read_engine_association(proj)
            && let Some(dir) = find_engine_by_association(&assoc) {
                return Some(dir);
            }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for minor in (0..=9).rev() {
        let key = format!("SOFTWARE\\EpicGames\\Unreal Engine\\5.{}", minor);
        if let Ok(k) = hklm.open_subkey(&key)
            && let Ok(dir) = k.get_value::<String, _>("InstalledDirectory") {
                let path = PathBuf::from(&dir);
                if is_valid_engine_dir(&path) { return Some(path); }
            }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(builds) = hkcu.open_subkey("Software\\Epic Games\\Unreal Engine\\Builds") {
        for (_, value) in builds.enum_values().flatten() {
            let path = PathBuf::from(value.to_string());
            if is_valid_engine_dir(&path) { return Some(path); }
        }
    }
    None
}

pub fn build_init_status(engine: &Option<PathBuf>, project: &Option<PathBuf>) -> String {
    let eng = match engine {
        Some(p) => format!("Engine: {}", p.display()),
        None    => "Engine: not found".into(),
    };
    let proj = match project {
        Some(p) => format!("Project: {}", p.file_name().unwrap_or_default().to_string_lossy()),
        None    => "Project: not set".into(),
    };
    format!("Ready  |  {}  |  {}", eng, proj)
}
