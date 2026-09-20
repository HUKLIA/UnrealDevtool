//! Command palette (Ctrl+K): every action in the app, reachable by typing.
//!
//! The app deliberately shows few buttons per page. The cost of that is that
//! some things are two clicks deep; the palette is the way round it for anyone
//! who knows what they want, without adding a single control to any page.

use eframe::egui;

use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::{BuildConfiguration, BuildTarget, PackageMethod, RunState, Sheet};

#[derive(Clone)]
enum Cmd {
    StartBuild,
    Sheet(Sheet),
    Manual,
    Target(BuildTarget),
    Method(PackageMethod),
    Config(BuildConfiguration),
    ToggleIterate,
    LaunchEditor,
    OpenProject,
    OpenBuilds,
    OpenLogs,
    GitMenu,
    GitSync,
    CopyReport,
}

struct Entry {
    label: String,
    hint:  &'static str,
    cmd:   Cmd,
}

/// True when every word typed appears in the label, in any order.
fn matches(query: &str, label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    query.split_whitespace().all(|w| label.contains(&w.to_ascii_lowercase()))
}

impl DevToolApp {
    pub fn toggle_palette(&mut self) {
        self.palette_open = !self.palette_open;
        self.palette_query.clear();
        self.palette_sel = 0;
    }

    /// Everything that can be done right now. Entries that make no sense in
    /// the current state (Start build while one is running) are left out
    /// rather than shown disabled, so a search never lands on a dead option.
    fn palette_entries(&self) -> Vec<Entry> {
        let mut v = Vec::new();
        let e = |label: &str, hint: &'static str, cmd: Cmd| Entry { label: label.to_string(), hint, cmd };
        let ready = matches!(self.run_state(), RunState::Ready) && !self.is_busy_now();
        let has_project = self.project_path.is_some();

        for (s, label) in [
            (Sheet::Monitor, "Open project monitor"),
            (Sheet::Diagnostics, "Open project setup & checks"),
            (Sheet::Chat, "Open dev assistant"),
            (Sheet::Browser, "Open browser"),
            (Sheet::Extras, "Open extras"),
            (Sheet::Settings, "Open settings"),
        ] {
            v.push(e(label, "", Cmd::Sheet(s)));
        }
        v.push(e("Open the manual", "F1", Cmd::Manual));
        // Not first, so Ctrl+K then Enter can never start a build by accident:
        // starting one closes the Unreal Editor. It has to be typed for.
        if ready {
            v.push(e("Start build", "Ctrl+Enter", Cmd::StartBuild));
        }

        if ready {
            for t in BuildTarget::ALL {
                if t.buildable_on_windows() && t != self.build_target {
                    v.push(Entry { label: format!("Platform: {}", t.label()), hint: "", cmd: Cmd::Target(t) });
                }
            }
            for m in PackageMethod::ALL {
                if m != self.package_method {
                    v.push(Entry { label: format!("Method: {}", m.label()), hint: "", cmd: Cmd::Method(m) });
                }
            }
            for c in [BuildConfiguration::Shipping, BuildConfiguration::Development] {
                if c != self.build_configuration {
                    v.push(Entry { label: format!("Configuration: {}", c.as_str()), hint: "", cmd: Cmd::Config(c) });
                }
            }
            v.push(e(if self.iterate_cook { "Turn iterative cook off" } else { "Turn iterative cook on" },
                "", Cmd::ToggleIterate));
        }
        if has_project {
            v.push(e("Launch Unreal Editor", "", Cmd::LaunchEditor));
            v.push(e("Open project folder", "", Cmd::OpenProject));
            v.push(e("Open editor logs folder", "", Cmd::OpenLogs));
            v.push(e("Open builds folder", "", Cmd::OpenBuilds));
            if self.git_project_dir().is_some() {
                v.push(e("Source control: menu", "", Cmd::GitMenu));
                v.push(e("Source control: sync with main", "", Cmd::GitSync));
            }
        }
        if self.last_build.is_some() {
            v.push(e("Copy last build report", "", Cmd::CopyReport));
        }
        v
    }

    fn run_palette_command(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let save_options = |app: &DevToolApp| {
            if let Some(p) = &app.project_path {
                crate::config::save_uat_options(p, app.compress_pak, &app.extra_uat_args, app.package_method);
                crate::config::save_project_config(p, app.pack_name_input.trim(), app.exe_name_input.trim(),
                    app.build_configuration, app.build_target);
            }
        };
        let dir = self.project_path.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        match cmd {
            Cmd::StartBuild => self.start_packaging(),
            Cmd::Sheet(s) => self.open_sheet(s),
            Cmd::Manual => self.open_guide(),
            Cmd::Target(t) => { self.build_target = t; self.refresh_doctor(); save_options(self); }
            Cmd::Method(m) => { self.package_method = m; save_options(self); }
            Cmd::Config(c) => { self.build_configuration = c; save_options(self); }
            Cmd::ToggleIterate => self.iterate_cook = !self.iterate_cook,
            Cmd::LaunchEditor => self.launch_editor(),
            Cmd::OpenProject => if let Some(d) = dir { let _ = crate::ops::cmd("explorer").arg(d).spawn(); },
            Cmd::OpenLogs => if let Some(d) = dir {
                let logs = d.join("Saved").join("Logs");
                let _ = crate::ops::cmd("explorer").arg(if logs.is_dir() { logs } else { d }).spawn();
            },
            Cmd::OpenBuilds => if let Some(d) = dir {
                let b = d.join("build");
                let _ = crate::ops::cmd("explorer").arg(if b.is_dir() { b } else { d }).spawn();
            },
            Cmd::GitMenu => {
                self.open_git_menu();
                self.sheet = None;
                self.show_git_sheet = true;
            }
            Cmd::GitSync => self.git_start_sync(),
            Cmd::CopyReport => if let Some(out) = self.last_build.clone() {
                let report = self.build_report(&out);
                ctx.copy_text(report);
                self.set_status("Build report copied to the clipboard.".into());
            },
        }
    }

    pub fn show_palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open { return; }

        // Keys are taken before the text box sees them: it would otherwise
        // swallow Enter and use the arrows to move its own cursor.
        let (esc, up, down, enter) = ctx.input_mut(|i| (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        ));
        if esc {
            self.palette_open = false;
            return;
        }

        let all = self.palette_entries();
        let shown: Vec<&Entry> = all.iter()
            .filter(|e| matches(&self.palette_query, &e.label))
            .take(9)
            .collect();
        if !shown.is_empty() {
            if down { self.palette_sel = (self.palette_sel + 1) % shown.len(); }
            if up   { self.palette_sel = (self.palette_sel + shown.len() - 1) % shown.len(); }
            self.palette_sel = self.palette_sel.min(shown.len() - 1);
        }

        let mut run: Option<Cmd> = None;
        if enter && let Some(e) = shown.get(self.palette_sel) {
            run = Some(e.cmd.clone());
        }
        let mut close = false;

        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("palette"))
            .fixed_pos(egui::Pos2::ZERO)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(170));
                if ui.interact(screen, egui::Id::new("palette_backdrop"), egui::Sense::click()).clicked() {
                    close = true;
                }

                let w = 520.0_f32.min(screen.width() - 32.0);
                let top = (screen.height() * 0.16).max(24.0);
                let rect = egui::Rect::from_min_size(
                    egui::pos2(screen.center().x - w / 2.0, top), egui::vec2(w, 10.0));
                let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, screen.max.y - 24.0))));
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(10, 20, 30))
                    .stroke(egui::Stroke::new(1.0, acc(90)))
                    .rounding(egui::Rounding::same(R_SECTION))
                    .inner_margin(egui::Margin::same(14.0))
                    .show(&mut card_ui, |ui| {
                        ui.set_width(w - 28.0);
                        let r = ui.add_sized([ui.available_width(), 32.0],
                            egui::TextEdit::singleline(&mut self.palette_query)
                                .hint_text("Type a command — build, monitor, android, restage…")
                                .font(body(14.0)));
                        r.request_focus();
                        if r.changed() { self.palette_sel = 0; }
                        ui.add_space(8.0);

                        if shown.is_empty() {
                            ui.label(hint("Nothing matches."));
                        }
                        for (i, e) in shown.iter().enumerate() {
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                            if resp.hovered() { self.palette_sel = i; }
                            if i == self.palette_sel {
                                ui.painter().rect_filled(rect, egui::Rounding::same(9.0), acc(30));
                            }
                            ui.painter().text(rect.left_center() + egui::vec2(12.0, 0.0),
                                egui::Align2::LEFT_CENTER, &e.label, body(13.0),
                                if i == self.palette_sel { TEXT } else { SOFT });
                            if !e.hint.is_empty() {
                                ui.painter().text(rect.right_center() - egui::vec2(12.0, 0.0),
                                    egui::Align2::RIGHT_CENTER, e.hint, mono(10.5), DIM);
                            }
                            if resp.clicked() { run = Some(e.cmd.clone()); }
                        }
                        ui.add_space(6.0);
                        ui.label(hint("↑↓ to choose · Enter to run · Esc to close"));
                    });
            });

        if close { self.palette_open = false; }
        if let Some(cmd) = run {
            self.palette_open = false;
            self.run_palette_command(ctx, cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn every_typed_word_must_appear_in_any_order() {
        assert!(matches("", "Start build"));
        assert!(matches("start", "Start build"));
        assert!(matches("build start", "Start build"));
        assert!(matches("ANDROID", "Platform: Android"));
        assert!(!matches("android linux", "Platform: Android"));
        assert!(!matches("zzz", "Start build"));
    }
}
