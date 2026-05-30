use std::path::PathBuf;
use winreg::enums::{HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER};
use winreg::RegKey;

/// Scans the Windows registry for a UE 5.x installation (5.9 → 5.0),
/// then falls back to custom/source builds in HKCU.
pub fn detect_unreal_engine() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for minor in (0..=9).rev() {
        let key = format!("SOFTWARE\\EpicGames\\Unreal Engine\\5.{}", minor);
        if let Ok(k) = hklm.open_subkey(&key) {
            if let Ok(dir) = k.get_value::<String, _>("InstalledDirectory") {
                let bat = PathBuf::from(&dir).join("Engine\\Build\\BatchFiles\\RunUAT.bat");
                if bat.exists() { return Some(PathBuf::from(dir)); }
            }
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(builds) = hkcu.open_subkey("Software\\Epic Games\\Unreal Engine\\Builds") {
        for (_, value) in builds.enum_values().flatten() {
            let dir = value.to_string();
            let bat = PathBuf::from(&dir).join("Engine\\Build\\BatchFiles\\RunUAT.bat");
            if bat.exists() { return Some(PathBuf::from(dir)); }
        }
    }
    None
}

pub fn build_init_status(engine: &Option<PathBuf>, project: &Option<PathBuf>) -> String {
    let eng = match engine {
        Some(p) => format!("Engine: {}", p.display()),
        None    => "Engine: not found".into(),
    };
    let proj = match project {
        Some(p) => format!("Project: {}", p.file_name().unwrap_or_default().to_string_lossy()),
        None    => "Project: not set".into(),
    };
    format!("Ready  |  {}  |  {}", eng, proj)
}
