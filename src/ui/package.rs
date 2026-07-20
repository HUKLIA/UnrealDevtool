use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::UploadAction;

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
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📤  Upload / Copy Packaged Build").size(13.0).color(accent()));
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
                        if ui.add_sized([80.0, 22.0], egui::Button::new("Browse…")).clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .set_title("Select destination folder")
                                .pick_folder()
                            {
                                self.upload_local_path = p.to_string_lossy().to_string();
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
                    // Check once (lazily) whether the "gdrive" remote exists —
                    // this reads rclone's local config file, so it's fast.
                    if self.gdrive_remote_status.is_none() {
                        self.gdrive_remote_status = Some(crate::ops::package::gdrive_remote_exists());
                    }

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("rclone destination:").size(11.0).color(egui::Color32::GRAY));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.upload_rclone_dest)
                            .hint_text("gdrive:/Builds/MyGame  or a Drive folder share link")
                            .desired_width(f32::INFINITY),
                    );

                    // Live feedback when the pasted value is a Drive share link —
                    // shows the folder ID that will actually be targeted on upload.
                    let dest_trim = self.upload_rclone_dest.trim();
                    if dest_trim.starts_with("http://") || dest_trim.starts_with("https://") {
                        match crate::ops::package::drive_folder_id_from_url(dest_trim) {
                            Some(id) => {
                                ui.colored_label(
                                    accent(),
                                    format!("✓ Folder link recognized — will upload into folder ID: {}", id),
                                );
                            }
                            None => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 150, 60),
                                    "⚠ Couldn't find a folder ID in that link — paste a folder share link\n  or use rclone path syntax (gdrive:/Builds/MyGame).",
                                );
                            }
                        }
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Uses the bundled rclone (or one in PATH) with a remote named \"gdrive\".\n\
                             Then either:\n\
                             •  Path syntax:   gdrive:/Builds/MobiusFish\n\
                             •  Or paste a folder share link — its folder ID is used automatically:\n\
                                https://drive.google.com/drive/folders/<FOLDER_ID>"
                        ).size(10.0).color(HINT_GRAY),
                    );

                    // Remote status + one-click setup for first-time users.
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        match self.gdrive_remote_status {
                            Some(true) => {
                                ui.colored_label(accent(), "✓ \"gdrive\" remote is configured.");
                            }
                            Some(false) => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 150, 60),
                                    "⚠ No \"gdrive\" remote found.",
                                );
                                if ui.add_sized([190.0, 24.0], egui::Button::new("⚙  Set up Google Drive remote…")).clicked() {
                                    match crate::ops::package::open_rclone_config_setup() {
                                        Ok(())   => *self.status_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                            "[INFO] Opened rclone config in a new window — \
                                             create a remote named \"gdrive\" and sign in via the browser prompt.\n\
                                             Come back here and click ↻ to refresh once you're done.".to_string(),
                                        Err(e)   => *self.status_message.lock().unwrap_or_else(|e| e.into_inner()) =
                                            format!("[ERROR] Could not open rclone config: {}", e),
                                    }
                                }
                            }
                            None => {}
                        }
                        if ui.add_sized([26.0, 24.0], egui::Button::new("↻")).on_hover_text("Re-check remote status").clicked() {
                            self.gdrive_remote_status = None;
                        }
                    });
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

    /// Returns `Some(false)` = start normal, `Some(true)` = start fast, `None` = no action.
    pub fn show_package_config_panel(&mut self, ui: &mut egui::Ui) -> Option<bool> {
        let mut action: Option<bool> = None;
        let auto_version_label = crate::ops::package::format_version(self.next_version_preview);
        let version_label = if self.use_custom_version {
            self.version_override.trim().to_string()
        } else {
            auto_version_label.clone()
        };
        let version_valid = !version_label.is_empty()
            && !version_label.chars().any(|c| "\\/:*?\"<>|".contains(c));
        let pack_preview  = format!(
            "-> build/{}/{}/   and   {}_{}.zip",
            version_label,
            self.pack_name_input.trim(),
            self.pack_name_input.trim(),
            version_label,
        );
        let exe_preview = format!("-> {}.exe", self.exe_name_input.trim());
        let can_start   = !self.pack_name_input.trim().is_empty()
                       && !self.exe_name_input.trim().is_empty()
                       && version_valid;

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📦  Package Configuration").size(13.0).color(accent()));
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
                    if self.use_custom_version {
                        ui.add(egui::TextEdit::singleline(&mut self.version_override).desired_width(80.0));
                    } else {
                        ui.colored_label(accent(), &auto_version_label);
                        ui.label(egui::RichText::new("(auto-incremented)").size(10.0).color(HINT_GRAY));
                    }
                    if ui.checkbox(&mut self.use_custom_version, "Custom").changed()
                        && self.use_custom_version
                        && self.version_override.trim().is_empty()
                    {
                        self.version_override = auto_version_label.clone();
                    }
                });
                if self.use_custom_version && !version_valid {
                    ui.label(
                        egui::RichText::new("Version cannot be empty or contain \\ / : * ? \" < > |")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(220, 100, 100)),
                    );
                }
                ui.add_space(12.0);

                self.show_space_warning_inline(ui);

                // ── Editor-open warning ───────────────────────────────────────
                if self.editor_is_running {
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(45, 35, 15))
                        .stroke(egui::Stroke::new(1.0, WARN_AMBER))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.colored_label(WARN_AMBER, "⚠  Unreal Editor is open");
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "Packaging with the editor open is possible, but may fail if:\n\
                                     •  You have unsaved assets (they won't be included in the build)\n\
                                     •  Live Coding or auto-compile is active (write conflict on Intermediate/)\n\n\
                                     Save all your work first (Ctrl+S in the editor), then choose below."
                                ).size(10.5).color(egui::Color32::from_rgb(210, 190, 140)),
                            );
                            ui.add_space(6.0);
                            ui.checkbox(
                                &mut self.close_editor_before_package,
                                egui::RichText::new("Close the editor automatically before packaging  (recommended)")
                                    .size(11.5)
                                    .color(egui::Color32::WHITE),
                            );
                            if !self.close_editor_before_package {
                                ui.add_space(2.0);
                                ui.colored_label(
                                    WARN_AMBER,
                                    "The editor will stay open. Save everything before starting.",
                                );
                            }
                        });
                    ui.add_space(8.0);
                }

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        if ui.add_sized([150.0, 32.0], egui::Button::new(">>  Start Packaging")).clicked() {
                            action = Some(false);
                        }
                        if ui.add_sized([110.0, 32.0], egui::Button::new("⚡  Fast Package")).clicked() {
                            action = Some(true);
                        }
                    });
                    if ui.add_sized([90.0, 32.0], egui::Button::new("x  Cancel")).clicked() {
                        self.show_package_config = false;
                    }
                });
            });
        action
    }

    pub fn show_open_folder_panel(&mut self, ui: &mut egui::Ui) {
        let path = self.pending_open_folder_path.clone();
        let display = path.to_string_lossy().to_string();

        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("📁  Packaging complete!").size(13.0).color(accent()));
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

    /// Shown when a Google Drive upload attempt fails (bad/expired auth, no
    /// remote configured, network blocked, etc.) — offers a manual fallback
    /// instead of leaving the user with only an error string to puzzle over.
    pub fn show_upload_fallback_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, WARN_AMBER))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.colored_label(WARN_AMBER, "⚠  Google Drive upload failed");
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("See Status / Output below for the exact reason. Upload manually instead:")
                        .size(11.0).color(HINT_GRAY),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([170.0, 30.0], egui::Button::new("📂  Open build folder")).clicked()
                        && let Some(folder) = self.upload_zip_path.parent() {
                            let _ = crate::ops::cmd("explorer").arg(folder).spawn();
                        }
                    if ui.add_sized([170.0, 30.0], egui::Button::new("🌐  Open Google Drive")).clicked() {
                        crate::ops::open_url("https://drive.google.com/drive/my-drive");
                    }
                });
                ui.add_space(8.0);
                if ui.add_sized([160.0, 26.0], egui::Button::new("↻  Retry upload")).clicked() {
                    self.show_upload_fallback_panel = false;
                    self.show_upload_panel          = true;
                }
                ui.add_space(4.0);
                if ui.add_sized([100.0, 26.0], egui::Button::new("< Back")).clicked() {
                    self.show_upload_fallback_panel = false;
                }
            });
    }
}
