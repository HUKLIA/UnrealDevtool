use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use crate::types::IdeChoice;

pub fn rebuild_vs_files(
    uproject: PathBuf,
    engine:   PathBuf,
    ide:      IdeChoice,
    status:   Arc<Mutex<String>>,
    cancel:   Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
) -> String {
    macro_rules! upd   { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }
    macro_rules! prog  { ($v:expr) => { *progress.lock().unwrap() = $v; }; }
    macro_rules! check { () => { if cancel.load(Ordering::Relaxed) {
        return "[CANCELLED] Operation was cancelled.".to_string();
    }}; }

    let project_dir = match uproject.parent() {
        Some(p) => p.to_path_buf(),
        None    => return "[ERROR] Bad project path.".into(),
    };

    // ── Step 1: clean ─────────────────────────────────────────────────────────
    const CLEAN_DIRS: &[&str] = &[
        "Binaries", "Intermediate", "Saved", ".idea", ".vs", "DerivedDataCache",
    ];
    prog!(0.02);
    upd!("[1/3] Cleaning generated files…");
    let total_clean = CLEAN_DIRS.len() as f32;
    for (i, name) in CLEAN_DIRS.iter().enumerate() {
        let p = project_dir.join(name);
        if p.exists() {
            upd!(format!("[1/3] Removing {}…", name));
            let _ = fs::remove_dir_all(&p);
        }
        prog!(0.02 + (i as f32 + 1.0) / total_clean * 0.23);
    }
    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sln")) {
                upd!(format!("[1/3] Removing {}…",
                    path.file_name().unwrap_or_default().to_string_lossy()));
                let _ = fs::remove_file(&path);
            }
        }
    }

    // ── Step 2: generate ──────────────────────────────────────────────────────
    let gpf_bat   = engine.join("Engine\\Build\\BatchFiles\\GenerateProjectFiles.bat");
    let build_bat = engine.join("Engine\\Build\\BatchFiles\\Build.bat");

    check!();
    let log_path = project_dir.join("GenerateProjectFiles.log");

    let (bat_path, bat_args): (&PathBuf, &[&str]) = if gpf_bat.exists() {
        upd!("[2/3] Running GenerateProjectFiles.bat…");
        (&gpf_bat, &["-game", "-rocket", "-progress"])
    } else if build_bat.exists() {
        upd!("[2/3] Running Build.bat -ProjectFiles…");
        (&build_bat, &["-ProjectFiles", "-game", "-rocket", "-progress"])
    } else {
        return format!(
            "[ERROR] No generator bat found in:\n{}",
            engine.join("Engine\\Build\\BatchFiles").display()
        );
    };

    let log_out = match fs::File::create(&log_path) {
        Ok(f)  => f,
        Err(e) => return format!("[ERROR] Create log: {}", e),
    };
    let log_err = match log_out.try_clone() {
        Ok(f)  => f,
        Err(e) => return format!("[ERROR] Clone log handle: {}", e),
    };

    let mut gen_child = match crate::ops::cmd("cmd")
        .args(["/c", &bat_path.to_string_lossy()])
        .arg(format!("-project={}", uproject.display()))
        .args(bat_args)
        .stdout(log_out)
        .stderr(log_err)
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return format!("[ERROR] Launch failed: {}", e),
    };

    prog!(0.30);
    let gen_exit = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = gen_child.kill();
            let _ = gen_child.wait();
            return "[CANCELLED] Project file generation was cancelled.".to_string();
        }
        match gen_child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None)    => {
                let cur = *progress.lock().unwrap();
                *progress.lock().unwrap() = cur + (0.88 - cur) * 0.01;
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(e) => return format!("[ERROR] Waiting for generator: {}", e),
        }
    };
    if !gen_exit.success() {
        return format!(
            "[ERROR] Generator failed (exit {}).\nLog → {}",
            gen_exit.code().unwrap_or(-1),
            log_path.display()
        );
    }
    prog!(0.90);

    // ── Step 3: open IDE ──────────────────────────────────────────────────────
    let sln = find_sln(&project_dir);
    match ide {
        IdeChoice::SkipOpen => {}
        IdeChoice::Rider => {
            upd!("[3/3] Opening with Rider…");
            if let Some(sln_path) = &sln {
                match find_rider() {
                    Some(exe) => { let _ = crate::ops::cmd(&exe.to_string_lossy()).arg(sln_path).spawn(); }
                    None      => shell_open(sln_path),
                }
            }
        }
        IdeChoice::VisualStudio => {
            upd!("[3/3] Opening with Visual Studio…");
            if let Some(sln_path) = &sln { shell_open(sln_path); }
        }
    }

    let _ = fs::remove_file(&log_path);
    prog!(1.0);

    match sln {
        Some(p) => format!("[DONE] Project files rebuilt.\nSolution → {}", p.display()),
        None    => "[DONE] Project files rebuilt (no .sln found).".to_string(),
    }
}

pub fn find_sln(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sln")))
}

pub fn find_rider() -> Option<PathBuf> {
    // 1. PATH (JetBrains Toolbox adds this)
    if let Ok(o) = crate::ops::cmd("where").arg("rider64.exe").output()
        && o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = s.lines().next() {
                let p = PathBuf::from(line.trim());
                if p.exists() { return Some(p); }
            }
        }
    // 2. JetBrains Toolbox install location
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(&local)
            .join("JetBrains").join("Toolbox").join("apps").join("Rider");
        if let Some(exe) = scan_for_rider(&base) { return Some(exe); }
    }
    // 3. Program Files direct install
    for root in &["C:\\Program Files", "C:\\Program Files (x86)"] {
        if let Some(exe) = scan_for_rider(&PathBuf::from(root).join("JetBrains")) {
            return Some(exe);
        }
    }
    None
}

pub fn scan_for_rider(base: &Path) -> Option<PathBuf> {
    if !base.exists() { return None; }
    for d1 in fs::read_dir(base).ok()?.flatten() {
        let c1 = d1.path().join("bin").join("rider64.exe");
        if c1.exists() { return Some(c1); }
        for d2 in fs::read_dir(d1.path()).ok()?.flatten() {
            let c2 = d2.path().join("bin").join("rider64.exe");
            if c2.exists() { return Some(c2); }
        }
    }
    None
}

pub fn shell_open(path: &Path) {
    let _ = crate::ops::cmd("cmd")
        .args(["/c", "start", "", &path.to_string_lossy()])
        .spawn();
}
