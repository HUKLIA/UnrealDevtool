//! Unreal development tools that run outside a package build: editor
//! commandlets (compile every Blueprint, validate data, fill the derived-data
//! cache…) and launching the editor or a game with common flags.
//!
//! Commandlets are run the way Unreal's own docs and CI setups run them —
//! `UnrealEditor-Cmd.exe <project> -run=<Name> …` — with output going to a log
//! file that is tailed into the UI, exactly like a packaging run. Nothing here
//! talks to a running editor; a commandlet needs the editor *closed*.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ops::run::RunProgress;

/// One ready-made commandlet.
pub struct Commandlet {
    pub label:   &'static str,
    pub blurb:   &'static str,
    /// Everything after the project path.
    pub args:    &'static [&'static str],
    /// Rewrites asset files on disk. Shown with a warning: commit first.
    pub modifies: bool,
}

pub const COMMANDLETS: &[Commandlet] = &[
    Commandlet {
        label: "Compile all Blueprints",
        blurb: "Recompiles every Blueprint and reports the ones that fail. Run it after changing C++ that Blueprints depend on.",
        args: &["-run=CompileAllBlueprints"],
        modifies: false,
    },
    Commandlet {
        label: "Validate data",
        blurb: "Runs the Data Validation rules over the project's assets — the same check the editor's Validate Assets command does.",
        args: &["-run=DataValidation"],
        modifies: false,
    },
    Commandlet {
        label: "Fill derived-data cache",
        blurb: "Builds the derived data (shaders, textures, meshes) for every asset now, so the first open in the editor is fast. Slow, but changes no project files.",
        args: &["-run=DerivedDataCache", "-fill"],
        modifies: false,
    },
    Commandlet {
        label: "Fix up redirectors",
        blurb: "Resaves assets so they point at where things really live, then the redirector stubs can be deleted.",
        args: &["-run=ResavePackages", "-FixupRedirects"],
        modifies: true,
    },
    Commandlet {
        label: "Resave all packages",
        blurb: "Loads and resaves every asset. Used after an engine upgrade to bring everything to the current format.",
        args: &["-run=ResavePackages"],
        modifies: true,
    },
];

/// What is running, or ran last.
pub struct ToolRun {
    pub label:    String,
    pub started:  Instant,
    /// `Some(exit code)` once the process has ended (`None` code: killed).
    pub finished: Option<Option<i32>>,
    pub ended_at: Option<Instant>,
    pub cancelled: bool,
    pub log_path: PathBuf,
    pub progress: RunProgress,
    cancel:       Arc<AtomicBool>,
}

impl ToolRun {
    pub fn running(&self) -> bool { self.finished.is_none() }
    pub fn request_cancel(&self) { self.cancel.store(true, Ordering::Relaxed); }
    pub fn elapsed(&self) -> Duration {
        self.ended_at.unwrap_or_else(Instant::now).duration_since(self.started)
    }
}

impl ToolRun {
    /// A run that already finished, rebuilt from its log file. Lets the output
    /// view be shown (and tested) without starting the editor.
    #[cfg(debug_assertions)]
    pub fn finished_from_log(label: &str, log_path: PathBuf, exit: i32) -> Self {
        let mut progress = RunProgress::default();
        progress.ingest(&log_path);
        let now = Instant::now();
        Self {
            label: label.to_string(),
            started: now - Duration::from_secs(99),
            finished: Some(Some(exit)),
            ended_at: Some(now),
            cancelled: false,
            log_path,
            progress,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// The editor's command-line executable for an engine folder.
pub fn editor_cmd_exe(engine: &Path) -> PathBuf {
    engine.join("Engine").join("Binaries").join("Win64").join("UnrealEditor-Cmd.exe")
}

/// The full argument list for a commandlet run.
pub fn commandlet_args(project: &Path, args: &[String]) -> Vec<String> {
    let mut v = vec![project.display().to_string()];
    v.extend(args.iter().cloned());
    // Unattended: never wait on a dialog nobody can see. `-stdout` and
    // `-FullStdOutLogOutput` make the log lines appear in the redirected file.
    v.extend(["-unattended", "-nopause", "-NoSplash", "-stdout", "-FullStdOutLogOutput"]
        .map(String::from));
    v
}

/// Starts a commandlet in the background and returns the shared run state.
///
/// Refuses if the editor is open (it holds the project's files), and never
/// starts a second run while one is still going.
pub fn start_commandlet(
    engine: &Path,
    project: &Path,
    label: &str,
    args: Vec<String>,
    ctx: eframe::egui::Context,
) -> Result<Arc<Mutex<ToolRun>>, String> {
    let exe = editor_cmd_exe(engine);
    if !exe.is_file() {
        return Err(format!("UnrealEditor-Cmd.exe not found at {}", exe.display()));
    }
    for editor in ["UnrealEditor.exe", "UE4Editor.exe"] {
        if crate::ops::package::is_process_running(editor) {
            return Err("Close Unreal Editor first — a commandlet needs the project's files to itself.".into());
        }
    }

    let logs = project.parent().map(|d| d.join("Saved").join("Logs")).unwrap_or_default();
    let _ = std::fs::create_dir_all(&logs);
    let slug: String = label.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let log_path = logs.join(format!("DevTool_{slug}.log"));
    let out = std::fs::File::create(&log_path).map_err(|e| format!("Could not create the log: {e}"))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;

    let mut child = crate::ops::cmd(&exe.to_string_lossy())
        .args(commandlet_args(project, &args))
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Could not start the editor: {e}"))?;

    let cancel = Arc::new(AtomicBool::new(false));
    let run = Arc::new(Mutex::new(ToolRun {
        label: label.to_string(),
        started: Instant::now(),
        finished: None,
        ended_at: None,
        cancelled: false,
        log_path: log_path.clone(),
        progress: RunProgress::default(),
        cancel: cancel.clone(),
    }));

    let shared = run.clone();
    std::thread::spawn(move || {
        let code = loop {
            if cancel.load(Ordering::Relaxed) {
                crate::ops::package::kill_process_tree(child.id());
                let _ = child.wait();
                shared.lock().unwrap_or_else(|e| e.into_inner()).cancelled = true;
                break None;
            }
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {
                    {
                        let mut r = shared.lock().unwrap_or_else(|e| e.into_inner());
                        let path = r.log_path.clone();
                        r.progress.ingest(&path);
                    }
                    ctx.request_repaint();
                    std::thread::sleep(Duration::from_millis(400));
                }
                Err(_) => break None,
            }
        };
        let mut r = shared.lock().unwrap_or_else(|e| e.into_inner());
        let path = r.log_path.clone();
        r.progress.ingest(&path);
        r.finished = Some(code);
        r.ended_at = Some(Instant::now());
        drop(r);
        ctx.request_repaint();
    });
    Ok(run)
}

// ── Launching ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// The full editor.
    Editor,
    /// The game inside the editor's executable (`-game`), without packaging.
    Standalone,
    /// A packaged build.
    Packaged,
}

#[derive(Clone)]
pub struct LaunchOpts {
    pub log_window: bool,
    pub windowed:   bool,
    pub res:        Option<(u32, u32)>,
    pub no_sound:   bool,
    /// Already validated dash arguments.
    pub extra:      Vec<String>,
}

impl Default for LaunchOpts {
    fn default() -> Self {
        Self { log_window: true, windowed: true, res: None, no_sound: false, extra: Vec::new() }
    }
}

/// Command-line arguments for a launch. The project path leads for the editor
/// and standalone modes; a packaged build is its own game and takes none.
pub fn launch_args(mode: LaunchMode, project: &Path, o: &LaunchOpts) -> Vec<String> {
    let mut v = Vec::new();
    if mode != LaunchMode::Packaged {
        v.push(project.display().to_string());
    }
    if mode == LaunchMode::Standalone {
        v.push("-game".into());
    }
    if o.log_window { v.push("-log".into()); }
    // Window options only mean something to a game.
    if mode != LaunchMode::Editor {
        if o.windowed { v.push("-windowed".into()); }
        if let Some((w, h)) = o.res {
            v.push(format!("-ResX={w}"));
            v.push(format!("-ResY={h}"));
        }
    }
    if o.no_sound { v.push("-nosound".into()); }
    v.extend(o.extra.iter().cloned());
    v
}

/// Starts a GUI program with its own working folder. Not hidden: this is the
/// editor or a game, and the person asked to see it.
pub fn launch(exe: &Path, args: &[String]) -> std::io::Result<()> {
    std::process::Command::new(exe)
        .args(args)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commandlet_arguments_lead_with_the_project_and_run_unattended() {
        let a = commandlet_args(Path::new(r"C:\P\G.uproject"), &["-run=DataValidation".to_string()]);
        assert_eq!(a[0], r"C:\P\G.uproject");
        assert_eq!(a[1], "-run=DataValidation");
        assert!(a.contains(&"-unattended".to_string()) && a.contains(&"-stdout".to_string()));
    }

    #[test]
    fn every_ready_made_commandlet_names_a_run() {
        for c in COMMANDLETS {
            assert!(c.args.iter().any(|a| a.starts_with("-run=")), "{}", c.label);
        }
        // The ones that rewrite assets are the ones that carry the warning.
        assert!(COMMANDLETS.iter().filter(|c| c.modifies).all(|c| c.args[0] == "-run=ResavePackages"));
    }

    #[test]
    fn launch_arguments_depend_on_what_is_launched() {
        let p = Path::new(r"C:\P\G.uproject");
        let o = LaunchOpts { res: Some((1280, 720)), no_sound: true, extra: vec!["-ExecCmds=stat fps".into()],
            ..LaunchOpts::default() };

        let editor = launch_args(LaunchMode::Editor, p, &o);
        assert_eq!(editor[0], r"C:\P\G.uproject");
        assert!(!editor.contains(&"-game".to_string()));
        assert!(!editor.iter().any(|a| a.starts_with("-ResX")), "resolution is for games");
        assert!(editor.contains(&"-log".to_string()) && editor.contains(&"-nosound".to_string()));

        let standalone = launch_args(LaunchMode::Standalone, p, &o);
        assert_eq!(&standalone[..2], [r"C:\P\G.uproject", "-game"]);
        assert!(standalone.contains(&"-ResX=1280".to_string()) && standalone.contains(&"-ResY=720".to_string()));
        assert!(standalone.contains(&"-windowed".to_string()));

        let packaged = launch_args(LaunchMode::Packaged, p, &o);
        assert_eq!(packaged[0], "-log", "a packaged build takes no project path");
        assert_eq!(packaged.last().unwrap(), "-ExecCmds=stat fps");
    }

    /// Runs the first ready-made commandlet named by `UDT_REAL_COMMANDLET` (a
    /// label from `COMMANDLETS`) against a real project. Ignored by default.
    #[test]
    #[ignore]
    fn real_commandlet() {
        let project = PathBuf::from(std::env::var("UDT_REAL_PROJECT").expect("UDT_REAL_PROJECT"));
        let engine = PathBuf::from(std::env::var("UDT_REAL_ENGINE").expect("UDT_REAL_ENGINE"));
        let label = std::env::var("UDT_REAL_COMMANDLET").expect("UDT_REAL_COMMANDLET");
        let c = COMMANDLETS.iter().find(|c| c.label == label).expect("a known commandlet");
        let run = start_commandlet(&engine, &project, c.label,
            c.args.iter().map(|s| s.to_string()).collect(), eframe::egui::Context::default())
            .expect("start");
        while run.lock().unwrap().running() { std::thread::sleep(Duration::from_secs(2)); }
        let r = run.lock().unwrap();
        eprintln!("FINISHED {:?} after {:?}; lines {} warnings {} errors {}",
            r.finished, r.elapsed(), r.progress.lines.len(), r.progress.warnings, r.progress.errors);
        for l in r.progress.lines.iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
            eprintln!("  {}", l.text);
        }
        assert!(matches!(r.finished, Some(Some(_))), "ran to an exit code");
    }
}
