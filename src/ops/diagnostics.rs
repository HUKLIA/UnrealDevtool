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
    ErrorSignature { pattern: "error LNK2019", explanation: "Unresolved C++ symbol.", fix: "Check module dependencies, definitions and API export macros." },
    ErrorSignature { pattern: "error LNK2005", explanation: "Duplicate symbol.", fix: "Inspect duplicate definitions and source files." },
    ErrorSignature { pattern: "fatal error C1060", explanation: "Compiler memory exhausted.", fix: "Reduce parallel compile jobs and check available RAM and page file." },
    ErrorSignature { pattern: "Out of memory", explanation: "A build tool exhausted memory.", fix: "Reduce worker count and check RAM and page file capacity." },
    ErrorSignature { pattern: "SDK is not installed properly", explanation: "Missing or incompatible SDK.", fix: "Run platform setup checks and install the SDK supported by your engine." },
    ErrorSignature { pattern: "Unable to instantiate module", explanation: "Module definition could not load.", fix: "Inspect the named Build.cs and plugin compatibility." },
    ErrorSignature { pattern: "LogBlueprint: Error:", explanation: "Blueprint compilation failed.", fix: "Open the named Blueprint, fix compile errors and save before cooking." },
    ErrorSignature { pattern: "LogDataValidation: Error:", explanation: "Asset validation failed.", fix: "Run data validation and inspect the named asset." },
    ErrorSignature { pattern: "LogCook: Error:", explanation: "Cook reported an error.", fix: "Inspect earlier errors for the asset or dependency causing this failure." },
    ErrorSignature { pattern: "Access is denied", explanation: "File access was refused.", fix: "Check permissions and Windows Security protection history; use a user-writable output folder." },
    ErrorSignature { pattern: "fatal error C1853", explanation: "Incompatible precompiled header.", fix: "Regenerate project files and rebuild intermediates with the selected compiler." },
    ErrorSignature { pattern: "Missing precompiled manifest", explanation: "Missing compatible module manifest.", fix: "Check plugin target support and rebuild for this engine and target." },

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
              package config panel), then check Windows Security protection history for a specific detection.",
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
    pub line_number: usize,
    pub context: String,
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
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
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
    scan_build_log_text(&content)
}

/// Same scan as [`scan_build_log`], but against an arbitrary string instead
/// of a file on disk — for a pasted log excerpt from somewhere other than
/// the most recent build (a teammate's log, an older run, etc.).
pub fn scan_build_log_text(content: &str) -> Vec<Diagnosis> {
    let lines: Vec<_> = content.lines().collect();
    let lower: Vec<_> = lines.iter().map(|l| l.to_ascii_lowercase()).collect();
    let mut diagnoses: Vec<_> = KNOWN_ERRORS.iter().filter_map(|sig| {
        let pattern = sig.pattern.to_ascii_lowercase();
        let index = lower.iter().position(|line| line.contains(&pattern))?;
        let begin = index.saturating_sub(3);
        let end = (index + 4).min(lines.len());
        let context = (begin..end).map(|i| format!("{}: {}", i + 1, lines[i]))
            .collect::<Vec<_>>().join("\n");
        Some(Diagnosis {
            line_number: index + 1,
            context,
            matched: lines[index].trim().to_string(),
            explanation: sig.explanation.to_string(),
            fix: sig.fix.to_string(),
        })
    }).collect();
    diagnoses.sort_by_key(|d| d.line_number);
    diagnoses
}

pub fn diagnostic_report(content: &str) -> String {
    let diagnoses = scan_build_log_text(content);
    let mut report = format!("Unreal DevTool {} diagnostic report\nLines: {}\nRecognized patterns: {}\n", env!("CARGO_PKG_VERSION"), content.lines().count(), diagnoses.len());
    if diagnoses.is_empty() { report.push_str("No known pattern found. This does not mean the build succeeded. Inspect the first error and surrounding lines.\n"); }
    for d in diagnoses {
        report.push_str(&format!("\nLine {}: {}\nCause: {}\nNext step: {}\nContext:\n{}\n", d.line_number, d.matched, d.explanation, d.fix, d.context));
    }
    report
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn case_insensitive_and_deduplicated() {
        let d = scan_build_log_text("ERROR lnk2019 symbol\nerror LNK2019 another");
        assert_eq!(d.len(), 1);
        assert!(d[0].fix.contains("module"));
    }
    #[test]
    fn unknown_is_not_success() {
        assert!(diagnostic_report("unfamiliar failure").contains("does not mean"));
    }
}

/// Bound the actual read, rather than only trusting metadata from before it.
pub fn read_log(path: &Path) -> Result<String, String> {
    use std::io::Read;
    const LIMIT: u64 = 8 * 1024 * 1024;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT { return Err("Log exceeds 8 MiB. Paste an excerpt instead.".into()); }
    decode_log(&bytes)
}

fn decode_log(bytes: &[u8]) -> Result<String, String> {
    let le = bytes.starts_with(&[0xff, 0xfe]);
    let be = bytes.starts_with(&[0xfe, 0xff]);
    if le || be {
        let bytes = &bytes[2..];
        if !bytes.len().is_multiple_of(2) { return Err("Incomplete UTF-16 log: odd byte count.".into()); }
        let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|p| {
            if le { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) }
        }).collect();
        String::from_utf16(&units).map_err(|e| format!("Invalid UTF-16 log: {e}"))
    } else {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        String::from_utf8(bytes.to_vec()).map_err(|_| "Unsupported text encoding. Save the log as UTF-8 or UTF-16 with a BOM.".into())
    }
}

/// Only known local roots are removed. Credentials and other secrets require
/// manual review; this deliberately makes no promise of universal redaction.
pub fn redact_paths(report: &str, paths: &[String]) -> String {
    let mut paths: Vec<_> = paths.iter().filter(|p| !p.is_empty()).collect();
    paths.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let mut result = report.to_owned();
    for path in paths {
        for variant in [path.to_string(), path.replace('\\', "/")] {
            let lower = result.to_ascii_lowercase();
            let needle = variant.to_ascii_lowercase();
            let positions: Vec<_> = lower.match_indices(&needle).map(|(i, _)| i).collect();
            for i in positions.into_iter().rev() {
                result.replace_range(i..i + variant.len(), "[local path]");
            }
        }
    }
    result
}

#[cfg(test)]
mod import_tests {
    use super::*;
    #[test]
    fn unicode_and_bom_encodings_are_decoded() {
        let text = "LogBlueprint: Error: 日本語 😀";
        for le in [true, false] {
            let mut bytes = if le { vec![0xff, 0xfe] } else { vec![0xfe, 0xff] };
            for word in text.encode_utf16() { bytes.extend(if le { word.to_le_bytes() } else { word.to_be_bytes() }); }
            assert_eq!(decode_log(&bytes).unwrap(), text);
        }
        assert_eq!(decode_log(b"\xef\xbb\xbfhello").unwrap(), "hello");
        assert!(decode_log(&[0xff, 0xfe, 0x01]).is_err());
        assert!(decode_log(&[0xff]).is_err());
    }
    #[test]
    fn diagnostics_are_chronological_and_preserve_context() {
        let d = scan_build_log_text("header\nLogCook: Error: asset\nnearby\nerror LNK2019 symbol");
        assert_eq!(d[0].line_number, 2);
        assert_eq!(d[1].line_number, 4);
        assert!(d[0].context.contains("1: header"));
        assert!(diagnostic_report("LogCook: Error: asset").contains("Context:"));
    }
    #[test]
    fn path_redaction_handles_case_slashes_and_overlapping_roots() {
        let paths = vec![r"C:\Users\Alice".into(), r"C:\Users\Alice\Project".into()];
        let text = "C:/Users/ALICE/Project/file.cpp c:\\users\\alice\\other.log 日本語";
        let redacted = redact_paths(text, &paths);
        assert!(!redacted.to_ascii_lowercase().contains("alice"));
        assert!(redacted.contains("日本語"));
    }
    #[test]
    fn oversized_file_is_rejected() {
        let path = std::env::temp_dir().join(format!("devtool-log-limit-{}.txt", std::process::id()));
        let f = fs::File::create(&path).unwrap();
        f.set_len(8 * 1024 * 1024 + 1).unwrap();
        drop(f);
        assert!(read_log(&path).unwrap_err().contains("8 MiB"));
        fs::remove_file(path).unwrap();
    }
}
