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
        .find(|c| c.is_dir() || std::fs::create_dir_all(c).is_ok())
}

fn alias_name(target: &Path) -> String {
    let base = target
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| std::borrow::Cow::Borrowed("link"));
    let safe_base: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in target.to_string_lossy().to_ascii_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    format!("{}-{:016x}", safe_base.trim_matches('_'), hash)
}

fn aliases_target(link: &Path, target: &Path) -> bool {
    std::fs::canonicalize(link).ok() == std::fs::canonicalize(target).ok()
}

/// Ensures a space-free directory junction exists pointing at `target` and
/// returns its path. If `target` already has no space, returns it unchanged.
/// Reuses an existing junction of the same name rather than recreating it —
/// junctions don't require admin rights on Windows, only a writable parent.
pub fn ensure_space_free_alias(target: &Path) -> Result<PathBuf, String> {
    if !has_space(target) { return Ok(target.to_path_buf()); }

    if !target.is_dir() {
        return Err(format!("target folder does not exist: {}", target.display()));
    }

    let root = link_root(target)
        .ok_or_else(|| "couldn't find a writable space-free folder to link from".to_string())?;
    let legacy_link = target.file_name().map(|n| root.join(n));
    if let Some(link) = &legacy_link
        && link.is_dir()
        && !has_space(link)
        && aliases_target(link, target)
    {
        return Ok(link.clone());
    }

    // Include a stable target hash so two projects with the same folder name
    // cannot accidentally reuse one another's junction.
    let link = root.join(alias_name(target));

    if link.exists() {
        if aliases_target(&link, target) {
            return Ok(link);
        }
        return Err(format!(
            "space-free link already exists and points elsewhere: {}",
            link.display()
        ));
    }

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

/// Runs the checks that need no I/O — pure string/path inspection, safe to
/// call directly on the UI thread. The disk-space check is separate (see
/// [`disk_space_check_item`]) since it needs a background thread.
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

    // Detail text here is intentionally terse (just the offending path) —
    // the full explanation and the one-click fix live in the amber callout
    // `show_space_warning_inline` renders right below this list. Repeating
    // the whole paragraph here too just doubled the same warning in two
    // different visual styles back to back.
    if let Some(e) = engine_dir.as_ref().filter(|e| has_space(e)) {
        items.push(CheckItem {
            status: CheckStatus::Warn,
            label:  "Engine path has spaces".into(),
            detail: format!("{}  (see fix below)", e.display()),
        });
    }

    if let Some(dir) = project_path.as_ref().and_then(|p| p.parent()).filter(|d| has_space(d)) {
        items.push(CheckItem {
            status: CheckStatus::Warn,
            label:  "Project path has spaces".into(),
            detail: format!("{}  (see fix below)", dir.display()),
        });
    }

    items
}

/// Disk-space check, split out from [`run_checks`] because it shells out to
/// PowerShell — slow enough (cold-start overhead, sometimes 200ms-1s+) that
/// running it synchronously on the UI thread freezes the window (Windows
/// shows the "not responding" ghost overlay until it returns). Callers must
/// run this on a background thread, same as every other slow operation in
/// this app, and post the result back via a shared `Arc<Mutex<_>>`.
pub fn disk_space_check_item(dir: &Path) -> Option<CheckItem> {
    let free_gb = free_space_gb(dir)?;
    Some(if free_gb < 15.0 {
        CheckItem {
            status: CheckStatus::Warn, label: "Disk space".into(),
            detail: format!(
                "Only {free_gb:.1} GB free on that drive — cook + stage + archive + zip \
                 typically needs 15-30+ GB."
            ),
        }
    } else {
        CheckItem {
            status: CheckStatus::Ok, label: "Disk space".into(),
            detail: format!("{free_gb:.1} GB free"),
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_spaces_without_treating_clean_paths_as_links() {
        assert!(has_space(Path::new(r"C:\Program Files\Epic Games")));
        assert!(!has_space(Path::new(r"C:\UEDevToolLink\UE_5.4")));
    }

    #[test]
    fn alias_names_are_space_free_and_target_specific() {
        let a = alias_name(Path::new(r"C:\Games\My Project"));
        let b = alias_name(Path::new(r"D:\Games\My Project"));
        assert!(!a.contains(' '));
        assert_ne!(a, b);
    }
}
