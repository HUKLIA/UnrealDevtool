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

        card()
            .stroke(egui::Stroke::new(1.0, acc(110)))
            .show(ui, |ui| {
                ui.label(heading("Upload / Copy Packaged Build", 15.0));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Zip: {}", zip_name))
                        .size(10.0).color(MUTED).monospace(),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Local / network path ──────────────────────────────────────
                ui.checkbox(
                    &mut self.upload_use_local,
                    egui::RichText::new("Copy to local / network path").size(12.0).color(TEXT),
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
                            .size(10.0).color(MUTED),
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
                    egui::RichText::new("Upload to Google Drive  (via rclone)").size(12.0).color(TEXT),
                );

                if self.upload_use_gdrive {
                    // rclone is no longer shipped inside this binary (see
                    // `ops::rclone` for why), so the first thing this section
                    // has to answer is whether it is installed at all. Until
                    // it is, the destination field and remote status below are
                    // meaningless, so they are replaced by a single install
                    // prompt rather than shown in a state that cannot work.
                    if !crate::ops::rclone::is_available() {
                        ui.add_space(6.0);
                        Self::show_rclone_setup_guide(ui);
                        ui.add_space(6.0);
                    } else {

                    // Check once (lazily) whether the "gdrive" remote exists —
                    // this reads rclone's local config file, so it's fast.
                    if self.gdrive_remote_status.is_none() {
                        self.gdrive_remote_status = Some(crate::ops::package::gdrive_remote_exists());
                    }

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("rclone destination:").size(11.0).color(MUTED));
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
                                    AMBER,
                                    "Couldn't find a folder ID in that link — paste a folder share link\n  or use rclone path syntax (gdrive:/Builds/MyGame).",
                                );
                            }
                        }
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Uses rclone with a remote named \"gdrive\".\n\
                             Then either:\n\
                             •  Path syntax:   gdrive:/Builds/MobiusFish\n\
                             •  Or paste a folder share link — its folder ID is used automatically:\n\
                                https://drive.google.com/drive/folders/<FOLDER_ID>"
                        ).size(10.0).color(MUTED),
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
                                    AMBER,
                                    "No \"gdrive\" remote found.",
                                );
                                if ui.add_sized([210.0, 26.0], primary("Set up Google Drive remote")).clicked() {
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
                        if ui.add_sized([26.0, 24.0], ghost("↻")).on_hover_text("Re-check remote status").clicked() {
                            self.gdrive_remote_status = None;
                        }
                    });
                    } // end: rclone installed
                }

                ui.add_space(14.0);

                // ── Action buttons ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_go, |ui| {
                        if ui.add_sized([180.0, 32.0], primary("Upload / Copy")).clicked() {
                            action = UploadAction::Upload;
                        }
                    });
                    if ui.add_sized([90.0, 32.0], ghost("Skip")).clicked() {
                        action = UploadAction::Skip;
                    }
                });

                if !can_go {
                    ui.add_space(4.0);
                    ui.colored_label(MUTED, "Check at least one destination above.");
                }
            });

        action
    }

    /// Shown in place of the destination fields when rclone is not installed.
    ///
    /// This app deliberately does not download or install rclone (see
    /// `ops::rclone` for the antivirus reasoning) — it points at the official
    /// download and walks through the one-time setup instead. Both are real
    /// links rather than instructions to go searching.
    fn show_rclone_setup_guide(ui: &mut egui::Ui) {
        // Still bounded, so the guide wraps against whatever it is given
        // rather than measuring its longest step unbounded.
        const FRAME_CHROME: f32 = 22.0; // 10px inner margin each side + stroke
        let inner_w = (ui.available_width() - FRAME_CHROME).max(180.0);
        callout(AMBER).show(ui, |ui| {
            ui.set_max_width(inner_w);
            ui.horizontal(|ui| {
                dot(ui, AMBER, 10.0);
                ui.label(egui::RichText::new("rclone is not installed")
                    .size(11.5).color(AMBER).strong());
            });
            ui.add_space(6.0);
            ui.label(hint(
                "Google Drive uploads are driven by rclone, a separate free tool. \
                 It is a one-time setup and this app never installs it for you.",
            ));

            ui.add_space(10.0);
            if ui.add_sized([210.0, 30.0], primary("Open rclone.org/downloads")).clicked() {
                crate::ops::open_url(crate::ops::rclone::DOWNLOAD_URL);
            }

            ui.add_space(12.0);
            ui.label(eyebrow("ONE-TIME SETUP"));
            ui.add_space(6.0);

            for (n, step) in [
                "Download the Windows AMD64 zip from the page above and unzip it.",
                "Put rclone.exe somewhere permanent — anywhere on your PATH, or \
                 simply next to this app's .exe, which is the easiest option.",
                "Open a terminal and run  rclone config",
                "Choose  n  for a new remote and name it exactly  gdrive",
                "Pick  Google Drive  from the storage list.",
                "Leave client_id and client_secret blank (press Enter twice) unless \
                 you have your own Google Cloud credentials.",
                "Choose scope  1  (full access), accept the remaining defaults, and \
                 say  y  to authorise — a browser opens for you to sign in.",
                "Back here, tick Upload to Google Drive and enter a destination such \
                 as  gdrive:/Builds/MyGame  — or paste a Drive folder share link.",
            ].iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.label(egui::RichText::new(format!("{}.", n + 1))
                        .size(10.5).monospace().color(accent()));
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(hint(step)).wrap());
                });
                ui.add_space(3.0);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_sized([170.0, 26.0], ghost("Google Drive setup docs")).clicked() {
                    crate::ops::open_url(crate::ops::rclone::DRIVE_DOCS_URL);
                }
                ui.add_space(4.0);
                ui.label(hint("Restart this app once rclone.exe is in place."));
            });
        });
    }

    pub fn show_open_folder_panel(&mut self, ui: &mut egui::Ui) {
        let path = self.pending_open_folder_path.clone();
        let display = path.to_string_lossy().to_string();

        card()
            .stroke(egui::Stroke::new(1.0, acc(110)))
            .show(ui, |ui| {
                ui.label(heading("Packaging complete!", 15.0));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Output: {}", display))
                        .size(10.0).color(MUTED).monospace(),
                );
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Open the output folder?").size(12.0).color(TEXT));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([140.0, 32.0], primary("Yes, open")).clicked() {
                        let _ = crate::ops::cmd("explorer").arg(&path).spawn();
                        self.show_open_folder_panel = false;
                        self.show_upload_panel      = true;
                    }
                    if ui.add_sized([140.0, 32.0], ghost("No, skip")).clicked() {
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
        card()
            .stroke(egui::Stroke::new(1.0, tint(AMBER, 110)))
            .show(ui, |ui| {
                ui.colored_label(AMBER, "Google Drive upload failed");
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("See Status / Output below for the exact reason. Upload manually instead:")
                        .size(11.0).color(MUTED),
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
                if ui.add_sized([160.0, 28.0], primary("Retry upload")).clicked() {
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
