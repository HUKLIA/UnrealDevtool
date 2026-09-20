mod bar_chart;
pub mod browser;
mod chat;
mod dashboard;
mod extras;
mod git;
pub mod guide;
mod intro;
mod monitor;
mod package;
mod palette;
mod panels;
mod preflight;
pub mod rail;
pub mod run;
mod selfcheck;
pub mod setup;
pub mod shell;
pub mod tools;
mod vs;

use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::{GitAction, GitState, GitTaskStatus, RunState, UploadAction};

/// How often to re-check GitHub for a new release while the app is open.
const UPDATE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl eframe::App for DevToolApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.center_window_on_startup(ctx);

        if self.show_intro {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(BG))
                .show(ctx, |ui| self.show_intro_screen(ui, ctx));
            return;
        }

        self.pump_background_state(ctx);

        self.handle_window_input(ctx);
        self.sync_monitor(ctx);

        let state = self.run_state();
        self.pending_webview = None;
        if self.guide_active {
            // Targets belong to this frame's layout.  Clearing them here
            // prevents an old button rectangle being highlighted after a
            // state change or a resize.
            self.guide_target = None;
        }

        egui::TopBottomPanel::top("topbar")
            .exact_height(shell::topbar_height(ctx.screen_rect().width()))
            .frame(egui::Frame::none())
            .show(ctx, |ui| self.show_topbar(ui));

        // Keep task results and non-UAT errors visible after a background
        // operation finishes. The main run log is deliberately UAT output;
        // this compact bar carries git, VS, upload, and setup status.
        egui::TopBottomPanel::bottom("status_output")
            .exact_height(28.0)
            // Side padding matches the surface gutter; the status dot used to
            // sit on the window edge with half of it clipped.
            .frame(egui::Frame::none().fill(BG_TOP)
                .inner_margin(egui::Margin::symmetric(GUTTER, 0.0)))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    dot(ui, if self.status_display.contains("[ERROR") { RED } else { accent() }, 6.0);
                    ui.add(egui::Label::new(egui::RichText::new(&self.status_display).font(mono(11.0)).color(SOFT)).truncate())
                        .on_hover_text(&self.status_display);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(BG)
                // No bottom margin: scrolling regions carry their own trailing
                // gap, so content runs off the bottom edge rather than being
                // cut off above an empty strip.
                .inner_margin(egui::Margin { left: GUTTER, right: GUTTER, top: GUTTER, bottom: 0.0 }))
            .show(ctx, |ui| {
                paint_ambient(ui);

                // The surface cross-fades and rises when the job changes
                // state, so Ready → Running → Done reads as one thing moving
                // rather than three screens swapping.
                let key = match state {
                    RunState::Setup => 0, RunState::Ready => 1,
                    RunState::Running => 2, RunState::Done => 3,
                };
                let t = ease_out(ctx.animate_bool_with_time(
                    egui::Id::new("surface").with(key), true,
                    anim_secs(T_BASE, self.frame_dt)));

                let layer = ui.layer_id();
                ui.scope(|ui| {
                    ui.multiply_opacity(t);
                    self.show_update_notice(ui);
                    self.show_surface(ui, state);
                });

                if t < 1.0 {
                    ctx.transform_layer_shapes(
                        layer,
                        egui::emath::TSTransform::from_translation(egui::vec2(0.0, (1.0 - t) * 10.0)),
                    );
                    ctx.request_repaint();
                }
            });

        // Hide any webview requested by the underlying surface before sheets
        // paint. The Browser sheet will request its own rectangle below.
        if self.sheet.is_some() {
            self.pending_webview = None;
        }
        // Sheets paint over everything, in their own foreground area.
        egui::Area::new(egui::Id::new("sheets"))
            .fixed_pos(egui::Pos2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                self.show_sheet_layer(ui, ctx);
            });

        // While a file is being dragged over the window, say what dropping does.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip, egui::Id::new("drop_hint")));
            painter.rect_filled(screen, 0.0, tint(BG, 210));
            let card = egui::Rect::from_center_size(screen.center(), egui::vec2(360.0, 110.0)
                .min(screen.size() - egui::vec2(32.0, 32.0)));
            painter.rect(card, egui::Rounding::same(R_SECTION), CARD,
                egui::Stroke::new(1.5, accent()));
            painter.text(card.center() - egui::vec2(0.0, 10.0), egui::Align2::CENTER_CENTER,
                "Drop to open project", display(18.0), TEXT);
            painter.text(card.center() + egui::vec2(0.0, 16.0), egui::Align2::CENTER_CENTER,
                "a .uproject file, or the folder that contains one", body(12.0), MUTED);
        }

        self.show_palette(ctx);

        // The manual paints last, above the sheets, because it describes them.
        if self.guide_active {
            // A tour step that opens the Browser would otherwise be narrating
            // a native child window drawn on top of its own callout.
            self.pending_webview = None;
            egui::Area::new(egui::Id::new("guide"))
                .fixed_pos(egui::Pos2::ZERO)
                .order(egui::Order::Tooltip)
                .show(ctx, |ui| {
                    ui.set_min_size(ctx.screen_rect().size());
                    self.show_guide_overlay(ui, ctx);
                });
        }

        // Sync the embedded WebView2 control to whatever asked for space.
        let ppp = ctx.pixels_per_point();
        if let Some(err) = self.webview_manager.update(self.pending_webview, ppp) {
            self.set_status(err);
        }
    }
}

impl DevToolApp {
    /// Window-level input: dropping a project onto the window, and the one
    /// keyboard shortcut for the thing this app is for.
    fn handle_window_input(&mut self, ctx: &egui::Context) {
        // Drop a .uproject (or a folder containing one) to open it.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
        });
        if !self.is_busy_now() {
            for path in dropped {
                let project = if path.is_dir() {
                    std::fs::read_dir(&path).ok().and_then(|rd| rd.flatten()
                        .map(|e| e.path())
                        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("uproject"))))
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("uproject")) {
                    Some(path)
                } else {
                    None
                };
                if let Some(p) = project {
                    self.apply_project_path(p);
                    self.set_status("Project opened from drag-and-drop.".into());
                    break;
                }
            }
        }

        // Ctrl+Enter starts the build, but only from the Ready surface with
        // nothing open over it, so it can never fire from inside a text box
        // on a sheet or during a git prompt.
        let ctrl_enter = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        if ctrl_enter
            && !self.guide_active
            && self.sheet.is_none()
            && self.active_web_panel.is_none()
            && !self.show_git_sheet
            && matches!(self.run_state(), RunState::Ready)
            && !self.is_busy_now()
        {
            self.start_packaging();
        }
        // A build scheduled for a time of day. It only fires from the Ready
        // surface with nothing open: starting a build closes the Unreal Editor,
        // and that must never happen while someone is part-way through
        // something else. If it cannot start, it says so rather than waiting
        // silently for a moment that has passed.
        if let Some((at, label)) = self.schedule.clone() {
            let now = std::time::Instant::now();
            if now >= at {
                self.schedule = None;
                if matches!(self.run_state(), RunState::Ready) && !self.is_busy_now() && !self.show_git_sheet {
                    self.set_status(format!("Scheduled build ({label}) starting…"));
                    self.start_packaging();
                } else {
                    self.set_status(format!(
                        "[WARNING] Scheduled build for {label} was skipped: the app was not on the Ready screen."));
                }
            } else {
                ctx.request_repaint_after((at - now).min(std::time::Duration::from_secs(1)));
            }
        }

        // Ctrl+K opens the command palette from anywhere.
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) && !self.guide_active {
            self.toggle_palette();
        }
        // F1 opens the manual from anywhere.
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) && !self.guide_active {
            self.open_guide();
        }
    }

    fn is_busy_now(&self) -> bool {
        *self.is_working.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Routes to whatever owns the surface this frame.
    ///
    /// The transient flows are checked first and take the whole surface: each
    /// is one focused decision (where did the build go, upload it or not,
    /// which IDE) and none of them make sense shown beside something else.
    fn show_surface(&mut self, ui: &mut egui::Ui, state: RunState) {
        if let Some(panel) = self.active_web_panel {
            self.show_web_panel_ui(ui, panel);
        } else if self.show_open_folder_panel {
            self.show_open_folder_panel(ui);
        } else if self.show_upload_fallback_panel {
            self.show_upload_fallback_panel(ui);
        } else if self.show_upload_panel {
            match self.show_upload_panel_ui(ui) {
                UploadAction::Upload => self.start_upload(),
                UploadAction::Skip   => self.show_upload_panel = false,
                UploadAction::None   => {}
            }
        } else if self.show_vs_config {
            if self.show_vs_config_panel(ui) { self.start_vs_rebuild(); }
        } else if self.show_git_sheet {
            self.show_git_flow(ui);
        } else {
            match state {
                RunState::Setup => self.show_setup_surface(ui),
                _               => self.show_run_surface(ui, state),
            }
        }
    }

    /// Everything that has to happen once per frame before the UI is drawn:
    /// draining background results, advancing the state machines, and the
    /// periodic polls that keep what is on screen live.
    fn pump_background_state(&mut self, ctx: &egui::Context) {
        self.sample_frame_time(ctx);

        // `unwrap_or_else(|e| e.into_inner())` recovers from a poisoned lock
        // rather than panicking: this is the render loop, called directly by
        // winit and not wrapped in catch_unwind, so a panic on any background
        // thread would otherwise take the whole app down on the next frame.
        let is_busy = *self.is_working.lock().unwrap_or_else(|e| e.into_inner());
        // Copy only when it actually changed. This ran a `String` clone every
        // frame — 144 allocations a second on this display for a value that
        // changes a few times a minute.
        {
            let latest = self.status_message.lock().unwrap_or_else(|e| e.into_inner());
            if *latest != self.status_display {
                self.status_display.clear();
                self.status_display.push_str(&latest);
            }
        }

        if let Some((branch, status)) = self.git_refresh_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.git_current_branch = branch;
            self.git_status         = status;
        }
        if let Some(running) = self.editor_check_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.editor_is_running = running;
        }
        if let Some(next) = self.version_check_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.next_version_preview = next;
        }
        if let Some(found) = self.builds_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.builds = found;
        }
        if let Some((items, log, diags)) = self.pc_check_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.pc_check_items      = items;
            self.build_log_path      = log;
            self.build_log_diagnosis = diags;
        }
        if let Some(found) = self.clean_pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
            // Keep any ticks the user already made; only seed defaults when the
            // list is first populated or its shape changed.
            if self.clean_selected.len() != found.len() {
                self.clean_selected = found.iter().map(|t| t.default_on && t.exists).collect();
            }
            self.clean_targets = found;
        }

        if self.update_info.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            let elapsed = self.last_update_check.elapsed();
            if elapsed >= UPDATE_CHECK_INTERVAL {
                self.check_for_updates(ctx.clone());
            } else {
                ctx.request_repaint_after(UPDATE_CHECK_INTERVAL - elapsed);
            }
        }

        let just_finished = self.was_working && !is_busy;
        self.was_working = is_busy;
        if just_finished {
            self.on_task_finished();
            // A 30-minute build finishes while you are somewhere else. Flash
            // the taskbar button (the OS's standard "needs attention" cue)
            // unless the window is already in front.
            if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Informational));
            }
        }

        if let Some(a) = &mut self.audio_player { a.tick(); }

        let chat_busy = *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner());
        if self.was_chat_busy && !chat_busy {
            let reply = std::mem::take(&mut *self.chat_streaming.lock().unwrap_or_else(|e| e.into_inner()));
            if !reply.is_empty() {
                self.chat_history.push(crate::ops::llm::ChatMessage { role: "assistant".into(), content: reply });
            }
        }
        self.was_chat_busy = chat_busy;

        // ── Keep the surface live ──────────────────────────────────────────
        //
        // The old version polled "whichever tab is active". There is one
        // surface now, and it shows git state, the version preview, the editor
        // check and the build list all at once, so each gets its own interval.
        // Skipped while busy: a packaging run already saturates the disk, and
        // spawning git/tasklist/PowerShell into that is exactly the contention
        // this app has been burned by before.
        if !is_busy && self.sheet.is_none() {
            if self.last_tab_poll.elapsed() >= std::time::Duration::from_secs(3) {
                self.last_tab_poll = std::time::Instant::now();
                self.refresh_package_observed_version_only();
                self.refresh_doctor();
                self.refresh_pc_check_cheap();
                if let Some(dir) = self.git_project_dir() {
                    self.refresh_git_status_async(dir);
                }
            }
            if self.last_editor_poll.elapsed() >= std::time::Duration::from_secs(10) {
                self.refresh_editor_check_async();
            }
            if self.last_disk_poll.elapsed() >= std::time::Duration::from_secs(20) {
                self.refresh_pc_check_disk_async();
            }
            // Build folders only change when a build finishes, so this is a
            // slow safety net rather than the primary refresh.
            if self.last_builds_poll.elapsed() >= std::time::Duration::from_secs(30) {
                self.last_builds_poll = std::time::Instant::now();
                self.refresh_builds();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(3));
        }
    }

    /// Runs once, on the frame a background task completes.
    fn on_task_finished(&mut self) {
        if let Some(a) = &mut self.audio_player {
            // Stop before resetting speed: `set_speed` restarts playback while
            // audio is still marked playing, and appending right after a stop
            // can block this thread until the audio thread drains.
            a.stop();
            a.set_speed(1.0);
        }
        self.last_tab_poll = std::time::Instant::now();
        self.refresh_package_observed();
        self.refresh_builds();

        // Freeze the run's numbers into a result the Done surface can show.
        // Taken as a snapshot rather than read live, so the timings stop when
        // the build does instead of continuing to tick on screen.
        let (stages, warnings, errors, any_stage, error_samples) = {
            let mut r = self.run.lock().unwrap_or_else(|e| e.into_inner());
            r.finish();
            let any = r.started.iter().any(|s| s.is_some());
            (r.elapsed, r.warnings, r.errors, any, r.error_samples.clone())
        };
        let produced = self.pending_zip.lock().unwrap_or_else(|e| e.into_inner()).take();
        if produced.is_some() || any_stage {
            let bytes = produced.as_ref()
                .and_then(|z| std::fs::metadata(z).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            let status = self.status_display.clone();
            let ok = !status.contains("[ERROR") && !status.contains("[CANCELLED");
            // The folder this run wrote to: beside the zip when there is one,
            // otherwise wherever the newest log is (a failed run has no zip).
            let log = produced.as_ref()
                .and_then(|z| z.parent())
                .map(|d| d.join("BuildLog.txt"))
                .filter(|l| l.is_file())
                .or_else(|| self.project_path.as_ref()
                    .and_then(|p| crate::ops::diagnostics::latest_build_log(p)));
            let version_dir = log.as_ref().and_then(|l| l.parent()).map(|d| d.to_path_buf());
            let version = version_dir.as_ref()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| crate::ops::package::format_version(self.next_version_preview));
            let duration = self.task_started_at.map(|t| t.elapsed()).unwrap_or_default();

            // Remember how this build was made and how long each stage took;
            // the next run's progress bars are paced from it.
            if let Some(dir) = &version_dir {
                crate::ops::history::write_info(dir, &crate::ops::history::BuildInfo {
                    platform: self.build_target.label().to_string(),
                    config:   self.build_configuration.as_str().to_string(),
                    ok,
                    secs:     duration.as_secs(),
                    warnings,
                    errors,
                    commit: version_dir.as_ref().and_then(|_| self.git_project_dir())
                        .and_then(|d| crate::ops::git::head_commit(&d)).unwrap_or_default(),
                    stage_secs: stages.map(|d| d.map(|d| d.as_secs()).unwrap_or(0)),
                });
            }
            // The log scan for the failure summary runs off-thread.
            self.refresh_pc_check_cheap();
            self.last_build = Some(crate::types::BuildOutcome {
                version,
                zip: produced.clone(),
                bytes,
                duration,
                stages,
                warnings,
                errors,
                ok,
                platform: self.build_target,
                config:   self.build_configuration,
                log,
                error_samples,
            });
            // The post-build prompts still run, but they now open over a
            // surface that already shows the result rather than instead of it.
            if let Some(zip) = produced
                && let Some(folder) = zip.parent() {
                    self.upload_zip_path = zip.clone();
                    self.upload_use_local = false;
                    self.upload_use_gdrive = false;
                    self.gdrive_remote_status = None;
                    self.pending_open_folder_path = folder.to_path_buf();
                }
        }

        let git_status = self.git_result.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(gs) = git_status {
            match gs {
                GitTaskStatus::Ok => {
                    self.git_state = self.git_next_state.clone();
                    if self.git_package_after_merge && self.git_state == GitState::AfterMerge {
                        self.git_package_after_merge = false;
                        self.git_state = GitState::Idle;
                        self.show_git_sheet = false;
                        self.start_packaging();
                    }
                }
                GitTaskStatus::Conflict | GitTaskStatus::Error => {
                    self.git_state = GitState::Idle;
                    self.git_package_after_merge = false;
                }
            }
            self.git_next_state = GitState::Idle;
            if let Some(dir) = self.git_project_dir() {
                self.refresh_git_status_async(dir);
            }
        }

        let gdrive_failed = {
            let mut f = self.gdrive_upload_failed.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *f)
        };
        if gdrive_failed {
            self.show_upload_fallback_panel = true;
        }
    }

    /// The full git flow, shown in place of the run surface while the user is
    /// part-way through one. It is a sequence of focused steps, so it takes
    /// the surface rather than living in the rail.
    fn show_git_flow(&mut self, ui: &mut egui::Ui) {
        if matches!(self.git_state, GitState::Idle) {
            self.show_git_sheet = false;
            return;
        }
        ui.horizontal(|ui| {
            ui.label(eyebrow("SOURCE CONTROL"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([72.0, 26.0], quiet("Close")).clicked() {
                    self.show_git_sheet = false;
                    self.git_state = GitState::Idle;
                }
            });
        });
        ui.add_space(10.0);

        // The flow and the repo detail travel together — the numbers you want
        // while deciding whether to push are the ones on this screen — and
        // they use the whole surface.
        //
        // This was one column capped at 660pt and left-aligned, which on a
        // wide window left more than a third of the screen empty beside it.
        // Wide, the two panels sit side by side and each scrolls on its own;
        // narrow, they stack under a single scroll region.
        const GIT_SPLIT_MIN: f32 = 760.0;
        const COL_GAP: f32 = 18.0;
        let region = ui.available_rect_before_wrap();
        let action = if region.width() >= GIT_SPLIT_MIN {
            let col_w = ((region.width() - COL_GAP) / 2.0).floor();
            let left  = egui::Rect::from_min_size(region.min, egui::vec2(col_w, region.height()));
            let right = egui::Rect::from_min_size(
                egui::pos2(region.max.x - col_w, region.min.y), egui::vec2(col_w, region.height()));
            ui.allocate_rect(region, egui::Sense::hover());
            let td = egui::Layout::top_down(egui::Align::Min);

            let mut left_ui = ui.new_child(egui::UiBuilder::new().max_rect(left).layout(td));
            let action = egui::ScrollArea::vertical()
                .id_salt("git_flow_actions")
                .auto_shrink([false, false])
                .show(&mut left_ui, |ui| {
                    let a = self.show_git_panel(ui);
                    ui.add_space(GUTTER);
                    a
                })
                .inner;

            let mut right_ui = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(td));
            egui::ScrollArea::vertical()
                .id_salt("git_flow_status")
                .auto_shrink([false, false])
                .show(&mut right_ui, |ui| {
                    self.show_git_status_panel(ui);
                    ui.add_space(GUTTER);
                });
            action
        } else {
            egui::ScrollArea::vertical()
                .id_salt("git_flow")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let a = self.show_git_panel(ui);
                    ui.add_space(14.0);
                    self.show_git_status_panel(ui);
                    ui.add_space(GUTTER);
                    a
                })
                .inner
        };
        match action {
            GitAction::StartCommitPush          => self.git_start_commit_push(),
            GitAction::StartSync                => self.git_start_sync(),
            GitAction::StartMerge               => self.git_start_merge(),
            GitAction::StartMergeAndPackage     => self.start_merge_and_package(),
            GitAction::StartCheckout { branch } => self.git_start_checkout(branch),
            GitAction::StartNewBranch { name }  => self.git_start_new_branch(name),
            GitAction::None                     => {}
        }
    }
}

impl DevToolApp {
    /// Update availability, as one strip above the surface.
    ///
    /// This used to be a full-width banner with its own heading, date line and
    /// two buttons — a permanent block of the page for something that is
    /// usually irrelevant and never urgent. One line says the same thing.
    fn show_update_notice(&mut self, ui: &mut egui::Ui) {
        if !self.show_update_banner { return; }
        // Documentation screenshots (debug builds only) should not show the
        // update banner a development version always triggers.
        #[cfg(debug_assertions)]
        if std::env::var_os("UDT_DEMO").is_some() { return; }
        let Some(info) = self.update_info.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };

        let mut install = false;
        callout(accent())
            .inner_margin(egui::Margin::symmetric(15.0, 9.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    dot(ui, accent(), 8.0);
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(format!("Version {} is available", info.version))
                        .font(body(12.0)).color(TEXT));
                    if self.update_confirm {
                        ui.label(egui::RichText::new("replaces this .exe and restarts")
                            .font(body(11.5)).color(AMBER));
                    } else {
                        ui.label(hint(&format!("released {}", info.published_at)));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([64.0, 26.0], quiet(
                            if self.update_confirm { "Cancel" } else { "Later" })).clicked() {
                            if self.update_confirm {
                                self.update_confirm = false;
                            } else {
                                self.show_update_banner = false;
                            }
                        }
                        // Two-step, because this is irreversible: one click
                        // downloads a release and replaces the running
                        // executable with it, with no undo. Mis-clicking it
                        // while aiming at the icon row beside it swaps the
                        // binary out from under you.
                        if self.update_confirm {
                            if ui.add_sized([116.0, 26.0], chip("Confirm update", true)).clicked() {
                                install = true;
                            }
                        } else if ui.add_sized([84.0, 26.0], chip("Update", true)).clicked() {
                            self.update_confirm = true;
                        }
                    });
                });
            });
        ui.add_space(12.0);
        if install {
            self.start_update_install(info);
        }
    }
}

impl DevToolApp {
    /// Records this frame's duration and keeps a smoothed interval.
    ///
    /// `stable_dt` is egui's own smoothed frame time, which is the right input
    /// for motion: it already reflects the display's real cadence, so a 144 Hz
    /// monitor and a 60 Hz one both get the same *duration* of animation
    /// rather than the same number of steps.
    fn sample_frame_time(&mut self, ctx: &egui::Context) {
        let raw = ctx.input(|i| i.stable_dt);
        let dt = raw.clamp(1.0 / 480.0, 1.0 / 10.0);
        self.frame_dt = dt;

        // Only frames that follow another frame are a measure of smoothness.
        //
        // This app sleeps when nothing is happening, so the first frame after
        // an idle gap carries the whole gap as its delta — a 700ms pause reads
        // as a 700ms "frame". Counting those made the tail look like severe
        // stalls when it was just the app waking up, which is exactly the
        // wrong conclusion to draw from a meter.
        // The sample buffer only exists while the meter is switched on; the
        // smoothed `frame_dt` above is what normal operation needs, and it
        // costs nothing.
        if std::env::var_os("UDT_FPS").is_none() {
            return;
        }

        const CONTINUOUS_MAX: f32 = 0.050;
        if raw > CONTINUOUS_MAX {
            self.wake_frames += 1;
            return;
        }
        if self.frame_ms.len() >= 120 {
            self.frame_ms.pop_front();
        }
        self.frame_ms.push_back(dt * 1000.0);

        if self.frame_ms.len() == 120 {
            use std::io::Write;
            let mut v: Vec<f32> = self.frame_ms.iter().copied().collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let avg = v.iter().sum::<f32>() / v.len() as f32;
            let p50 = v[v.len() / 2];
            let p95 = v[v.len() * 95 / 100];
            let worst = v[v.len() - 1];
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true)
                .open(std::env::temp_dir().join("udt_fps.txt")) {
                let _ = writeln!(f, "avg={avg:.2}ms p50={p50:.2} p95={p95:.2} worst={worst:.2} -> {:.0} fps  (wakes excluded: {})",
                    1000.0 / avg, self.wake_frames);
            }
            self.frame_ms.clear();
        }
    }
}

/// A single soft glow, pushed mostly off-canvas.
///
/// The previous version drew an accent grid across the whole page plus two
/// stacked glow stacks; at any alpha where the glow read at all, the grid was
/// a visible lattice over every empty region. An aurora is something glass
/// refracts, not something you look at.
fn paint_ambient(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let c = rect.left_top() + egui::vec2(-120.0, -150.0);
    // Fewer, larger, fainter rings. Stacked fills compound, so five at alpha 2
    // read as one solid teal wash across the top-left of every screen.
    for i in 0..4 {
        let radius = 220.0 + i as f32 * 120.0;
        ui.painter().circle_filled(c, radius, acc(1));
    }
}
