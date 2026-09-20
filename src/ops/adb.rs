//! Installing an Android build on a connected device, through Google's `adb`.
//!
//! `adb` ships in the Android SDK's platform-tools, which Unreal's own
//! `SetupAndroid` step installs. Nothing here downloads or installs anything:
//! it finds the `adb` that is already there, lists the devices it can see, and
//! runs `adb install` when asked.

use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Debug)]
pub struct Device {
    pub serial: String,
    /// `device` is ready; `unauthorized` means the phone has not accepted this PC.
    pub state:  String,
    pub model:  String,
}

impl Device {
    pub fn ready(&self) -> bool { self.state == "device" }
    pub fn label(&self) -> String {
        if self.model.is_empty() { self.serial.clone() } else { self.model.replace('_', " ") }
    }
}

/// `adb.exe` from the SDK the environment points at, or from `PATH`.
pub fn find_adb() -> Option<PathBuf> {
    let rel = Path::new("platform-tools").join("adb.exe");
    let from_sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"].iter()
        .filter_map(std::env::var_os)
        .map(|sdk| PathBuf::from(sdk).join(&rel))
        .chain(std::env::var_os("LOCALAPPDATA")
            .map(|l| PathBuf::from(l).join("Android").join("Sdk").join(&rel)))
        .find(|p| p.is_file());
    from_sdk.or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).map(|d| d.join("adb.exe")).find(|p| p.is_file())
        })
    })
}

/// Parses `adb devices -l`.
///
/// ```text
/// List of devices attached
/// R58M123ABC             device usb:1-1 product:o1s model:SM_G991B device:o1s transport_id:2
/// emulator-5554          offline
/// ```
pub fn parse_devices(text: &str) -> Vec<Device> {
    text.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts.find_map(|p| p.strip_prefix("model:")).unwrap_or("").to_string();
            Some(Device { serial, state, model })
        })
        .collect()
}

pub fn devices(adb: &Path) -> Result<Vec<Device>, String> {
    let out = crate::ops::cmd(&adb.to_string_lossy())
        .args(["devices", "-l"])
        .output()
        .map_err(|e| format!("Could not run adb: {e}"))?;
    Ok(parse_devices(&String::from_utf8_lossy(&out.stdout)))
}

/// Installs (replacing any existing copy) and returns adb's own verdict.
pub fn install(adb: &Path, serial: &str, apk: &Path) -> Result<String, String> {
    let out = crate::ops::cmd(&adb.to_string_lossy())
        .args(["-s", serial, "install", "-r"])
        .arg(apk)
        .output()
        .map_err(|e| format!("Could not run adb: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let text = text.trim();
    if out.status.success() && text.contains("Success") {
        Ok("Installed.".into())
    } else {
        // adb's last line is the reason, e.g. INSTALL_FAILED_UPDATE_INCOMPATIBLE.
        Err(text.lines().last().unwrap_or("adb reported no reason").to_string())
    }
}

/// The largest `.apk` under a build's output, searched a few levels deep.
pub fn find_apk(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0u8)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 4 { stack.push((p, depth + 1)); }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("apk")) {
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                if best.as_ref().is_none_or(|(l, _)| len > *l) { best = Some((len, p)); }
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_parsed_with_state_and_model() {
        let text = "List of devices attached\n\
                    R58M123ABC             device usb:1-1 product:o1s model:SM_G991B device:o1s transport_id:2\n\
                    emulator-5554          offline\n\
                    0123456789ABCDEF       unauthorized usb:1-2 transport_id:3\n\n";
        let d = parse_devices(text);
        assert_eq!(d.len(), 3);
        assert!(d[0].ready() && d[0].label() == "SM G991B");
        assert_eq!((d[1].serial.as_str(), d[1].state.as_str(), d[1].label().as_str()), ("emulator-5554", "offline", "emulator-5554"));
        assert!(!d[2].ready(), "an unauthorised phone cannot be installed to");
        assert!(parse_devices("adb: command not found").is_empty());
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn the_apk_is_found_below_the_build_output() {
        let root = std::env::temp_dir().join(format!("udt-apk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Android_ASTC/Game")).unwrap();
        std::fs::write(root.join("Android_ASTC/Game/small.apk"), vec![0u8; 10]).unwrap();
        std::fs::write(root.join("Android_ASTC/Game/Game-arm64.apk"), vec![0u8; 500]).unwrap();
        std::fs::write(root.join("Android_ASTC/Game/Game.obb"), vec![0u8; 900]).unwrap();
        assert_eq!(find_apk(&root).unwrap().file_name().unwrap(), "Game-arm64.apk");
        assert!(find_apk(&root.join("nope")).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
