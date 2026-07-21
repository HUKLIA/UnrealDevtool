use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;

impl DevToolApp {
    /// Dashboard tab: project/engine setup, Rebuild VS Files, and inline
    /// preflight diagnostics (merges what used to be the separate "Check PC
    /// Setup" overlay — matches the reference mockup's single-screen
    /// dashboard instead of a dedicated button+panel).
    pub fn show_dashboard_tab(&mut self, ui: &mut egui::Ui) {
        if self.show_vs_config {
            let go = self.show_vs_config_panel(ui);
            if go { self.start_vs_rebuild(); }
            return;
        }

        card_frame().show(ui, |ui| {
            self.show_project_path_row(ui);
            ui.add_space(10.0);
            self.show_engine_path_row(ui);
        });
        ui.add_space(10.0);

        let have_project = self.project_path.is_some();
        ui.add_enabled_ui(have_project, |ui| {
            if ui.add_sized([ui.available_width(), 34.0], egui::Button::new("🔧  Rebuild Visual Studio Files")).clicked() {
                self.open_vs_config();
            }
        });
        if !have_project {
            ui.add_space(4.0);
            ui.colored_label(WARN_AMBER, "(!)  Set a project path above to enable build actions.");
        }
        ui.add_space(10.0);

        card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("🔍  PREFLIGHT DIAGNOSTICS").size(11.0).color(HINT_GRAY));
            ui.add_space(8.0);
            self.show_pc_check_content(ui);
        });
        ui.add_space(10.0);

        self.show_paste_log_scanner(ui);
    }

    /// Paste-a-log-excerpt scanner — a small feature gap vs. the reference
    /// mockup, which only supports pasting (no on-disk auto-scan); this adds
    /// that alongside the existing auto-scan-from-disk behavior rather than
    /// replacing it.
    fn show_paste_log_scanner(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("📋  PASTE A LOG TO SCAN").size(11.0).color(HINT_GRAY));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "For a log that isn't the most recent one on disk — paste any excerpt below."
                ).size(10.0).color(HINT_GRAY),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.pasted_log_input)
                    .desired_rows(3)
                    .desired_width(ui.available_width())
                    .hint_text("e.g. 'C:\\Program' is not recognized as an internal or external command..."),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can_scan = !self.pasted_log_input.trim().is_empty();
                ui.add_enabled_ui(can_scan, |ui| {
                    if ui.add_sized([120.0, 26.0], egui::Button::new("Analyze")).clicked() {
                        self.scan_pasted_log();
                    }
                });
                if !self.pasted_log_diagnosis.is_empty()
                    && ui.add_sized([80.0, 26.0], egui::Button::new("Clear")).clicked() {
                        self.pasted_log_input.clear();
                        self.pasted_log_diagnosis.clear();
                    }
            });

            if !self.pasted_log_diagnosis.is_empty() {
                ui.add_space(8.0);
                for d in &self.pasted_log_diagnosis {
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(45, 35, 15))
                        .stroke(egui::Stroke::new(1.0, WARN_AMBER))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.colored_label(WARN_AMBER, format!("⚠  {}", d.matched));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(&d.explanation).size(10.5).color(egui::Color32::from_rgb(210, 190, 140)));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(format!("Fix: {}", d.fix)).size(10.5).color(accent()));
                        });
                    ui.add_space(6.0);
                }
            }
        });
    }
}
