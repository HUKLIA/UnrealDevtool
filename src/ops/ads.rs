use std::path::PathBuf;
use std::sync::OnceLock;

// Embedded at compile time — mirrors the exact "embed + extract to %APPDATA%
// on first use" pattern already used for the bundled rclone.exe in
// `package.rs::rclone_program`.
static AD_VIDEO_BYTES: &[u8] = include_bytes!("../../Ads/Trailer_ANIM (1).mp4");

static AD_URL: OnceLock<Option<String>> = OnceLock::new();

fn ad_video_appdata_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("UnrealDevtool").join("ads").join("tachyon_ad.mp4"))
}

/// Extracts the embedded ad video to `%APPDATA%` on first use and returns a
/// `file://` URL to it, cached for the process lifetime. Returns `None` if
/// extraction fails (missing `%APPDATA%`, disk full, permissions) — callers
/// must treat `None` as "skip the ad, proceed normally," never block
/// packaging on it.
pub fn ad_video_url() -> Option<&'static str> {
    AD_URL.get_or_init(|| {
        let dest = ad_video_appdata_path()?;
        if !dest.is_file() {
            std::fs::create_dir_all(dest.parent()?).ok()?;
            std::fs::write(&dest, AD_VIDEO_BYTES).ok()?;
        }
        let path = dest.to_string_lossy().replace('\\', "/").replace(' ', "%20");
        Some(format!("file:///{path}"))
    }).as_deref()
}
