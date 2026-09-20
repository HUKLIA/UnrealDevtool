//! Removing Unreal's regenerable folders.
//!
//! "Delete Intermediate, Saved and DerivedDataCache, then regenerate project
//! files" is the standard first move for a large share of Unreal build
//! failures, and it is done by hand in Explorer every time. Everything here is
//! rebuilt by the engine on the next open or build, so none of it is source —
//! but it is still the user's disk, so nothing is deleted without them
//! choosing the targets and confirming.

use std::path::{Path, PathBuf};

/// One removable folder under the project.
#[derive(Clone)]
pub struct Target {
    pub name:    &'static str,
    /// What it holds and what regenerates it, shown next to the checkbox.
    pub detail:  &'static str,
    pub path:    PathBuf,
    pub bytes:   u64,
    pub exists:  bool,
    /// Ticked by default. `Binaries` is not — deleting it forces a full C++
    /// rebuild, which is rarely what you want and very slow.
    pub default_on: bool,
}

/// The folders worth offering, in the order they are shown.
pub fn scan(project_dir: &Path) -> Vec<Target> {
    const SPEC: &[(&str, &str, bool)] = &[
        ("Intermediate",
         "Build artifacts and generated headers. Rebuilt on the next build.",
         true),
        ("Saved",
         "Logs, autosaves, cooked output and crash dumps. Rebuilt as you work.",
         true),
        ("DerivedDataCache",
         "Local shader and asset cache. Rebuilt on demand — first load is slower.",
         true),
        ("Binaries",
         "Compiled C++ output. Forces a full rebuild, which takes a long time.",
         false),
    ];

    SPEC.iter().map(|(name, detail, default_on)| {
        let path = project_dir.join(name);
        let exists = path.is_dir();
        Target {
            name,
            detail,
            bytes: if exists { dir_size(&path, 0) } else { 0 },
            path,
            exists,
            default_on: *default_on,
        }
    }).collect()
}

/// Recursive size with a depth cap, same bound as the build scanner: these
/// trees hold tens of thousands of files and the number only needs to be
/// right to "1.4 GB" precision.
fn dir_size(dir: &Path, depth: u32) -> u64 {
    if depth > 5 { return 0; }
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().map(|e| match e.metadata() {
        Ok(m) if m.is_file() => m.len(),
        Ok(m) if m.is_dir()  => dir_size(&e.path(), depth + 1),
        _ => 0,
    }).sum()
}

/// Deletes each path given. Returns a human-readable summary.
///
/// Blocking, so callers run it on a background thread. A failure on one folder
/// does not stop the others — a locked file in `Saved` (the editor holding a
/// log open, most often) should not prevent `Intermediate` from going.
pub fn remove(paths: &[PathBuf]) -> String {
    let mut freed = 0u64;
    let mut done: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| p.display().to_string());
        let before = dir_size(p, 0);
        match std::fs::remove_dir_all(p) {
            Ok(()) => { freed += before; done.push(name); }
            Err(e) => failed.push(format!("{name} ({e})")),
        }
    }

    let freed_s = crate::ops::history::format_bytes(freed);
    if failed.is_empty() {
        format!("[OK] Cleaned {} — {freed_s} freed.\n\
                 Regenerate project files next, then build.", done.join(", "))
    } else {
        format!("[WARNING] Cleaned {} — {freed_s} freed.\n\
                 Could not remove: {}.\n\
                 Close the Unreal Editor and any open file in those folders, then retry.",
                if done.is_empty() { "nothing".to_string() } else { done.join(", ") },
                failed.join(", "))
    }
}
