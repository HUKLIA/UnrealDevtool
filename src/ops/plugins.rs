//! Plugins: which ones a project enables, and turning them on or off.
//!
//! A project's plugin list is the `Plugins` array in its `.uproject`. Editing
//! it is the same thing the editor's Plugins window does, minus opening the
//! editor — useful for switching off a plugin that stops the project loading,
//! or checking what a build is actually pulling in. The file is rewritten with
//! its keys in the original order and Unreal's tab indentation, after a
//! one-time backup.

use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Debug)]
pub struct PluginEntry {
    pub name:        String,
    pub enabled:     bool,
    /// Lives in the project's own `Plugins` folder (as opposed to the engine's).
    pub local:       bool,
    pub description: String,
    pub version:     String,
}

fn read_descriptor(project: &Path) -> Result<(serde_json::Value, bool), String> {
    let text = std::fs::read_to_string(project).map_err(|e| format!("Could not read the .uproject: {e}"))?;
    let crlf = text.contains("\r\n");
    let json = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("The .uproject is not valid JSON: {e}"))?;
    Ok((json, crlf))
}

/// Plugins found under `<project>/Plugins`, as (name, description, version).
fn local_plugins(project: &Path) -> Vec<(String, String, String)> {
    let Some(dir) = project.parent().map(|d| d.join("Plugins")) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![(dir, 0u8)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 3 { stack.push((p, depth + 1)); }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("uplugin")) {
                let name = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let meta = std::fs::read_to_string(&p).ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok());
                let field = |k: &str| meta.as_ref()
                    .and_then(|m| m.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
                out.push((name, field("Description"), field("VersionName")));
            }
        }
    }
    out
}

/// Every plugin the project mentions or ships, alphabetically.
pub fn list(project: &Path) -> Result<Vec<PluginEntry>, String> {
    let (json, _) = read_descriptor(project)?;
    let mut entries: Vec<PluginEntry> = json.get("Plugins").and_then(|p| p.as_array())
        .map(|a| a.iter().filter_map(|p| {
            let name = p.get("Name")?.as_str()?.to_string();
            let enabled = p.get("Enabled").and_then(|e| e.as_bool()).unwrap_or(false);
            Some(PluginEntry { name, enabled, local: false, description: String::new(), version: String::new() })
        }).collect())
        .unwrap_or_default();

    for (name, description, version) in local_plugins(project) {
        match entries.iter_mut().find(|e| e.name == name) {
            Some(e) => { e.local = true; e.description = description; e.version = version; }
            // A plugin in the folder that the .uproject does not list is
            // enabled by default unless its descriptor says otherwise; show it
            // as not listed rather than guessing.
            None => entries.push(PluginEntry { name, enabled: false, local: true, description, version }),
        }
    }
    entries.sort_by_key(|e| e.name.to_ascii_lowercase());
    Ok(entries)
}

/// Turns one plugin on or off in the `.uproject`.
///
/// Makes `<project>.uproject.devtool-backup` first if there is not one yet, so
/// the original is always recoverable however many edits follow.
pub fn set_enabled(project: &Path, name: &str, enabled: bool) -> Result<(), String> {
    let (mut json, crlf) = read_descriptor(project)?;
    let root = json.as_object_mut().ok_or("The .uproject is not a JSON object.")?;
    let plugins = root.entry("Plugins").or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let list = plugins.as_array_mut().ok_or("The .uproject's Plugins entry is not a list.")?;

    match list.iter_mut().find(|p| p.get("Name").and_then(|n| n.as_str()) == Some(name)) {
        Some(p) => {
            if let Some(o) = p.as_object_mut() {
                o.insert("Enabled".into(), serde_json::Value::Bool(enabled));
            }
        }
        None => {
            let mut o = serde_json::Map::new();
            o.insert("Name".into(), name.into());
            o.insert("Enabled".into(), enabled.into());
            list.push(serde_json::Value::Object(o));
        }
    }

    let backup = backup_path(project);
    if !backup.exists() {
        std::fs::copy(project, &backup).map_err(|e| format!("Could not make a backup: {e}"))?;
    }
    let mut text = to_tab_json(&json)?;
    if crlf { text = text.replace('\n', "\r\n"); }
    std::fs::write(project, text).map_err(|e| format!("Could not write the .uproject: {e}"))
}

/// Points the project at a different engine (`"5.6"`, or a custom build's GUID).
///
/// The same backed-up, order-preserving rewrite as [`set_enabled`]. This does
/// not convert anything: the next open of the project in that engine is what
/// upgrades (or downgrades) the assets, which is why the UI warns first.
pub fn set_engine_association(project: &Path, value: &str) -> Result<(), String> {
    let (mut json, crlf) = read_descriptor(project)?;
    let root = json.as_object_mut().ok_or("The .uproject is not a JSON object.")?;
    root.insert("EngineAssociation".into(), value.into());

    let backup = backup_path(project);
    if !backup.exists() {
        std::fs::copy(project, &backup).map_err(|e| format!("Could not make a backup: {e}"))?;
    }
    let mut text = to_tab_json(&json)?;
    if crlf { text = text.replace('\n', "\r\n"); }
    std::fs::write(project, text).map_err(|e| format!("Could not write the .uproject: {e}"))
}

pub fn backup_path(project: &Path) -> PathBuf {
    let mut s = project.as_os_str().to_os_string();
    s.push(".devtool-backup");
    PathBuf::from(s)
}

/// Pretty JSON with tab indentation, the way Unreal writes descriptors.
fn to_tab_json(v: &serde_json::Value) -> Result<String, String> {
    use serde::Serialize;
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    v.serialize(&mut ser).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tag: &str, body: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("udt-plugins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Plugins/Local")).unwrap();
        let file = root.join("Game.uproject");
        std::fs::write(&file, body).unwrap();
        (root, file)
    }

    const BODY: &str = "{\n\t\"FileVersion\": 3,\n\t\"EngineAssociation\": \"5.7\",\n\t\"Plugins\": [\n\t\t{\n\t\t\t\"Name\": \"Water\",\n\t\t\t\"Enabled\": true\n\t\t}\n\t]\n}";

    #[test]
    fn listing_merges_the_project_list_with_local_plugins() {
        let (root, file) = project("list", BODY);
        std::fs::write(root.join("Plugins/Local/Cool.uplugin"),
            r#"{"Description":"Does cool things","VersionName":"2.1"}"#).unwrap();
        let l = list(&file).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "Cool");
        assert!(l[0].local && !l[0].enabled, "in the folder but not listed by the project");
        assert_eq!((l[0].description.as_str(), l[0].version.as_str()), ("Does cool things", "2.1"));
        assert!(l[1].name == "Water" && l[1].enabled && !l[1].local);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggling_edits_in_place_keeps_key_order_and_makes_one_backup() {
        let (root, file) = project("set", BODY);
        set_enabled(&file, "Water", false).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert!(after.contains("\t\t\t\"Enabled\": false"), "tab indentation, value flipped: {after}");
        assert!(after.find("FileVersion").unwrap() < after.find("EngineAssociation").unwrap()
            && after.find("EngineAssociation").unwrap() < after.find("Plugins").unwrap(),
            "keys keep their order");
        let backup = std::fs::read_to_string(backup_path(&file)).unwrap();
        assert_eq!(backup, BODY, "the backup is the original");

        // A second edit does not overwrite the backup.
        set_enabled(&file, "Water", true).unwrap();
        assert_eq!(std::fs::read_to_string(backup_path(&file)).unwrap(), BODY);
        // Enabling a plugin the file does not list adds it.
        set_enabled(&file, "Extra", true).unwrap();
        let l = list(&file).unwrap();
        assert!(l.iter().any(|p| p.name == "Extra" && p.enabled));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crlf_files_stay_crlf_and_bad_json_is_an_error_not_a_wipe() {
        let (root, file) = project("crlf", &BODY.replace('\n', "\r\n"));
        set_enabled(&file, "Water", false).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("\r\n"));

        std::fs::write(&file, "{ broken").unwrap();
        assert!(set_enabled(&file, "Water", true).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ broken", "untouched on error");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_engine_association_can_be_changed_in_place() {
        let (root, file) = project("assoc", BODY);
        set_engine_association(&file, "5.6").unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert!(after.contains("\"EngineAssociation\": \"5.6\""));
        assert!(after.find("FileVersion").unwrap() < after.find("EngineAssociation").unwrap(), "position kept");
        assert_eq!(std::fs::read_to_string(backup_path(&file)).unwrap(), BODY);
        std::fs::remove_dir_all(root).unwrap();
    }
}
