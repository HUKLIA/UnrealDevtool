//! The Project monitor sheet: live processes, the editor's log, and the state
//! of the project folder.
//!
//! Layout follows the rest of the app: wide, two independently scrolling
//! columns built from explicit rectangles; narrow, one scroll area stacking
//! them. One scroll region per piece of content, always.

use eframe::egui;

use crate::app::DevToolApp;
use crate::ops::monitor::{ago, uptime, LogEntry, MonitorData, MonitorHandle, ProcGroup};
use crate::ops::run::Level;
use crate::theme::*;
use crate::types::LogFilter;

/// Below this the two columns stack. Derived from what each needs to be
/// readable (a process row and a log line), not a magic breakpoint.
const SPLIT_MIN: f32 = 860.0;
const COL_GAP: f32 = 18.0;
const LOG_ROW_H: f32 = 16.0;

impl DevToolApp {
    /// Starts the monitor when its sheet opens and stops it when it closes.
    ///
    /// Centralised here rather than in `open_sheet`/`close_sheet` because
    /// sheets are also closed by other paths (the tour, the git chip), and a
    /// polling thread must never outlive the view it feeds.
    pub fn sync_monitor(&mut self, ctx: &egui::Context) {
        let wanted = self.sheet == Some(crate::types::Sheet::Monitor);
        if !wanted {
            if self.monitor.take().is_some() {
                self.monitor_frozen = None;
            }
            return;
        }
        let Some(project) = self.project_path.clone() else { return };
        let stale = self.monitor.as_ref().is_none_or(|m| m.project != project);
        if stale {
            self.monitor = Some(MonitorHandle::start(project, self.is_working.clone(), ctx.clone()));
            self.monitor_frozen = None;
        }
    }

    pub fn show_monitor_sheet(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project_path.clone() else {
            ui.label(hint("Open a project first — the monitor watches one Unreal project."));
            return;
        };
        let Some(handle) = &self.monitor else {
            ui.label(hint("Starting…"));
            return;
        };
        let data = handle.data.clone();
        self.show_monitor_toolbar(ui, &data, &project);
        ui.add_space(12.0);

        let region = ui.available_rect_before_wrap();
        if region.width() >= SPLIT_MIN {
            let left_w = ((region.width() - COL_GAP) * 0.44).floor();
            let left = egui::Rect::from_min_size(region.min, egui::vec2(left_w, region.height()));
            let right = egui::Rect::from_min_max(
                egui::pos2(region.min.x + left_w + COL_GAP, region.min.y), region.max);
            ui.allocate_rect(region, egui::Sense::hover());
            let td = egui::Layout::top_down(egui::Align::Min);

            let mut l = ui.new_child(egui::UiBuilder::new().max_rect(left).layout(td));
            egui::ScrollArea::vertical().id_salt("monitor_left").auto_shrink([false, false])
                .show(&mut l, |ui| {
                    self.show_processes_card(ui, &data);
                    ui.add_space(14.0);
                    self.show_health_card(ui, &data, &project);
                    ui.add_space(GUTTER);
                });

            let mut r = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(td));
            let h = right.height() - 4.0;
            self.show_log_card(&mut r, &data, &project, h);
        } else {
            egui::ScrollArea::vertical().id_salt("monitor_stacked").auto_shrink([false, false])
                .show(ui, |ui| {
                    self.show_processes_card(ui, &data);
                    ui.add_space(14.0);
                    self.show_health_card(ui, &data, &project);
                    ui.add_space(14.0);
                    self.show_log_card(ui, &data, &project, 360.0);
                    ui.add_space(GUTTER);
                });
        }
    }

    // ── Toolbar ─────────────────────────────────────────────────────────────

    fn show_monitor_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        data: &std::sync::Arc<std::sync::Mutex<MonitorData>>,
        project: &std::path::Path,
    ) {
        let (editor, shaders, cook) = {
            let d = data.lock().unwrap_or_else(|e| e.into_inner());
            (d.editor, d.shaders_left, d.cook_remaining)
        };
        let dir = project.parent().map(|p| p.to_path_buf());
        ui.horizontal_wrapped(|ui| {
            match editor {
                Some((pid, age)) => {
                    dot(ui, GREEN, 8.0);
                    ui.label(egui::RichText::new("Editor running").font(body(13.0)).color(TEXT));
                    ui.label(hint(&format!("pid {pid} · up {}", uptime(age))));
                }
                None => {
                    dot(ui, FAINT, 8.0);
                    ui.label(egui::RichText::new("Editor not running").font(body(13.0)).color(SOFT));
                }
            }
            // Live counters the editor prints as it works.
            if let Some(n) = shaders.filter(|n| *n > 0) {
                ui.label(egui::RichText::new(format!("{n} shaders to compile")).font(body(12.0)).color(AMBER));
            }
            if let Some(n) = cook.filter(|n| *n > 0) {
                ui.label(egui::RichText::new(format!("{n} packages left to cook")).font(body(12.0)).color(accent()));
            }
            ui.add_space(8.0);
            if editor.is_none() && ui.add(chip("Launch editor", false))
                .on_hover_text("Open this project in Unreal Editor").clicked() {
                self.launch_editor();
            }
            if let Some(d) = &dir {
                if ui.add(chip("Project folder", false)).clicked() {
                    let _ = crate::ops::cmd("explorer").arg(d).spawn();
                }
                if ui.add(chip("Logs folder", false)).clicked() {
                    let logs = d.join("Saved").join("Logs");
                    let _ = crate::ops::cmd("explorer").arg(if logs.is_dir() { logs } else { d.clone() }).spawn();
                }
            }
        });
    }

    /// Opens the project in the editor that matches it: the detected engine's
    /// own `UnrealEditor.exe` when there is one, otherwise whatever Windows has
    /// registered for `.uproject` files.
    pub(crate) fn launch_editor(&mut self) {
        let Some(project) = self.project_path.clone() else { return };
        let exe = self.engine_dir.as_ref()
            .map(|e| e.join("Engine").join("Binaries").join("Win64").join("UnrealEditor.exe"))
            .filter(|p| p.is_file());
        let result = match exe {
            Some(exe) => std::process::Command::new(exe).arg(&project).spawn(),
            None      => crate::ops::cmd("explorer").arg(&project).spawn(),
        };
        match result {
            Ok(_)  => self.set_status("Opening the project in Unreal Editor…".into()),
            Err(e) => self.set_status(format!("[ERROR] Could not launch the editor: {e}")),
        }
    }

    // ── Processes ───────────────────────────────────────────────────────────

    fn show_processes_card(&self, ui: &mut egui::Ui, data: &std::sync::Arc<std::sync::Mutex<MonitorData>>) {
        let d = data.lock().unwrap_or_else(|e| e.into_inner());
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("UNREAL PROCESSES"));
            ui.add_space(10.0);

            if d.procs.is_empty() {
                ui.label(hint("Nothing from Unreal is running. Open the editor, or start a build, and it appears here."));
                return;
            }

            // Totals with their recent history.
            let total_cpu: f32 = d.procs.iter().map(|g| g.cpu).sum();
            let total_mem: f32 = d.procs.iter().map(|g| g.mem_mb).sum();
            let w = ui.available_width();
            let gap = ui.spacing().item_spacing.x;
            let cell = ((w - gap) / 2.0).floor();
            ui.horizontal(|ui| {
                stat_cell(ui, cell, "CPU", &format!("{total_cpu:.0}%"), accent(),
                    &d.cpu_hist, 100.0);
                let mem_max = d.mem_hist.iter().cloned().fold(1024.0_f32, f32::max);
                stat_cell(ui, cell, "MEMORY", &format_mem(total_mem), AMBER,
                    &d.mem_hist, mem_max);
            });
            ui.add_space(12.0);

            for g in &d.procs {
                process_row(ui, g);
            }
        });
    }

    // ── Project health ──────────────────────────────────────────────────────

    fn show_health_card(
        &self,
        ui: &mut egui::Ui,
        data: &std::sync::Arc<std::sync::Mutex<MonitorData>>,
        project: &std::path::Path,
    ) {
        let building = self.is_busy_now();
        let d = data.lock().unwrap_or_else(|e| e.into_inner());
        let h = &d.health;
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(eyebrow("PROJECT"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let note = match (building, d.health_at) {
                        (true, _)       => "paused while building".to_string(),
                        (_, Some(at))   => format!("scanned {}", ago(at.elapsed().as_secs())),
                        (_, None)       => "scanning…".to_string(),
                    };
                    ui.label(hint(&note));
                });
            });
            ui.add_space(10.0);

            if h.folders.is_empty() {
                ui.label(hint("Reading the project folder…"));
                return;
            }

            // Folder sizes, with the biggest as the scale for the bars.
            let biggest = h.folders.iter().map(|f| f.bytes).max().unwrap_or(1).max(1);
            for f in &h.folders {
                ui.horizontal(|ui| {
                    left_label(ui, 128.0, egui::RichText::new(f.name).font(body(12.0))
                        .color(if f.exists { SOFT } else { FAINT }), None);
                    let bar_w = (ui.available_width() - 84.0).max(20.0);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, egui::Rounding::same(3.0), TRACK);
                    if f.exists {
                        let frac = (f.bytes as f32 / biggest as f32).clamp(0.0, 1.0);
                        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, 6.0));
                        ui.painter().rect_filled(fill, egui::Rounding::same(3.0), acc(150));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = if !f.exists { "—".to_string() } else if f.partial {
                            format!("{}+", crate::ops::history::format_bytes(f.bytes))
                        } else { crate::ops::history::format_bytes(f.bytes) };
                        ui.label(numeral(&text, 11.5, if f.exists { DIM } else { FAINT }));
                    });
                });
                ui.add_space(2.0);
            }

            ui.add_space(8.0);
            divider(ui);
            ui.add_space(10.0);

            // Latest edits.
            ui.horizontal(|ui| {
                ui.label(eyebrow("LATEST EDITS"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(hint(&format!("{} changed in the last 10 min", h.changed_10m)));
                });
            });
            ui.add_space(6.0);
            if h.recent.is_empty() {
                ui.label(hint("No files yet."));
            }
            for (path, secs) in &h.recent {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 76.0).max(40.0);
                    left_label(ui, w, egui::RichText::new(path).font(mono(11.0)).color(SOFT), Some(path));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(hint(&ago(*secs)));
                    });
                });
            }

            // Crashes.
            ui.add_space(8.0);
            divider(ui);
            ui.add_space(10.0);
            match &h.crash {
                Some((name, secs)) => {
                    let recent = *secs < 86_400;
                    ui.horizontal(|ui| {
                        dot(ui, if recent { RED } else { AMBER }, 7.0);
                        ui.add(egui::Label::new(egui::RichText::new(format!("Last crash {}", ago(*secs)))
                            .font(body(12.0)).color(if recent { RED } else { SOFT })).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(quiet("Open")).clicked() {
                                let dir = project.parent().map(|d| d.join("Saved").join("Crashes").join(name));
                                if let Some(dir) = dir {
                                    let _ = crate::ops::cmd("explorer").arg(dir).spawn();
                                }
                            }
                        });
                    });
                    ui.label(hint(name));
                }
                None => {
                    ui.horizontal(|ui| {
                        dot(ui, GREEN, 7.0);
                        ui.label(egui::RichText::new("No crashes recorded").font(body(12.0)).color(SOFT));
                    });
                }
            }
        });
    }

    // ── Live log ────────────────────────────────────────────────────────────

    /// The editor log. `height` is the whole card, borders included.
    fn show_log_card(
        &mut self,
        ui: &mut egui::Ui,
        data: &std::sync::Arc<std::sync::Mutex<MonitorData>>,
        project: &std::path::Path,
        height: f32,
    ) {
        const PAD_Y: f32 = 16.0;
        let filter = self.monitor_filter;
        let paused = self.monitor_frozen.is_some();

        let mut new_filter = filter;
        let mut toggle_pause = false;
        let mut copy = false;
        let mut open_file: Option<std::path::PathBuf> = None;

        let frozen = self.monitor_frozen.clone();
        let d = data.lock().unwrap_or_else(|e| e.into_inner());
        let live: &std::collections::VecDeque<LogEntry> = &d.log;

        deep().inner_margin(egui::Margin::symmetric(16.0, PAD_Y)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let inner_h = (height - 2.0 * PAD_Y - 2.0).max(120.0);
            ui.set_min_height(inner_h);
            let y0 = ui.cursor().min.y;

            ui.horizontal_wrapped(|ui| {
                ui.label(eyebrow("EDITOR LOG"));
                ui.add_space(6.0);
                let (w, e) = (d.warnings, d.errors);
                for (f, label) in [
                    (LogFilter::All, "All".to_string()),
                    (LogFilter::Warnings, format!("Warnings {w}")),
                    (LogFilter::Errors, format!("Errors {e}")),
                ] {
                    if ui.add(chip(&label, filter == f)).clicked() { new_filter = f; }
                }
                if ui.add(chip(if paused { "Paused" } else { "Pause" }, paused))
                    .on_hover_text("Freeze the view to read it while the editor keeps writing").clicked() {
                    toggle_pause = true;
                }
            });
            ui.add_space(8.0);

            let entries: Vec<&LogEntry> = match &frozen {
                Some(v) => v.iter().collect(),
                None    => live.iter().collect(),
            };
            let shown: Vec<&LogEntry> = entries.into_iter().filter(|e| match filter {
                LogFilter::All      => true,
                LogFilter::Warnings => e.level == Level::Warn,
                LogFilter::Errors   => e.level == Level::Error,
            }).collect();

            let footer_h = 30.0;
            let list_h = (inner_h - (ui.cursor().min.y - y0) - footer_h - ui.spacing().item_spacing.y).max(60.0);

            if d.log_path.as_ref().is_none_or(|p| !p.is_file()) {
                ui.allocate_ui(egui::vec2(ui.available_width(), list_h), |ui| {
                    ui.label(hint(&format!(
                        "No editor log yet. It appears at Saved/Logs/{}.log the first time the project is opened in Unreal Editor.",
                        project.file_stem().unwrap_or_default().to_string_lossy())));
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("monitor_log")
                    .max_height(list_h)
                    .min_scrolled_height(list_h)
                    .auto_shrink([false, false])
                    .stick_to_bottom(!paused)
                    .show_rows(ui, LOG_ROW_H, shown.len(), |ui, range| {
                        for e in &shown[range] {
                            let c = match e.level {
                                Level::Error  => RED,
                                Level::Warn   => AMBER,
                                Level::Normal => egui::Color32::from_rgb(124, 152, 166),
                            };
                            ui.add(egui::Label::new(
                                egui::RichText::new(&e.text).font(mono(11.0)).color(c)).truncate());
                        }
                        if shown.is_empty() {
                            ui.label(hint(match filter {
                                LogFilter::All      => "Nothing logged yet.",
                                LogFilter::Warnings => "No warnings.",
                                LogFilter::Errors   => "No errors.",
                            }));
                        }
                    });
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(quiet("Copy shown")).clicked() { copy = true; }
                if let Some(p) = d.log_path.clone().filter(|p| p.is_file())
                    && ui.add(quiet("Open log file")).clicked() {
                    open_file = Some(p);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(hint(&format!("{} lines", shown.len())));
                });
            });

            if copy {
                let text = shown.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join("\n");
                ui.ctx().copy_text(text);
            }
        });

        // Snapshot for Pause is taken from the live buffer, outside the paint.
        if toggle_pause {
            self.monitor_frozen = if paused { None } else { Some(d.log.iter().cloned().collect()) };
        }
        drop(d);
        self.monitor_filter = new_filter;
        if let Some(p) = open_file {
            let _ = crate::ops::cmd("explorer").arg(p).spawn();
        }
    }
}

// ── Pieces ───────────────────────────────────────────────────────────────────

/// A label, a big number and the recent trace behind it.
fn stat_cell(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    value: &str,
    color: egui::Color32,
    hist: &std::collections::VecDeque<f32>,
    max: f32,
) {
    ui.allocate_ui_with_layout(egui::vec2(width, 74.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.horizontal(|ui| {
            ui.label(eyebrow(label));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(numeral(value, 15.0, TEXT));
            });
        });
        ui.add_space(4.0);
        sparkline(ui, egui::vec2(width, 44.0), hist, max, color);
    });
}

fn process_row(ui: &mut egui::Ui, g: &ProcGroup) {
    ui.horizontal(|ui| {
        let busy = g.cpu >= 1.0;
        dot(ui, if g.role == "Editor" { GREEN } else if busy { accent() } else { FAINT }, 7.0);
        let name_w = (ui.available_width() * 0.5).max(90.0);
        ui.allocate_ui_with_layout(egui::vec2(name_w, 30.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let title = if g.count > 1 { format!("{}  ×{}", g.role, g.count) } else { g.role.to_string() };
            ui.add(egui::Label::new(egui::RichText::new(title).font(body(12.5)).color(TEXT)).truncate());
            ui.add(egui::Label::new(egui::RichText::new(&g.image).font(mono(10.0)).color(DIM)).truncate());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(numeral(&format_mem(g.mem_mb), 11.5, DIM));
            ui.add_space(6.0);
            ui.label(numeral(&format!("{:>3.0}%", g.cpu), 11.5, if busy { accent() } else { DIM }));
        });
    });
    ui.add_space(2.0);
}

/// A label pinned to the left of a fixed-width slot. `add_sized` centres its
/// widget, which put short folder names in the middle of their column and let
/// long ones spill out of it.
fn left_label(ui: &mut egui::Ui, width: f32, text: egui::RichText, tip: Option<&str>) {
    let r = ui.allocate_ui_with_layout(
        egui::vec2(width, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // Reserve the whole slot; a child ui otherwise reports only as much
            // width as its text used, and the next column starts early.
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).truncate());
        },
    );
    if let Some(t) = tip {
        r.response.on_hover_text(t);
    }
}

fn format_mem(mb: f32) -> String {
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") }
}

use crate::ui::run::divider;
