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
    let version_str = format!("v0.0.{}", version_num);
    let version_dir = build_dir.join(&version_str);
    let log_path    = version_dir.join("BuildLog.txt");

    prog!(0.02);
    upd!(format!("[1/5] Creating output directory…\n→ {}", version_dir.display()));
    if let Err(e) = fs::create_dir_all(&version_dir) {
        return format!("[ERROR] mkdir: {}", e);
    }
    prog!(0.05);

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

// ── Post-package: upload to Google Drive ─────────────────────────────────────
//
// Architecture: reqwest (streaming) + yup-oauth2 (OAuth2 / tokencache.json)
//
// Phase 1 – Handshake
//   POST file metadata to …?uploadType=resumable
//   Extract the dedicated upload channel URI from the Location response header.
//
// Phase 2 – Chunked Streaming  (O(1) memory)
//   tokio::fs::File  →  FramedRead<_, BytesCodec>  →  reqwest::Body::wrap_stream
//   The runtime reads only one 8 KB chunk at a time; RAM stays constant
//   regardless of file size — safe for multi-GB Unreal packages.
//
// First run  : yup-oauth2 starts a local HTTP server, opens the system browser
//              for Google consent, waits for the 127.0.0.1 redirect with the
//              auth code, exchanges it for tokens, writes tokencache.json.
// Later runs : tokencache.json is read; access token is silently refreshed;
//              the browser is never opened again unless the user signs out.

pub fn upload_to_gdrive_oauth(
    zip:         &Path,
    folder_id:   &str,
    secret_path: &Path,   // path to client_secret.json from Google Cloud Console
    token_path:  &Path,   // path to tokencache.json
    status:      &Arc<Mutex<String>>,
) -> String {
    // ── Guard clauses — fail fast, no runtime needed ──────────────────────────
    if folder_id.trim().is_empty() {
        return "[ERROR] Google Drive Folder ID is required.".to_string();
    }
    if !secret_path.exists() {
        return format!(
            "[ERROR] client_secret.json not found: {}\n\
             Download it from console.cloud.google.com → APIs & Services → Credentials.",
            secret_path.display()
        );
    }
    if !zip.exists() {
        return format!("[ERROR] Zip file not found: {}", zip.display());
    }

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return format!("[ERROR] Could not start async runtime: {}", e),
    };

    let zip         = zip.to_path_buf();
    let folder_id   = folder_id.trim().to_string();
    let secret_path = secret_path.to_path_buf();
    let token_path  = token_path.to_path_buf();
    let status_arc  = Arc::clone(status);

    match rt.block_on(gdrive_upload_pipeline(zip, folder_id, secret_path, token_path, status_arc)) {
        Ok(msg)  => msg,
        Err(msg) => msg,
    }
}

async fn gdrive_upload_pipeline(
    zip:         std::path::PathBuf,
    folder_id:   String,
    secret_path: std::path::PathBuf,
    token_path:  std::path::PathBuf,
    status:      Arc<Mutex<String>>,
) -> Result<String, String> {
    use tokio_util::codec::{BytesCodec, FramedRead};

    // ── Read client_secret.json ───────────────────────────────────────────────
    let secret = yup_oauth2::read_application_secret(&secret_path)
        .await
        .map_err(|e| format!("[ERROR] Read client_secret.json: {}", e))?;

    // Ensure AppData dir exists before yup-oauth2 writes tokencache.json
    if let Some(parent) = token_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    // Show different status depending on whether this is a first-time auth
    // or a cached token refresh — .build().await blocks until the browser
    // flow completes, so the message must be set BEFORE calling it.
    if token_path.exists() {
        *status.lock().unwrap() =
            "[AUTH] Loading saved Google session…".to_string();
    } else {
        *status.lock().unwrap() =
            "[AUTH] Your browser should open for Google sign-in.\n\
             Please sign in and click Allow to continue.\n\
             (This only happens once — session will be saved after.)".to_string();
    }

    let auth = yup_oauth2::InstalledFlowAuthenticator::builder(
            secret,
            yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(&token_path)
        .build()
        .await
        .map_err(|e| format!("[ERROR] OAuth2 setup: {}", e))?;

    // Acquire access token (silently refreshed when tokencache.json is valid)
    let scopes = &["https://www.googleapis.com/auth/drive.file"];
    let token  = auth.token(scopes)
        .await
        .map_err(|e| format!("[ERROR] Acquire token: {}", e))?;
    let access_token = token.token()
        .ok_or_else(|| "[ERROR] Google returned an empty access token.".to_string())?
        .to_string();

    // ── Fetch and cache the signed-in user's email ────────────────────────────
    let http = reqwest::Client::new();
    if let Ok(resp) = http
        .get("https://www.googleapis.com/drive/v3/about?fields=user")
        .bearer_auth(&access_token)
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(email) = json["user"]["emailAddress"].as_str() {
                crate::config::save_gdrive_user(email);
            }
        }
    }

    // ── File metadata ─────────────────────────────────────────────────────────
    let file_name = zip.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "build.zip".to_string());
    let file_size = tokio::fs::metadata(&zip)
        .await
        .map_err(|e| format!("[ERROR] Stat zip: {}", e))?
        .len();

    // ── Phase 1: POST metadata → receive resumable upload channel URI ─────────
    *status.lock().unwrap() =
        "[UPLOADING] Initialising Google Drive resumable upload session…".to_string();

    let metadata = serde_json::json!({ "name": file_name, "parents": [folder_id] });

    let init = http
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable")
        .bearer_auth(&access_token)
        .header("Content-Type",            "application/json; charset=UTF-8")
        .header("X-Upload-Content-Type",   "application/zip")
        .header("X-Upload-Content-Length", file_size)
        .json(&metadata)
        .send()
        .await
        .map_err(|e| format!("[ERROR] Init upload session: {}", e))?;

    if !init.status().is_success() {
        let code = init.status();
        let body = init.text().await.unwrap_or_default();
        return Err(format!("[ERROR] Init upload HTTP {}: {}", code, &body[..body.len().min(400)]));
    }

    let upload_uri = init
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "[ERROR] No Location header in upload init response.".to_string())?
        .to_string();

    // ── Phase 2: stream the zip into the upload channel (O(1) memory) ─────────
    *status.lock().unwrap() = format!(
        "[UPLOADING] Streaming {} ({:.1} MB) to Google Drive…",
        file_name, file_size as f64 / 1_048_576.0,
    );

    // tokio::fs::File::open  — never loads the file into memory
    let file   = tokio::fs::File::open(&zip)
        .await
        .map_err(|e| format!("[ERROR] Open zip: {}", e))?;

    // FramedRead reads 8 KB chunks; BytesCodec yields Bytes per chunk.
    // wrap_stream pipes the async stream directly into the PUT request body.
    let stream = FramedRead::new(file, BytesCodec::new());
    let body   = reqwest::Body::wrap_stream(stream);

    let upload = http
        .put(&upload_uri)
        .header("Content-Type",   "application/zip")
        .header("Content-Length", file_size)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("[ERROR] Upload: {}", e))?;

    if upload.status().is_success() {
        Ok(format!(
            "[DONE] Uploaded {} ({:.1} MB) to Google Drive.",
            file_name, file_size as f64 / 1_048_576.0,
        ))
    } else {
        let code = upload.status();
        let body = upload.text().await.unwrap_or_default();
        Err(format!("[ERROR] Upload HTTP {}: {}", code, &body[..body.len().min(400)]))
    }
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
