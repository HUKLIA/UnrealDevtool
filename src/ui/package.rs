use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::UploadAction;
use rfd;

impl DevToolApp {
    pub fn show_upload_panel_ui(&mut self, ui: &mut egui::Ui) -> UploadAction {
        let mut action = UploadAction::None;

        let zip_name = self.upload_zip_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.upload_zip_path.display().to_string());

        let can_go = self.upload_use_local || self.upload_use_gdrive;

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, MIKU_TEAL))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📤  Upload / Copy Packaged Build").size(13.0).color(MIKU_TEAL));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Zip: {}", zip_name))
                        .size(10.0).color(HINT_GRAY).monospace(),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Local / network path ──────────────────────────────────────
                ui.checkbox(
                    &mut self.upload_use_local,
                    egui::RichText::new("Copy to local / network path").size(12.0).color(egui::Color32::WHITE),
                );

                if self.upload_use_local {
                    ui.add_space(4.0);
                    let current = if self.upload_local_path.is_empty() {
                        "not set".to_string()
                    } else {
                        self.upload_local_path.clone()
                    };
                    ui.label(
                        egui::RichText::new(format!("Current: {}", current))
                            .size(10.0).color(HINT_GRAY),
                    );
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.upload_local_path)
                                .hint_text("Paste path or Browse…")
                                .desired_width(ui.available_width() - 86.0),
                        );
                        if ui.add_sized([80.0, 22.0], egui::Button::new("Browse…")).clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_title("Select destination folder")
                                .pick_folder()
                            {
                                self.upload_local_path = p.to_string_lossy().to_string();
                            }
                        }
                    });
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Google Drive via rclone ───────────────────────────────────
                ui.checkbox(
                    &mut self.upload_use_gdrive,
                    egui::RichText::new("Upload to Google Drive  (via rclone)").size(12.0).color(egui::Color32::WHITE),
                );

                if self.upload_use_gdrive {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("rclone destination:").size(11.0).color(egui::Color32::GRAY));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.upload_rclone_dest)
                            .hint_text("gdrive:/Builds/MyGame")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Requires rclone in PATH with a configured remote.\n\
                             Run  rclone config  in PowerShell to set one up.\n\
                             Example:  gdrive:/Builds/MobiusFish"
                        ).size(10.0).color(HINT_GRAY),
                    );
                }

                ui.add_space(14.0);

                // ── Action buttons ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_go, |ui| {
                        if ui.add_sized([180.0, 32.0], egui::Button::new(">>  Upload / Copy")).clicked() {
                            action = UploadAction::Upload;
                        }
                    });
                    if ui.add_sized([80.0, 32.0], egui::Button::new("x  Skip")).clicked() {
                        action = UploadAction::Skip;
                    }
                });

                if !can_go {
                    ui.add_space(4.0);
                    ui.colored_label(HINT_GRAY, "Check at least one destination above.");
                }
            });

        action
    }

    pub fn show_package_config_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut do_start = false;
        let version_label = crate::ops::package::format_version(self.next_version_preview);
        let pack_preview  = format!(
            "-> build/{}/{}/   and   {}_{}.zip",
            version_label,
            self.pack_name_input.trim(),
            self.pack_name_input.trim(),
            version_label,
        );
        let exe_preview = format!("-> {}.exe", self.exe_name_input.trim());
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
                        if ui.add_sized([190.0, 32.0], egui::Button::new(">>  Start Packaging")).clicked() {
                            do_start = true;
                        }
                    });
                    if ui.add_sized([90.0, 32.0], egui::Button::new("x  Cancel")).clicked() {
                        self.show_package_config = false;
                    }
                });
            });
        do_start
    }

    pub fn show_open_folder_panel(&mut self, ui: &mut egui::Ui) {
        let path = self.pending_open_folder_path.clone();
        let display = path.to_string_lossy().to_string();

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, MIKU_TEAL))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📁  Packaging complete!").size(13.0).color(MIKU_TEAL));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Output: {}", display))
                        .size(10.0).color(HINT_GRAY).monospace(),
                );
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Open the output folder?").size(12.0).color(egui::Color32::WHITE));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([140.0, 32.0], egui::Button::new("📂  Yes, open")).clicked() {
                        let _ = crate::ops::cmd("explorer").arg(&path).spawn();
                        self.show_open_folder_panel = false;
                        self.show_upload_panel      = true;
                    }
                    if ui.add_sized([140.0, 32.0], egui::Button::new("—  No, skip")).clicked() {
                        self.show_open_folder_panel = false;
                        self.show_upload_panel      = true;
                    }
                });
            });
    }
}
