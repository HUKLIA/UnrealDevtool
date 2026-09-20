use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::types::BuildConfiguration;

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
    configuration: BuildConfiguration,
    target:       crate::types::BuildTarget,
    status:       Arc<Mutex<String>>,
    pending_zip:  Arc<Mutex<Option<PathBuf>>>,
    cancel:       Arc<AtomicBool>,
    progress:     Arc<Mutex<f32>>,
    // Live stage/log state, read by the run surface while this executes.
    run:          Arc<Mutex<crate::ops::run::RunProgress>>,
    // Reuse the previous cook instead of starting from scratch.
    iterate:      bool,
    use_space_free_link: bool,
    // Already validated by `parse_extra_uat_args`; appended verbatim.
    extra_args:   Vec<String>,
    method:       crate::types::PackageMethod,
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
    if !uproject.is_file() {
        return format!("[ERROR] Project file not found: {}", uproject.display());
    }
    if let Err(e) = validate_leaf_name(&pack_name, "Package name") {
        return format!("[ERROR] {e}");
    }
    if let Err(e) = validate_leaf_name(&exe_name, "Executable name") {
        return format!("[ERROR] {e}");
    }
    if let Err(e) = validate_leaf_name(&version_str, "Version") {
        return format!("[ERROR] {e}");
    }

    let build_dir   = project_dir.join("build");
    let version_dir = build_dir.join(&version_str);
    let log_path    = version_dir.join("BuildLog.txt");

    if version_dir.exists() {
        if !version_dir.is_dir() {
            return format!("[ERROR] Version output path is not a folder: {}", version_dir.display());
        }
        match fs::read_dir(&version_dir) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    return format!(
                        "[ERROR] Version output already exists: {}\nChoose a new version before packaging.",
                        version_dir.display(),
                    );
                }
            }
            Err(e) => return format!("[ERROR] Read version output folder: {e}"),
        }
    }

    check!();
    upd!("[1/5] Closing Unreal Editor before packaging…");
    if let Err(e) = close_editor_if_running(&status) {
        return format!("[ERROR] {e}");
    }
    check!();

    prog!(0.02);
    upd!(format!("[1/5] Creating output directory…\n-> {}", version_dir.display()));
    if let Err(e) = fs::create_dir_all(&version_dir) {
        return format!("[ERROR] mkdir: {}", e);
    }
    prog!(0.05);

    // UAT's own batch scripts break on spaces in paths (most commonly hit via
    // the default "C:\Program Files\Epic Games\..." engine install). If the
    // user opted into the fix, alias the engine/project dirs to space-free
    // directory junctions and build the UAT command line from those instead —
    // the junctions are transparent to the filesystem, so output still lands
    // in the real `version_dir` computed above.
    let use_space_free_link = use_space_free_link
        || crate::ops::preflight::has_space(&engine)
        || crate::ops::preflight::has_space(&project_dir);
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
    if !runuat.is_file() {
        return format!("[ERROR] RunUAT.bat not found: {}", runuat.display());
    }
    upd!(format!("[2/5] Running UAT BuildCookRun…  (may take 30+ min)\nLog -> {}", log_path.display()));

    use crate::types::PackageMethod;
    use crate::ops::run::Stage;

    // Check again immediately before launching anything so a user cannot
    // reopen Unreal during the small setup window after the initial pre-flight.
    check!();
    upd!("[2/5] Verifying Unreal Editor is closed…");
    if let Err(e) = close_editor_if_running(&status) {
        return format!("[ERROR] {e}");
    }

    let stem = uproject.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let steps = StepEnv {
        status: &status, run: &run, progress: &progress, cancel: &cancel, log_path: &log_path,
    };

    // Restaging reuses a cook, so there has to be one for this platform.
    if method == PackageMethod::Restage && !has_cook_for(&project_dir, target.output_prefix()) {
        return format!(
            "[ERROR] No previous {} cook found under {}.\nRun a Full package once first, then Restage can reuse it.",
            target.label(),
            project_dir.join("Saved").join("Cooked").display(),
        );
    }

    // Stepwise compiles the editor and game targets itself, which is only
    // possible when the project has C++ targets to compile. A Blueprint-only
    // project — or one whose target files are not where UnrealBuildTool expects
    // them — falls back to UAT doing its own compile, and says so.
    let code_targets = if method == PackageMethod::Stepwise { find_code_targets(&project_dir, &stem) } else { None };
    let compile_first = code_targets.is_some();
    if method == PackageMethod::Stepwise && !compile_first {
        upd!("[2/5] No C++ targets found — UAT will do any compiling itself…");
    }
    let mut uat_from = 0.08;

    if compile_first {
        let build_bat = engine_for_cmd.join(r"Engine\Build\BatchFiles\Build.bat");
        if !build_bat.is_file() {
            return format!("[ERROR] Build.bat not found: {}", build_bat.display());
        }
        // Editor target first — cooking runs the editor's code — then the game.
        let (game_target, editor_target) = code_targets.clone().unwrap_or_default();
        let plan: [(&str, String, &str, &str, f32, f32); 2] = [
            ("Compile (editor)", editor_target, "Win64", "Development", 0.08, 0.17),
            ("Compile (game)", game_target, target.ubt_platform(), configuration.as_str(), 0.17, 0.27),
        ];
        for (i, (name, tgt, plat, cfg, lo, hi)) in plan.into_iter().enumerate() {
            upd!(format!("[2/5] {name}: UnrealBuildTool {tgt} {plat} {cfg}…\nLog -> {}", log_path.display()));
            let mut c = crate::ops::batch_cmd(&build_bat);
            c.arg(&tgt).arg(plat).arg(cfg)
                .arg(format!("-Project={}", uproject_for_cmd.display()))
                .args(["-WaitMutex", "-FromMsBuild"]);
            if let Err(msg) = run_logged_step(&steps, name, c, i > 0, lo, hi, Some(Stage::Compile), false) {
                return msg;
            }
        }
        uat_from = 0.27;
    }

    let uat_label = match method {
        PackageMethod::Full     => "UAT BuildCookRun",
        PackageMethod::Stepwise => "UAT cook and package",
        PackageMethod::Restage  => "UAT restage",
    };
    upd!(format!("[2/5] Running {uat_label}…  (may take 30+ min)\nLog -> {}", log_path.display()));

    let uat = uat_command(
        &runuat, &uproject_for_cmd, target, configuration, method,
        compile_first, iterate, &extra_args, &archive_dir_for_cmd,
    );

    if let Err(msg) = run_logged_step(&steps, "UAT", uat, compile_first, uat_from, 0.80, None, true) {
        return msg;
    }
    // One last read so the tail of the log is not lost, then close the open
    // stage so its duration is final rather than still counting.
    {
        let mut r = run.lock().unwrap_or_else(|e| e.into_inner());
        r.ingest(&log_path);
        r.finish();
    }
    prog!(0.80);

    // UAT archives into <archivedirectory>/<Platform>/. The folder is matched
    // by prefix, not by exact name: Win64 archives to "Windows" rather than
    // "Win64", and Android appends the texture format ("Android_ASTC",
    // "Android_DXT", …) so the exact name is not known ahead of time.
    let prefix = target.output_prefix();
    let Some(uat_out) = find_output_dir(&version_dir, prefix) else {
        return format!(
            "[ERROR] UAT output not found: no {}* folder under {}{}",
            prefix,
            version_dir.display(),
            uat_failure_details(&log_path),
        );
    };

    let package_dir = if uat_out.file_name().is_some_and(|n| n.eq_ignore_ascii_case(pack_name.as_str())) {
        uat_out
    } else {
        let from = uat_out.file_name().map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| prefix.to_string());
        upd!(format!("[3/5] Renaming: {from} -> {}", pack_name));
        let dest = version_dir.join(&pack_name);
        if let Err(e) = fs::rename(&uat_out, &dest) {
            return format!("[ERROR] rename folder: {}", e);
        }
        dest
    };

    prog!(0.85);
    // Only Windows has a single `.exe` worth renaming. Android emits an
    // apk/aab, Linux a bare ELF and Mac an `.app` bundle — renaming any of
    // those to `<name>.exe` would break the output rather than tidy it, so
    // the step is skipped and the platform's own artifact is left alone.
    if target.renames_executable() {
        upd!("[4/5] Renaming executable…");
        let target_exe = package_dir.join(format!("{}.exe", exe_name));
        let found = if target_exe.is_file() {
            target_exe.clone()
        } else {
            let Some(found) = find_main_exe(&package_dir) else {
                return format!("[ERROR] No packaged game executable found in {}", package_dir.display());
            };
            found
        };
        if found != target_exe
            && let Err(e) = fs::rename(&found, &target_exe)
        {
            return format!("[ERROR] rename exe: {}", e);
        }
    } else {
        upd!(format!("[4/5] {} output left as packaged…", target.label()));
    }

    let zip_name = format!("{}_{}.zip", pack_name, version_str);
    let zip_path = version_dir.join(&zip_name);

    // Set pending_zip now (before the zip runs) so the "Open Folder" panel
    // appears even if Compress-Archive fails — UAT already succeeded, so the
    // build is good regardless of whether we managed to zip it.
    *pending_zip.lock().unwrap() = Some(zip_path.clone());

    upd!(format!("[5/5] Creating {}…", zip_name));

    if !package_dir.exists() {
        return format!("[ERROR] Package folder missing: {}", package_dir.display());
    }
    check!();
    prog!(0.90);
    // Zipped in-process. This used to launch `powershell Compress-Archive`,
    // which was slow to start, failed on archives over 2 GB, and made an
    // unsigned executable look like a script launcher to antivirus engines.
    let zip_result = zip_directory(&package_dir, &zip_path, &cancel, |frac| {
        *progress.lock().unwrap() = 0.90 + 0.09 * frac;
    });
    match zip_result {
        Ok(()) => {}
        Err(ZipFailure::Cancelled) => {
            let _ = fs::remove_file(&zip_path);
            return "[CANCELLED] Zip was cancelled.".to_string();
        }
        Err(ZipFailure::Io(e)) => {
            let _ = fs::remove_file(&zip_path);
            return format!("[ERROR] Zip failed: {e}
Log -> {}", log_path.display());
        }
    }
    prog!(1.0);

    format!(
        "[DONE] {} — packaged!\nOutput -> {}\nZip    -> {}",
        version_str, version_dir.display(), zip_name,
    )
}

/// The UAT command line for a method.
///
/// Pulled out of `package_game` so the arguments each method produces can be
/// checked without running Unreal.
#[allow(clippy::too_many_arguments)]
fn uat_command(
    runuat: &Path,
    uproject: &Path,
    target: crate::types::BuildTarget,
    configuration: BuildConfiguration,
    method: crate::types::PackageMethod,
    compiled_already: bool,
    iterate: bool,
    extra_args: &[String],
    archive_dir: &Path,
) -> std::process::Command {
    use crate::types::PackageMethod;
    let mut uat = crate::ops::batch_cmd(runuat);
    uat.arg("BuildCookRun")
        .arg(format!("-project={}", uproject.display()))
        .args(["-noP4", "-unattended",
               &format!("-platform={}", target.uat_name()),
               &format!("-clientconfig={}", configuration.as_str()),
               &format!("-serverconfig={}", configuration.as_str())]);
    match method {
        // Everything in one run.
        PackageMethod::Full => { uat.args(["-cook", "-allmaps", "-build", "-stage", "-pak", "-archive"]); }
        // Already compiled above — or nothing to compile, in which case UAT's
        // own `-build` is harmless and keeps Blueprint-only projects working.
        PackageMethod::Stepwise => {
            if compiled_already { uat.arg("-skipbuild"); } else { uat.arg("-build"); }
            uat.args(["-cook", "-allmaps", "-stage", "-pak", "-archive"]);
        }
        PackageMethod::Restage => { uat.args(["-skipbuild", "-skipcook", "-stage", "-pak", "-archive"]); }
    }
    // `-iterate` makes the cooker reuse whatever is already in Saved/Cooked
    // and process only what changed. Off by default because a stale iterative
    // cook can mask a content problem that a clean cook would surface — so the
    // release build you actually ship should be a full one. It means nothing
    // to a restage, which does not cook.
    if iterate && method != PackageMethod::Restage { uat.arg("-iterate"); }
    uat.args(extra_args)
        .arg(format!("-archivedirectory={}", archive_dir.display()));
    uat
}

// ── Running one logged step ──────────────────────────────────────────────────

/// What every step needs to report into and be cancelled through.
struct StepEnv<'a> {
    status:   &'a Arc<Mutex<String>>,
    run:      &'a Arc<Mutex<crate::ops::run::RunProgress>>,
    progress: &'a Arc<Mutex<f32>>,
    cancel:   &'a AtomicBool,
    log_path: &'a Path,
}

/// Runs one process with its output going to the build log, tailing that log
/// into the run surface until it exits.
///
/// Every step of every method goes through here, so cancelling, failure
/// reporting and progress behave the same whether the step is UnrealBuildTool
/// or UAT. `Err` carries the finished `[CANCELLED]` / `[ERROR]` message.
///
/// * `append` — keep what earlier steps wrote instead of starting the log over.
/// * `stage`  — mark a stage as started, for steps that print no UAT banner.
/// * `by_stage_weights` — derive progress from the stages UAT announces;
///   otherwise it creeps towards the end of this step's `from..to` range.
#[allow(clippy::too_many_arguments)]
fn run_logged_step(
    env: &StepEnv,
    name: &str,
    mut command: std::process::Command,
    append: bool,
    from: f32,
    to: f32,
    stage: Option<crate::ops::run::Stage>,
    by_stage_weights: bool,
) -> Result<(), String> {
    let open = |append: bool| -> std::io::Result<fs::File> {
        if append {
            fs::OpenOptions::new().create(true).append(true).open(env.log_path)
        } else {
            fs::File::create(env.log_path)
        }
    };
    let out = open(append).map_err(|e| format!("[ERROR] Create log: {e}"))?;
    let err = out.try_clone().map_err(|e| format!("[ERROR] Clone log handle: {e}"))?;

    if let Some(s) = stage {
        env.run.lock().unwrap_or_else(|e| e.into_inner()).mark_stage(s);
    }
    let mut child = command.stdout(out).stderr(err).spawn()
        .map_err(|e| format!("[ERROR] {name} launch: {e}"))?;

    *env.progress.lock().unwrap_or_else(|e| e.into_inner()) = from;
    let exit = loop {
        if env.cancel.load(Ordering::Relaxed) {
            // `cmd /c RunUAT.bat` spawns AutomationTool, which in turn spawns
            // UnrealBuildTool and UnrealEditor-Cmd as separate child
            // processes. Killing just the cmd.exe leaves those running —
            // UnrealEditor-Cmd then keeps the project locked, so the *next*
            // build fails. Kill the whole process tree instead, and make sure
            // no Unreal Editor process is left holding the project open.
            kill_process_tree(child.id());
            let _ = child.wait();
            let close_note = close_editor_if_running(env.status)
                .err()
                .map(|e| format!("\n[WARNING] {e}"))
                .unwrap_or_default();
            return Err(format!(
                "[CANCELLED] {name} was cancelled.\nPartial log -> {}{}",
                env.log_path.display(), close_note,
            ));
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                // Read whatever the step appended since the last tick. This is
                // what makes the stage rail and the output view real rather
                // than timer-driven — see `ops::run`.
                let mut r = env.run.lock().unwrap_or_else(|e| e.into_inner());
                r.ingest(env.log_path);
                let mut prog = env.progress.lock().unwrap_or_else(|e| e.into_inner());
                *prog = if by_stage_weights && r.current.is_some() {
                    from + r.overall() * (to - from)
                } else {
                    // Blind creep towards the end of this step's share.
                    *prog + ((to - 0.02) - *prog) * 0.008
                };
                drop(prog);
                drop(r);
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(e) => return Err(format!("[ERROR] Waiting for {name}: {e}")),
        }
    };
    if !exit.success() {
        let close_note = close_editor_if_running(env.status)
            .err()
            .map(|e| format!("\n[WARNING] {e}"))
            .unwrap_or_default();
        env.run.lock().unwrap_or_else(|e| e.into_inner()).ingest(env.log_path);
        return Err(format!(
            "[ERROR] {name} failed (exit {}).{}{}",
            exit.code().unwrap_or(-1),
            uat_failure_details(env.log_path),
            close_note,
        ));
    }
    Ok(())
}

/// The (game, editor) target names of a C++ project, read from its
/// `Source/*.Target.cs` files.
///
/// They are not necessarily named after the project: a project renamed from a
/// template keeps its original targets (here `MobiusFish.uproject` builds
/// `UnrealProjectBase` and `UnrealProjectBaseEditor`), and UnrealBuildTool needs
/// the real names. A pair is a game target with a matching `<Name>Editor`; the
/// one named after the project wins when there are several. `None` means
/// Blueprint-only or no such pair, and the caller lets UAT compile instead.
fn find_code_targets(project_dir: &Path, stem: &str) -> Option<(String, String)> {
    let names: Vec<String> = fs::read_dir(project_dir.join("Source")).ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".Target.cs").map(str::to_string))
        .collect();
    let mut pairs: Vec<(String, String)> = names.iter()
        .filter(|n| !n.ends_with("Editor"))
        .filter(|g| names.iter().any(|e| *e == format!("{g}Editor")))
        .map(|g| (g.clone(), format!("{g}Editor")))
        .collect();
    pairs.sort();
    pairs.iter().position(|(g, _)| g == stem).map(|i| pairs.swap_remove(i))
        .or_else(|| pairs.into_iter().next())
}

/// Whether `Saved/Cooked` holds a non-empty cook for this platform.
fn has_cook_for(project_dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = fs::read_dir(project_dir.join("Saved").join("Cooked")) else { return false };
    entries.flatten().any(|e| {
        let p = e.path();
        p.is_dir()
            && p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.len() >= prefix.len() && n[..prefix.len()].eq_ignore_ascii_case(prefix)
            })
            && fs::read_dir(&p).is_ok_and(|mut d| d.next().is_some())
    })
}

/// Validates user-supplied UAT arguments and splits them into tokens.
///
/// They reach `RunUAT.bat`, which is run through `cmd`, so anything cmd would
/// interpret (`& | < > ^ %` and quotes) is refused outright. The options this
/// app already controls are refused too: silently letting a typed
/// `-platform=` win over the chip on screen would build the wrong thing.
pub fn parse_extra_uat_args(text: &str) -> Result<Vec<String>, String> {
    const OWNED: [&str; 6] = ["-project", "-platform", "-archivedirectory",
                              "-clientconfig", "-serverconfig", "-iterate"];
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        if !tok.starts_with('-') {
            return Err(format!("\"{tok}\" is not an option. Each argument must start with a dash, like -nocompile."));
        }
        if let Some(bad) = tok.chars().find(|c|
            !(c.is_ascii_alphanumeric() || r"-_=.,:/\+".contains(*c)))
        {
            return Err(format!("\"{tok}\" contains '{bad}', which is not allowed."));
        }
        let name = tok.split('=').next().unwrap_or(tok).to_ascii_lowercase();
        if OWNED.contains(&name.as_str()) {
            return Err(format!("{name} is set by the build controls above — change it there."));
        }
        out.push(tok.to_string());
    }
    Ok(out)
}

// ── Zip ──────────────────────────────────────────────────────────────────────

enum ZipFailure {
    Cancelled,
    Io(String),
}

/// Writes the contents of `src` (not the folder itself) into a zip at `dst`.
///
/// Entries use forward slashes and are stored relative to `src`. Large-file
/// (zip64) support is always on because a packaged game routinely holds files
/// over 4 GB. `on_progress` receives 0.0–1.0 by bytes read.
fn zip_directory(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f32),
) -> Result<(), ZipFailure> {
    use std::io::{Read, Write};

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() { walk(&path, out)?; } else { out.push(path); }
        }
        Ok(())
    }
    let io = |e: std::io::Error| ZipFailure::Io(e.to_string());

    let mut files = Vec::new();
    walk(src, &mut files).map_err(io)?;
    let total: u64 = files.iter()
        .filter_map(|f| fs::metadata(f).ok())
        .map(|m| m.len())
        .sum::<u64>()
        .max(1);

    let mut writer = zip::ZipWriter::new(fs::File::create(dst).map_err(io)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(3))
        .large_file(true);

    let mut done = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    for file in &files {
        let rel = file.strip_prefix(src).unwrap_or(file);
        let name = rel.to_string_lossy().replace('\\', "/");
        writer.start_file(name, options).map_err(|e| ZipFailure::Io(e.to_string()))?;
        let mut input = fs::File::open(file).map_err(io)?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ZipFailure::Cancelled);
            }
            let n = input.read(&mut buf).map_err(io)?;
            if n == 0 { break; }
            writer.write_all(&buf[..n]).map_err(io)?;
            done += n as u64;
            on_progress(done as f32 / total as f32);
        }
    }
    writer.finish().map_err(|e| ZipFailure::Io(e.to_string()))?;
    Ok(())
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

/// Path to the rclone binary to drive, or `None` when it is not installed.
///
/// rclone used to be embedded in this binary and silently extracted on first
/// use; it is now resolved (and, on request, downloaded and checksum-verified)
/// by [`crate::ops::rclone`] — see that module for why the embed had to go.
fn rclone_program() -> Option<String> {
    crate::ops::rclone::resolve().map(|p| p.to_string_lossy().to_string())
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
    let Some(program) = rclone_program() else { return false };
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
    let Some(program) = rclone_program() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "rclone is not installed yet",
        ));
    };
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

    let Some(program) = rclone_program() else {
        return "[ERROR] rclone is not installed.
                Open the Package tab's Google Drive section and click \"Install rclone\" (downloads the official build from rclone.org and verifies its checksum)."
            .to_string();
    };
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
    const EDITORS: &[&str] = &[
        "UnrealEditor.exe",
        "UE4Editor.exe",
        "UnrealEditor-Cmd.exe",
        "UE4Editor-Cmd.exe",
    ];
    EDITORS.iter().any(|e| is_process_running(e))
}

fn close_editor_if_running(status: &Arc<Mutex<String>>) -> Result<(), String> {
    const EDITORS: &[&str] = &[
        "UnrealEditor.exe",
        "UE4Editor.exe",
        "UnrealEditor-Cmd.exe",
        "UE4Editor-Cmd.exe",
    ];
    for editor_exe in EDITORS {
        if !is_process_running(editor_exe) { continue; }

        *status.lock().unwrap() = format!(
            "[PRE-FLIGHT] {} is open — closing it before packaging…",
            editor_exe
        );

        // Graceful close first (sends WM_CLOSE)
        crate::ops::cmd("taskkill")
            .args(["/im", editor_exe])
            .status()
            .map_err(|e| format!("Could not request {} to close: {}", editor_exe, e))?;

        // Wait up to 30 s for graceful exit (poll every 500 ms)
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(500));
            if !is_process_running(editor_exe) { break; }
        }

        // Force-kill if it still hasn't exited
        if is_process_running(editor_exe) {
            crate::ops::cmd("taskkill")
                .args(["/f", "/t", "/im", editor_exe])
                .status()
                .map_err(|e| format!("Could not force-close {}: {}", editor_exe, e))?;
            std::thread::sleep(Duration::from_secs(2));
        }

        if is_process_running(editor_exe) {
            return Err(format!(
                "Could not close {}. Save your work, close Unreal, and try again.",
                editor_exe,
            ));
        }
    }

    if is_editor_running() {
        return Err("Unreal is still running. Close it before packaging.".to_string());
    }
    Ok(())
}

fn uat_failure_details(log_path: &Path) -> String {
    let diagnoses = crate::ops::diagnostics::scan_build_log(log_path);
    let mut details = String::new();
    if !diagnoses.is_empty() {
        details.push_str("\nDetected problems:\n");
        for diagnosis in diagnoses {
            details.push_str("- ");
            details.push_str(&diagnosis.matched);
            details.push('\n');
        }
    }

    details.push_str(&format!("\nLog -> {}\nLast UAT output:\n", log_path.display()));
    match fs::read_to_string(log_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().filter(|line| !line.trim().is_empty()).collect();
            if lines.is_empty() {
                details.push_str("(log is empty)\n");
            } else {
                let start = lines.len().saturating_sub(25);
                for line in &lines[start..] {
                    let trimmed = line.trim();
                    let shortened: String = trimmed.chars().take(240).collect();
                    details.push_str(&shortened);
                    if trimmed.chars().count() > 240 {
                        details.push('…');
                    }
                    details.push('\n');
                }
            }
        }
        Err(e) => details.push_str(&format!("(could not read log: {e})\n")),
    }
    details
}

/// Kills `pid` and its entire descendant process tree (e.g. `cmd.exe` ->
/// AutomationTool -> UnrealBuildTool / UnrealEditor-Cmd). Plain `Child::kill`
/// only kills the immediate process and leaves such descendants running.
pub(crate) fn kill_process_tree(pid: u32) {
    let _ = crate::ops::cmd("taskkill")
        .args(["/f", "/t", "/pid", &pid.to_string()])
        .output();
}

pub(crate) fn is_process_running(exe_name: &str) -> bool {
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
/// The folder UAT archived into, matched by prefix.
///
/// An exact name is not knowable ahead of time: Win64 archives to `Windows`,
/// and Android appends the texture format (`Android_ASTC` and friends). An
/// exact match is preferred when one exists so a project literally named
/// `Android` cannot shadow the real output.
fn find_output_dir(version_dir: &Path, prefix: &str) -> Option<PathBuf> {
    let exact = version_dir.join(prefix);
    if exact.is_dir() {
        return Some(exact);
    }
    let mut best: Option<PathBuf> = None;
    for e in fs::read_dir(version_dir).ok()?.flatten() {
        let p = e.path();
        if !p.is_dir() { continue; }
        let name = p.file_name()?.to_string_lossy().to_string();
        if name.len() > prefix.len()
            && name[..prefix.len()].eq_ignore_ascii_case(prefix)
            && name.as_bytes()[prefix.len()] == b'_'
        {
            best = Some(p);
            break;
        }
    }
    best
}

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
    let mut candidates: Vec<PathBuf> = fs::read_dir(dir).ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
        .filter(|e| {
            let stem = e.path().file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            !SKIP.contains(&stem.as_str())
        })
        .map(|e| e.path())
        .collect();
    candidates.sort_unstable();
    candidates.into_iter().next()
}

/// Validates a user-controlled Windows file or directory name.
///
/// Package and executable names become path components later in the pipeline,
/// so separators, reserved device names, and trailing dots/spaces must be
/// rejected before any UAT work starts.
pub fn validate_leaf_name(value: &str, label: &str) -> Result<(), String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(format!("{label} cannot be empty."));
    }
    if name != value {
        return Err(format!("{label} cannot start or end with whitespace."));
    }
    if name == "." || name == ".." {
        return Err(format!("{label} cannot be '.' or '..'."));
    }
    if name.chars().any(|c| {
        c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return Err(format!("{label} contains a character Windows cannot use."));
    }
    if name.ends_with([' ', '.']) {
        return Err(format!("{label} cannot end with a space or period."));
    }

    let stem = name.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        return Err(format!("{label} uses a reserved Windows device name."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_windows_leaf_names() {
        assert!(validate_leaf_name("MyGame", "Name").is_ok());
        assert!(validate_leaf_name("My Game 2", "Name").is_ok());
        for invalid in ["", "..", "bad/name", "CON.txt", "game.", "game ", "LPT1"] {
            assert!(validate_leaf_name(invalid, "Name").is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn finds_next_version_after_existing_versions() {
        let root = std::env::temp_dir().join(format!(
            "unreal-devtool-version-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("v0.0.1")).unwrap();
        fs::create_dir_all(root.join("v0.1.0")).unwrap();
        fs::create_dir_all(root.join("not-a-version")).unwrap();

        assert_eq!(find_next_version(&root), 101);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failure_details_explain_the_error_and_show_log_tail() {
        let path = std::env::temp_dir().join(format!(
            "unreal-devtool-failure-log-{}.txt",
            std::process::id()
        ));
        fs::write(
            &path,
            "header\n'C:\\Program' is not recognized as an internal or external command\nfinal UAT error\n",
        )
        .unwrap();

        let details = uat_failure_details(&path);
        assert!(details.contains("Detected problems:"));
        assert!(details.contains("final UAT error"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn zip_directory_round_trips_nested_files_with_forward_slashes() {
        let root = std::env::temp_dir().join(format!("unreal-devtool-zip-{}", std::process::id()));
        let src = root.join("src");
        fs::create_dir_all(src.join("Content/Paks")).unwrap();
        fs::write(src.join("Game.exe"), b"exe").unwrap();
        fs::write(src.join("Content/Paks/a.pak"), vec![7u8; 3_000_000]).unwrap();
        let dst = root.join("out.zip");

        let cancel = AtomicBool::new(false);
        let mut last = 0.0;
        zip_directory(&src, &dst, &cancel, |f| last = f).ok().expect("zip");
        assert!((last - 1.0).abs() < 1e-3, "progress reaches 1.0");

        let mut archive = zip::ZipArchive::new(fs::File::open(&dst).unwrap()).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string()).collect();
        names.sort();
        assert_eq!(names, ["Content/Paks/a.pak", "Game.exe"]);

        // A cancelled run reports it rather than leaving a half-written success.
        let cancelled = AtomicBool::new(true);
        assert!(matches!(zip_directory(&src, &dst, &cancelled, |_| {}), Err(ZipFailure::Cancelled)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extra_uat_args_are_validated() {
        assert_eq!(parse_extra_uat_args("-nocompile  -ddc=Shared").unwrap(), ["-nocompile", "-ddc=Shared"]);
        assert!(parse_extra_uat_args("").unwrap().is_empty());
        assert!(parse_extra_uat_args("nocompile").is_err(), "needs a dash");
        assert!(parse_extra_uat_args("-x&calc").is_err(), "cmd metacharacter");
        assert!(parse_extra_uat_args("-x%PATH%").is_err());
        assert!(parse_extra_uat_args("-Platform=Android").is_err(), "owned option, any case");
    }

    #[test]
    fn code_targets_and_cooks_are_detected_from_the_project_layout() {
        let root = std::env::temp_dir().join(format!("udt-methods-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Source")).unwrap();
        assert_eq!(find_code_targets(&root, "Game"), None, "no target files yet");
        fs::write(root.join("Source/Base.Target.cs"), "").unwrap();
        assert_eq!(find_code_targets(&root, "Game"), None, "editor target missing");
        fs::write(root.join("Source/BaseEditor.Target.cs"), "").unwrap();
        // Targets keep a template's name after the project is renamed.
        assert_eq!(find_code_targets(&root, "Game"), Some(("Base".into(), "BaseEditor".into())));
        fs::write(root.join("Source/Game.Target.cs"), "").unwrap();
        fs::write(root.join("Source/GameEditor.Target.cs"), "").unwrap();
        assert_eq!(find_code_targets(&root, "Game"), Some(("Game".into(), "GameEditor".into())),
            "the one named after the project wins");

        assert!(!has_cook_for(&root, "Windows"));
        fs::create_dir_all(root.join("Saved/Cooked/Windows")).unwrap();
        assert!(!has_cook_for(&root, "Windows"), "an empty cook folder is not a cook");
        fs::write(root.join("Saved/Cooked/Windows/a.uasset"), "x").unwrap();
        assert!(has_cook_for(&root, "Windows"));
        assert!(has_cook_for(&root, "windows"), "case-insensitive");
        assert!(!has_cook_for(&root, "Android"));
        fs::remove_dir_all(root).unwrap();
    }

    fn args_of(c: &std::process::Command) -> Vec<String> {
        c.get_args().map(|a| a.to_string_lossy().to_string()).collect()
    }

    fn uat_args(method: crate::types::PackageMethod, compiled: bool, iterate: bool) -> Vec<String> {
        args_of(&uat_command(
            Path::new(r"C:\E\RunUAT.bat"), Path::new(r"C:\P\G.uproject"),
            crate::types::BuildTarget::Android, BuildConfiguration::Shipping,
            method, compiled, iterate, &["-compressed".to_string()], Path::new(r"C:\P\build\v1"),
        ))
    }

    #[test]
    fn each_method_builds_its_own_uat_command() {
        use crate::types::PackageMethod::*;
        let has = |a: &[String], f: &str| a.iter().any(|x| x == f);

        let full = uat_args(Full, false, true);
        assert!(has(&full, "-build") && has(&full, "-cook") && has(&full, "-iterate"));
        assert!(has(&full, "-platform=Android") && has(&full, "-clientconfig=Shipping"));
        assert!(!has(&full, "-skipbuild"));

        let stepwise = uat_args(Stepwise, true, false);
        assert!(has(&stepwise, "-skipbuild") && has(&stepwise, "-cook"));
        assert!(!has(&stepwise, "-build"));
        // No C++ to compile first: UAT does it.
        let bp_only = uat_args(Stepwise, false, false);
        assert!(has(&bp_only, "-build") && !has(&bp_only, "-skipbuild"));

        let restage = uat_args(Restage, false, true);
        assert!(has(&restage, "-skipcook") && has(&restage, "-skipbuild") && has(&restage, "-stage"));
        assert!(!has(&restage, "-cook") && !has(&restage, "-build"));
        assert!(!has(&restage, "-iterate"), "iterate is meaningless without a cook");

        // Extras and the archive directory are always last-but-one/last.
        for a in [&full, &stepwise, &restage] {
            assert!(has(a, "-compressed"));
            assert!(a.last().unwrap().starts_with("-archivedirectory="));
        }
    }

    /// Runs a real (fake) batch file through the same step runner UAT uses:
    /// output goes to the log, the log is tailed into the run state, and a
    /// stage banner is recognised.
    #[test]
    fn a_step_runs_logs_and_reports_stages() {
        let root = std::env::temp_dir().join(format!("udt-step-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let bat = root.join("Fake.bat");
        fs::write(&bat, "@echo off\r\necho ********** COOK COMMAND STARTED **********\r\necho LogCook: Error: fake problem\r\nexit /b 0\r\n").unwrap();
        let log = root.join("BuildLog.txt");

        let status = Arc::new(Mutex::new(String::new()));
        let run = Arc::new(Mutex::new(crate::ops::run::RunProgress::default()));
        let progress = Arc::new(Mutex::new(0.0f32));
        let cancel = AtomicBool::new(false);
        let env = StepEnv { status: &status, run: &run, progress: &progress, cancel: &cancel, log_path: &log };

        let mut cmd = crate::ops::batch_cmd(&bat);
        cmd.arg("x");
        run_logged_step(&env, "Fake", cmd, false, 0.1, 0.5, Some(crate::ops::run::Stage::Compile), true)
            .expect("a zero exit is success");
        run.lock().unwrap().ingest(&log);
        let r = run.lock().unwrap();
        assert!(r.started[crate::ops::run::Stage::Compile.index()].is_some(), "marked stage recorded");
        assert!(r.started[crate::ops::run::Stage::Cook.index()].is_some(), "banner recognised");
        assert_eq!(r.errors, 1);
        assert_eq!(r.error_samples.len(), 1);
        drop(r);

        // A second step appends rather than truncating.
        let mut again = crate::ops::batch_cmd(&bat);
        again.arg("y");
        run_logged_step(&env, "Fake 2", again, true, 0.5, 0.8, None, false).unwrap();
        let text = fs::read_to_string(&log).unwrap();
        assert_eq!(text.matches("COOK COMMAND STARTED").count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    /// A real packaging run, for checking a method against an actual project.
    /// Ignored by default (it takes tens of minutes and writes into the
    /// project): `UDT_REAL_PROJECT`, `UDT_REAL_ENGINE`, `UDT_REAL_VERSION` and
    /// optionally `UDT_REAL_METHOD` (full | stepwise | restage).
    #[test]
    #[ignore]
    fn real_project_build() {
        let project = PathBuf::from(std::env::var("UDT_REAL_PROJECT").expect("UDT_REAL_PROJECT"));
        let engine = PathBuf::from(std::env::var("UDT_REAL_ENGINE").expect("UDT_REAL_ENGINE"));
        let version = std::env::var("UDT_REAL_VERSION").expect("UDT_REAL_VERSION");
        let method = crate::types::PackageMethod::from_key(
            &std::env::var("UDT_REAL_METHOD").unwrap_or_else(|_| "full".into())).expect("method");
        let name = project.file_stem().unwrap().to_string_lossy().to_string();

        let status = Arc::new(Mutex::new(String::new()));
        let run = Arc::new(Mutex::new(crate::ops::run::RunProgress::default()));
        let result = package_game(
            project, engine, name.clone(), name, version,
            BuildConfiguration::Development, crate::types::BuildTarget::Win64,
            status, Arc::new(Mutex::new(None)), Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(0.0)), run.clone(), false, false, Vec::new(), method,
        );
        let r = run.lock().unwrap();
        eprintln!("RESULT: {result}");
        eprintln!("stages: {:?}", r.elapsed);
        eprintln!("warnings {} errors {}", r.warnings, r.errors);
        assert!(result.starts_with("[DONE]"), "{result}");
    }
}
