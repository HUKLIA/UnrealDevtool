mod git;
mod package;
mod vs;

use eframe::egui;
use crate::app::DevToolApp;
use crate::config::{clear_project_path, save_project_path};
use crate::theme::*;
use crate::types::{GitAction, GitState, GitTaskStatus, UploadAction};
use std::sync::atomic::Ordering;

// ── eframe::App — the main update loop ───────────────────────────────────────

impl eframe::App for DevToolApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_busy = *self.is_working.lock().unwrap();
        self.status_display = self.status_message.lock().unwrap().clone();

        // Detect background-task completion and advance git state machine
        let just_finished = self.was_working && !is_busy;
        self.was_working = is_busy;
        if just_finished {
            let git_status = self.git_result.lock().unwrap().take();
            if let Some(gs) = git_status {
                match gs {
                    GitTaskStatus::Ok => {
                        self.git_state = self.git_next_state.clone();
                    }
                    GitTaskStatus::Conflict | GitTaskStatus::Error => {
                        self.git_state = GitState::Idle;
                    }
                }
                self.git_next_state = GitState::Idle;
            }

            // If packaging produced a zip, open the upload panel
            if let Some(zip) = self.pending_zip.lock().unwrap().take() {
                self.upload_zip_path   = zip;
                self.upload_use_local  = false;
                self.upload_use_gdrive = false;
                self.show_upload_panel = true;
            }

            // Refresh the signed-in Google account display after every task
            // (covers the case where an upload just completed and saved the email)
            self.reload_gdrive_user();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading(egui::RichText::new("Unreal Master Toolbox").color(egui::Color32::WHITE));
            ui.separator();
            ui.add_space(6.0);

            if is_busy {
                self.show_busy_view(ui, ctx);
            } else {
                self.show_idle_view(ui);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            self.show_status_area(ui);
        });
    }
}

// ── Shared UI methods ─────────────────────────────────────────────────────────

impl DevToolApp {
    pub fn show_busy_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(p) = &self.project_path {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            ui.horizontal(|ui| {
                ui.colored_label(MIKU_TEAL, "●");
                ui.label(egui::RichText::new(name.as_ref()).color(egui::Color32::LIGHT_GRAY));
            });
            ui.add_space(6.0);
        }
        let dt = ctx.input(|i| i.stable_dt);
        if let Some(gif) = &mut self.gif_player { gif.advance(ctx, dt); }

        let gif_size = egui::vec2(300.0, 252.0);
        egui::Frame::none()
            .fill(GIF_BG)
            .stroke(egui::Stroke::new(1.5, MIKU_TEAL))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(gif) = &self.gif_player {
                        gif.show(ui, gif_size);
                    } else {
                        ui.add_space(gif_size.y);
                        ui.colored_label(MIKU_TEAL, "[ working… ]");
                    }
                });
            });

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(&self.busy_label).size(15.0).color(MIKU_TEAL));
            ui.add_space(6.0);

            let prog = *self.progress.lock().unwrap();
            ui.add(
                egui::ProgressBar::new(prog)
                    .desired_width(ui.available_width().min(340.0))
                    .fill(MIKU_TEAL)
                    .show_percentage(),
            );

            ui.add_space(4.0);
            ui.label(egui::RichText::new("see Status / Output below for progress")
                .size(11.0).color(HINT_GRAY));
            ui.add_space(8.0);
            if ui.add_sized([110.0, 26.0], egui::Button::new("Cancel")).clicked() {
                self.cancel_flag.store(true, Ordering::Relaxed);
                self.set_status("[CANCELLING] Stopping — please wait…".into());
            }
        });
        ui.add_space(8.0);
    }

    pub fn show_idle_view(&mut self, ui: &mut egui::Ui) {
        self.show_project_path_row(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        if self.show_upload_panel {
            let action = self.show_upload_panel_ui(ui);
            match action {
                UploadAction::Upload  => self.start_upload(),
                UploadAction::Skip    => { self.show_upload_panel = false; }
                UploadAction::SignOut => self.gdrive_sign_out(),
                UploadAction::None    => {}
            }

        } else if self.show_package_config {
            let go = self.show_package_config_panel(ui);
            if go { self.start_packaging(); }

        } else if self.show_vs_config {
            let go = self.show_vs_config_panel(ui);
            if go { self.start_vs_rebuild(); }

        } else if !matches!(self.git_state, GitState::Idle) {
            let action = self.show_git_panel(ui);
            match action {
                GitAction::StartCommitPush          => self.git_start_commit_push(),
                GitAction::StartSync                => self.git_start_sync(),
                GitAction::StartMerge               => self.git_start_merge(),
                GitAction::StartCheckout { branch } => self.git_start_checkout(branch),
                GitAction::StartNewBranch { name }  => self.git_start_new_branch(name),
                GitAction::None                     => {}
            }
        } else {
            self.show_action_buttons(ui);
        }
    }

    pub fn show_action_buttons(&mut self, ui: &mut egui::Ui) {
        let have_project = self.project_path.is_some();
        ui.add_enabled_ui(have_project, |ui| {
            let w = [ui.available_width(), 40.0];
            if ui.add_sized(w, egui::Button::new("🔧  Rebuild Visual Studio Files")).clicked() {
                self.open_vs_config();
            }
            ui.add_space(8.0);
            if ui.add_sized(w, egui::Button::new("📦  Build and Package Game")).clicked() {
                self.open_package_config();
            }
            ui.add_space(8.0);
            if ui.add_sized(w, egui::Button::new("🐙  Git")).clicked() {
                self.open_git_menu();
            }
        });
        if !have_project {
            ui.add_space(6.0);
            ui.colored_label(WARN_AMBER, "⚠  Set a project path above to enable these actions.");
        }
    }

    pub fn show_project_path_row(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Unreal Project  (.uproject)")
            .size(12.0).color(egui::Color32::GRAY));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let has_path  = self.project_path.is_some();
            let extra_btn = if has_path { 88.0 } else { 0.0 };
            let browse_w  = 78.0;
            let gap       = ui.spacing().item_spacing.x;
            let text_w    = (ui.available_width() - browse_w - extra_btn - gap * 3.0).max(60.0);

            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.project_path_input)
                    .hint_text("Select or paste path to .uproject…")
                    .desired_width(text_w),
            );
            if resp.lost_focus() { self.try_apply_typed_path(); }

            if ui.add_sized([browse_w, 22.0], egui::Button::new("Browse…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Unreal Project", &["uproject"])
                    .set_title("Select your .uproject file")
                    .pick_file()
                {
                    save_project_path(&path);
                    self.project_path_input = path.to_string_lossy().to_string();
                    self.project_path       = Some(path);
                    self.refresh_status();
                }
            }

            if has_path {
                if ui.add_sized([extra_btn - gap, 22.0], egui::Button::new("✕ Clear")).clicked() {
                    clear_project_path();
                    self.project_path = None;
                    self.project_path_input.clear();
                    self.refresh_status();
                }
            }
        });

        ui.add_space(2.0);
        match &self.project_path {
            Some(p) => {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                ui.colored_label(MIKU_TEAL, format!("✓  {}", name));
            }
            None if !self.project_path_input.trim().is_empty() => {
                ui.colored_label(ERR_RED, "✗  File not found or not a .uproject");
            }
            _ => {}
        }
    }

    pub fn show_status_area(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Status / Output").size(12.0).color(egui::Color32::GRAY));
        egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.status_display)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(4),
            );
        });
    }
}
