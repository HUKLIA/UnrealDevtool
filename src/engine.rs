use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER};
use winreg::RegKey;

/// Resolve official Launcher versions, custom build aliases/GUIDs, and native
/// project layouts without guessing that the newest engine is compatible.
pub(crate) fn find_engine_by_association(assoc: &str) -> Option<PathBuf> {
    let assoc = assoc.trim();
    let custom = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Epic Games\\Unreal Engine\\Builds").ok()
        .and_then(|k| k.get_value::<String, _>(assoc).ok()).map(PathBuf::from);
    if let Some(p) = custom.filter(|p| is_valid_engine_dir(p)) { return Some(p); }
    let installed = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!("SOFTWARE\\EpicGames\\Unreal Engine\\{assoc}")).ok()
        .and_then(|k| k.get_value::<String, _>("InstalledDirectory").ok()).map(PathBuf::from);
    if let Some(p) = installed.filter(|p| is_valid_engine_dir(p)) { return Some(p); }
    launcher_installs().into_iter().find(|(id, p)| id == assoc && is_valid_engine_dir(p)).map(|(_, p)| p)
}

pub(crate) fn launcher_installs() -> Vec<(String, PathBuf)> {
    let Some(root) = std::env::var_os("ProgramData") else { return Vec::new() };
    let path = PathBuf::from(root).join("Epic/UnrealEngineLauncher/LauncherInstalled.dat");
    let Some(json) = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else { return Vec::new() };
    json.get("InstallationList").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|v| {
        let id = v.get("AppName")?.as_str()?.strip_prefix("UE_")?.to_string();
        Some((id, PathBuf::from(v.get("InstallLocation")?.as_str()?)))
    }).collect()
}

pub(crate) fn project_engine(project: &Path) -> Option<PathBuf> {
    if let Some(assoc) = crate::ops::engines::project_association(project) {
        return find_engine_by_association(&assoc);
    }
    // Empty association is meaningful for a native project: search ancestors.
    project.parent()?.ancestors().find(|p| is_valid_engine_dir(p)).map(Path::to_path_buf)
}

/// True if `dir` looks like the root of an Unreal Engine install — i.e. it
/// contains `Engine\Build\BatchFiles\RunUAT.bat`. Used both by auto-detection
/// and to validate a folder the user picks manually.
pub fn is_valid_engine_dir(dir: &Path) -> bool {
    dir.join("Engine\\Build\\BatchFiles\\RunUAT.bat").exists()
}

/// Finds the Unreal Engine installation that matches `uproject` (if provided),
/// then falls back to the discovered installations sorted by their real engine
/// version, including Launcher metadata and custom builds. Using the project's `EngineAssociation` field avoids picking
/// the wrong version when multiple engine versions are installed side-by-side.
pub fn detect_unreal_engine(uproject: Option<&Path>) -> Option<PathBuf> {
    if let Some(proj) = uproject && let Some(dir) = project_engine(proj) {
        return Some(dir);
    }
    crate::ops::engines::list(&[]).into_iter().next().map(|e| e.dir)
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

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn empty_association_uses_native_engine_in_an_ancestor() {
        let root = std::env::temp_dir().join(format!("udt-native-{}-{}", std::process::id(), crate::ops::editor_repair::unique_id()));
        std::fs::create_dir_all(root.join("Engine/Build/BatchFiles")).unwrap();
        std::fs::write(root.join("Engine/Build/BatchFiles/RunUAT.bat"), "fixture").unwrap();
        std::fs::create_dir_all(root.join("Games/Native")).unwrap();
        let project = root.join("Games/Native/Native.uproject");
        std::fs::write(&project, r#"{"EngineAssociation":""}"#).unwrap();
        assert_eq!(project_engine(&project), Some(root.clone()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
