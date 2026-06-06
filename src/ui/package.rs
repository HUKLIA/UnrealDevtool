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

                // ── Google Drive ──────────────────────────────────────────────
                ui.checkbox(
                    &mut self.upload_use_gdrive,
                    egui::RichText::new("Upload to Google Drive").size(12.0).color(egui::Color32::WHITE),
                );

                if self.upload_use_gdrive {
                    ui.add_space(4.0);

                    // ── Connected account ─────────────────────────────────────
                    if self.upload_gdrive_user_email.is_empty() {
                        ui.horizontal(|ui| {
                            ui.colored_label(HINT_GRAY, "●");
                            ui.label(
                                egui::RichText::new("Not signed in — browser will open when you upload.")
                                    .size(11.0).color(HINT_GRAY),
                            );
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.colored_label(MIKU_TEAL, "●");
                            ui.label(
                                egui::RichText::new(format!("Signed in as: {}", self.upload_gdrive_user_email))
                                    .size(11.0).color(egui::Color32::WHITE),
                            );
                            if ui.add_sized([70.0, 20.0], egui::Button::new("Sign Out")).clicked() {
                                action = UploadAction::SignOut;
                            }
                        });
                    }
                    ui.add_space(6.0);

                    // ── Folder ID ─────────────────────────────────────────────
                    ui.label(egui::RichText::new("Drive Folder ID:").size(11.0).color(egui::Color32::GRAY));
                    let folder_display = if self.upload_gdrive_folder_id.is_empty() {
                        "not set".to_string()
                    } else {
                        self.upload_gdrive_folder_id.clone()
                    };
                    ui.label(egui::RichText::new(format!("Current: {}", folder_display)).size(10.0).color(HINT_GRAY));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.upload_gdrive_folder_id)
                            .hint_text("Paste folder ID or full Drive folder URL…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);

                    // ── client_secret.json (collapsible) ─────────────────────
                    egui::CollapsingHeader::new(
                        egui::RichText::new("▸ client_secret.json  (one-time setup)").size(11.0).color(MIKU_TEAL)
                    )
                    .id_source("gdrive_creds")
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "1. console.cloud.google.com → APIs & Services → Credentials\n\
                                 2. Create OAuth 2.0 Client ID  →  Application type: Desktop app\n\
                                 3. Download JSON → rename to client_secret.json\n\
                                 4. Browse to it below.\n\
                                 On first upload the browser opens for Google sign-in.\n\
                                 Your session is saved in tokencache.json — no browser next time."
                            ).size(10.0).color(HINT_GRAY),
                        );
                        ui.add_space(6.0);

                        let path_set  = !self.upload_gdrive_secret_path.is_empty();
                        let path_exists = path_set && std::path::Path::new(&self.upload_gdrive_secret_path).exists();
                        let status_text = if path_exists {
                            "✓ found"
                        } else if path_set {
                            "✗ file not found"
                        } else {
                            "not set"
                        };
                        let status_color = if path_exists { MIKU_TEAL } else { crate::theme::WARN_AMBER };

                        ui.label(egui::RichText::new("client_secret.json:").size(11.0).color(egui::Color32::GRAY));
                        ui.horizontal(|ui| {
                            ui.colored_label(status_color, format!("Current: {}", status_text));
                        });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.upload_gdrive_secret_path)
                                    .hint_text("Paste path or Browse…")
                                    .desired_width(ui.available_width() - 86.0),
                            );
                            if ui.add_sized([80.0, 22.0], egui::Button::new("Browse…")).clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("JSON credentials", &["json"])
                                    .set_title("Select client_secret.json")
                                    .pick_file()
                                {
                                    self.upload_gdrive_secret_path = p.to_string_lossy().to_string();
                                }
                            }
                        });
                    });
                }

                ui.add_space(14.0);

                // ── Action buttons ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_go, |ui| {
                        if ui.add_sized([180.0, 32.0], egui::Button::new("▶  Upload / Copy")).clicked() {
                            action = UploadAction::Upload;
                        }
                    });
                    if ui.add_sized([80.0, 32.0], egui::Button::new("✕  Skip")).clicked() {
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
