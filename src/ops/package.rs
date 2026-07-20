use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Packages the Unreal project using UAT BuildCookRun.
/// This function has many arguments by necessity (it runs on a background thread
/// and receives all inputs by value so no shared references are needed).
#[allow(clippy::too_many_arguments)]
pub fn package_game(
    uproject:     PathBuf,
    engine:       PathBuf,
    pack_name:    String,
    exe_name:     String,
    version_str:  String,
    status:       Arc<Mutex<String>>,
    pending_zip:  Arc<Mutex<Option<PathBuf>>>,
    cancel:       Arc<AtomicBool>,
    progress:     Arc<Mutex<f32>>,
    close_editor: bool,
    use_space_free_link: bool,
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
    let version_dir = build_dir.join(&version_str);
    let log_path    = version_dir.join("BuildLog.txt");

    prog!(0.02);
    upd!(format!("[1/5] Creating output directory…\n→ {}", version_dir.display()));
    if let Err(e) = fs::create_dir_all(&version_dir) {
        return format!("[ERROR] mkdir: {}", e);
    }
    prog!(0.05);

    check!();
    if close_editor {
        close_editor_if_running(&status);
    }
    check!();

    // UAT's own batch scripts break on spaces in paths (most commonly hit via
    // the default "C:\Program Files\Epic Games\..." engine install). If the
    // user opted into the fix, alias the engine/project dirs to space-free
    // directory junctions and build the UAT command line from those instead —
    // the junctions are transparent to the filesystem, so output still lands
    // in the real `version_dir` computed above.
    let (engine_for_cmd, project_dir_for_cmd) = if use_space_free_link {
        let engine_alias = match crate::ops::preflight::ensure_space_free_alias(&engine) {
            Ok(p)  => p,
            Err(e) => return format!("[ERROR] Could not create space-free link for engine path: {e}"),
        };
        let project_alias = match crate::ops::preflight::ensure_space_free_alias(&project_dir) {
            Ok(p)  => p,
            Err(e) => return format!("[ERROR] Could not create space-free link for project path: {e}"),
        };
        (engine_alias, project_alias)
    } else {
        (engine.clone(), project_dir.clone())
    };
    let uproject_for_cmd = project_dir_for_cmd.join(uproject.file_name().unwrap_or_default());
    let archive_dir_for_cmd = project_dir_for_cmd.join("build").join(&version_str);

    let runuat = engine_for_cmd.join("Engine\\Build\\BatchFiles\\RunUAT.bat");
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
        .arg(format!("-project={}", uproject_for_cmd.display()))
        .args(["-noP4", "-unattended", "-platform=Win64",
               "-clientconfig=Development", "-serverconfig=Development",
               "-cook", "-allmaps", "-build", "-stage", "-pak", "-archive"])
        .arg(format!("-archivedirectory={}", archive_dir_for_cmd.display()))
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
            // `cmd /c RunUAT.bat` spawns AutomationTool, which in turn spawns
            // UnrealBuildTool and UnrealEditor-Cmd as separate child
            // processes. Killing just the cmd.exe (uat_child) leaves those
            // running — UnrealEditor-Cmd then keeps the project locked, so
            // the *next* build fails. Kill the whole process tree instead,
            // and make sure no Unreal Editor process is left holding the
            // project open.
            kill_process_tree(uat_child.id());
            let _ = uat_child.wait();
            close_editor_if_running(&status);
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
        let editor_hint = if !close_editor && is_editor_running() {
            "\nTip: Unreal Editor is still open — save your work, close it, then try again."
        } else {
            ""
        };
        return format!(
            "[ERROR] UAT failed (exit {}).\nLog → {}{}",
            uat_exit.code().unwrap_or(-1),
            log_path.display(),
            editor_hint,
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
        if found != target_exe
            && let Err(e) = fs::rename(&found, &target_exe) {
                return format!("[ERROR] rename exe: {}", e);
            }
    }

    let zip_name = format!("{}_{}.zip", pack_name, version_str);
    let zip_path = version_dir.join(&zip_name);
    upd!(format!("[5/5] Creating {}…", zip_name));

    if !package_dir.exists() {
        return format!("[ERROR] Package folder missing: {}", package_dir.display());
    }
    // Escape single quotes so paths like "Nick's Game" don't break PS string literals
    let src_esc = package_dir.display().to_string().replace('\'', "''");
    let dst_esc = zip_path.display().to_string().replace('\'', "''");
    let ps = format!(
        "$ErrorActionPreference='Stop'; \
         Compress-Archive -Path '{src}\\*' -DestinationPath '{dst}' -Force; \
         Write-Host 'Zip OK'",
        src = src_esc,
        dst = dst_esc,
    );
    check!();
    prog!(0.90);
    let mut zip_child = match crate::ops::cmd("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!("[ERROR] PowerShell launch: {}", e),
    };
    let zip_exit = loop {
        if cancel.load(Ordering::Relaxed) {
            kill_process_tree(zip_child.id());
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
        // Read whatever PowerShell wrote to stderr for a useful error message
        let stderr_msg = zip_child.stderr
            .take()
            .and_then(|mut r| {
                let mut s = String::new();
                use std::io::Read;
                r.read_to_string(&mut s).ok().map(|_| s)
            })
            .unwrap_or_default();
        return if stderr_msg.trim().is_empty() {
            format!(
                "[ERROR] Compress-Archive failed (exit {}).\nLog → {}",
                zip_exit.code().unwrap_or(-1),
                log_path.display()
            )
        } else {
            format!(
                "[ERROR] Compress-Archive failed (exit {}):\n{}\nLog → {}",
                zip_exit.code().unwrap_or(-1),
                stderr_msg.trim(),
                log_path.display()
            )
        };
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
// Uses the rclone copy bundled at Reclone/rclone-v1.74.3-windows-amd64/rclone.exe
// next to the app (falls back to a "rclone" on PATH if that folder is missing).
// Either way a remote named "gdrive" must be set up once with `rclone config`
// in PowerShell — that's what links rclone to your Google account.
//
// The destination field accepts two forms:
//   - An rclone path, e.g.            gdrive:/Builds/MobiusFish
//   - A Drive folder share link, e.g. https://drive.google.com/drive/folders/<ID>
//     (the folder ID is extracted and passed via --drive-root-folder-id,
//      still routed through the "gdrive" remote — a share link alone carries
//      no credentials, so the remote must already have access to that folder)

const DRIVE_REMOTE: &str = "gdrive:";

// rclone.exe is embedded directly into this binary at compile time.
// On first use it is extracted to %APPDATA%\UnrealDevtool\rclone\rclone.exe
// and reused from there on subsequent runs.
static RCLONE_EXE_BYTES: &[u8] =
    include_bytes!("../../Reclone/rclone-v1.74.3-windows-amd64/rclone.exe");

fn rclone_appdata_path() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(Path::new(&appdata).join("UnrealDevtool").join("rclone").join("rclone.exe"))
}

/// Returns a path to rclone.exe, extracting the embedded copy to %APPDATA% if
/// it hasn't been extracted yet. Falls back to a PATH lookup if that fails.
fn rclone_program() -> String {
    if let Some(dest) = rclone_appdata_path() {
        if dest.is_file() {
            return dest.to_string_lossy().to_string();
        }
        if let Some(parent) = dest.parent()
            && std::fs::create_dir_all(parent).is_ok()
                && std::fs::write(&dest, RCLONE_EXE_BYTES).is_ok()
            {
                return dest.to_string_lossy().to_string();
            }
    }
    "rclone".to_string()
}

/// Pulls the folder ID out of a Google Drive share link, e.g.
/// "https://drive.google.com/drive/folders/<ID>?usp=sharing" -> "<ID>"
/// or  "https://drive.google.com/open?id=<ID>"               -> "<ID>"
pub fn drive_folder_id_from_url(url: &str) -> Option<String> {
    let id_chars = |s: &str| -> String {
        s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect()
    };
    if let Some(rest) = url.split("/folders/").nth(1) {
        let id = id_chars(rest);
        if !id.is_empty() { return Some(id); }
    }
    if let Some(rest) = url.split("id=").nth(1) {
        let id = id_chars(rest);
        if !id.is_empty() { return Some(id); }
    }
    None
}

/// Quick local check (reads rclone's config file, no network) — true if a
/// remote named "gdrive" is already set up.
pub fn gdrive_remote_exists() -> bool {
    let program = rclone_program();
    match crate::ops::cmd(&program).arg("listremotes").output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|line| line.trim() == DRIVE_REMOTE),
        Err(_) => false,
    }
}

/// Opens a new visible console window running `rclone config`, so the user
/// can interactively create the "gdrive" remote. The OAuth step opens a
/// browser for the user to sign in to the Google account that should have
/// access to the destination folder — that part can't be automated.
pub fn open_rclone_config_setup() -> std::io::Result<()> {
    let program = rclone_program();
    std::process::Command::new("cmd")
        .args(["/C", "start", "rclone config — set up the \"gdrive\" remote", &program, "config"])
        .spawn()?;
    Ok(())
}

pub fn upload_via_rclone(
    zip:         &Path,
    rclone_dest: &str,
    status:      &Arc<Mutex<String>>,
    cancel:      &Arc<AtomicBool>,
) -> String {
    let dest = rclone_dest.trim();
    if dest.is_empty() {
        return "[ERROR] rclone destination is empty.\n\
                Enter a path like  gdrive:/Builds/MyGame  or paste a Drive folder share link.".to_string();
    }
    if !zip.exists() {
        return format!("[ERROR] Zip file not found: {}", zip.display());
    }

    let file_name = zip.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "build.zip".to_string());
    let zip_str = zip.to_string_lossy().to_string();

    // A pasted Drive share link isn't an rclone path — translate it into
    // "gdrive:" + --drive-root-folder-id so it lands in that exact folder.
    let (copy_args, target_label, remote_name): (Vec<String>, String, String) =
        if dest.starts_with("http://") || dest.starts_with("https://") {
            match drive_folder_id_from_url(dest) {
                Some(folder_id) => (
                    vec![
                        "copy".to_string(), zip_str.clone(), DRIVE_REMOTE.to_string(),
                        "--drive-root-folder-id".to_string(), folder_id.clone(),
                    ],
                    format!("Drive folder {}", folder_id),
                    DRIVE_REMOTE.trim_end_matches(':').to_string(),
                ),
                None => return "[ERROR] Could not find a folder ID in that Google Drive link.\n\
                                Paste a folder link like:\n\
                                https://drive.google.com/drive/folders/<FOLDER_ID>\n\
                                or use rclone path syntax:  gdrive:/Builds/MyGame".to_string(),
            }
        } else {
            let remote = dest.split(':').next().unwrap_or(dest).to_string();
            (vec!["copy".to_string(), zip_str.clone(), dest.to_string()], dest.to_string(), remote)
        };

    *status.lock().unwrap() = format!(
        "[UPLOADING] Sending {}  ->  {}\n(via rclone — this may take a while for large builds)",
        file_name, target_label,
    );

    let program = rclone_program();
    let mut child = match crate::ops::cmd(&program)
        .args(&copy_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!(
            "[ERROR] Could not launch rclone ({}): {}\n\
             Download: https://rclone.org/",
            program, e
        ),
    };

    // Drain stdout/stderr on their own threads as rclone writes them. If we
    // only read after the process exits, a chatty run (e.g. several retry
    // warnings) can fill the OS pipe buffer; rclone then blocks on write()
    // forever and the upload looks like it "just hangs" — this keeps the
    // pipes empty the whole time so that can't happen.
    let drain = |mut r: Box<dyn std::io::Read + Send>| -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = String::new();
            use std::io::Read;
            let _ = r.read_to_string(&mut buf);
            let _ = tx.send(buf);
        });
        rx
    };
    let stdout_rx = child.stdout.take().map(|s| drain(Box::new(s)));
    let stderr_rx = child.stderr.take().map(|s| drain(Box::new(s)));

    let exit = loop {
        if cancel.load(Ordering::Relaxed) {
            kill_process_tree(child.id());
            let _ = child.wait();
            return "[CANCELLED] rclone upload cancelled.".to_string();
        }
        match child.try_wait() {
            Ok(Some(code)) => break code,
            Ok(None)       => std::thread::sleep(Duration::from_millis(500)),
            Err(e)         => return format!("[ERROR] Waiting for rclone: {}", e),
        }
    };

    if exit.success() {
        return format!("[DONE] Uploaded {} to {}", file_name, target_label);
    }

    // rclone writes the actual reason (expired auth, no permission on the
    // destination, bad folder ID, network blocked, etc.) to stdout/stderr —
    // without capturing it the user only ever sees an exit code and has no
    // way to tell why the upload "just isn't working".
    let mut detail = String::new();
    if let Some(rx) = stdout_rx {
        detail.push_str(&rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default());
    }
    if let Some(rx) = stderr_rx {
        let err = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
        if !err.trim().is_empty() {
            if !detail.trim().is_empty() { detail.push('\n'); }
            detail.push_str(&err);
        }
    }
    let detail = detail.trim();

    if detail.is_empty() {
        format!(
            "[ERROR] rclone exited with code {}.\n\
             Check that the \"{}\" remote is configured (run  rclone config)\n\
             and has access to the destination.",
            exit.code().unwrap_or(-1), remote_name
        )
    } else {
        format!(
            "[ERROR] rclone exited with code {}:\n{}\n\
             Check that the \"{}\" remote is configured (run  rclone config)\n\
             and has access to the destination.",
            exit.code().unwrap_or(-1), detail, remote_name
        )
    }
}

/// Returns `true` if any known Unreal Editor process is currently running.
/// Fast — reads the OS process list, no network or disk I/O.
pub fn is_editor_running() -> bool {
    const EDITORS: &[&str] = &["UnrealEditor.exe", "UE4Editor.exe"];
    EDITORS.iter().any(|e| is_process_running(e))
}

fn close_editor_if_running(status: &Arc<Mutex<String>>) {
    const EDITORS: &[&str] = &["UnrealEditor.exe", "UE4Editor.exe"];
    for editor_exe in EDITORS {
        if !is_process_running(editor_exe) { continue; }

        *status.lock().unwrap() = format!(
            "[PRE-FLIGHT] {} is open — closing it before packaging…",
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

/// Kills `pid` and its entire descendant process tree (e.g. `cmd.exe` ->
/// AutomationTool -> UnrealBuildTool / UnrealEditor-Cmd). Plain `Child::kill`
/// only kills the immediate process and leaves such descendants running.
fn kill_process_tree(pid: u32) {
    let _ = crate::ops::cmd("taskkill")
        .args(["/f", "/t", "/pid", &pid.to_string()])
        .output();
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
                if let (Some(m), Some(p)) = (parts.next(), parts.next())
                    && let (Ok(minor), Ok(patch)) = (m.parse::<u32>(), p.parse::<u32>()) {
                        let flat = minor * 100 + patch;
                        if flat > highest { highest = flat; }
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
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
        .filter(|e| {
            let stem = e.path().file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            !SKIP.contains(&stem.as_str())
        })
        .map(|e| e.path())
        .next()
}
