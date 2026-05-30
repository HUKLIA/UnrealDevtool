use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;

impl DevToolApp {
    pub fn show_package_config_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut do_start = false;
        let version_label = format!("v0.0.{}", self.next_version_preview);
        let pack_preview  = format!(
            "→ build/{}/{}/   and   {}_{}.zip",
            version_label,
            self.pack_name_input.trim(),
            self.pack_name_input.trim(),
            version_label,
        );
        let exe_preview = format!("→ {}.exe", self.exe_name_input.trim());
        let can_start   = !self.pack_name_input.trim().is_empty()
                       && !self.exe_name_input.trim().is_empty();

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, MIKU_TEAL))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📦  Package Configuration").size(13.0).color(MIKU_TEAL));
                ui.add_space(10.0);

                ui.label(egui::RichText::new("Package / folder name:").size(11.0).color(egui::Color32::GRAY));
                ui.add(egui::TextEdit::singleline(&mut self.pack_name_input).desired_width(f32::INFINITY));
                ui.label(egui::RichText::new(&pack_preview).size(10.0).color(HINT_GRAY));
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Executable name  (.exe):").size(11.0).color(egui::Color32::GRAY));
                ui.add(egui::TextEdit::singleline(&mut self.exe_name_input).desired_width(f32::INFINITY));
                ui.label(egui::RichText::new(&exe_preview).size(10.0).color(HINT_GRAY));
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Version:").size(11.0).color(egui::Color32::GRAY));
                    ui.colored_label(MIKU_TEAL, &version_label);
                    ui.label(egui::RichText::new("(auto-incremented)").size(10.0).color(HINT_GRAY));
                });
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        if ui.add_sized([190.0, 32.0], egui::Button::new("▶  Start Packaging")).clicked() {
                            do_start = true;
                        }
                    });
                    if ui.add_sized([90.0, 32.0], egui::Button::new("✕  Cancel")).clicked() {
                        self.show_package_config = false;
                    }
                });
            });
        do_start
    }
}
