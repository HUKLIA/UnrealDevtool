//! Packaging-readiness checks: things in the project's own files that are
//! known to make a packaged build fail, or succeed and then misbehave.
//!
//! Everything here is a plain file read of a handful of small config files —
//! no Unreal, no process, no network — so it is safe to run on the UI thread.
//! A check only reports what it can actually see; anything it cannot read
//! (a config Unreal has not generated yet) is treated as "unknown", never as a
//! failure.

use std::path::Path;

use crate::ops::preflight::{CheckItem, CheckStatus};
use crate::types::BuildTarget;

fn item(status: CheckStatus, label: &str, detail: impl Into<String>) -> CheckItem {
    CheckItem { status, label: label.into(), detail: detail.into() }
}

/// Runs every readiness check for `project` when building for `target`.
pub fn check_project(project: &Path, target: BuildTarget) -> Vec<CheckItem> {
    let Some(dir) = project.parent() else { return Vec::new() };
    let mut out = Vec::new();

    // The project file itself.
    let descriptor = std::fs::read_to_string(project)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let Some(desc) = descriptor else {
        out.push(item(CheckStatus::Fail, "Project file",
            "The .uproject could not be read as JSON. Unreal will not open it either."));
        return out;
    };

    // Code or Blueprint-only.
    let has_modules = desc.get("Modules").and_then(|m| m.as_array()).is_some_and(|m| !m.is_empty());
    if has_modules {
        if dir.join("Source").is_dir() {
            out.push(item(CheckStatus::Ok, "C++ project", "Source folder present."));
            if !dir.join("Binaries").is_dir() {
                out.push(item(CheckStatus::Ok, "Not compiled yet",
                    "There is no Binaries folder, so the first build compiles everything and will be slow."));
            }
        } else {
            out.push(item(CheckStatus::Fail, "Source folder missing",
                "The project lists code modules but has no Source folder — packaging cannot compile it."));
        }
    } else {
        out.push(item(CheckStatus::Ok, "Blueprint-only project", "Nothing to compile."));
    }

    // Default map: without one a packaged game starts in an empty world.
    let engine_ini = std::fs::read_to_string(dir.join("Config").join("DefaultEngine.ini")).ok();
    match engine_ini.as_deref().and_then(|t| ini_value(t, "/Script/EngineSettings.GameMapsSettings", "GameDefaultMap")) {
        Some(map) if !map.is_empty() => {
            match map_file(dir, &map) {
                Some(path) if path.is_file() =>
                    out.push(item(CheckStatus::Ok, "Default map", map)),
                Some(_) => out.push(item(CheckStatus::Warn, "Default map not found",
                    format!("{map} is set as the game's start map but no matching file is under Content."))),
                None => out.push(item(CheckStatus::Ok, "Default map", map)),
            }
        }
        _ if engine_ini.is_none() => {} // no config yet: unknown, not a failure
        _ => out.push(item(CheckStatus::Warn, "No default map set",
            "GameDefaultMap is not set in DefaultEngine.ini. A packaged game will open to an empty world. \
             Set it in Project Settings → Maps & Modes.")),
    }

    // Android needs a real package name; the template default is rejected by
    // the Play Store and by some device installers.
    if target == BuildTarget::Android {
        let game_ini = std::fs::read_to_string(dir.join("Config").join("DefaultEngine.ini")).ok();
        let name = game_ini.as_deref()
            .and_then(|t| ini_value(t, "/Script/AndroidRuntimeSettings.AndroidRuntimeSettings", "PackageName"));
        match name {
            Some(n) if !n.contains("YourCompany") && !n.trim().is_empty() =>
                out.push(item(CheckStatus::Ok, "Android package name", n)),
            Some(n) => out.push(item(CheckStatus::Warn, "Android package name is the template default",
                format!("{n} — set your own in Project Settings → Platforms → Android."))),
            None => out.push(item(CheckStatus::Warn, "Android package name not set",
                "The default com.YourCompany.[PROJECT] is used. Set your own in Project Settings → Platforms → Android.")),
        }
    }

    out
}

/// The value of `key` inside `[section]` of an Unreal `.ini`, ignoring
/// comments, quotes and the `+`/`-`/`!` list prefixes.
pub fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    let mut found = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with(';') || line.is_empty() { continue; }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_section = name.eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section { continue; }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim().trim_start_matches(['+', '-', '.', '!']);
        if k.eq_ignore_ascii_case(key) {
            // The last assignment wins, as it does in Unreal.
            found = Some(v.trim().trim_matches('"').to_string());
        }
    }
    found
}

/// `/Game/Maps/Main.Main` → `<project>/Content/Maps/Main.umap`.
fn map_file(project_dir: &Path, asset_path: &str) -> Option<std::path::PathBuf> {
    let rest = asset_path.strip_prefix("/Game/")?;
    let no_object = rest.split('.').next()?;
    Some(project_dir.join("Content").join(format!("{no_object}.umap")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, uproject: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("udt-doctor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Config")).unwrap();
        std::fs::create_dir_all(root.join("Content/Maps")).unwrap();
        let file = root.join("Game.uproject");
        std::fs::write(&file, uproject).unwrap();
        (root, file)
    }

    fn statuses(items: &[CheckItem]) -> Vec<(&str, bool)> {
        items.iter().map(|i| (i.label.as_str(), matches!(i.status, CheckStatus::Ok))).collect()
    }

    #[test]
    fn ini_lookup_handles_sections_prefixes_and_last_wins() {
        let text = "; c\n[A]\nKey=1\n[/Script/X.Y]\n+Foo=bar\nKey=\"old\"\nKey=new\n[B]\nKey=other\n";
        assert_eq!(ini_value(text, "/Script/X.Y", "Key").as_deref(), Some("new"));
        assert_eq!(ini_value(text, "/Script/X.Y", "Foo").as_deref(), Some("bar"));
        assert_eq!(ini_value(text, "/script/x.y", "key").as_deref(), Some("new"), "case-insensitive");
        assert_eq!(ini_value(text, "Nope", "Key"), None);
    }

    #[test]
    fn a_blueprint_project_without_a_default_map_is_warned() {
        let (root, file) = project("bp", r#"{"FileVersion":3,"EngineAssociation":"5.7"}"#);
        std::fs::write(root.join("Config/DefaultEngine.ini"), "[/Script/EngineSettings.GameMapsSettings]\n").unwrap();
        let out = check_project(&file, BuildTarget::Win64);
        assert!(out.iter().any(|i| i.label == "Blueprint-only project"));
        assert!(out.iter().any(|i| i.label == "No default map set" && matches!(i.status, CheckStatus::Warn)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_default_map_is_resolved_against_content() {
        let (root, file) = project("map", r#"{"Modules":[]}"#);
        std::fs::write(root.join("Config/DefaultEngine.ini"),
            "[/Script/EngineSettings.GameMapsSettings]\nGameDefaultMap=/Game/Maps/Main.Main\n").unwrap();
        assert!(check_project(&file, BuildTarget::Win64).iter()
            .any(|i| i.label == "Default map not found"));
        std::fs::write(root.join("Content/Maps/Main.umap"), "x").unwrap();
        let ok = check_project(&file, BuildTarget::Win64);
        assert!(ok.iter().any(|i| i.label == "Default map" && matches!(i.status, CheckStatus::Ok)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn code_projects_need_a_source_folder_and_android_needs_a_package_name() {
        let (root, file) = project("cpp", r#"{"Modules":[{"Name":"Game"}]}"#);
        let out = check_project(&file, BuildTarget::Win64);
        assert!(out.iter().any(|i| i.label == "Source folder missing" && matches!(i.status, CheckStatus::Fail)));

        std::fs::create_dir_all(root.join("Source")).unwrap();
        let out = check_project(&file, BuildTarget::Win64);
        assert!(out.iter().any(|i| i.label == "Not compiled yet"));
        assert!(!out.iter().any(|i| i.label.starts_with("Android")), "only when building for Android");

        std::fs::write(root.join("Config/DefaultEngine.ini"),
            "[/Script/AndroidRuntimeSettings.AndroidRuntimeSettings]\nPackageName=com.YourCompany.[PROJECT]\n").unwrap();
        let out = check_project(&file, BuildTarget::Android);
        assert!(out.iter().any(|i| i.label.contains("template default")));
        std::fs::write(root.join("Config/DefaultEngine.ini"),
            "[/Script/AndroidRuntimeSettings.AndroidRuntimeSettings]\nPackageName=com.acme.fish\n").unwrap();
        let out = check_project(&file, BuildTarget::Android);
        assert!(statuses(&out).contains(&("Android package name", true)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_broken_project_file_is_a_failure_and_stops_early() {
        let (root, file) = project("bad", "{ not json");
        let out = check_project(&file, BuildTarget::Win64);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].status, CheckStatus::Fail));
        std::fs::remove_dir_all(root).unwrap();
    }
}
