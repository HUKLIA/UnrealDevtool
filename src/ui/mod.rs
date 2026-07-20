mod chat;
mod git;
mod package;
mod preflight;
mod selfcheck;
mod vs;

use eframe::egui;
use crate::app::DevToolApp;
use crate::config::{clear_project_path, save_project_path, save_ui_config, UiConfig};
use crate::theme::*;
use crate::types::{GitAction, GitState, GitTaskStatus, UploadAction};
use std::sync::atomic::Ordering;

// ── eframe::App — the main update loop ───────────────────────────────────────

/// How often to re-check GitHub for a new release while the app is open.
/// 5 minutes: notices a new build quickly without burning the 60 req/hr limit.
const UPDATE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Upper bound on how long the TACHYON ad is allowed to occupy the busy view
/// before we give up waiting for its `ended` event and revert to normal
/// gif+music ourselves (video failed to load, autoplay got blocked, etc.).
/// The clip is ~54s; this gives generous slack for a slow WebView2 cold start.
const AD_SAFETY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

impl eframe::App for DevToolApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.center_window_on_startup(ctx);

        // `unwrap_or_else(|e| e.into_inner())` recovers from a poisoned lock
        // instead of panicking. Without this, a panic on any background
        // thread (packaging, git, upload, update-check) would poison one of
        // these shared mutexes, and this being the main render loop — called
        // directly by winit, not wrapped in catch_unwind — the very next
        // frame would panic too and take the whole app down with it.
        let is_busy = *self.is_working.lock().unwrap_or_else(|e| e.into_inner());
        self.status_display = self.status_message.lock().unwrap_or_else(|e| e.into_inner()).clone();

        // Periodic update check. Once an update is found we stop polling.
        // `request_repaint_after` lets egui sleep until the next check is due
        // rather than painting every frame just to watch the clock.
        if self.update_info.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            let elapsed = self.last_update_check.elapsed();
            if elapsed >= UPDATE_CHECK_INTERVAL {
                self.check_for_updates(ctx.clone());
            } else {
                ctx.request_repaint_after(UPDATE_CHECK_INTERVAL - elapsed);
            }
        }

        // Detect background-task completion and advance git state machine
        let just_finished = self.was_working && !is_busy;
        self.was_working = is_busy;
        if just_finished {
            if let Some(a) = &mut self.audio_player {
                // Stop before resetting speed, not after: `set_speed` restarts
                // playback (rodio `Player::append`) whenever audio is still
                // marked playing, and appending right after a stop while the
                // old sound is still draining makes rodio block the caller
                // until the audio thread catches up (`sleep_until_end`).
                // That block runs on this UI thread — normally sub-millisecond,
                // but if the audio thread is stalled (e.g. the OS deprioritizes
                // it while the window is locked/minimized) it can hang the
                // whole app until the audio thread resumes. Stopping first
                // marks us as not-playing, so `set_speed` skips the restart.
                a.stop();
                a.set_speed(1.0);
            }
            // Defensive: the real UAT task finished (e.g. a fast cancel)
            // while the ad was still playing. Going to idle view either way,
            // so just clear the flags — no music should start here (the
            // `a.stop()` above already covers audio correctly).
            self.ad_playing    = false;
            self.ad_started_at = None;
            self.fast_package_mode = false;
            let git_status = self.git_result.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(gs) = git_status {
                match gs {
                    GitTaskStatus::Ok => {
                        self.git_state = self.git_next_state.clone();
                        // After merge: auto-open package config if requested
                        if self.git_package_after_merge && self.git_state == GitState::AfterMerge {
                            self.git_package_after_merge = false;
                            self.git_state = GitState::Idle;
                            self.open_package_config();
                        }
                    }
                    GitTaskStatus::Conflict | GitTaskStatus::Error => {
                        self.git_state = GitState::Idle;
                        self.git_package_after_merge = false;
                    }
                }
                self.git_next_state = GitState::Idle;
            }

            // A Google Drive upload just failed — offer a manual fallback
            // instead of leaving the user with only an error string.
            let gdrive_failed = {
                let mut f = self.gdrive_upload_failed.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::take(&mut *f)
            };
            if gdrive_failed {
                self.show_upload_fallback_panel = true;
            }

            // If packaging produced a zip, ask about the output folder first
            if let Some(zip) = self.pending_zip.lock().unwrap_or_else(|e| e.into_inner()).take() {
                self.upload_zip_path   = zip.clone();
                self.upload_use_local  = false;
                self.upload_use_gdrive = false;
                if let Some(folder) = zip.parent() {
                    self.pending_open_folder_path = folder.to_path_buf();
                    self.show_open_folder_panel   = true;
                } else {
                    self.show_upload_panel = true;
                }
            }

        }

        // TACHYON ad: revert to normal gif+music on its `ended` event, or on
        // a safety-timeout if that event never arrives.
        if self.ad_playing {
            let ended     = self.webview_manager.take_ad_ended();
            let timed_out = self.ad_started_at.is_some_and(|t| t.elapsed() > AD_SAFETY_TIMEOUT);
            if ended || timed_out {
                self.end_ad();
            }
        }

        if let Some(a) = &mut self.audio_player { a.tick(); }

        // Chat: once the background stream finishes, fold the accumulated
        // reply into history — mirrors the was_working/just_finished pattern
        // above, since the streaming buffer only lives in a shared Mutex the
        // background thread can reach, not directly in chat_history.
        let chat_busy_now = *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner());
        if self.was_chat_busy && !chat_busy_now {
            let reply = std::mem::take(&mut *self.chat_streaming.lock().unwrap_or_else(|e| e.into_inner()));
            if !reply.is_empty() {
                self.chat_history.push(crate::ops::llm::ChatMessage { role: "assistant".into(), content: reply });
            }
        }
        self.was_chat_busy = chat_busy_now;

        self.pending_webview = None;

        // Status/Output is pinned to the bottom — always visible regardless
        // of how tall the button list above grows. Panels must be added
        // before CentralPanel so it reserves its space first.
        egui::TopBottomPanel::bottom("status_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            self.show_status_area(ui);
            ui.add_space(4.0);
        });

        // Everything else scrolls, so no content is ever unreachable just
        // because the window is shorter than the current button list —
        // the window is still freely resizable too, this is on top of that.
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
            });
        });

        // Sync the embedded WebView2 panel (3D Miku / Cookie Clicker / Sponder
        // Bird) to whatever panel (if any) requested space this frame.
        let ppp = ctx.pixels_per_point();
        if let Some(err) = self.webview_manager.update(self.pending_webview, ppp) {
            self.set_status(err);
            // Don't make the user wait out the full safety-timeout if the
            // webview itself failed to load (e.g. WebView2 runtime missing).
            if self.ad_playing { self.end_ad(); }
        }
    }
}

// ── Shared UI methods ─────────────────────────────────────────────────────────

impl DevToolApp {
    pub fn show_busy_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(p) = &self.project_path {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            ui.horizontal(|ui| {
                ui.colored_label(accent(), "*");
                ui.label(egui::RichText::new(name.as_ref()).color(egui::Color32::LIGHT_GRAY));
            });
            ui.add_space(6.0);
        }
        let dt = ctx.input(|i| i.stable_dt);
        if !self.ad_playing && !self.miku_mode_3d {
            let gif_dt = if self.fast_package_mode { dt * 5.0 } else { dt };
            if let Some(gif) = &mut self.gif_player { gif.advance(ctx, gif_dt); }
        }

        // ── 2D / 3D toggle ────────────────────────────────────────────────────
        // Hidden while the ad plays — switching preview mode mid-ad doesn't
        // mean anything; whatever mode was selected resumes automatically
        // once the ad ends, since `miku_mode_3d` is never touched below.
        if !self.ad_playing {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Miku:").size(11.0).color(HINT_GRAY));

                let active   = egui::Color32::from_rgb(0, 180, 160);
                let inactive = egui::Color32::from_rgb(40, 40, 55);

                let btn2d = egui::Button::new(egui::RichText::new("2D").size(11.0))
                    .fill(if !self.miku_mode_3d { active } else { inactive });
                if ui.add_sized([36.0, 20.0], btn2d).clicked() && self.miku_mode_3d {
                    self.miku_mode_3d = false;
                    if let Some(g) = &mut self.gif_player { g.reset(); }
                }

                let btn3d = egui::Button::new(egui::RichText::new("3D").size(11.0))
                    .fill(if self.miku_mode_3d { active } else { inactive });
                if ui.add_sized([36.0, 20.0], btn3d).clicked() && !self.miku_mode_3d {
                    self.miku_mode_3d = true;
                }
            });
        }
        ui.add_space(4.0);

        let gif_size = egui::vec2(300.0, 252.0);
        egui::Frame::none()
            .fill(GIF_BG)
            .stroke(egui::Stroke::new(1.5, accent()))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if self.ad_playing {
                        let (rect, _) = ui.allocate_exact_size(gif_size, egui::Sense::hover());
                        self.pending_webview = Some((crate::webview::WebPanel::Ad, rect));
                    } else if self.miku_mode_3d {
                        let (rect, _) = ui.allocate_exact_size(gif_size, egui::Sense::hover());
                        self.pending_webview = Some((crate::webview::WebPanel::Miku3D, rect));
                    } else if let Some(gif) = &self.gif_player {
                        gif.show(ui, gif_size);
                    } else {
                        ui.add_space(gif_size.y);
                        ui.colored_label(accent(), "[ working… ]");
                    }
                });
            });

        let audio_playing = self.audio_player.as_ref().is_some_and(|a| a.is_playing());
        if audio_playing {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 280.0).max(0.0) / 2.0);

                    let mute_icon = if self.audio_muted { "🔇" } else { "🔊" };
                    if ui.add_sized([28.0, 22.0], egui::Button::new(mute_icon)).clicked() {
                        let muted = !self.audio_muted;
                        self.set_audio_muted(muted);
                    }

                    let mut volume = self.audio_volume;
                    let resp = ui.add_enabled(
                        !self.audio_muted,
                        egui::Slider::new(&mut volume, 0..=100).text("volume"),
                    );
                    if resp.changed() {
                        self.set_audio_volume(volume);
                    }
                });
            });
        }

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(&self.busy_label).size(15.0).color(accent()));
            ui.add_space(6.0);

            let real_prog = *self.progress.lock().unwrap_or_else(|e| e.into_inner());

            if self.fast_package_mode {
                let done = real_prog >= 1.0;
                let elapsed = self.task_started_at
                    .map(|t| t.elapsed().as_secs_f32())
                    .unwrap_or(0.0);

                // Each step fills in 2.5s sequentially; overall rushes to ~0.95.
                let step = |start: f32| -> f32 {
                    if done { return 1.0; }
                    ((elapsed - start) / 2.5).clamp(0.0, 1.0)
                };
                let overall = if done { 1.0 } else {
                    (1.0 - (-elapsed * 0.6_f32).exp()) * 0.95
                };

                let bar_w = ui.available_width().min(340.0);
                let steps: &[(&str, f32, egui::Color32)] = &[
                    ("Compile ",  step(0.0),  accent()),
                    ("Cook    ",  step(2.5),  MIKU_PINK),
                    ("Stage   ",  step(5.0),  egui::Color32::from_rgb(180, 160, 60)),
                    ("Pack    ",  step(7.5),  egui::Color32::from_rgb(80, 160, 220)),
                ];
                for (label, prog, color) in steps {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(*label).size(10.0).color(HINT_GRAY).monospace());
                        ui.add(
                            egui::ProgressBar::new(*prog)
                                .desired_width(bar_w - 60.0)
                                .fill(*color),
                        );
                    });
                    ui.add_space(2.0);
                }
                ui.add_space(6.0);
                ui.add(
                    egui::ProgressBar::new(overall)
                        .desired_width(bar_w)
                        .fill(accent())
                        .show_percentage(),
                );

                if !done {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
            } else {
                ui.add(
                    egui::ProgressBar::new(real_prog)
                        .desired_width(ui.available_width().min(340.0))
                        .fill(accent())
                        .show_percentage(),
                );
            }

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
        if let Some(download_url) = self.show_update_banner_ui(ui) {
            self.start_update_install(download_url);
        }

        self.show_project_path_row(ui);
        ui.add_space(10.0);
        self.show_engine_path_row(ui);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        if let Some(panel) = self.active_web_panel {
            self.show_web_panel_ui(ui, panel);

        } else if self.show_open_folder_panel {
            self.show_open_folder_panel(ui);

        } else if self.show_upload_fallback_panel {
            self.show_upload_fallback_panel(ui);

        } else if self.show_upload_panel {
            let action = self.show_upload_panel_ui(ui);
            match action {
                UploadAction::Upload => self.start_upload(),
                UploadAction::Skip   => { self.show_upload_panel = false; }
                UploadAction::None   => {}
            }

        } else if self.show_package_config {
            match self.show_package_config_panel(ui) {
                Some(false) => self.start_packaging(),
                Some(true)  => self.start_fast_packaging(),
                None        => {}
            }

        } else if self.show_vs_config {
            let go = self.show_vs_config_panel(ui);
            if go { self.start_vs_rebuild(); }

        } else if self.show_pc_check {
            self.show_pc_check_panel(ui);

        } else if self.show_chat_panel {
            self.show_chat_panel_ui(ui);

        } else if self.show_app_check {
            self.show_app_check_panel(ui);

        } else if !matches!(self.git_state, GitState::Idle) {
            let action = self.show_git_panel(ui);
            match action {
                GitAction::StartCommitPush          => self.git_start_commit_push(),
                GitAction::StartSync                => self.git_start_sync(),
                GitAction::StartMerge               => self.git_start_merge(),
                GitAction::StartMergeAndPackage     => self.start_merge_and_package(),
                GitAction::StartCheckout { branch } => self.git_start_checkout(branch),
                GitAction::StartNewBranch { name }  => self.git_start_new_branch(name),
                GitAction::None                     => {}
            }
        } else if self.show_dm_spencer_panel {
            self.show_dm_spencer_panel(ui);
        } else if self.show_media_config {
            self.show_media_config_panel(ui);
        } else {
            self.show_action_buttons(ui);
        }
    }

    /// If a newer release was found by the background update check, show a
    /// dismissible banner. Returns `Some(download_url)` if the user clicked
    /// "Update Now".
    pub fn show_update_banner_ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        if !self.show_update_banner { return None; }
        let info = self.update_info.lock().unwrap_or_else(|e| e.into_inner()).clone()?;

        let mut clicked_update = false;
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(35, 55, 50))
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.colored_label(accent(), format!("Update available: {}", info.version));
                        ui.label(egui::RichText::new(format!("Released {}", info.published_at))
                            .size(11.0).color(HINT_GRAY));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([60.0, 24.0], egui::Button::new("Dismiss")).clicked() {
                            self.show_update_banner = false;
                        }
                        if ui.add_sized([100.0, 24.0], egui::Button::new("Update Now")).clicked() {
                            clicked_update = true;
                        }
                    });
                });
            });
        ui.add_space(8.0);

        if clicked_update { Some(info.download_url) } else { None }
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
            ui.colored_label(WARN_AMBER, "(!)  Set a project path above to enable these actions.");
        }

        ui.add_space(8.0);
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("🔍  Check PC Setup")).clicked() {
            self.open_pc_check();
        }
        ui.add_space(8.0);
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("💬  Dev Assistant")).clicked() {
            self.open_chat_panel();
        }
        ui.add_space(8.0);
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("⚙  App Self-Check")).clicked() {
            self.open_app_check();
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        let w = [ui.available_width(), 40.0];
        if ui.add_sized(w, egui::Button::new("🍪  Cookie Clicker")).clicked() {
            self.active_web_panel = Some(crate::webview::WebPanel::CookieClicker);
        }
        ui.add_space(8.0);
        if ui.add_sized(w, egui::Button::new("💬  DM Spencer")).clicked() {
            self.show_dm_spencer_panel = true;
        }
        ui.add_space(8.0);
        if ui.add_sized(w, egui::Button::new("🎨  Customize Miku & Sound")).clicked() {
            self.open_media_config();
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Quick Links").size(11.0).color(HINT_GRAY));
        ui.add_space(6.0);

        let gap    = ui.spacing().item_spacing.x;
        let link_w = (ui.available_width() - gap * 3.0) / 4.0;
        ui.horizontal(|ui| {
            if ui.add_sized([link_w, 30.0], egui::Button::new("Claude")).clicked()  { crate::ops::open_url("https://claude.ai/new"); }
            if ui.add_sized([link_w, 30.0], egui::Button::new("ChatGPT")).clicked() { crate::ops::open_url("https://chatgpt.com/"); }
            if ui.add_sized([link_w, 30.0], egui::Button::new("Gemini")).clicked()  { crate::ops::open_url("https://gemini.google.com/app"); }
            if ui.add_sized([link_w, 30.0], egui::Button::new("Epic Games")).clicked() { crate::ops::open_url("https://www.epicgames.com/"); }
        });
        ui.add_space(8.0);
        if ui.add_sized(w, egui::Button::new("📘  Unreal Docs")).clicked() {
            crate::ops::open_url("https://dev.epicgames.com/community/assistant/unreal-engine");
        }
    }

    pub fn show_media_config_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("🎨  Customize Miku & Sound").size(13.0).color(accent()));
                ui.add_space(10.0);

                ui.label(egui::RichText::new("2D Image / GIF").size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                let ctx = ui.ctx().clone();
                ui.horizontal(|ui| {
                    let thumb_max = 96.0;
                    egui::Frame::none()
                        .fill(GIF_BG)
                        .stroke(egui::Stroke::new(1.0, accent()))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::same(4.0))
                        .show(ui, |ui| {
                            if let Some(gif) = &mut self.gif_player {
                                gif.ensure_texture(&ctx);
                                let size  = gif.size();
                                let scale = (thumb_max / size.x.max(size.y).max(1.0)).min(1.0);
                                gif.show(ui, size * scale);
                            } else {
                                ui.allocate_exact_size(egui::vec2(thumb_max, thumb_max), egui::Sense::hover());
                            }
                        });

                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        let gif_label = self.custom_gif_path.as_ref()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "(default Miku GIF)".to_string());
                        ui.label(egui::RichText::new(gif_label).size(10.0).color(HINT_GRAY).monospace());
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.add_sized([100.0, 24.0], egui::Button::new("Browse…")).clicked() {
                                self.choose_custom_gif();
                            }
                            ui.add_enabled_ui(self.custom_gif_path.is_some(), |ui| {
                                if ui.add_sized([80.0, 24.0], egui::Button::new("Reset")).clicked() {
                                    self.reset_gif_to_default();
                                }
                            });
                        });
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Looping Sound  (mp3 / wav)").size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                let (sound_name, sound_path_hint) = match &self.custom_sound_path {
                    Some(p) => (
                        p.file_name().map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.to_string_lossy().to_string()),
                        Some(p.to_string_lossy().to_string()),
                    ),
                    None => ("Ievan Polkka  (default)".to_string(), None),
                };
                ui.horizontal(|ui| {
                    ui.colored_label(accent(), "🔊");
                    ui.label(egui::RichText::new(sound_name).size(13.0).color(egui::Color32::WHITE).strong());
                });
                if let Some(hint) = sound_path_hint {
                    ui.label(egui::RichText::new(hint).size(10.0).color(HINT_GRAY).monospace());
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([100.0, 24.0], egui::Button::new("Browse…")).clicked() {
                        self.choose_custom_sound();
                    }
                    ui.add_enabled_ui(self.custom_sound_path.is_some(), |ui| {
                        if ui.add_sized([80.0, 24.0], egui::Button::new("Reset")).clicked() {
                            self.reset_sound_to_default();
                        }
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Accent Color").size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let mut color = accent();
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        crate::theme::set_accent(ui.ctx(), color);
                        save_ui_config(&UiConfig { accent_rgb: Some((color.r(), color.g(), color.b())) });
                    }
                    ui.add_space(8.0);
                    if ui.add_sized([160.0, 24.0], egui::Button::new("Reset to default teal")).clicked() {
                        crate::theme::set_accent(ui.ctx(), crate::theme::default_accent());
                        save_ui_config(&UiConfig { accent_rgb: None });
                    }
                });

                ui.add_space(14.0);
                if ui.add_sized([100.0, 28.0], egui::Button::new("< Back")).clicked() {
                    self.show_media_config = false;
                }
            });
    }

    /// Renders an embedded web page (Cookie Clicker, Sponder Bird, 3D Miku)
    /// with a "< Back" button. The actual WebView2 control is positioned by
    /// `WebViewManager::update` after this frame's layout is known.
    pub fn show_web_panel_ui(&mut self, ui: &mut egui::Ui, panel: crate::webview::WebPanel) {
        ui.horizontal(|ui| {
            if ui.add_sized([90.0, 26.0], egui::Button::new("< Back")).clicked() {
                self.active_web_panel = None;
            }
            ui.add_space(8.0);
            ui.colored_label(accent(), panel.title());
        });
        ui.add_space(6.0);

        let avail = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        self.pending_webview = Some((panel, rect));
    }

    pub fn show_dm_spencer_panel(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("💬  DM on Discord")
                .size(16.0).color(egui::Color32::WHITE).strong());
        });
        ui.add_space(12.0);

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 30, 40))
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Discord username to search:")
                    .size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.add(egui::TextEdit::singleline(&mut self.dm_target_name)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. gonkindroid"));
                ui.add_space(10.0);

                let can_open = !self.dm_target_name.trim().is_empty();
                let btn_w = ui.available_width();
                ui.add_enabled_ui(can_open, |ui| {
                    if ui.add_sized([btn_w, 34.0],
                        egui::Button::new("🔍  Open Discord & Search")).clicked()
                    {
                        crate::ops::discord::open_discord_dm(&self.dm_target_name, None, None);
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Quick messages (click to send):")
                    .size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(can_open, |ui| {
                        for preset in self.dm_message_presets.clone() {
                            if ui.button(&preset).clicked() {
                                crate::ops::discord::open_discord_dm(&self.dm_target_name, Some(&preset), None);
                            }
                        }
                    });
                });
                ui.add_space(10.0);

                ui.label(egui::RichText::new("Custom message:")
                    .size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.dm_custom_message)
                        .desired_width(ui.available_width() - 70.0)
                        .hint_text("Type a message…"));
                    let can_send = can_open && !self.dm_custom_message.trim().is_empty();
                    ui.add_enabled_ui(can_send, |ui| {
                        if ui.button("Send").clicked() {
                            crate::ops::discord::open_discord_dm(&self.dm_target_name, Some(&self.dm_custom_message.clone()), None);
                            self.dm_custom_message.clear();
                        }
                    });
                });

                ui.add_space(8.0);
                ui.label(egui::RichText::new("Image to send: ")
                .size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.dm_image_path)
                        .desired_width(ui.available_width() - 130.0)
                        .hint_text("C:\\path\\to\\image.png"));
                    if ui.button("Browse...").clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "png", "webp"]).pick_file() {
                            self.dm_image_path = path.to_string_lossy().to_string();
                        }
                    let can_imag = can_open && !self.dm_image_path.trim().is_empty();
                    ui.add_enabled_ui(can_imag, |ui| {
                        if ui.button("Send").clicked() {
                            crate::ops::discord::open_discord_dm(
                                &self.dm_target_name,
                                None,
                                Some(&self.dm_image_path.clone()),
                            );
                        }
                    })
                })
            });

        ui.add_space(8.0);
        ui.label(egui::RichText::new(
            "Opens Discord on this PC, presses Ctrl+K and types the username automatically.")
            .size(10.0).color(HINT_GRAY));
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.add_sized([100.0, 28.0], egui::Button::new("< Back")).clicked() {
                self.show_dm_spencer_panel = false;
            }
            ui.add_space(8.0);
            if ui.add_sized([160.0, 28.0], egui::Button::new("🐦  Play Sponder Bird")).clicked() {
                self.active_web_panel = Some(crate::webview::WebPanel::SponderBird);
            }
        });
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
            if !self.project_path_input.is_empty() {
                let full_path = self.project_path_input.clone();
                resp.clone().on_hover_text(full_path);
            }
            if resp.lost_focus() { self.try_apply_typed_path(); }

            if ui.add_sized([browse_w, 22.0], egui::Button::new("Browse…")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Unreal Project", &["uproject"])
                    .set_title("Select your .uproject file")
                    .pick_file()
                {
                    save_project_path(&path);
                    self.project_path_input = path.to_string_lossy().to_string();
                    self.project_path       = Some(path);
                    self.redetect_engine();
                }

            if has_path
                && ui.add_sized([extra_btn - gap, 22.0], egui::Button::new("x  Clear")).clicked() {
                    clear_project_path();
                    self.project_path = None;
                    self.project_path_input.clear();
                    self.refresh_status();
                }
        });

        ui.add_space(2.0);
        match &self.project_path {
            Some(p) => {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                ui.colored_label(accent(), format!("[OK]  {}", name));
            }
            None if !self.project_path_input.trim().is_empty() => {
                ui.colored_label(ERR_RED, "[!]  File not found or not a .uproject");
            }
            _ => {}
        }
    }

    /// Engine location row: shows the auto-detected (or manually overridden)
    /// engine folder, with a "Browse…" escape hatch for when auto-detection
    /// (registry / EngineAssociation lookup) can't find it — e.g. a source
    /// build or a non-standard install path.
    pub fn show_engine_path_row(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Unreal Engine")
            .size(12.0).color(egui::Color32::GRAY));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let has_override = self.engine_override.is_some();
            let path_text = self.engine_dir.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(not found — click Browse to select it manually)".to_string());
            // `Label` defaults to `TextWrapMode::Extend` — it does NOT clip to
            // its allocated size, it grows past it. Without `.truncate()` a
            // long engine path overlaps the Browse/Auto-detect buttons that
            // follow it in this row instead of eliding with "…".
            ui.add_sized(
                [ui.available_width() - if has_override { 172.0 } else { 86.0 }, 22.0],
                egui::Label::new(egui::RichText::new(&path_text).size(12.0).color(HINT_GRAY)).truncate(),
            ).on_hover_text(&path_text);

            if ui.add_sized([78.0, 22.0], egui::Button::new("Browse…")).clicked() {
                self.choose_engine_dir();
            }
            if has_override && ui.add_sized([86.0, 22.0], egui::Button::new("x  Auto-detect")).clicked() {
                self.clear_engine_override();
            }
        });

        ui.add_space(2.0);
        match (&self.engine_dir, self.engine_override.is_some()) {
            (Some(_), true)  => { ui.colored_label(accent(), "[OK]  Manual override"); }
            (Some(_), false) => { ui.colored_label(accent(), "[OK]  Auto-detected"); }
            (None, _) => {
                ui.colored_label(ERR_RED, "[!]  Engine not found — select your Unreal Engine install folder");
            }
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
