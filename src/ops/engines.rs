//! Installed Unreal Engine versions.
//!
//! Anyone who has worked across projects has several engines installed side by
//! side. This lists them (from the registry entries the Epic launcher writes,
//! from custom builds, and from sibling folders of engines already in use) and
//! reads each one's real version from `Engine/Build/Build.version`, so the app
//! can say which engine a project asks for and which one it is using.

use std::path::{Path, PathBuf};

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

#[derive(Clone, PartialEq, Debug)]
pub struct EngineInstall {
    pub dir:     PathBuf,
    /// `5.7.1` when `Build.version` could be read, else the folder name.
    pub version: String,
    /// `5.7` — the form a `.uproject`'s `EngineAssociation` uses.
    pub short:   Option<String>,
}

/// (major, minor, patch) from `Engine/Build/Build.version`.
pub fn read_version(engine: &Path) -> Option<(u32, u32, u32)> {
    let text = std::fs::read_to_string(engine.join("Engine").join("Build").join("Build.version")).ok()?;
    let json: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let n = |k: &str| json.get(k).and_then(|v| v.as_u64()).map(|v| v as u32);
    Some((n("MajorVersion")?, n("MinorVersion")?, n("PatchVersion").unwrap_or(0)))
}

fn describe(dir: PathBuf) -> Option<EngineInstall> {
    if !crate::engine::is_valid_engine_dir(&dir) { return None; }
    let (version, short) = match read_version(&dir) {
        Some((a, b, c)) => (format!("{a}.{b}.{c}"), Some(format!("{a}.{b}"))),
        None => (dir.file_name().unwrap_or_default().to_string_lossy().to_string(), None),
    };
    Some(EngineInstall { dir, version, short })
}

/// Every engine that can be found. `also_near` are engines already known (the
/// one in use, a manual override): their sibling folders are checked too,
/// which finds installs the registry does not list.
pub fn list(also_near: &[PathBuf]) -> Vec<EngineInstall> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\EpicGames\\Unreal Engine") {
        for name in k.enum_keys().flatten() {
            if let Ok(sub) = k.open_subkey(&name)
                && let Ok(dir) = sub.get_value::<String, _>("InstalledDirectory") {
                dirs.push(PathBuf::from(dir));
            }
        }
    }
    if let Ok(b) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Epic Games\\Unreal Engine\\Builds") {
        for (_, v) in b.enum_values().flatten() {
            dirs.push(PathBuf::from(v.to_string()));
        }
    }
    for known in also_near {
        dirs.push(known.clone());
        if let Some(parent) = known.parent() && let Ok(rd) = std::fs::read_dir(parent) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with("ue_") || name.starts_with("ue4_") || name.starts_with("unrealengine") {
                    dirs.push(e.path());
                }
            }
        }
    }

    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<EngineInstall> = Vec::new();
    for d in dirs {
        let key = d.to_string_lossy().to_ascii_lowercase().trim_end_matches(['\\', '/']).to_string();
        if seen.contains(&key) { continue; }
        seen.push(key);
        if let Some(e) = describe(d) { out.push(e); }
    }
    // Newest first.
    out.sort_by_key(|e| std::cmp::Reverse(version_key(&e.version)));
    out
}

fn version_key(v: &str) -> (u32, u32, u32) {
    let mut p = v.split('.').map(|x| x.parse::<u32>().unwrap_or(0));
    (p.next().unwrap_or(0), p.next().unwrap_or(0), p.next().unwrap_or(0))
}

/// The `EngineAssociation` a project asks for.
pub fn project_association(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project).ok()?;
    let json: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    json.get("EngineAssociation")?.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_engine(root: &Path, name: &str, version: Option<&str>) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("Engine/Build/BatchFiles")).unwrap();
        std::fs::write(dir.join("Engine/Build/BatchFiles/RunUAT.bat"), "").unwrap();
        if let Some(v) = version {
            std::fs::write(dir.join("Engine/Build/Build.version"), v).unwrap();
        }
        dir
    }

    #[test]
    fn sibling_engines_are_found_versioned_and_sorted_newest_first() {
        let root = std::env::temp_dir().join(format!("udt-engines-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let e56 = fake_engine(&root, "UE_5.6", Some(r#"{"MajorVersion":5,"MinorVersion":6,"PatchVersion":1}"#));
        fake_engine(&root, "UE_5.7", Some(r#"{"MajorVersion":5,"MinorVersion":7,"PatchVersion":0}"#));
        fake_engine(&root, "UE_5.10", None);
        std::fs::create_dir_all(root.join("UE_broken")).unwrap(); // no RunUAT: not an engine
        std::fs::create_dir_all(root.join("Other")).unwrap();

        let found: Vec<EngineInstall> = list(&[e56.clone()]).into_iter()
            .filter(|e| e.dir.starts_with(&root)).collect();
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].version, "5.7.0");
        assert_eq!(found[0].short.as_deref(), Some("5.7"));
        assert_eq!(found[1].version, "5.6.1");
        // No Build.version: named by its folder, and no association form.
        assert_eq!((found[2].version.as_str(), found[2].short.as_deref()), ("UE_5.10", None));
        assert_eq!(read_version(&e56), Some((5, 6, 1)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_projects_engine_association_is_read() {
        let root = std::env::temp_dir().join(format!("udt-assoc-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("G.uproject");
        std::fs::write(&f, r#"{"EngineAssociation":"5.7"}"#).unwrap();
        assert_eq!(project_association(&f).as_deref(), Some("5.7"));
        std::fs::write(&f, r#"{"EngineAssociation":""}"#).unwrap();
        assert_eq!(project_association(&f), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
