use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::preflight::CheckStatus;
use crate::theme::*;

impl DevToolApp {
    pub fn show_app_check_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("⚙  App Self-Check").size(13.0).color(accent()));
                ui.label(
                    egui::RichText::new("The DevTool app's own install/config/update health — \
                                          see Check PC Setup for your Unreal project/engine.")
                        .size(10.0).color(HINT_GRAY),
                );
                ui.add_space(10.0);

                let mut has_leftover_binary = false;
                for item in &self.app_check_items {
                    if item.label == "Leftover update file" && matches!(item.status, CheckStatus::Warn) {
                        has_leftover_binary = true;
                    }
                    Self::show_check_item(ui, item);
                }

                if has_leftover_binary
                    && ui.add_sized([180.0, 26.0], egui::Button::new("🗑  Clean up now")).clicked() {
                        self.cleanup_leftover_binary_now();
                    }
                ui.add_space(6.0);

                // GitHub reachability runs on a background thread (network
                // call) — show a pending state until it reports back.
                match &*self.app_check_github.lock().unwrap_or_else(|e| e.into_inner()) {
                    Some(item) => Self::show_check_item(ui, item),
                    None => {
                        ui.colored_label(HINT_GRAY, "[..]  GitHub connectivity");
                        ui.label(egui::RichText::new("Checking…").size(10.5).color(HINT_GRAY));
                        ui.add_space(6.0);
                        ui.ctx().request_repaint();
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([100.0, 28.0], egui::Button::new("↻  Refresh")).clicked() {
                        self.refresh_app_check();
                    }
                    if ui.add_sized([100.0, 28.0], egui::Button::new("< Back")).clicked() {
                        self.extras_tab = crate::types::ExtrasTab::Miku;
                    }
                });
            });
    }
}
