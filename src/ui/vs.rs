use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::IdeChoice;

impl DevToolApp {
    pub fn show_vs_config_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut do_start = false;

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("🔧  Rebuild Project Files").size(13.0).color(accent()));
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Will clean from project folder:").size(11.0).color(egui::Color32::GRAY));
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(18, 18, 26))
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for item in &["Binaries/","Intermediate/","Saved/",".idea/",".vs/","DerivedDataCache/","*.sln"] {
                                ui.label(egui::RichText::new(format!("- {}", item))
                                    .size(11.0).color(egui::Color32::from_rgb(200, 100, 80)));
                            }
                        });
                    });
                ui.add_space(10.0);

                ui.label(egui::RichText::new("Open with after generation:").size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (choice, label) in [
                        (IdeChoice::Rider,        "🚀  Rider"),
                        (IdeChoice::VisualStudio, "🖥  Visual Studio"),
                        (IdeChoice::SkipOpen,     "x  Don't open"),
                    ] {
                        let selected = self.ide_choice == choice;
                        let btn = egui::Button::new(
                            egui::RichText::new(label)
                                .color(if selected { DARK_BG } else { egui::Color32::LIGHT_GRAY }),
                        )
                        .fill(if selected { accent() } else { PANEL_BG });
                        if ui.add_sized([110.0, 30.0], btn).clicked() {
                            self.ide_choice = choice;
                        }
                        ui.add_space(4.0);
                    }
                });
                ui.add_space(12.0);

                self.show_space_warning_inline(ui);

                ui.horizontal(|ui| {
                    if ui.add_sized([190.0, 32.0], egui::Button::new(">>  Confirm & Rebuild")).clicked() {
                        do_start = true;
                    }
                    if ui.add_sized([90.0, 32.0], egui::Button::new("x  Cancel")).clicked() {
                        self.show_vs_config = false;
                    }
                });
            });
        do_start
    }
}
