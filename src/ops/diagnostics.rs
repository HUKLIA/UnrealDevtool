use std::fs;
use std::path::{Path, PathBuf};

/// One recognized failure pattern in a `BuildLog.txt`, with a plain-English
/// explanation and suggested fix. Substring match, not regex — every pattern
/// here is a distinctive, literal string taken from real UAT/UBT output, so
/// substring matching is precise enough without pulling in a regex crate.
struct ErrorSignature {
    pattern:     &'static str,
    explanation: &'static str,
    fix:         &'static str,
}

/// Best-effort, not exhaustive — a small table of the most common ways UAT
/// packaging fails, seeded from well-documented Unreal Engine issues. Easy
/// to extend: add a row here for any new signature you recognize in a log.
static KNOWN_ERRORS: &[ErrorSignature] = &[
    ErrorSignature {
        pattern: "is not recognized as an internal or external command",
        explanation: "UAT/UBT's batch scripts broke on a space in a path — most commonly the \
                      default \"C:\\Program Files\\Epic Games\\...\" engine install, or a \
                      project folder with a space in its name.",
        fix: "Use the space-free-link fix in Check PC Setup / the package config panel \
              (creates an NTFS junction to a space-free path automatically).",
    },
    ErrorSignature {
        pattern: "There is not enough space on the disk",
        explanation: "The build drive ran out of space mid-cook/stage/archive.",
        fix: "Free up disk space (cook + stage + archive + zip typically needs 15-30+ GB), \
              or point the project/build output at a drive with more room.",
    },
    ErrorSignature {
        pattern: "because it is being used by another process",
        explanation: "A file UAT needs to write is locked by another process — usually the \
                      Unreal Editor still has the project open, or an antivirus scan is \
                      holding the file.",
        fix: "Close the Unreal Editor before packaging (there's a toggle for this in the \
              package config panel), or add the project/engine folders to your antivirus's \
              exclusion list.",
    },
    ErrorSignature {
        pattern: "Appropriate MSBuild.exe not found",
        explanation: "No compatible Visual Studio / MSBuild installation was found for \
                      compiling the C++ project.",
        fix: "Install Visual Studio (or the Build Tools) with the \"Game development with \
              C++\" workload, matching the Unreal Engine version's requirements.",
    },
    ErrorSignature {
        pattern: "fatal error C1083: Cannot open include file",
        explanation: "A C++ compile step couldn't find a header file — usually a missing \
                      module dependency or a stale/corrupted Intermediate folder.",
        fix: "Try \"Rebuild Visual Studio Files\", and if that doesn't help, delete the \
              project's Intermediate and Binaries folders and rebuild from scratch.",
    },
    ErrorSignature {
        pattern: "UnrealBuildTool Exception",
        explanation: "UnrealBuildTool hit an unhandled exception, usually from a corrupted \
                      intermediate build state or a malformed .Build.cs / .Target.cs file.",
        fix: "Delete the project's Intermediate and Binaries folders and try again. If it \
              persists, check recent changes to any .Build.cs/.Target.cs files.",
    },
];

/// One matched signature from a scanned log.
pub struct Diagnosis {
    pub matched:     String,
    pub explanation: String,
    pub fix:         String,
}

/// Finds the most recently modified log worth scanning: either a packaging
/// run's `BuildLog.txt` (under `<project_dir>/build/vX.Y.Z/`) or a failed
/// "Rebuild Visual Studio Files" run's `GenerateProjectFiles.log` (written
/// straight to the project root, and only left behind on failure — a
/// successful rebuild deletes its own log). Picks whichever is newest so
/// the most recent failure — from either flow — is what gets diagnosed.
pub fn latest_build_log(project_path: &Path) -> Option<PathBuf> {
    let project_dir = project_path.parent()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;

    let mut consider = |log: PathBuf| {
        let Ok(meta) = fs::metadata(&log) else { return };
        let Ok(modified) = meta.modified() else { return };
        if best.as_ref().map_or(true, |(t, _)| modified > *t) {
            best = Some((modified, log));
        }
    };

    if let Ok(entries) = fs::read_dir(project_dir.join("build")) {
        for entry in entries.flatten() {
            consider(entry.path().join("BuildLog.txt"));
        }
    }
    consider(project_dir.join("GenerateProjectFiles.log"));

    best.map(|(_, p)| p)
}

/// Scans a build log for known error signatures. Pure disk read + substring
/// search — fast enough to call directly on the UI thread.
pub fn scan_build_log(log_path: &Path) -> Vec<Diagnosis> {
    let Ok(content) = fs::read_to_string(log_path) else { return Vec::new() };
    KNOWN_ERRORS.iter()
        .filter_map(|sig| {
            content.lines().find(|l| l.contains(sig.pattern)).map(|line| Diagnosis {
                matched:     line.trim().to_string(),
                explanation: sig.explanation.to_string(),
                fix:         sig.fix.to_string(),
            })
        })
        .collect()
}
