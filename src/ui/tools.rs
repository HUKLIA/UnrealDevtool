//! The Unreal tools sheet: run editor commandlets, launch the editor or a game
//! with common flags, see where a project's (or a build's) bytes are, and turn
//! plugins on and off.
//!
//! These are the things Unreal developers otherwise do from a terminal or by
//! opening the editor just to click one menu. Each tab is one job.

use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::app::DevToolApp;
use crate::ops::insights::{self, Insights};
use crate::ops::plugins::{self, PluginEntry};
use crate::ops::run::Level;
use crate::ops::tools::{self, LaunchMode, LaunchOpts, ToolRun, COMMANDLETS};
use crate::theme::*;
use crate::ui::run::divider;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolsTab {
    #[default]
    Commandlets,
    Launch,
    Content,
    Plugins,
    Engines,
    Cheatsheet,
}

/// Resolution choices for launching a game.
const RESOLUTIONS: [(&str, Option<(u32, u32)>); 4] = [
    ("Default", None),
    ("1280×720", Some((1280, 720))),
    ("1920×1080", Some((1920, 1080))),
    ("2560×1440", Some((2560, 1440))),
];

/// A device list on its way from the background thread: the devices, or why not.
pub type AdbPending = Arc<Mutex<Option<Result<Vec<crate::ops::adb::Device>, String>>>>;

/// Everything the sheet remembers between frames.
pub struct ToolsState {
    pub tab: ToolsTab,

    // Commandlets
    pub run: Option<Arc<Mutex<ToolRun>>>,
    /// Index of a rewriting commandlet waiting for its second click.
    pub confirm: Option<usize>,
    pub custom_name: String,
    pub custom_args: String,
    pub notice: Option<String>,

    // Launch
    pub log_window: bool,
    pub windowed: bool,
    pub no_sound: bool,
    pub res: usize,
    pub extra: String,

    // Content
    pub from_build: bool,
    pub scanning: bool,
    pub scan_pending: Arc<Mutex<Option<Insights>>>,
    pub insights: Option<Insights>,
    pub compare_pending: Arc<Mutex<Option<(String, String, insights::Comparison)>>>,
    pub comparison: Option<(String, String, insights::Comparison)>,
    pub comparing: bool,

    // Plugins
    pub plugins: Option<Result<Vec<PluginEntry>, String>>,
    pub plugin_filter: String,
    /// Whether the editor was running at the last look, and when that was.
    /// Asking costs a process spawn, so it is not done every frame.
    pub editor_seen: Option<(std::time::Instant, bool)>,

    // Engines
    pub engines: Option<Vec<crate::ops::engines::EngineInstall>>,
    /// Engine (by index) waiting for a second click to become the project's.
    pub engine_confirm: Option<usize>,

    // Cheatsheet
    pub cheat_filter: String,

    // Android install (shown on the result screen)
    /// `None` until the first look; then the devices adb reported.
    pub adb_devices: Option<Vec<crate::ops::adb::Device>>,
    pub adb_pending: AdbPending,
    pub adb_result: Arc<Mutex<Option<String>>>,
    pub adb_note: Option<String>,
    pub adb_busy: bool,
    /// Where adb and the APK were found, looked up once per result rather than
    /// walking the build folder every frame: (build folder, adb, apk).
    pub adb_lookup: Option<(std::path::PathBuf, Option<std::path::PathBuf>, Option<std::path::PathBuf>)>,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            tab: ToolsTab::Commandlets,
            run: None, confirm: None,
            custom_name: String::new(), custom_args: String::new(), notice: None,
            log_window: true, windowed: true, no_sound: false, res: 0, extra: String::new(),
            from_build: false, scanning: false,
            scan_pending: Arc::new(Mutex::new(None)), insights: None,
            compare_pending: Arc::new(Mutex::new(None)), comparison: None, comparing: false,
            plugins: None, plugin_filter: String::new(), editor_seen: None,
            engines: None, engine_confirm: None,
            cheat_filter: String::new(),
            adb_devices: None, adb_pending: Arc::new(Mutex::new(None)),
            adb_result: Arc::new(Mutex::new(None)), adb_note: None, adb_busy: false, adb_lookup: None,
        }
    }
}

impl DevToolApp {
    pub fn show_tools_sheet(&mut self, ui: &mut egui::Ui) {
        if self.project_path.is_none() {
            ui.label(hint("Open a project first — these tools work on one Unreal project."));
            return;
        }
        // Results from the background scan land here.
        if let Some(found) = self.tools.scan_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.tools.insights = Some(found);
            self.tools.scanning = false;
        }

        if let Some(found) = self.tools.compare_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.tools.comparison = Some(found);
            self.tools.comparing = false;
        }

        ui.horizontal_wrapped(|ui| {
            for (t, label) in [
                (ToolsTab::Commandlets, "Commandlets"),
                (ToolsTab::Launch, "Launch"),
                (ToolsTab::Content, "Size & content"),
                (ToolsTab::Plugins, "Plugins"),
                (ToolsTab::Engines, "Engines"),
                (ToolsTab::Cheatsheet, "Cheatsheet"),
            ] {
                if ui.add(chip(label, self.tools.tab == t)).clicked() && self.tools.tab != t {
                    self.tools.tab = t;
                    if t == ToolsTab::Plugins { self.reload_plugins(); }
                    if t == ToolsTab::Engines { self.tools.engines = None; self.tools.engine_confirm = None; }
                }
            }
        });
        ui.add_space(12.0);

        let tab = self.tools.tab;
        egui::ScrollArea::vertical().id_salt("tools_scroll").auto_shrink([false, false]).show(ui, |ui| {
            match tab {
                ToolsTab::Commandlets => self.show_commandlets(ui),
                ToolsTab::Launch      => self.show_launch(ui),
                ToolsTab::Content     => self.show_content_insights(ui),
                ToolsTab::Plugins     => self.show_plugins(ui),
                ToolsTab::Engines     => self.show_engines(ui),
                ToolsTab::Cheatsheet  => self.show_cheatsheet(ui),
            }
            ui.add_space(GUTTER);
        });
    }

    // ── Commandlets ─────────────────────────────────────────────────────────

    fn tool_running(&self) -> bool {
        self.tools.run.as_ref().is_some_and(|r| r.lock().unwrap_or_else(|e| e.into_inner()).running())
    }

    fn start_tool(&mut self, ctx: &egui::Context, label: &str, args: Vec<String>) {
        let (Some(engine), Some(project)) = (self.engine_dir.clone(), self.project_path.clone()) else {
            self.tools.notice = Some("Engine or project not set.".into());
            return;
        };
        if self.is_busy_now() {
            self.tools.notice = Some("A build is running — wait for it to finish.".into());
            return;
        }
        match tools::start_commandlet(&engine, &project, label, args, ctx.clone()) {
            Ok(run) => { self.tools.run = Some(run); self.tools.notice = None; }
            Err(e)  => self.tools.notice = Some(e),
        }
    }

    fn show_commandlets(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let running = self.tool_running();
        let mut to_run: Option<(String, Vec<String>)> = None;

        ui.add(egui::Label::new(hint(
            "Run Unreal's own maintenance jobs on this project without opening the editor. \
             The editor has to be closed. Output appears below as it is written.")).wrap());
        ui.add_space(10.0);

        for (i, c) in COMMANDLETS.iter().enumerate() {
            card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let btn_w = 96.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width() - btn_w - 8.0, 20.0),
                        egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.label(egui::RichText::new(c.label).font(body(13.0)).color(TEXT));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let armed = self.tools.confirm == Some(i);
                        let text = if armed { "Confirm" } else { "Run" };
                        if ui.add_enabled(!running, if c.modifies { danger(text) } else { primary(text) }.min_size(egui::vec2(btn_w, 28.0)))
                            .clicked() {
                            if c.modifies && !armed {
                                self.tools.confirm = Some(i);
                            } else {
                                self.tools.confirm = None;
                                to_run = Some((c.label.to_string(),
                                    c.args.iter().map(|s| s.to_string()).collect()));
                            }
                        }
                    });
                });
                ui.add(egui::Label::new(hint(c.blurb)).wrap());
                if c.modifies {
                    ui.add_space(4.0);
                    let msg = if self.tools.confirm == Some(i) {
                        "This rewrites asset files. Commit or back up first — click Confirm to go ahead."
                    } else {
                        "Rewrites asset files — commit first."
                    };
                    ui.label(egui::RichText::new(msg).font(body(11.5)).color(AMBER));
                }
            });
            ui.add_space(8.0);
        }

        // Anything else.
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("ANY COMMANDLET"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_sized([150.0, 26.0],
                    egui::TextEdit::singleline(&mut self.tools.custom_name).hint_text("Name, e.g. GatherText"));
                let w = (ui.available_width() - 76.0 - ui.spacing().item_spacing.x).max(60.0);
                ui.add_sized([w, 26.0],
                    egui::TextEdit::singleline(&mut self.tools.custom_args).hint_text("-Option -Key=Value"));
                let name = self.tools.custom_name.trim().to_string();
                let name_ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                let args = crate::ops::package::parse_extra_uat_args(&self.tools.custom_args);
                if ui.add_enabled(!running && name_ok && args.is_ok(), ghost("Run").min_size(egui::vec2(76.0, 26.0))).clicked()
                    && let Ok(a) = args {
                    let mut all = vec![format!("-run={name}")];
                    all.extend(a);
                    to_run = Some((name, all));
                }
            });
            if let Err(e) = crate::ops::package::parse_extra_uat_args(&self.tools.custom_args) {
                ui.label(egui::RichText::new(e).font(body(11.5)).color(RED));
            }
        });

        if let Some(n) = &self.tools.notice {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(n).font(body(12.0)).color(RED));
        }
        if let Some((label, args)) = to_run {
            self.start_tool(&ctx, &label, args);
        }

        ui.add_space(10.0);
        self.show_tool_output(ui);
    }

    fn show_tool_output(&mut self, ui: &mut egui::Ui) {
        let Some(run) = self.tools.run.clone() else { return };
        let mut cancel = false;
        let mut open_log: Option<std::path::PathBuf> = None;
        let mut copy = false;

        deep().inner_margin(egui::Margin::symmetric(16.0, 12.0)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let r = run.lock().unwrap_or_else(|e| e.into_inner());
            ui.horizontal_wrapped(|ui| {
                ui.label(eyebrow(&r.label.to_uppercase()));
                let (text, color) = match (&r.finished, r.cancelled) {
                    (None, _)              => (format!("running · {}", crate::ui::shell::clock(r.elapsed().as_secs())), accent()),
                    (Some(_), true)        => ("cancelled".to_string(), AMBER),
                    (Some(Some(0)), false) => (format!("finished OK · {}", crate::ui::shell::clock(r.elapsed().as_secs())), GREEN),
                    (Some(c), false)       => (format!("exited with {} · {}",
                        c.map(|c| c.to_string()).unwrap_or_else(|| "no code".into()),
                        crate::ui::shell::clock(r.elapsed().as_secs())), RED),
                };
                ui.label(egui::RichText::new(text).font(body(12.0)).color(color));
                ui.label(hint(&format!("{} warnings · {} errors", r.progress.warnings, r.progress.errors)));
            });
            ui.add_space(8.0);

            egui::ScrollArea::vertical().id_salt("tool_output").max_height(300.0)
                .min_scrolled_height(300.0).auto_shrink([false, false]).stick_to_bottom(true)
                .show_rows(ui, 16.0, r.progress.lines.len().max(1), |ui, range| {
                    if r.progress.lines.is_empty() {
                        ui.label(hint("Waiting for output…"));
                        return;
                    }
                    for l in r.progress.lines.iter().skip(range.start).take(range.len()) {
                        let c = match l.level {
                            Level::Error  => RED,
                            Level::Warn   => AMBER,
                            Level::Normal => egui::Color32::from_rgb(124, 152, 166),
                        };
                        ui.add(egui::Label::new(egui::RichText::new(&l.text).font(mono(11.0)).color(c)).truncate());
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if r.running() && ui.add(danger("Cancel")).clicked() {
                    cancel = true;
                }
                if ui.add(quiet("Open log")).clicked() { open_log = Some(r.log_path.clone()); }
                if ui.add(quiet("Copy output")).clicked() { copy = true; }
            });
            if cancel { r.request_cancel(); }
            if copy {
                let text = r.progress.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
                ui.ctx().copy_text(text);
            }
        });
        if let Some(p) = open_log {
            let _ = crate::ops::cmd("explorer").arg(p).spawn();
        }
        if self.tool_running() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(500)); }
    }

    // ── Launch ──────────────────────────────────────────────────────────────

    fn launch_options(&self) -> Result<LaunchOpts, String> {
        Ok(LaunchOpts {
            log_window: self.tools.log_window,
            windowed:   self.tools.windowed,
            res:        RESOLUTIONS[self.tools.res].1,
            no_sound:   self.tools.no_sound,
            extra:      crate::ops::package::parse_extra_uat_args(&self.tools.extra)?,
        })
    }

    /// The newest packaged Windows build's executable, if there is one.
    fn latest_packaged_exe(&self) -> Option<std::path::PathBuf> {
        let name = self.pack_name_input.trim();
        self.builds.iter()
            .map(|b| b.dir.join(name))
            .find_map(|dir| crate::ops::package::find_main_exe(&dir))
    }

    fn show_launch(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::Label::new(hint(
            "Start the editor, the game from the editor, or your latest packaged build, with the \
             flags you would normally type. The command line is shown so nothing is hidden.")).wrap());
        ui.add_space(10.0);

        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("OPTIONS"));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (on, label, tip) in [
                    (&mut self.tools.log_window, "Log window", "-log: show the engine's log console"),
                    (&mut self.tools.windowed, "Windowed", "-windowed (games only)"),
                    (&mut self.tools.no_sound, "No sound", "-nosound"),
                ] {
                    if ui.add(chip(label, *on)).on_hover_text(tip).clicked() { *on = !*on; }
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(hint("Resolution"));
                for (i, (label, _)) in RESOLUTIONS.iter().enumerate() {
                    if ui.add(chip(label, self.tools.res == i)).clicked() { self.tools.res = i; }
                }
            });
            ui.add_space(6.0);
            ui.label(hint("Extra arguments"));
            let w = ui.available_width();
            ui.add_sized([w, 26.0],
                egui::TextEdit::singleline(&mut self.tools.extra).hint_text("-ExecCmds=stat"));
        });
        ui.add_space(10.0);

        let opts = self.launch_options();
        let project = self.project_path.clone().unwrap_or_default();
        let engine_exe = self.engine_dir.as_ref()
            .map(|e| e.join("Engine").join("Binaries").join("Win64").join("UnrealEditor.exe"))
            .filter(|p| p.is_file());
        let packaged = self.latest_packaged_exe();

        if let Err(e) = &opts {
            ui.label(egui::RichText::new(e).font(body(12.0)).color(RED));
            ui.add_space(6.0);
        }

        let mut go: Option<(LaunchMode, std::path::PathBuf)> = None;
        let rows: [(LaunchMode, &str, &str, Option<std::path::PathBuf>); 3] = [
            (LaunchMode::Editor, "Launch editor", "Open the project in Unreal Editor.", engine_exe.clone()),
            (LaunchMode::Standalone, "Play standalone game", "The game in its own window, straight from the editor's files — no packaging.", engine_exe),
            (LaunchMode::Packaged, "Run latest packaged build", "Your newest Windows build.", packaged),
        ];
        for (mode, label, blurb, exe) in rows {
            card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 120.0).max(60.0), 20.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| { ui.label(egui::RichText::new(label).font(body(13.0)).color(TEXT)); });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let ready = exe.is_some() && opts.is_ok();
                        if ui.add_enabled(ready, primary("Launch").min_size(egui::vec2(96.0, 28.0)))
                            .on_disabled_hover_text(if exe.is_none() { "Not found." } else { "Fix the arguments first." })
                            .clicked()
                            && let Some(e) = &exe {
                            go = Some((mode, e.clone()));
                        }
                    });
                });
                ui.add(egui::Label::new(hint(blurb)).wrap());
                if let (Some(e), Ok(o)) = (&exe, &opts) {
                    let args = tools::launch_args(mode, &project, o).join(" ");
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(
                        egui::RichText::new(format!("{} {}", e.file_name().unwrap_or_default().to_string_lossy(), args))
                            .font(mono(10.5)).color(DIM)).truncate());
                }
            });
            ui.add_space(8.0);
        }

        if let (Some((mode, exe)), Ok(o)) = (go, opts) {
            let args = tools::launch_args(mode, &project, &o);
            match tools::launch(&exe, &args) {
                Ok(()) => self.set_status(format!("Launched {}.", exe.display())),
                Err(e) => self.set_status(format!("[ERROR] Could not launch {}: {e}", exe.display())),
            }
        }
    }

    // ── Size & content ──────────────────────────────────────────────────────

    fn show_content_insights(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::Label::new(hint(
            "Where the bytes are. Scan the project's Content folder to see what makes it big, or your \
             latest build to see what makes the package big.")).wrap());
        ui.add_space(10.0);

        let project_dir = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        let build_dir = self.builds.first().map(|b| b.dir.join(self.pack_name_input.trim()))
            .filter(|d| d.is_dir())
            .or_else(|| self.builds.first().map(|b| b.dir.clone()));

        let mut scan_root: Option<std::path::PathBuf> = None;
        ui.horizontal_wrapped(|ui| {
            if ui.add(chip("Project Content", !self.tools.from_build)).clicked() { self.tools.from_build = false; }
            if ui.add_enabled(build_dir.is_some(), chip("Latest build", self.tools.from_build)).clicked() {
                self.tools.from_build = true;
            }
            let root = if self.tools.from_build { build_dir.clone() } else { project_dir.map(|d| d.join("Content")) };
            let label = if self.tools.scanning { "Scanning…" } else { "Scan" };
            if ui.add_enabled(!self.tools.scanning && root.is_some(), primary(label).min_size(egui::vec2(96.0, 28.0))).clicked() {
                scan_root = root;
            }
        });
        if let Some(root) = scan_root {
            self.tools.scanning = true;
            let out = self.tools.scan_pending.clone();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let found = insights::scan(&root, insights::DEFAULT_BUDGET);
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
                ctx.request_repaint();
            });
        }
        if self.tools.scanning || self.tools.comparing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }

        // Compare the two newest builds: what made the package grow.
        let pack = self.pack_name_input.trim().to_string();
        let pair: Option<(String, std::path::PathBuf, String, std::path::PathBuf)> = match (self.builds.first(), self.builds.get(1)) {
            (Some(n), Some(o)) => {
                let root = |b: &crate::ops::history::BuildRecord| {
                    let d = b.dir.join(&pack);
                    if d.is_dir() { d } else { b.dir.clone() }
                };
                Some((o.version.clone(), root(o), n.version.clone(), root(n)))
            }
            _ => None,
        };
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let label = if self.tools.comparing { "Comparing…" } else { "Compare the last two builds" };
            if ui.add_enabled(pair.is_some() && !self.tools.comparing, chip(label, false))
                .on_hover_text("What grew or shrank between your two newest builds").clicked()
                && let Some((ov, op, nv, np)) = pair.clone() {
                self.tools.comparing = true;
                let out = self.tools.compare_pending.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let c = insights::compare(&op, &np, insights::DEFAULT_BUDGET);
                    *out.lock().unwrap_or_else(|e| e.into_inner()) = Some((ov, nv, c));
                    ctx.request_repaint();
                });
            }
        });
        ui.add_space(6.0);
        self.show_comparison(ui);
        ui.add_space(12.0);

        let Some(i) = self.tools.insights.clone() else {
            ui.label(hint("Nothing scanned yet."));
            return;
        };

        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(numeral(&crate::ops::history::format_bytes(i.total), 22.0, TEXT));
                ui.label(hint(&format!("in {} files · {}{:.1}s", i.files, if i.partial { "partial, hit the time limit · " } else { "" }, i.secs)));
            });
            ui.label(hint(&i.root.display().to_string()));
        });
        ui.add_space(10.0);

        let total = i.total.max(1);
        let bar_rows = |ui: &mut egui::Ui, title: &str, rows: &[(String, u64, Option<usize>)]| {
            card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(eyebrow(title));
                ui.add_space(8.0);
                for (name, bytes, n) in rows {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2(190.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_min_width(190.0);
                            ui.set_max_width(190.0);
                            ui.add(egui::Label::new(egui::RichText::new(name).font(body(12.0)).color(SOFT)).truncate());
                        });
                        let bar_w = (ui.available_width() - 130.0).max(20.0);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, egui::Rounding::same(3.0), TRACK);
                        let frac = (*bytes as f32 / total as f32).clamp(0.0, 1.0);
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, 6.0)),
                            egui::Rounding::same(3.0), acc(170));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let t = match n {
                                Some(n) => format!("{} · {n}", crate::ops::history::format_bytes(*bytes)),
                                None    => crate::ops::history::format_bytes(*bytes),
                            };
                            ui.label(numeral(&t, 11.0, DIM));
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        };
        let kinds: Vec<_> = i.by_kind.iter().map(|(k, b, n)| (k.clone(), *b, Some(*n))).collect();
        bar_rows(ui, "BY KIND  (size · files)", &kinds);
        ui.add_space(10.0);
        let dirs: Vec<_> = i.top_dirs.iter().map(|(d, b)| (d.clone(), *b, None)).collect();
        bar_rows(ui, "BY FOLDER", &dirs);
        ui.add_space(10.0);

        let mut reveal: Option<std::path::PathBuf> = None;
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("LARGEST FILES"));
            ui.add_space(8.0);
            for (path, bytes) in &i.top_files {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 90.0).max(60.0);
                    let r = ui.allocate_ui_with_layout(egui::vec2(w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(w);
                        ui.set_max_width(w);
                        ui.add(egui::Label::new(egui::RichText::new(path).font(mono(11.0)).color(SOFT))
                            .truncate().sense(egui::Sense::click()))
                    });
                    if r.inner.on_hover_text("Click to show in Explorer").clicked() {
                        reveal = Some(i.root.join(path));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(numeral(&crate::ops::history::format_bytes(*bytes), 11.0, DIM));
                    });
                });
            }
        });
        if let Some(p) = reveal {
            let _ = crate::ops::cmd("explorer").arg(format!("/select,{}", p.display())).spawn();
        }
    }

    // ── Cheatsheet ──────────────────────────────────────────────────────────

    fn show_cheatsheet(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::Label::new(hint(
            "The console commands and launch flags people look up again and again. Click one to copy it. \
             Console commands go in the in-game console (the ` key); flags go on the command line — the \
             Launch tab's extra-arguments box takes them.")).wrap());
        ui.add_space(8.0);
        let w = ui.available_width();
        ui.add_sized([w, 26.0], egui::TextEdit::singleline(&mut self.tools.cheat_filter)
            .hint_text("Filter — fps, wireframe, windowed…"));
        ui.add_space(10.0);

        let found = crate::ops::cheatsheet::search(&self.tools.cheat_filter);
        if found.is_empty() {
            ui.label(hint("Nothing matches."));
            return;
        }
        let mut copied: Option<&'static str> = None;
        let mut last_group = "";
        for e in &found {
            if e.group != last_group {
                if !last_group.is_empty() { ui.add_space(10.0); }
                ui.label(eyebrow(&e.group.to_uppercase()));
                ui.add_space(4.0);
                last_group = e.group;
            }
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect, egui::Rounding::same(9.0), acc(22)); }
            ui.painter().text(rect.left_top() + egui::vec2(12.0, 6.0), egui::Align2::LEFT_TOP,
                e.text, mono(12.5), if e.flag { accent() } else { TEXT });
            // The description, truncated to the row rather than wrapping into the next.
            let galley = ui.painter().layout(e.what.to_string(), body(11.5), MUTED, rect.width() - 24.0);
            ui.painter().galley(rect.left_top() + egui::vec2(12.0, 24.0), galley, MUTED);
            if resp.on_hover_text(format!("{}\n\nClick to copy", e.what)).clicked() {
                copied = Some(e.text);
            }
        }
        if let Some(text) = copied {
            ui.ctx().copy_text(text.to_string());
            self.set_status(format!("Copied: {text}"));
        }
    }

    // ── Engines ─────────────────────────────────────────────────────────────

    fn show_engines(&mut self, ui: &mut egui::Ui) {
        if self.tools.engines.is_none() {
            let near: Vec<std::path::PathBuf> = self.engine_dir.iter().chain(self.engine_override.iter()).cloned().collect();
            self.tools.engines = Some(crate::ops::engines::list(&near));
        }
        let installs = self.tools.engines.clone().unwrap_or_default();
        let project = self.project_path.clone();
        let wants = project.as_ref().and_then(|p| crate::ops::engines::project_association(p));

        ui.add(egui::Label::new(hint(
            "The Unreal Engine versions installed on this PC, the one this project asks for, and the one \
             the app is using for builds.")).wrap());
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(hint("Project asks for"));
                ui.label(numeral(wants.as_deref().unwrap_or("not set"), 13.0, TEXT));
                ui.add_space(12.0);
                ui.label(hint("App is using"));
                let using = self.engine_dir.as_ref()
                    .and_then(|d| installs.iter().find(|e| e.dir == *d))
                    .map(|e| e.version.clone())
                    .or_else(|| self.engine_dir.as_ref().map(|d| d.display().to_string()))
                    .unwrap_or_else(|| "none found".into());
                ui.label(numeral(&using, 13.0, TEXT));
            });
            // A build with the wrong engine is a build that converts assets.
            let mismatch = match (&wants, self.engine_dir.as_ref().and_then(|d| installs.iter().find(|e| e.dir == *d))) {
                (Some(w), Some(e)) => e.short.as_deref().is_some_and(|s| s != w),
                _ => false,
            };
            if mismatch {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(
                    "The engine in use is not the one the project asks for. Building with it makes Unreal \
                     convert the project's assets.").font(body(12.0)).color(AMBER));
            }
        });
        ui.add_space(10.0);

        if installs.is_empty() {
            ui.label(hint("No engine installs found."));
        }

        let mut use_engine: Option<std::path::PathBuf> = None;
        let mut launch: Option<std::path::PathBuf> = None;
        let mut open: Option<std::path::PathBuf> = None;
        let mut retarget: Option<(usize, String)> = None;

        for (i, e) in installs.iter().enumerate() {
            let in_use = self.engine_dir.as_ref() == Some(&e.dir);
            let is_projects = wants.as_deref().is_some_and(|w| e.short.as_deref() == Some(w));
            card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("Unreal Engine {}", e.version)).font(body(13.5)).color(TEXT));
                    if in_use { ui.label(egui::RichText::new("in use").font(body(11.5)).color(GREEN)); }
                    if is_projects { ui.label(egui::RichText::new("this project's engine").font(body(11.5)).color(accent())); }
                });
                ui.add(egui::Label::new(egui::RichText::new(e.dir.display().to_string()).font(mono(10.5)).color(DIM)).truncate());
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!in_use, chip("Use for builds", false))
                        .on_hover_text("Build with this engine from now on").clicked() {
                        use_engine = Some(e.dir.clone());
                    }
                    if ui.add(chip("Launch editor", false)).on_hover_text("Open this engine without a project").clicked() {
                        launch = Some(e.dir.join("Engine").join("Binaries").join("Win64").join("UnrealEditor.exe"));
                    }
                    if ui.add(chip("Open folder", false)).clicked() { open = Some(e.dir.clone()); }
                    if let Some(short) = &e.short && !is_projects && project.is_some() {
                        let armed = self.tools.engine_confirm == Some(i);
                        if ui.add(chip(if armed { "Click again to switch the project" } else { "Switch project to this" }, armed))
                            .on_hover_text("Edits EngineAssociation in the .uproject (a backup is kept). The assets are converted the next time the project is opened, so commit first.")
                            .clicked() {
                            if armed { retarget = Some((i, short.clone())); } else { self.tools.engine_confirm = Some(i); }
                        }
                    }
                });
            });
            ui.add_space(8.0);
        }

        if let Some(dir) = use_engine {
            crate::config::save_engine_path(&dir);
            self.engine_override = Some(dir.clone());
            self.redetect_engine();
            self.set_status(format!("[OK] Building with {}", dir.display()));
        }
        if let Some(exe) = launch {
            match tools::launch(&exe, &[]) {
                Ok(()) => self.set_status(format!("Launched {}.", exe.display())),
                Err(e) => self.set_status(format!("[ERROR] Could not launch {}: {e}", exe.display())),
            }
        }
        if let Some(d) = open {
            let _ = crate::ops::cmd("explorer").arg(d).spawn();
        }
        if let (Some((_, short)), Some(p)) = (retarget, project) {
            self.tools.engine_confirm = None;
            match crate::ops::plugins::set_engine_association(&p, &short) {
                Ok(()) => self.set_status(format!("The project now asks for Unreal Engine {short}. A backup of the .uproject was kept.")),
                Err(e) => self.set_status(format!("[ERROR] {e}")),
            }
        }
    }

    fn show_comparison(&mut self, ui: &mut egui::Ui) {
        let Some((older, newer, c)) = self.tools.comparison.clone() else { return };
        let signed = |d: i64| {
            let s = crate::ops::history::format_bytes(d.unsigned_abs());
            if d > 0 { format!("+{s}") } else if d < 0 { format!("−{s}") } else { "no change".into() }
        };
        let delta = c.newer_total as i64 - c.older_total as i64;
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow(&format!("CHANGE FROM {older} TO {newer}")));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(numeral(&signed(delta), 22.0, if delta > 0 { AMBER } else if delta < 0 { GREEN } else { TEXT }));
                ui.label(hint(&format!("{} → {}{}",
                    crate::ops::history::format_bytes(c.older_total), crate::ops::history::format_bytes(c.newer_total),
                    if c.partial { " · partial" } else { "" })));
            });
            let rows = |ui: &mut egui::Ui, title: &str, rows: &[(String, i64)]| {
                if rows.is_empty() { return; }
                ui.add_space(10.0);
                ui.label(eyebrow(title));
                ui.add_space(4.0);
                for (name, d) in rows {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 110.0).max(60.0);
                        ui.allocate_ui_with_layout(egui::vec2(w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_min_width(w);
                            ui.set_max_width(w);
                            ui.add(egui::Label::new(egui::RichText::new(name).font(body(12.0)).color(SOFT)).truncate());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(numeral(&signed(*d), 11.0, if *d > 0 { AMBER } else { GREEN }));
                        });
                    });
                }
            };
            rows(ui, "BY KIND", &c.kinds);
            rows(ui, "BY FOLDER", &c.dirs);
            let files = |list: &[(String, i64, bool)], tag: &str| -> Vec<(String, i64)> {
                list.iter().map(|(p, d, edge)| (if *edge { format!("{p}  ({tag})") } else { p.clone() }, *d)).collect()
            };
            rows(ui, "FILES THAT GREW", &files(&c.grown, "new"));
            rows(ui, "FILES THAT SHRANK", &files(&c.shrunk, "removed"));
        });
    }

    // ── Plugins ─────────────────────────────────────────────────────────────

    fn reload_plugins(&mut self) {
        self.tools.plugins = self.project_path.as_ref().map(|p| plugins::list(p));
    }

    fn show_plugins(&mut self, ui: &mut egui::Ui) {
        if self.tools.plugins.is_none() { self.reload_plugins(); }
        let editor_open = match self.tools.editor_seen {
            Some((at, open)) if at.elapsed() < std::time::Duration::from_secs(3) => open,
            _ => {
                let open = crate::ops::package::is_process_running("UnrealEditor.exe");
                self.tools.editor_seen = Some((std::time::Instant::now(), open));
                open
            }
        };
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(3));
        ui.add(egui::Label::new(hint(
            "The plugins this project lists, and the ones in its own Plugins folder. Switching one \
             edits the .uproject — the first change keeps a copy as <project>.uproject.devtool-backup.")).wrap());
        if editor_open {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Unreal Editor is open — close it before changing plugins.")
                .font(body(12.0)).color(AMBER));
        }
        ui.add_space(8.0);
        let w = ui.available_width();
        ui.add_sized([w, 26.0], egui::TextEdit::singleline(&mut self.tools.plugin_filter).hint_text("Filter plugins…"));
        ui.add_space(8.0);

        let mut toggle: Option<(String, bool)> = None;
        match self.tools.plugins.clone() {
            None => {}
            Some(Err(e)) => { ui.label(egui::RichText::new(e).font(body(12.0)).color(RED)); }
            Some(Ok(list)) => {
                let q = self.tools.plugin_filter.to_ascii_lowercase();
                let shown: Vec<&PluginEntry> = list.iter()
                    .filter(|p| q.is_empty() || p.name.to_ascii_lowercase().contains(&q)).collect();
                ui.label(hint(&format!("{} of {} plugins", shown.len(), list.len())));
                ui.add_space(6.0);
                card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for (n, p) in shown.iter().enumerate() {
                        if n > 0 { divider(ui); }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let btn_w = 92.0;
                            ui.allocate_ui_with_layout(
                                egui::vec2((ui.available_width() - btn_w - 8.0).max(60.0), 34.0),
                                egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(&p.name).font(body(12.5)).color(TEXT)).truncate());
                                    if p.local { ui.label(hint("project")); }
                                    if !p.version.is_empty() { ui.label(hint(&format!("v{}", p.version))); }
                                });
                                if !p.description.is_empty() {
                                    ui.add(egui::Label::new(egui::RichText::new(&p.description).font(body(11.0)).color(DIM)).truncate());
                                }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if p.enabled { "Enabled" } else { "Disabled" };
                                if ui.add_enabled(!editor_open, chip(label, p.enabled).min_size(egui::vec2(btn_w, 26.0))).clicked() {
                                    toggle = Some((p.name.clone(), !p.enabled));
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
            }
        }
        if let (Some((name, on)), Some(project)) = (toggle, self.project_path.clone()) {
            match plugins::set_enabled(&project, &name, on) {
                Ok(()) => self.set_status(format!("{name} {} in the .uproject.", if on { "enabled" } else { "disabled" })),
                Err(e) => self.set_status(format!("[ERROR] {e}")),
            }
            self.reload_plugins();
        }
    }
}
