//! Past builds, read back off disk.
//!
//! The packaging pipeline already writes everything this needs — a
//! `build/v0.0.N/` folder per run, with the archived output under `Windows/`
//! and a `MobiusFish_v0.0.N.zip` beside it. Nothing recorded it anywhere, so
//! the app could tell you the *next* version number and nothing about any
//! build you had actually made. This reads the folder back.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One previous build.
#[derive(Clone)]
pub struct BuildRecord {
    /// Version folder name, e.g. `v0.0.11`.
    pub version:  String,
    /// Sort key — the trailing component of the version.
    pub number:   u32,
    /// Size of the zip, else the staged folder's total.
    pub bytes:    u64,
    /// Newest mtime seen in the version folder.
    pub modified: Option<SystemTime>,
    /// The version folder itself, for "open in Explorer".
    pub dir:      PathBuf,
    /// What the run recorded about itself, when it was made by this app.
    pub info:     Option<BuildInfo>,
}

/// A build's own record, saved as `build-info.txt` beside its output.
///
/// Plain `key=value` lines rather than JSON so it can be read at a glance or
/// diffed, and so a hand-edited or half-written file degrades to "no info"
/// instead of an error. It exists for two things the folder cannot say on its
/// own: how the build was made, and how long each stage really took — the
/// latter is what makes the next run's progress estimate honest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildInfo {
    pub platform: String,
    pub config:   String,
    pub ok:       bool,
    pub secs:     u64,
    pub warnings: u32,
    pub errors:   u32,
    /// The git commit the build was made from (empty when not a repository).
    pub commit:   String,
    /// Seconds per stage in `ops::run::Stage` order; 0 = not reached.
    pub stage_secs: [u64; 4],
}

pub const INFO_FILE: &str = "build-info.txt";

impl BuildInfo {
    pub fn to_text(&self) -> String {
        format!(
            "platform={}\nconfig={}\nok={}\nsecs={}\nwarnings={}\nerrors={}\nstages={},{},{},{}\ncommit={}\n",
            self.platform, self.config, self.ok, self.secs, self.warnings, self.errors,
            self.stage_secs[0], self.stage_secs[1], self.stage_secs[2], self.stage_secs[3], self.commit,
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut info = BuildInfo::default();
        let mut seen = false;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            seen = true;
            match k.trim() {
                "platform" => info.platform = v.to_string(),
                "config"   => info.config = v.to_string(),
                "commit"   => info.commit = v.to_string(),
                "ok"       => info.ok = v == "true",
                "secs"     => info.secs = v.parse().unwrap_or(0),
                "warnings" => info.warnings = v.parse().unwrap_or(0),
                "errors"   => info.errors = v.parse().unwrap_or(0),
                "stages"   => {
                    for (i, part) in v.split(',').take(4).enumerate() {
                        info.stage_secs[i] = part.trim().parse().unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        seen.then_some(info)
    }

    /// One line for a tooltip: "Development · Windows · 11:32".
    pub fn summary(&self) -> String {
        let (m, s) = (self.secs / 60, self.secs % 60);
        let mut out = format!("{} · {} · {m}:{s:02}", self.config, self.platform);
        if !self.ok { out.push_str(" · failed"); }
        out
    }
}

/// Writes the record. Best effort: a read-only drive must not fail a build.
pub fn write_info(version_dir: &Path, info: &BuildInfo) {
    let _ = std::fs::write(version_dir.join(INFO_FILE), info.to_text());
}

pub fn read_info(version_dir: &Path) -> Option<BuildInfo> {
    BuildInfo::parse(&std::fs::read_to_string(version_dir.join(INFO_FILE)).ok()?)
}

/// Stage durations of the newest successful build in `records`, if any were
/// recorded. Used to replace the fixed "typical" durations behind the progress
/// bars with what this project actually takes.
pub fn recent_stage_secs(records: &[BuildRecord]) -> Option<[u64; 4]> {
    records.iter()
        .filter_map(|r| r.info.as_ref())
        .find(|i| i.ok && i.stage_secs.iter().all(|s| *s > 0))
        .map(|i| i.stage_secs)
}

impl BuildRecord {
    /// "1.24 GB" / "812 MB" / "— " when nothing was measured.
    pub fn size_label(&self) -> String {
        format_bytes(self.bytes)
    }

    /// Coarse relative age. Deliberately coarse: an exact timestamp is noise
    /// in a list whose job is "which of these is the recent one".
    pub fn age_label(&self) -> String {
        let Some(m) = self.modified else { return String::new() };
        let Ok(elapsed) = SystemTime::now().duration_since(m) else {
            return "just now".into();
        };
        let secs = elapsed.as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        let days = hours / 24;
        if mins < 2 { "just now".into() }
        else if mins < 60 { format!("{mins} min ago") }
        else if hours < 24 { format!("{hours} hr ago") }
        else if days == 1 { "yesterday".into() }
        else if days < 14 { format!("{days} days ago") }
        else { format!("{} weeks ago", days / 7) }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if bytes == 0 { "—".into() }
    else if b >= GB { format!("{:.2} GB", b / GB) }
    else if b >= MB { format!("{:.0} MB", b / MB) }
    else { format!("{:.0} KB", b / KB) }
}

/// Parses `v0.0.11` into `11`. Anything that is not a version folder returns
/// `None` and is skipped, so a stray folder under `build/` cannot appear as a
/// build or upset the ordering.
fn version_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('v')?;
    let last = rest.rsplit('.').next()?;
    last.parse().ok()
}

/// Everything under `<project>/build/`, newest first.
///
/// Blocking: it stats a directory tree, so callers run it off the UI thread
/// the same way every other disk read in this app does.
pub fn scan(project_dir: &Path) -> Vec<BuildRecord> {
    let build_dir = project_dir.join("build");
    let Ok(entries) = std::fs::read_dir(&build_dir) else { return Vec::new() };

    let mut out: Vec<BuildRecord> = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            if !dir.is_dir() { return None; }
            let name = dir.file_name()?.to_string_lossy().to_string();
            let number = version_number(&name)?;

            // The zip is the shippable artifact, so it is what "size" means
            // when one exists. A run cancelled before archiving leaves only
            // the staged tree, and measuring that is still more useful than
            // showing nothing.
            let zip = newest_zip(&dir);
            let (bytes, modified) = match &zip {
                Some(z) => {
                    let md = std::fs::metadata(z).ok();
                    (
                        md.as_ref().map(|m| m.len()).unwrap_or(0),
                        md.as_ref().and_then(|m| m.modified().ok()),
                    )
                }
                None => (dir_size(&dir, 0), newest_mtime(&dir, 0)),
            };

            let info = read_info(&dir);
            Some(BuildRecord { version: name, number, bytes, modified, dir, info })
        })
        .collect();

    out.sort_by_key(|record| std::cmp::Reverse(record.number));
    out
}

fn newest_zip(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) {
            let t = e.metadata().ok().and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best = Some((t, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Recursive size with a depth cap.
///
/// A staged Unreal build is tens of thousands of files; the cap keeps this
/// bounded, and the top few levels are enough for a size that reads correctly
/// at "1.2 GB" precision.
fn dir_size(dir: &Path, depth: u32) -> u64 {
    if depth > 4 { return 0; }
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().map(|e| {
        match e.metadata() {
            Ok(m) if m.is_file() => m.len(),
            Ok(m) if m.is_dir()  => dir_size(&e.path(), depth + 1),
            _ => 0,
        }
    }).sum()
}

fn newest_mtime(dir: &Path, depth: u32) -> Option<SystemTime> {
    if depth > 3 { return None; }
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().filter_map(|e| {
        let md = e.metadata().ok()?;
        if md.is_dir() {
            newest_mtime(&e.path(), depth + 1)
        } else {
            md.modified().ok()
        }
    }).max()
}

#[cfg(test)]
mod info_tests {
    use super::*;

    #[test]
    fn build_info_round_trips_and_tolerates_junk() {
        let info = BuildInfo {
            platform: "Windows".into(), config: "Shipping".into(), ok: true,
            secs: 692, warnings: 4, errors: 0, commit: "abc123def".into(), stage_secs: [120, 400, 60, 112],
        };
        assert_eq!(BuildInfo::parse(&info.to_text()), Some(info.clone()));
        assert_eq!(info.summary(), "Shipping · Windows · 11:32");
        assert_eq!(BuildInfo::parse("not a record"), None);
        assert_eq!(BuildInfo::parse("secs=abc\nzzz=1").map(|i| i.secs), Some(0));
    }
}
