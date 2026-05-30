use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn package_game(
    uproject:  PathBuf,
    engine:    PathBuf,
    pack_name: String,
    exe_name:  String,
    status:    Arc<Mutex<String>>,
) -> String {
    macro_rules! upd { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }

    let project_dir = match uproject.parent() {
        Some(p) => p.to_path_buf(),
        None    => return "[ERROR] Bad project path.".into(),
    };
    let build_dir   = project_dir.join("build");
    let version_num = find_next_version(&build_dir);
    let version_str = format!("v0.0.{}", version_num);
    let version_dir = build_dir.join(&version_str);
    let log_path    = version_dir.join("BuildLog.txt");

    upd!(format!("[1/5] Creating output directory…\n→ {}", version_dir.display()));
    if let Err(e) = fs::create_dir_all(&version_dir) {
        return format!("[ERROR] mkdir: {}", e);
    }

    let runuat = engine.join("Engine\\Build\\BatchFiles\\RunUAT.bat");
    upd!(format!("[2/5] Running UAT BuildCookRun…  (may take 30+ min)\nLog → {}", log_path.display()));

    let uat = std::process::Command::new("cmd")
        .args(["/c", &runuat.to_string_lossy()])
        .arg("BuildCookRun")
        .arg(format!("-project={}", uproject.display()))
        .args(["-noP4", "-unattended", "-platform=Win64",
               "-clientconfig=Development", "-serverconfig=Development",
               "-cook", "-allmaps", "-build", "-stage", "-pak", "-archive"])
        .arg(format!("-archivedirectory={}", version_dir.display()))
        .output();

    let uat_out = match uat {
        Err(e) => return format!("[ERROR] UAT launch: {}", e),
        Ok(o)  => o,
    };
    let _ = fs::write(&log_path, format!(
        "=== STDOUT ===\n{}\n\n=== STDERR ===\n{}",
        String::from_utf8_lossy(&uat_out.stdout),
        String::from_utf8_lossy(&uat_out.stderr),
    ));
    if !uat_out.status.success() {
        return format!(
            "[ERROR] UAT failed (exit {}).\nLog → {}",
            uat_out.status.code().unwrap_or(-1),
            log_path.display()
        );
    }

    // UAT places the packaged game in <archivedirectory>/Windows/ for Win64
    let uat_windows = version_dir.join("Windows");
    if !uat_windows.exists() {
        return format!(
            "[ERROR] UAT output not found: {}\nLog → {}",
            uat_windows.display(), log_path.display()
        );
    }

    let package_dir = if pack_name.eq_ignore_ascii_case("Windows") {
        uat_windows
    } else {
        upd!(format!("[3/5] Renaming: Windows → {}", pack_name));
        let target = version_dir.join(&pack_name);
        if let Err(e) = fs::rename(&uat_windows, &target) {
            return format!("[ERROR] rename folder: {}", e);
        }
        target
    };

    upd!("[4/5] Renaming executable…");
    if let Some(found) = find_main_exe(&package_dir) {
        let target_exe = package_dir.join(format!("{}.exe", exe_name));
        if found != target_exe {
            if let Err(e) = fs::rename(&found, &target_exe) {
                return format!("[ERROR] rename exe: {}", e);
            }
        }
    }

    let zip_name = format!("{}_{}.zip", pack_name, version_str);
    let zip_path = version_dir.join(&zip_name);
    upd!(format!("[5/5] Creating {}…", zip_name));

    if !package_dir.exists() {
        return format!("[ERROR] Package folder missing: {}", package_dir.display());
    }
    let ps = format!(
        "$ErrorActionPreference='Stop'; \
         Compress-Archive -Path '{src}\\*' -DestinationPath '{dst}' -Force; \
         Write-Host 'Zip OK'",
        src = package_dir.display(),
        dst = zip_path.display(),
    );
    let zip_out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output();
    match zip_out {
        Err(e)                       => return format!("[ERROR] PowerShell: {}", e),
        Ok(o) if !o.status.success() => return format!("[ERROR] Zip:\n{}", String::from_utf8_lossy(&o.stderr)),
        _                            => {}
    }

    format!(
        "[DONE] {} — packaged!\nOutput → {}\nZip    → {}",
        version_str, version_dir.display(), zip_name,
    )
}

pub fn find_next_version(build_dir: &Path) -> u32 {
    if !build_dir.exists() { return 1; }
    let mut highest = 0u32;
    if let Ok(entries) = fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() { continue; }
            let name = entry.file_name();
            let s    = name.to_string_lossy();
            if let Some(rest) = s.strip_prefix("v0.0.") {
                if let Ok(n) = rest.parse::<u32>() {
                    if n > highest { highest = n; }
                }
            }
        }
    }
    highest + 1
}

pub fn find_main_exe(dir: &Path) -> Option<PathBuf> {
    const SKIP: &[&str] = &["CrashReportClient", "UEPrereqSetup_x64", "UEPrereqSetup_x86"];
    fs::read_dir(dir).ok()?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |x| x.eq_ignore_ascii_case("exe")))
        .filter(|e| {
            let stem = e.path().file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            !SKIP.contains(&stem.as_str())
        })
        .map(|e| e.path())
        .next()
}
