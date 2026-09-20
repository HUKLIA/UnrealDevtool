//! Locating rclone, which the Google Drive upload step drives.
//!
//! # Why this is only a lookup
//!
//! rclone.exe used to be compiled straight into this binary with
//! `include_bytes!` and written out to `%APPDATA%` on first use. That worked,
//! but it is also — byte for byte — the shape of a malware dropper, and it was
//! the main reason Windows Defender and friends flagged this app: a 79 MB PE
//! executable sitting in another executable's data section is the strongest
//! "packed dropper" heuristic there is, and writing it to disk and running it
//! is the behavioural half of the same signature. rclone is dual-use on top of
//! that (ransomware crews use it to exfiltrate data) and several engines flag
//! it as riskware in its own right, so the embedded copy was detected *inside*
//! our binary before it was ever extracted.
//!
//! This app therefore does not ship, download or install rclone at all. It
//! only *finds* one the user installed themselves, and the Package tab links
//! out to rclone.org with setup instructions. That keeps the one process that
//! writes an executable to this machine as the user's own deliberate act,
//! which is both the honest arrangement and the one no scanner objects to.

use std::path::{Path, PathBuf};

/// Official download page — the single button in the UI opens this.
pub const DOWNLOAD_URL: &str = "https://rclone.org/downloads/";

/// Official Google Drive configuration guide, linked beside the setup steps.
pub const DRIVE_DOCS_URL: &str = "https://rclone.org/drive/";

/// Where a copy extracted by an older version of this app would be. Still
/// checked first so anyone carrying one forward keeps working untouched.
pub fn install_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(Path::new(&appdata).join("UnrealDevtool").join("rclone").join("rclone.exe"))
}

/// Scans `PATH` for `rclone.exe` without spawning anything. A `rclone version`
/// probe would be more authoritative, but this runs on the UI thread during
/// panel layout, and spawning a process every frame is not acceptable there.
fn on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("rclone.exe"))
        .find(|candidate| candidate.is_file())
}

/// Resolve rclone in preference order: a copy left by an older build, then one
/// sitting next to this app, then whatever is on `PATH`. `None` means rclone is
/// not installed — callers surface that as an actionable prompt rather than
/// shelling out to a name that does not resolve.
pub fn resolve() -> Option<PathBuf> {
    if let Some(p) = install_path().filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(sibling) = std::env::current_exe().ok()
        .and_then(|exe| exe.parent().map(|d| d.join("rclone.exe")))
        .filter(|p| p.is_file())
    {
        return Some(sibling);
    }
    on_path()
}

pub fn is_available() -> bool {
    resolve().is_some()
}
