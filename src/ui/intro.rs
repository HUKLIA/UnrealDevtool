use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;

impl DevToolApp {
    /// Full-window boot splash shown once at launch: reveals `intro_log`
    /// line by line, then an "Enter" button once done. Purely cosmetic —
    /// the real detection it narrates already happened in `new()`.
    pub fn show_intro_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.tick_intro(ctx);

        ui.add_space(ui.available_height() * 0.12);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("UNREAL DEVTOOL")
                    .size(22.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            );
            ui.label(
                egui::RichText::new("STUDY & RESEARCH PROJECT")
                    .size(10.0)
                    .color(HINT_GRAY),
            );
            ui.add_space(16.0);

            let box_width = (ui.available_width() - 24.0).min(460.0);
            ui.allocate_ui_with_layout(
                egui::vec2(box_width, 250.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    card_frame().show(ui, |ui| {
                        ui.set_min_size(egui::vec2(box_width - 28.0, 220.0));

                        let progress = self.intro_revealed as f32 / self.intro_log.len().max(1) as f32;
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .desired_width(ui.available_width())
                                .fill(accent())
                                .show_percentage(),
                        );
                        ui.add_space(8.0);

                        // `build_intro_log` never produces more than 7 lines, so
                        // this is sized to fit all of them at once — with
                        // `stick_to_bottom`, a shorter box would scroll such
                        // that the topmost line sits half-clipped by the
                        // viewport edge instead of fully visible (that's what
                        // was happening before this was widened: line [1]
                        // rendered as a sliver overlapping the progress bar).
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for (i, line) in self.intro_log.iter().take(self.intro_revealed).enumerate() {
                                    let is_warning = line.starts_with("WARNING");
                                    let color = if is_warning { WARN_AMBER } else if i == self.intro_log.len() - 1 {
                                        accent()
                                    } else {
                                        egui::Color32::LIGHT_GRAY
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("[{}]", i + 1))
                                                .monospace().size(10.0).color(HINT_GRAY),
                                        );
                                        ui.label(egui::RichText::new(line).monospace().size(10.5).color(color));
                                    });
                                }
                            });
                    });
                },
            );

            ui.add_space(20.0);

            if self.intro_done {
                if ui.add_sized([180.0, 34.0], egui::Button::new(
                    egui::RichText::new("OPEN DEVTOOL").strong(),
                )).clicked() {
                    self.show_intro = false;
                }
            } else {
                ui.label(egui::RichText::new("DETECTING LOCAL SDKs...").size(10.0).color(HINT_GRAY));
            }
        });
    }
}
