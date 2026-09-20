use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// `owner/repo` on GitHub that publishes release builds (see `.github/workflows/release.yml`).
const REPO: &str = "HUKLIA/UnrealDevtool";

/// Name of the release artifact uploaded by the release workflow.
const ASSET_NAME: &str = "unreal_devtool.exe";

/// Sidecar published alongside it holding the artifact's SHA-256. See
/// `.github/workflows/release.yml`.
const CHECKSUM_ASSET_NAME: &str = "unreal_devtool.exe.sha256";

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ReleaseInfo {
    tag_name: String,
    published_at: String,
    assets: Vec<ReleaseAsset>,
}

/// Info about a newer release than the one currently running.
#[derive(Clone)]
pub struct UpdateInfo {
    pub version:      String,
    pub published_at: String,
    pub download_url: String,
    /// URL of the `.sha256` sidecar, when the release published one. Releases
    /// built before checksums were added have none, which is why this is an
    /// `Option` rather than a hard requirement.
    pub checksum_url: Option<String>,
}

/// Parses `v1.0.1`, `1.0.1` or `v1.0` into (major, minor, patch).
///
/// Versions are compared as a whole. Releases used to be tagged
/// `v0.0.<run_number>` and compared by their last component alone, which only
/// worked while the first two were always zero — `v1.0.1` would have looked
/// *older* than `v0.0.65` and nobody would ever have been offered it.
/// Old-style tags still parse, and still order correctly against new ones.
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}

/// Show only the date portion of an ISO-8601 timestamp like `2026-06-13T10:23:45Z`.
fn date_only(timestamp: &str) -> &str {
    timestamp.split('T').next().unwrap_or(timestamp)
}

/// Query GitHub for the latest release and return its details if it is newer
/// than `current_version` (i.e. `env!("CARGO_PKG_VERSION")`).
pub fn check_for_update(current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = ureq::get(&url)
        .set("User-Agent", "UnrealDevTool-Updater")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let info: ReleaseInfo = resp.into_json().map_err(|e| e.to_string())?;

    let latest  = parse_version(&info.tag_name).ok_or("unrecognised release tag")?;
    let current = parse_version(current_version).ok_or("unrecognised current version")?;
    if latest <= current {
        return Ok(None);
    }

    let asset = info.assets.iter().find(|a| a.name == ASSET_NAME)
        .ok_or("latest release has no exe asset")?;

    let checksum_url = info.assets.iter()
        .find(|a| a.name == CHECKSUM_ASSET_NAME)
        .map(|a| a.browser_download_url.clone());

    Ok(Some(UpdateInfo {
        version:      info.tag_name,
        published_at: date_only(&info.published_at).to_string(),
        download_url: asset.browser_download_url.clone(),
        checksum_url,
    }))
}

/// Retries a fallible file operation a few times with backoff — the standard
/// fix for Windows `ERROR_SHARING_VIOLATION` (os error 32, "being used by
/// another process"), which very commonly happens for a brief window right
/// after writing a fresh .exe to disk: antivirus real-time protection grabs
/// it for scanning the instant our own handle closes, holding an exclusive
/// lock for anywhere from a few hundred ms up to a couple of seconds. This
/// covers about 3s of total backoff, which comfortably rides that out.
fn retry_file_op<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut last_err = None;
    for attempt in 0..8u32 {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1) as u64));
            }
        }
    }
    Err(last_err.unwrap())
}

/// True if we can actually write to `dir` — cheap probe (create+delete a
/// temp file). Directories like `C:\Program Files\...` are writable by
/// Administrators only by default; a standard user's process installed
/// there can never self-update no matter how many times it retries, so this
/// lets us fail fast with a clear, actionable message instead of a cryptic
/// OS error after downloading the whole release.
pub fn dir_is_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".unreal_devtool_write_test");
    match std::fs::File::create(&probe) {
        Ok(_) => { let _ = std::fs::remove_file(&probe); true }
        Err(_) => false,
    }
}

/// Download the new exe and replace the running one in-place, then relaunch it.
///
/// Windows allows renaming a running executable (it only blocks deletion of an
/// in-use file without `FILE_SHARE_DELETE`), so we move the current exe aside,
/// drop the freshly downloaded one in its place, and spawn it. The caller is
/// expected to exit the process immediately after this returns `Ok`.
/// Fetches the `.sha256` sidecar and returns the bare hex digest from it.
///
/// The file is in `sha256sum` format ("<hex>  <filename>"), so only the first
/// whitespace-delimited field is the digest.
fn fetch_expected_digest(url: &str) -> Result<String, String> {
    let body = ureq::get(url)
        .set("User-Agent", "UnrealDevTool-Updater")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let digest = body.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("checksum file was not in the expected format".into());
    }
    Ok(digest)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn download_and_install(
    download_url: &str,
    checksum_url: Option<&str>,
    status:       &Arc<Mutex<String>>,
    cancel:       &Arc<AtomicBool>,
    progress:     &Arc<Mutex<f32>>,
) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = current_exe.parent().ok_or("could not resolve install directory")?;

    if !dir_is_writable(dir) {
        let hint = if dir.to_string_lossy().to_ascii_lowercase().contains("program files") {
            " (it's installed under Program Files, which normal user accounts can't write to — \
              right-click the exe and \"Run as administrator\", or move the app to a folder like \
              Documents or a dedicated tools folder outside Program Files)"
        } else {
            ""
        };
        return Err(format!("no write access to {}{hint}", dir.display()));
    }

    let new_path = dir.join("unreal_devtool_update.exe");
    let old_path = dir.join("unreal_devtool_old.exe");

    *status.lock().unwrap_or_else(|e| e.into_inner()) = "Downloading update…".into();
    let resp = ureq::get(download_url)
        .set("User-Agent", "UnrealDevTool-Updater")
        .call()
        .map_err(|e| e.to_string())?;

    let total = resp.header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&new_path).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut hasher = Sha256::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&new_path);
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if total > 0 {
            *progress.lock().unwrap_or_else(|e| e.into_inner()) =
                (downloaded as f32 / total as f32).min(1.0);
        }
    }
    drop(file);

    // Verify before this file is allowed anywhere near the running exe.
    //
    // This is a self-replacing updater — it downloads an executable over the
    // network and then makes it the program the user runs. Checking the
    // publisher's digest first is the minimum bar for that: a truncated
    // download, a corrupted CDN object or a tampered artifact is caught here
    // rather than being installed and launched. It is also what lets the
    // README tell a wary user (or their IT department) how to independently
    // confirm that what they are running matches the published release.
    //
    // `checksum_url` is `None` for releases published before the workflow
    // started emitting the sidecar; those still install, since refusing them
    // would strand anyone on an older build with no way forward.
    if let Some(url) = checksum_url {
        *status.lock().unwrap_or_else(|e| e.into_inner()) = "Verifying download…".into();
        let expected = fetch_expected_digest(url)?;
        let actual   = hex(&hasher.finalize());
        if actual != expected {
            let _ = std::fs::remove_file(&new_path);
            return Err(format!(
                "checksum mismatch — update refused and the download was deleted.\n  \
                 expected {expected}\n  got      {actual}"
            ));
        }
    }

    *status.lock().unwrap_or_else(|e| e.into_inner()) = "Installing update…".into();
    let _ = std::fs::remove_file(&old_path);
    // Every step below touches a file that antivirus may have just grabbed
    // for scanning (the exe we're renaming aside, or the one we just
    // finished writing) — retry_file_op rides out that transient lock
    // instead of failing on the first sharing violation.
    retry_file_op(|| std::fs::rename(&current_exe, &old_path))
        .map_err(|e| format!("could not replace running exe: {e}"))?;
    retry_file_op(|| std::fs::rename(&new_path, &current_exe))
        .map_err(|e| e.to_string())?;

    retry_file_op(|| std::process::Command::new(&current_exe).spawn())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes a leftover `unreal_devtool_old.exe` from a previous update, if
/// present. Safe to call on every startup.
///
/// This runs right after an update relaunches into the new exe (see
/// `download_and_install`) — at that point the *old* process only just
/// called `spawn()` on us and hasn't necessarily hit `exit(0)` yet, so its
/// file handle on `unreal_devtool_old.exe` can still be open for a brief
/// moment. Deleting a still-running exe fails with the same sharing
/// violation the rename/install steps guard against, so this needs the same
/// retry — and since retrying means sleeping, it runs on a background
/// thread so a slow-to-exit previous process can never delay startup.
pub fn cleanup_old_binary() {
    let Ok(current_exe) = std::env::current_exe() else { return };
    let Some(dir) = current_exe.parent() else { return };
    let old_path = dir.join("unreal_devtool_old.exe");
    if !old_path.exists() { return; }
    std::thread::spawn(move || {
        let _ = retry_file_op(|| std::fs::remove_file(&old_path));
    });
}

/// Byte size of the leftover `unreal_devtool_old.exe`, if one exists —
/// used by the app self-check to surface it if automatic cleanup ever fails.
pub fn leftover_old_binary_size() -> Option<u64> {
    let current_exe = std::env::current_exe().ok()?;
    let dir = current_exe.parent()?;
    let meta = std::fs::metadata(dir.join("unreal_devtool_old.exe")).ok()?;
    Some(meta.len())
}

#[cfg(test)]
mod version_tests {
    use super::parse_version;

    #[test]
    fn versions_parse_in_every_form_a_tag_takes() {
        assert_eq!(parse_version("v1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_version("1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_version("v0.0.65"), Some((0, 0, 65)));
        assert_eq!(parse_version("v2.1"), Some((2, 1, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("v1.x.3"), None);
    }

    #[test]
    fn a_new_major_outranks_any_old_build_counter() {
        let v = |s| parse_version(s).unwrap();
        assert!(v("v1.0.1") > v("v0.0.65"), "the release that must reach existing users");
        assert!(v("v1.0.2") > v("v1.0.1"));
        assert!(v("v1.1.0") > v("v1.0.99"));
        assert!(v("v0.0.66") > v("v0.0.65"), "old-style tags still order");
        assert!(!(v("1.0.1") > v("v1.0.1")), "a build is not newer than itself");
    }
}
