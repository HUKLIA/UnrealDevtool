use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn package_game(
    uproject:    PathBuf,
    engine:      PathBuf,
    pack_name:   String,
    exe_name:    String,
    status:      Arc<Mutex<String>>,
    pending_zip: Arc<Mutex<Option<PathBuf>>>,
    cancel:      Arc<AtomicBool>,
    progress:    Arc<Mutex<f32>>,
) -> String {
    macro_rules! upd   { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }
    macro_rules! prog  { ($v:expr) => { *progress.lock().unwrap() = $v; }; }
    macro_rules! check { () => { if cancel.load(Ordering::Relaxed) {
        return "[CANCELLED] Packaging was cancelled.".to_string();
    }}; }

    let project_dir = match uproject.parent() {
        Some(p) => p.to_path_buf(),
        None    => return "[ERROR] Bad project path.".into(),
    };
    let build_dir   = project_dir.join("build");
    let version_num = find_next_version(&build_dir);
    let version_str = format_version(version_num);
    let version_dir = build_dir.join(&version_str);
    let log_path    = version_dir.join("BuildLog.txt");

    prog!(0.02);
    upd!(format!("[1/5] Creating output directory…\n→ {}", version_dir.display()));
    if let Err(e) = fs::create_dir_all(&version_dir) {
        return format!("[ERROR] mkdir: {}", e);
    }
    prog!(0.05);

    check!();
    close_editor_if_running(&status);
    check!();
    let runuat = engine.join("Engine\\Build\\BatchFiles\\RunUAT.bat");
    upd!(format!("[2/5] Running UAT BuildCookRun…  (may take 30+ min)\nLog → {}", log_path.display()));

    // Use spawn() so we can kill the process if the user cancels
    let log_stdout = match fs::File::create(&log_path) {
        Ok(f)  => f,
        Err(e) => return format!("[ERROR] Create log: {}", e),
    };
    let log_stderr = match log_stdout.try_clone() {
        Ok(f)  => f,
        Err(e) => return format!("[ERROR] Clone log handle: {}", e),
    };

    let mut uat_child = match crate::ops::cmd("cmd")
        .args(["/c", &runuat.to_string_lossy()])
        .arg("BuildCookRun")
        .arg(format!("-project={}", uproject.display()))
        .args(["-noP4", "-unattended", "-platform=Win64",
               "-clientconfig=Development", "-serverconfig=Development",
               "-cook", "-allmaps", "-build", "-stage", "-pak", "-archive"])
        .arg(format!("-archivedirectory={}", version_dir.display()))
        .stdout(log_stdout)
        .stderr(log_stderr)
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!("[ERROR] UAT launch: {}", e),
    };

    prog!(0.08); // UAT creep starts here
    let uat_exit = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = uat_child.kill();
            let _ = uat_child.wait();
            return format!("[CANCELLED] UAT was cancelled.\nPartial log → {}", log_path.display());
        }
        match uat_child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None)    => {
                let cur = *progress.lock().unwrap();
                *progress.lock().unwrap() = cur + (0.78 - cur) * 0.008;
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(e) => return format!("[ERROR] Waiting for UAT: {}", e),
        }
    };
    if !uat_exit.success() {
        return format!(
            "[ERROR] UAT failed (exit {}).\nLog → {}",
            uat_exit.code().unwrap_or(-1),
            log_path.display()
        );
    }
    prog!(0.80);

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

    prog!(0.85);
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
    check!();
    prog!(0.90);
    let mut zip_child = match crate::ops::cmd("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!("[ERROR] PowerShell launch: {}", e),
    };
    let zip_exit = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = zip_child.kill();
            let _ = zip_child.wait();
            return "[CANCELLED] Zip was cancelled.".to_string();
        }
        match zip_child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None)    => {
                let cur = *progress.lock().unwrap();
                *progress.lock().unwrap() = cur + (0.99 - cur) * 0.05;
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return format!("[ERROR] Waiting for zip: {}", e),
        }
    };
    if !zip_exit.success() {
        return "[ERROR] Compress-Archive failed — check the log.".to_string();
    }
    prog!(1.0);

    *pending_zip.lock().unwrap() = Some(zip_path.clone());
    format!(
        "[DONE] {} — packaged!\nOutput → {}\nZip    → {}",
        version_str, version_dir.display(), zip_name,
    )
}

// ── Post-package: copy to local / network path ────────────────────────────────

pub fn copy_to_local(zip: &Path, dest: &str) -> String {
    let dest = dest.trim();
    if dest.is_empty() {
        return "[ERROR] Local destination path is empty.".to_string();
    }
    let dest_dir = PathBuf::from(dest);
    if let Err(e) = fs::create_dir_all(&dest_dir) {
        return format!("[ERROR] Create destination dir: {}", e);
    }
    let file_name = zip.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "build.zip".to_string());
    let dest_file = dest_dir.join(&file_name);
    match fs::copy(zip, &dest_file) {
        Ok(_)  => format!("[DONE] Copied to: {}", dest_file.display()),
        Err(e) => format!("[ERROR] Copy failed: {}", e),
    }
}

// ── Post-package: upload to Google Drive via rclone ──────────────────────────
//
// Requires rclone (https://rclone.org) installed and in PATH with a remote
// configured. Set the remote up once with `rclone config` in PowerShell —
// name it "gdrive" (or whatever prefix you use in the destination field).
//
// Example destination:  gdrive:/Builds/MobiusFish
// Command run:          rclone copy <zip> <rclone_dest>

pub fn upload_via_rclone(
    zip:         &Path,
    rclone_dest: &str,
    status:      &Arc<Mutex<String>>,
    cancel:      &Arc<AtomicBool>,
) -> String {
    let dest = rclone_dest.trim();
    if dest.is_empty() {
        return "[ERROR] rclone destination is empty.\n\
                Enter a path like:  gdrive:/Builds/MyGame".to_string();
    }
    if !zip.exists() {
        return format!("[ERROR] Zip file not found: {}", zip.display());
    }

    let file_name = zip.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "build.zip".to_string());

    *status.lock().unwrap() = format!(
        "[UPLOADING] Sending {}  ->  {}\n(via rclone — this may take a while for large builds)",
        file_name, dest,
    );

    let mut child = match crate::ops::cmd("rclone")
        .args(["copy", &zip.to_string_lossy(), dest])
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!(
            "[ERROR] Could not launch rclone: {}\n\
             Make sure rclone is installed and available in your PATH.\n\
             Download: https://rclone.org/",
            e
        ),
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return "[CANCELLED] rclone upload cancelled.".to_string();
        }
        match child.try_wait() {
            Ok(Some(code)) => {
                return if code.success() {
                    format!("[DONE] Uploaded {} to {}", file_name, dest)
                } else {
                    format!(
                        "[ERROR] rclone exited with code {}.\n\
                         Check that your remote name and destination path are correct.",
                        code.code().unwrap_or(-1)
                    )
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(500)),
            Err(e)   => return format!("[ERROR] Waiting for rclone: {}", e),
        }
    }
}

fn close_editor_if_running(status: &Arc<Mutex<String>>) {
    const EDITORS: &[&str] = &["UnrealEditor.exe", "UE4Editor.exe"];
    for editor_exe in EDITORS {
        if !is_process_running(editor_exe) { continue; }

        *status.lock().unwrap() = format!(
            "[PRE-FLIGHT] {} is open — closing it before packaging…\n\
             (packaging requires the editor to be closed)",
            editor_exe
        );

        // Graceful close first (sends WM_CLOSE)
        let _ = crate::ops::cmd("taskkill")
            .args(["/im", editor_exe])
            .output();

        // Wait up to 30 s for graceful exit (poll every 500 ms)
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(500));
            if !is_process_running(editor_exe) { break; }
        }

        // Force-kill if it still hasn't exited
        if is_process_running(editor_exe) {
            let _ = crate::ops::cmd("taskkill")
                .args(["/f", "/im", editor_exe])
                .output();
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

fn is_process_running(exe_name: &str) -> bool {
    crate::ops::cmd("tasklist")
        .args(["/fi", &format!("imagename eq {}", exe_name), "/fo", "csv", "/nh"])
        .output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains(&exe_name.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

/// Returns a flat build number (minor*100 + patch). Display with [`format_version`].
/// Parses both old `v0.0.X` dirs and new `v0.M.P` dirs so upgrades are seamless.
pub fn find_next_version(build_dir: &Path) -> u32 {
    if !build_dir.exists() { return 1; }
    let mut highest = 0u32;
    if let Ok(entries) = fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() { continue; }
            let name = entry.file_name();
            let s    = name.to_string_lossy();
            // strip "v0." then parse "minor.patch"
            if let Some(rest) = s.strip_prefix("v0.") {
                let mut parts = rest.splitn(2, '.');
                if let (Some(m), Some(p)) = (parts.next(), parts.next()) {
                    if let (Ok(minor), Ok(patch)) = (m.parse::<u32>(), p.parse::<u32>()) {
                        let flat = minor * 100 + patch;
                        if flat > highest { highest = flat; }
                    }
                }
            }
        }
    }
    highest + 1
}

/// Converts a flat build number into `v0.minor.patch` (rolls over at 100).
/// n=1 → "v0.0.1", n=99 → "v0.0.99", n=100 → "v0.1.0", n=200 → "v0.2.0".
pub fn format_version(n: u32) -> String {
    format!("v0.{}.{}", n / 100, n % 100)
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
