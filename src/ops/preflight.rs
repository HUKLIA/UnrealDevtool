use std::path::{Path, PathBuf};

/// True if `path`'s string form contains a space anywhere.
///
/// Unreal's own UAT/UBT batch scripts have long-standing bugs handling
/// spaces in paths — most commonly hit via the *default* Epic Games Launcher
/// install location (`C:\Program Files\Epic Games\UE_5.x`) or a project
/// folder with a space in its name. When it bites, packaging fails ~30
/// minutes in with a cryptic `'C:\Program' is not recognized...` buried in
/// the build log, so this is checked proactively instead.
pub fn has_space(path: &Path) -> bool {
    path.to_string_lossy().contains(' ')
}

fn drive_prefix(path: &Path) -> Option<String> {
    match path.components().next()? {
        std::path::Component::Prefix(p) => Some(p.as_os_str().to_string_lossy().to_string()),
        _ => None,
    }
}

/// Picks (and creates if needed) a space-free folder to hold directory
/// junctions that alias space-containing paths. Tries the same drive's root
/// first, then `%ProgramData%`.
fn link_root(same_drive_as: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(d) = drive_prefix(same_drive_as) {
        candidates.push(PathBuf::from(format!("{d}\\UEDevToolLink")));
    }
    if let Ok(pd) = std::env::var("ProgramData") {
        candidates.push(PathBuf::from(pd).join("UEDevToolLink"));
    }
    candidates.into_iter()
        .filter(|c| !has_space(c))
        .find(|c| c.exists() || std::fs::create_dir_all(c).is_ok())
}

/// Ensures a space-free directory junction exists pointing at `target` and
/// returns its path. If `target` already has no space, returns it unchanged.
/// Reuses an existing junction of the same name rather than recreating it —
/// junctions don't require admin rights on Windows, only a writable parent.
pub fn ensure_space_free_alias(target: &Path) -> Result<PathBuf, String> {
    if !has_space(target) { return Ok(target.to_path_buf()); }

    let root = link_root(target)
        .ok_or_else(|| "couldn't find a writable space-free folder to link from".to_string())?;
    let name = target.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "link".to_string());
    let link = root.join(&name);

    if link.exists() { return Ok(link); }

    let status = crate::ops::cmd("cmd")
        .arg("/c").arg("mklink").arg("/J")
        .arg(&link)
        .arg(target)
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() && link.exists() {
        Ok(link)
    } else {
        Err(format!(
            "mklink failed (exit {:?}) for {} — try moving it to a path without spaces instead",
            status.code(), target.display()
        ))
    }
}

// ── PC setup checks ───────────────────────────────────────────────────────

pub enum CheckStatus { Ok, Warn, Fail }

pub struct CheckItem {
    pub status: CheckStatus,
    pub label:  String,
    pub detail: String,
}

/// Runs a fast, synchronous set of environment checks — safe to call
/// directly from a UI button click (nothing here should take more than a
/// couple hundred ms).
pub fn run_checks(engine_dir: &Option<PathBuf>, project_path: &Option<PathBuf>) -> Vec<CheckItem> {
    let mut items = Vec::new();

    match engine_dir {
        Some(p) => items.push(CheckItem {
            status: CheckStatus::Ok, label: "Unreal Engine".into(), detail: p.display().to_string(),
        }),
        None => items.push(CheckItem {
            status: CheckStatus::Fail, label: "Unreal Engine".into(),
            detail: "Not found — use Browse… above to select your install folder.".into(),
        }),
    }

    match project_path {
        Some(p) if p.exists() => items.push(CheckItem {
            status: CheckStatus::Ok, label: "Project file".into(), detail: p.display().to_string(),
        }),
        Some(p) => items.push(CheckItem {
            status: CheckStatus::Fail, label: "Project file".into(),
            detail: format!("{} does not exist.", p.display()),
        }),
        None => items.push(CheckItem {
            status: CheckStatus::Fail, label: "Project file".into(), detail: "Not set.".into(),
        }),
    }

    if let Some(e) = engine_dir.as_ref().filter(|e| has_space(e)) {
        items.push(CheckItem {
            status: CheckStatus::Warn,
            label:  "Engine path has spaces".into(),
            detail: format!(
                "\"{}\" contains a space. Unreal's UAT/UBT batch scripts break on this \
                 (it's why the default \"C:\\Program Files\\Epic Games\\...\" install trips people up) \
                 — packaging can fail with a cryptic \"is not recognized\" error.",
                e.display()
            ),
        });
    }

    if let Some(dir) = project_path.as_ref().and_then(|p| p.parent()).filter(|d| has_space(d)) {
        items.push(CheckItem {
            status: CheckStatus::Warn,
            label:  "Project path has spaces".into(),
            detail: format!("\"{}\" contains a space, which can also break UAT.", dir.display()),
        });
    }

    if let Some(dir) = project_path.as_ref().and_then(|p| p.parent()) {
        if let Some(free_gb) = free_space_gb(dir) {
            if free_gb < 15.0 {
                items.push(CheckItem {
                    status: CheckStatus::Warn, label: "Disk space".into(),
                    detail: format!(
                        "Only {free_gb:.1} GB free on that drive — cook + stage + archive + zip \
                         typically needs 15-30+ GB."
                    ),
                });
            } else {
                items.push(CheckItem {
                    status: CheckStatus::Ok, label: "Disk space".into(),
                    detail: format!("{free_gb:.1} GB free"),
                });
            }
        }
    }

    items
}

fn free_space_gb(path: &Path) -> Option<f64> {
    let drive  = drive_prefix(path)?;
    let letter = drive.trim_end_matches(':').trim_end_matches('\\').chars().last()?;
    let ps = format!("[System.IO.DriveInfo]::new('{letter}').AvailableFreeSpace");
    let out = crate::ops::cmd("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .ok()?;
    let bytes: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(bytes / 1_073_741_824.0)
}
