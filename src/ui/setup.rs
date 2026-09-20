use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;

impl DevToolApp {
    /// First run: no project set.
    ///
    /// Previously this was the normal Dashboard with an amber "set a project
    /// path" strip on it — every other control present but disabled, which
    /// shows someone a whole tool they cannot use and makes them hunt for the
    /// one field that matters. With nothing set there is exactly one useful
    /// action, so it is the only thing on screen.
    pub fn show_setup_surface(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let content_w = (avail.x - 24.0).clamp(0.0, 560.0);
        ui.allocate_ui_with_layout(
            avail,
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(content_w, avail.y.min(620.0)),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.add_space(20.0);

                        let (mark, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                        ui.painter().rect_filled(mark, egui::Rounding::same(11.0), acc(40));
                        paint_icon(ui.painter(), mark.center(), Icon::Note, accent());

                        ui.add_space(22.0);
                        ui.label(heading("Point me at a project", 28.0));
                        ui.add_space(9.0);
                        ui.label(hint("Pick a .uproject file. The engine is found from it automatically."));

                        ui.add_space(26.0);
                        card().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.label(eyebrow("PROJECT FILE"));
                                ui.add_space(10.0);
                                let narrow = ui.available_width() < 300.0;
                                if narrow {
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 38.0],
                                        egui::TextEdit::singleline(&mut self.project_path_input)
                                            .hint_text("Paste a .uproject path…")
                                            .font(mono(12.5)),
                                    );
                                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        self.try_apply_typed_path();
                                    }
                                    let browse = ui.add_sized([ui.available_width(), 38.0], primary("Browse…"));
                                    self.guide_anchor(crate::ui::guide::step::PROJECT, browse.rect);
                                    if browse.clicked() { self.choose_project(); }
                                } else {
                                    ui.horizontal(|ui| {
                                        let bw = 104.0;
                                        let fw = (ui.available_width() - bw - ui.spacing().item_spacing.x).max(0.0);
                                        let resp = ui.add_sized(
                                            [fw, 38.0],
                                            egui::TextEdit::singleline(&mut self.project_path_input)
                                                .hint_text("Paste a .uproject path…")
                                                .font(mono(12.5)),
                                        );
                                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                            self.try_apply_typed_path();
                                        }
                                        let browse = ui.add_sized([bw, 38.0], primary("Browse…"));
                                        self.guide_anchor(crate::ui::guide::step::PROJECT, browse.rect);
                                        if browse.clicked() { self.choose_project(); }
                                    });
                                }

                                ui.add_space(18.0);
                                crate::ui::run::divider(ui);
                                ui.add_space(14.0);

                                ui.label(eyebrow("RECENT"));
                                ui.add_space(9.0);
                                let recents = crate::config::load_recent_projects();
                                if recents.is_empty() {
                                    ui.label(hint("Nothing yet — the projects you open will be listed here."));
                                }
                                let mut pick: Option<std::path::PathBuf> = None;
                                for (i, p) in recents.iter().enumerate() {
                                    let exists = p.is_file();
                                    let name = p.file_stem()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| p.display().to_string());
                                    if recent_row(ui, &name, p, exists, i == 0) && exists {
                                        pick = Some(p.clone());
                                    }
                                    ui.add_space(3.0);
                                }
                                if let Some(p) = pick {
                                    self.apply_project_path(p);
                                }
                            });
                        });

                        ui.add_space(14.0);
                        callout(AMBER).show(ui, |ui| {
                            // Match the card above without exceeding a narrow
                            // window, where a fixed width used to clip text.
                            ui.set_min_width(ui.available_width());
                            ui.set_max_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                dot(ui, AMBER, 9.0);
                                ui.add_space(2.0);
                                ui.add(egui::Label::new(
                                    egui::RichText::new(
                                        "Avoid paths with spaces — Unreal's build scripts break on them. \
                                         The tool can link around it for you.")
                                        .font(body(11.5)).color(egui::Color32::from_rgb(217, 192, 151)),
                                ).wrap());
                            });
                        });
                    },
                );
            },
        );
    }
}

/// One recent-project row. A missing file still shows, greyed — silently
/// hiding it looks like the app forgot the project rather than the folder
/// having moved.
fn recent_row(ui: &mut egui::Ui, name: &str, path: &std::path::Path, exists: bool, first: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 36.0), egui::Sense::click());
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && exists, T_FAST);

    let base = if first && exists { acc(16) } else { egui::Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, egui::Rounding::same(10.0), mix(base, acc(30), hov));

    let c = if exists { GREEN } else { RED };
    let dot_c = rect.left_center() + egui::vec2(13.0, 0.0);
    ui.painter().circle_filled(dot_c, 3.5, c);

    let p = ui.painter();
    p.text(rect.left_center() + egui::vec2(26.0, 0.0), egui::Align2::LEFT_CENTER,
           name, body(13.0), if exists { TEXT } else { DIM });
    let tail = if exists {
        path.parent().and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    } else {
        "missing".into()
    };
    p.text(rect.right_center() - egui::vec2(12.0, 0.0), egui::Align2::RIGHT_CENTER,
           &tail, mono(11.5), DIM);

    if hov > 0.0 && hov < 1.0 { ui.ctx().request_repaint(); }
    resp.clicked()
}
