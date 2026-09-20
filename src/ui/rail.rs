use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::run::Stage;
use crate::theme::*;
use crate::types::GitAction;
use crate::ui::shell::clock;

impl DevToolApp {
    /// The rail beside a Ready/Done surface: source control, then the builds
    /// you have already made.
    pub fn show_context_rail(&mut self, ui: &mut egui::Ui, scroll: bool) {
        rail_scroll(ui, "context_rail", scroll, |ui, viewport_h| {
            let top = ui.cursor().min.y;
            self.show_git_rail(ui);
            ui.add_space(14.0);
            let above = ui.cursor().min.y - top;
            self.show_recent_builds(ui, viewport_h.map(|h| (h - above).max(MIN_BUILDS_H)));
        });
    }

    /// Source control, reduced to what you check before a build.
    ///
    /// Git used to be a full page of equal weight to packaging. In practice
    /// you look at it to answer one question — am I about to build something
    /// I have not committed, or something out of date — so the rail answers
    /// that and the full state machine opens from the Git menu.
    fn show_git_rail(&mut self, ui: &mut egui::Ui) {
        if self.git_project_dir().is_none() { return; }
        let mut action = GitAction::None;
        let mut open_full = false;

        // Read the two scalars this panel needs instead of deep-cloning the
        // whole summary (a String plus two Vecs) on every frame.
        let uncommitted  = self.git_status.uncommitted;
        let ahead_behind = self.git_status.ahead_behind;
        let dirty        = uncommitted > 0;
        let branch       = std::mem::take(&mut self.git_current_branch);

        let git_card = card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(eyebrow("SOURCE CONTROL"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if dirty {
                        ui.label(egui::RichText::new(format!("{uncommitted} uncommitted"))
                            .font(body(11.5)).color(AMBER));
                    } else {
                        ui.label(egui::RichText::new("clean").font(body(11.5)).color(GREEN));
                    }
                });
            });

            ui.add_space(11.0);
            ui.label(numeral(if branch.is_empty() { "—" } else { &branch }, 13.5, TEXT));

            if let Some((ahead, behind)) = ahead_behind {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    dot(ui, if ahead > 0 { GREEN } else { FAINT }, 7.0);
                    ui.label(egui::RichText::new(format!("{ahead} ahead")).font(body(12.0)).color(SOFT));
                    ui.add_space(8.0);
                    dot(ui, if behind > 0 { AMBER } else { FAINT }, 7.0);
                    ui.label(egui::RichText::new(format!("{behind} behind")).font(body(12.0)).color(SOFT));
                });
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                if ui.add_sized([w, 32.0], chip("Sync", false)).clicked() {
                    action = GitAction::StartSync;
                }
                if ui.add_sized([w, 32.0], chip("Git menu", false)).clicked() {
                    open_full = true;
                }
            });
        });

        self.guide_anchor(crate::ui::guide::step::SOURCE, git_card.response.rect);

        // Put the branch string back before anything else can observe it.
        self.git_current_branch = branch;

        if open_full {
            self.open_git_menu();
            self.git_state = crate::types::GitState::Menu;
            self.show_git_sheet = true;
        }
        if matches!(action, GitAction::StartSync) {
            self.git_start_sync();
        }
    }

    /// Builds already on disk.
    ///
    /// New surface: the tool knew the *next* version number and nothing about
    /// any build you had actually produced. Every row is a real folder, so
    /// clicking one opens it.
    ///
    /// `fill` is the height the card should take. With it, the card ends
    /// exactly at the bottom of the rail and the *list inside it* scrolls, so
    /// every border stays on screen. Without it (a stacked layout, where an
    /// outer region scrolls) the card is as tall as its rows.
    pub fn show_recent_builds(&mut self, ui: &mut egui::Ui, fill: Option<f32>) {
        let mut open: Option<std::path::PathBuf> = None;
        let mut open_root = false;
        // Moved out and put back rather than cloned: the list holds a String
        // and a PathBuf per entry, so this was a few dozen allocations every
        // frame for data that only changes when a build finishes.
        let builds = std::mem::take(&mut self.builds);

        const FOOTER_H: f32 = 32.0;
        const CARD_PAD_Y: f32 = 16.0;
        let builds_card = card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // -2 keeps the bottom stroke inside the scroll clip; without it the
            // last hairline was half outside and drawn faint or not at all.
            let inner_h = fill.map(|h| (h - 2.0 * CARD_PAD_Y - 2.0).max(0.0));
            if let Some(h) = inner_h {
                ui.set_min_height(h);
            }
            let y0 = ui.cursor().min.y;
            let total: u64 = builds.iter().map(|b| b.bytes).sum();
            let heading = if total > 0 {
                format!("RECENT BUILDS · {} on disk", crate::ops::history::format_bytes(total))
            } else {
                "RECENT BUILDS".to_string()
            };
            ui.label(eyebrow(&heading));
            ui.add_space(11.0);

            if builds.is_empty() {
                ui.label(hint("Nothing built yet. Your first build will appear here."));
            }

            let mut rows = |ui: &mut egui::Ui| {
                for (i, b) in builds.iter().enumerate() {
                    let tip = b.info.as_ref().map(|i| i.summary());
                    if build_row(ui, &b.version, &b.age_label(), &b.size_label(), i == 0, tip.as_deref()) {
                        open = Some(b.dir.clone());
                    }
                    ui.add_space(3.0);
                }
            };
            match inner_h {
                Some(h) => {
                    // Whatever is left after the header and the footer button.
                    let sp = ui.spacing().item_spacing.y;
                    let used = ui.cursor().min.y - y0;
                    let list_h = (h - used - FOOTER_H - 8.0 - 2.0 * sp).max(40.0);
                    egui::ScrollArea::vertical()
                        .id_salt("recent_builds_list")
                        .max_height(list_h)
                        .min_scrolled_height(list_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| rows(ui));
                }
                None => rows(ui),
            }

            ui.add_space(8.0);
            if ui.add_sized([ui.available_width(), FOOTER_H], quiet("Open build folder")).clicked() {
                open_root = true;
            }
        });

        self.guide_anchor(crate::ui::guide::step::BUILDS, builds_card.response.rect);
        self.builds = builds;

        if let Some(dir) = open {
            let _ = crate::ops::cmd("explorer").arg(dir).spawn();
        }
        if open_root && let Some(root) = self.project_path.as_ref().and_then(|p| p.parent()) {
            let _ = crate::ops::cmd("explorer").arg(root.join("build")).spawn();
        }
    }

    /// The rail after a build: where the time actually went, then the usual
    /// context. The per-stage split is the part you cannot get anywhere else —
    /// "the build took 11 minutes" is far less useful than knowing the cook
    /// was 6 of them.
    pub fn show_done_rail(&mut self, ui: &mut egui::Ui, scroll: bool) {
        rail_scroll(ui, "done_rail", scroll, |ui, vp| self.show_done_rail_body(ui, vp));
    }

    fn show_done_rail_body(&mut self, ui: &mut egui::Ui, viewport_h: Option<f32>) {
        let top = ui.cursor().min.y;
        if let Some(out) = self.last_build.take() {
            let total = out.stages.iter().flatten().map(|d| d.as_secs_f32()).sum::<f32>().max(0.001);
            card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(eyebrow("STAGE TIMING"));
                ui.add_space(13.0);
                for s in Stage::ALL {
                    let d = out.stages[s.index()];
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(s.label()).font(body(12.0))
                            .color(if d.is_some() { SOFT } else { FAINT }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let t = d.map(|d| clock(d.as_secs())).unwrap_or_else(|| "—".into());
                            ui.label(numeral(&t, 11.5, DIM));
                        });
                    });
                    ui.add_space(5.0);
                    let frac = d.map(|d| d.as_secs_f32() / total).unwrap_or(0.0);
                    segment(ui, ui.available_width(), frac, accent());
                    ui.add_space(11.0);
                }
            });
            ui.add_space(14.0);
            self.last_build = Some(out);
        }
        let above = ui.cursor().min.y - top;
        self.show_recent_builds(ui, viewport_h.map(|h| (h - above).max(MIN_BUILDS_H)));
    }

    /// The rail while a build runs: Miku, the facts of this run, and Cancel.
    ///
    /// Miku moved here from being a full-screen takeover. The old busy view
    /// replaced the entire window with a GIF and a progress bar, which meant
    /// that for the 30 minutes a build takes, the app could show you nothing
    /// else — including the log you would actually want to read.
    pub fn show_run_rail(&mut self, ui: &mut egui::Ui, scroll: bool) {
        rail_scroll(ui, "run_rail", scroll, |ui, vp| self.show_run_rail_body(ui, vp));
    }

    fn show_run_rail_body(&mut self, ui: &mut egui::Ui, viewport_h: Option<f32>) {
        let top = ui.cursor().min.y;
        let ctx = ui.ctx().clone();
        let dt = ctx.input(|i| i.stable_dt);
        if !self.miku_mode_3d
            && let Some(g) = &mut self.gif_player {
                g.advance(&ctx, dt);
            }

        let mut cancel = false;
        let mut want_3d: Option<bool> = None;
        let mut pending_rect: Option<egui::Rect> = None;

        // The picture gives way first. Below it sit the timing card and the
        // Cancel button, and a window that is not very tall used to push
        // Cancel off the bottom of the rail — the one control that has to be
        // reachable during a run. ~400 is what everything else needs.
        let stage_h = match viewport_h {
            Some(h) => (h - 404.0).clamp(70.0, 238.0),
            None    => 238.0,
        };
        deep()
            .inner_margin(egui::Margin::ZERO)
            .stroke(egui::Stroke::new(1.0, acc(56)))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let w = ui.available_width();

                if self.miku_mode_3d {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, stage_h), egui::Sense::hover());
                    pending_rect = Some(rect);
                } else {
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, stage_h),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            if let Some(gif) = &self.gif_player {
                                let size = gif.size();
                                let scale = ((stage_h - 20.0) / size.y.max(1.0)).min(1.0);
                                gif.show(ui, size * scale);
                            } else {
                                ui.label(hint("working…"));
                            }
                        },
                    );
                }

                // Media controls, on one line.
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(13.0, 10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let icon = if self.audio_muted { "off" } else { "on" };
                            if ui.add_sized([38.0, 26.0], quiet(icon)).on_hover_text("Mute").clicked() {
                                let m = !self.audio_muted;
                                self.set_audio_muted(m);
                            }
                            let mut vol = self.audio_volume;
                            let sw = (ui.available_width() - 76.0).max(40.0);
                            if ui.add_sized([sw, 20.0],
                                egui::Slider::new(&mut vol, 0..=100).show_value(false)).changed() {
                                    self.set_audio_volume(vol);
                                }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let on3d = self.miku_mode_3d;
                                if ui.add_sized([30.0, 24.0], chip("3D", on3d)).clicked() {
                                    want_3d = Some(!on3d);
                                }
                            });
                        });
                    });
            });

        if let Some(rect) = pending_rect {
            self.pending_webview = Some((crate::webview::WebPanel::Miku3D, rect));
        }
        if let Some(v) = want_3d {
            self.miku_mode_3d = v;
            if !v && let Some(g) = &mut self.gif_player { g.reset(); }
        }

        ui.add_space(14.0);

        // Facts of this run, including live per-stage timing.
        let run = self.run.lock().unwrap_or_else(|e| e.into_inner());
        let times: Vec<(Stage, Option<std::time::Duration>, bool)> = Stage::ALL.iter()
            .map(|s| (*s, run.stage_time(*s), run.elapsed[s.index()].is_some()))
            .collect();
        let warnings = run.warnings;
        let errors = run.errors;
        drop(run);

        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(eyebrow("STAGE TIMING"));
            ui.add_space(12.0);
            for (s, d, done) in &times {
                ui.horizontal(|ui| {
                    let c = if *done { GREEN } else if d.is_some() { TEXT } else { FAINT };
                    ui.label(egui::RichText::new(s.label()).font(body(12.0)).color(c));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let t = d.map(|d| clock(d.as_secs())).unwrap_or_else(|| "—".into());
                        ui.label(numeral(&t, 11.5, if *done { SOFT } else { DIM }));
                    });
                });
                ui.add_space(6.0);
            }
            if warnings > 0 || errors > 0 {
                ui.add_space(4.0);
                divider(ui);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if errors > 0 {
                        dot(ui, RED, 7.0);
                        ui.label(egui::RichText::new(format!("{errors} {}", if errors == 1 { "error" } else { "errors" })).font(body(12.0)).color(RED));
                        ui.add_space(8.0);
                    }
                    if warnings > 0 {
                        dot(ui, AMBER, 7.0);
                        ui.label(egui::RichText::new(format!("{warnings} {}", if warnings == 1 { "warning" } else { "warnings" })).font(body(12.0)).color(AMBER));
                    }
                });
            }
        });

        // Pushed to the bottom only when there is room to push it to. A
        // `bottom_up` layout gets nothing to work with on a short rail and
        // draws the button over whatever is above it.
        //
        // Measured against the viewport, not `available_height()`: inside a
        // scroll area that is unbounded, so this used to ask for an
        // effectively infinite gap.
        let slack = match viewport_h {
            Some(h) => h - (ui.cursor().min.y - top) - 44.0 - 3.0 * ui.spacing().item_spacing.y,
            None => 0.0,
        };
        ui.add_space(if slack > 0.0 { slack } else { 12.0 });
        // Two clicks. The first arms the button for a few seconds; the second
        // confirms. Killing a build 25 minutes in is not something to do with
        // one accidental click.
        let armed = self.cancel_armed
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(4));
        if !armed { self.cancel_armed = None; }
        let label = if armed { "Click again to stop the build" } else { "Cancel build" };
        if ui.add_sized([ui.available_width(), 44.0], danger(label)).clicked() {
            if armed { cancel = true; self.cancel_armed = None; }
            else { self.cancel_armed = Some(std::time::Instant::now()); }
        }
        if armed { ui.ctx().request_repaint_after(std::time::Duration::from_millis(500)); }

        if cancel {
            self.cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            self.set_status("[CANCELLING] Stopping — please wait…".into());
        }
    }
}

use crate::ui::run::divider;

/// A rail column that scrolls when its cards do not fit.
///
/// Every rail goes through this rather than stretching its cards to fill:
/// cards keep their natural height and the column scrolls, which is the one
/// arrangement that holds at any window size without per-size tuning.
fn rail_scroll<R>(
    ui: &mut egui::Ui,
    id: &str,
    scroll: bool,
    add: impl FnOnce(&mut egui::Ui, Option<f32>) -> R,
) -> R {
    if !scroll {
        // An outer region already scrolls this content; adding another here
        // would show a second bar beside the first.
        return add(ui, None);
    }
    // The height the rail really has, taken before entering the scroll area
    // (inside it the height is unbounded). The trailing gutter is part of the
    // scrolled content, so it is not available to the cards.
    // The trailing gutter and the item spacing that precedes it both come
    // out of the cards' share, or the content overflows by a few points and a
    // scrollbar appears on a rail that fits.
    let viewport_h = (ui.available_height() - GUTTER - ui.spacing().item_spacing.y - 3.0).max(0.0);
    let res = egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let out = add(ui, Some(viewport_h));
            ui.add_space(GUTTER);
            out
        });
    res.inner
}

/// Least height the recent-builds card is given before the rail itself
/// scrolls: header, a couple of rows and the footer button.
const MIN_BUILDS_H: f32 = 170.0;

/// One row in the recent-builds list.
fn build_row(ui: &mut egui::Ui, version: &str, age: &str, size: &str, newest: bool, tip: Option<&str>) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 34.0), egui::Sense::click());
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), T_FAST);

    let base = if newest { acc(16) } else { egui::Color32::TRANSPARENT };
    let fill = if hov > 0.0 { mix(base, acc(30), hov) } else { base };
    ui.painter().rect_filled(rect, egui::Rounding::same(10.0), fill);

    // Laid out from measured text, not fixed pixel offsets.
    //
    // Version sat at +11, age at a hard-coded +78 and size right-aligned; in a
    // narrow rail the age ran straight through the size and both were
    // illegible. The age is the least important of the three, so it is what
    // gets dropped when the row cannot fit all of it.
    let version_pos = rect.left_center() + egui::vec2(11.0, 0.0);
    let size_pos    = rect.right_center() - egui::vec2(11.0, 0.0);

    let version_w = ui.fonts(|f| f.layout_no_wrap(
        version.to_owned(), mono(12.5), TEXT).size().x);
    let size_w = ui.fonts(|f| f.layout_no_wrap(
        size.to_owned(), mono(11.5), DIM).size().x);
    let age_w = ui.fonts(|f| f.layout_no_wrap(
        age.to_owned(), body(11.5), MUTED).size().x);

    let p = ui.painter();
    p.text(version_pos, egui::Align2::LEFT_CENTER,
           version, mono(12.5), if newest { TEXT } else { SOFT });
    p.text(size_pos, egui::Align2::RIGHT_CENTER,
           size, mono(11.5), DIM);

    let age_start = version_pos.x + version_w + 10.0;
    let age_limit = size_pos.x - size_w - 10.0;
    if age_start + age_w <= age_limit {
        p.text(egui::pos2(age_start, rect.center().y), egui::Align2::LEFT_CENTER,
               age, body(11.5), MUTED);
    }

    if hov > 0.0 && hov < 1.0 { ui.ctx().request_repaint(); }
    let hover = match tip {
        Some(t) => format!("{t}
Click to open this build's folder"),
        None    => "Open this build's folder".to_string(),
    };
    resp.on_hover_text(hover).clicked()
}
