use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::types::IdeChoice;

pub fn rebuild_vs_files(
    uproject: PathBuf,
    engine:   PathBuf,
    ide:      IdeChoice,
    status:   Arc<Mutex<String>>,
) -> String {
    macro_rules! upd { ($s:expr) => { *status.lock().unwrap() = $s.to_string(); }; }

    let project_dir = match uproject.parent() {
        Some(p) => p.to_path_buf(),
        None    => return "[ERROR] Bad project path.".into(),
    };

    // ── Step 1: clean ─────────────────────────────────────────────────────────
    const CLEAN_DIRS: &[&str] = &[
        "Binaries", "Intermediate", "Saved", ".idea", ".vs", "DerivedDataCache",
    ];
    upd!("[1/3] Cleaning generated files…");
    for name in CLEAN_DIRS {
        let p = project_dir.join(name);
        if p.exists() {
            upd!(format!("[1/3] Removing {}…", name));
            let _ = fs::remove_dir_all(&p);
        }
    }
    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e.eq_ignore_ascii_case("sln")) {
                upd!(format!("[1/3] Removing {}…",
                    path.file_name().unwrap_or_default().to_string_lossy()));
                let _ = fs::remove_file(&path);
            }
        }
    }

    // ── Step 2: generate ──────────────────────────────────────────────────────
    let gpf_bat   = engine.join("Engine\\Build\\BatchFiles\\GenerateProjectFiles.bat");
    let build_bat = engine.join("Engine\\Build\\BatchFiles\\Build.bat");

    let gen_result = if gpf_bat.exists() {
        upd!("[2/3] Running GenerateProjectFiles.bat…");
        std::process::Command::new("cmd")
            .args(["/c", &gpf_bat.to_string_lossy()])
            .arg(format!("-project={}", uproject.display()))
            .args(["-game", "-rocket", "-progress"])
            .output()
    } else if build_bat.exists() {
        upd!("[2/3] Running Build.bat -ProjectFiles…");
        std::process::Command::new("cmd")
            .args(["/c", &build_bat.to_string_lossy()])
            .arg("-ProjectFiles")
            .arg(format!("-project={}", uproject.display()))
            .args(["-game", "-rocket", "-progress"])
            .output()
    } else {
        return format!(
            "[ERROR] No generator bat found in:\n{}",
            engine.join("Engine\\Build\\BatchFiles").display()
        );
    };

    let gen_out = match gen_result {
        Err(e) => return format!("[ERROR] Launch failed: {}", e),
        Ok(o)  => o,
    };
    let log_path = project_dir.join("GenerateProjectFiles.log");
    let _ = fs::write(&log_path, format!(
        "=== STDOUT ===\n{}\n\n=== STDERR ===\n{}",
        String::from_utf8_lossy(&gen_out.stdout),
        String::from_utf8_lossy(&gen_out.stderr),
    ));
    if !gen_out.status.success() {
        return format!(
            "[ERROR] Generator failed (exit {}).\nLog → {}",
            gen_out.status.code().unwrap_or(-1),
            log_path.display()
        );
    }

    // ── Step 3: open IDE ──────────────────────────────────────────────────────
    let sln = find_sln(&project_dir);
    match ide {
        IdeChoice::SkipOpen => {}
        IdeChoice::Rider => {
            upd!("[3/3] Opening with Rider…");
            if let Some(sln_path) = &sln {
                match find_rider() {
                    Some(exe) => { let _ = std::process::Command::new(&exe).arg(sln_path).spawn(); }
                    None      => shell_open(sln_path),
                }
            }
        }
        IdeChoice::VisualStudio => {
            upd!("[3/3] Opening with Visual Studio…");
            if let Some(sln_path) = &sln { shell_open(sln_path); }
        }
    }

    match sln {
        Some(p) => format!(
            "[DONE] Project files rebuilt.\nSolution → {}\nLog → {}",
            p.display(), log_path.display()
        ),
        None => format!(
            "[DONE] Project files rebuilt (no .sln found).\nLog → {}",
            log_path.display()
        ),
    }
}

pub fn find_sln(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().map_or(false, |x| x.eq_ignore_ascii_case("sln")))
}

pub fn find_rider() -> Option<PathBuf> {
    // 1. PATH (JetBrains Toolbox adds this)
    if let Ok(o) = std::process::Command::new("where").arg("rider64.exe").output() {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = s.lines().next() {
                let p = PathBuf::from(line.trim());
                if p.exists() { return Some(p); }
            }
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
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", &path.to_string_lossy()])
        .spawn();
}
