use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

/// `owner/repo` on GitHub that publishes release builds (see `.github/workflows/release.yml`).
const REPO: &str = "HUKLIA/UnrealDevtool";

/// Name of the release artifact uploaded by the release workflow.
const ASSET_NAME: &str = "unreal_devtool.exe";

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
}

/// Releases are tagged `v0.0.<run_number>` and `CARGO_PKG_VERSION` is bumped to
/// `0.0.<run_number>` at build time, so the trailing component is a monotonically
/// increasing build counter we can compare directly.
fn build_number(version: &str) -> Option<u64> {
    version.trim_start_matches('v').rsplit('.').next()?.parse().ok()
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

    let latest  = build_number(&info.tag_name).ok_or("unrecognised release tag")?;
    let current = build_number(current_version).ok_or("unrecognised current version")?;
    if latest <= current {
        return Ok(None);
    }

    let asset = info.assets.iter().find(|a| a.name == ASSET_NAME)
        .ok_or("latest release has no exe asset")?;

    Ok(Some(UpdateInfo {
        version:      info.tag_name,
        published_at: date_only(&info.published_at).to_string(),
        download_url: asset.browser_download_url.clone(),
    }))
}

/// Download the new exe and replace the running one in-place, then relaunch it.
///
/// Windows allows renaming a running executable (it only blocks deletion of an
/// in-use file without `FILE_SHARE_DELETE`), so we move the current exe aside,
/// drop the freshly downloaded one in its place, and spawn it. The caller is
/// expected to exit the process immediately after this returns `Ok`.
pub fn download_and_install(
    download_url: &str,
    status:       &Arc<Mutex<String>>,
    cancel:       &Arc<AtomicBool>,
    progress:     &Arc<Mutex<f32>>,
) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = current_exe.parent().ok_or("could not resolve install directory")?;
    let new_path = dir.join("unreal_devtool_update.exe");
    let old_path = dir.join("unreal_devtool_old.exe");

    *status.lock().unwrap() = "Downloading update…".into();
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
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&new_path);
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if total > 0 {
            *progress.lock().unwrap() = (downloaded as f32 / total as f32).min(1.0);
        }
    }
    drop(file);

    *status.lock().unwrap() = "Installing update…".into();
    let _ = std::fs::remove_file(&old_path);
    std::fs::rename(&current_exe, &old_path)
        .map_err(|e| format!("could not replace running exe: {e}"))?;
    std::fs::rename(&new_path, &current_exe).map_err(|e| e.to_string())?;

    std::process::Command::new(&current_exe).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a leftover `unreal_devtool_old.exe` from a previous update, if present.
/// Safe to call on every startup.
pub fn cleanup_old_binary() {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(dir) = current_exe.parent() {
            let _ = std::fs::remove_file(dir.join("unreal_devtool_old.exe"));
        }
}
