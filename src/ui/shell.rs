use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::{RunState, Sheet};

/// Height of the top bar. The only chrome the app has.
pub const TOPBAR_H: f32 = 54.0;

/// Give the compact top bar a second row when the host window is narrower
/// than the normal desktop layout. This keeps the controls visible instead
/// of letting a fixed right rail push them off-screen.
/// Width the icon cluster needs: eight 32px buttons, seven 8px gaps, and the
/// trailing inset.
pub const ICONS_W: f32 = 8.0 * 32.0 + 7.0 * 8.0 + 14.0;

/// Smallest identity region worth keeping on the same row as the icons.
const IDENTITY_MIN: f32 = 180.0;

/// True when the bar has to stack into two rows.
pub fn topbar_stacked(width: f32) -> bool {
    width < ICONS_W + IDENTITY_MIN
}

pub fn topbar_height(width: f32) -> f32 {
    if topbar_stacked(width) { 90.0 } else { TOPBAR_H }
}

impl DevToolApp {
    /// Top bar: identity on the left, live context in the middle, and the six
    /// secondary surfaces on the right.
    ///
    /// This replaced a 216px sidebar that was on screen at all times to offer
    /// five destinations you rarely switch between. Everything it held is
    /// either context (now inline here, where it is read rather than clicked)
    /// or a sheet (the icons), and the build surface gets the whole window.
    pub fn show_topbar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, egui::Rounding::ZERO, topbar_background());
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, LINE));

        let (left_rect, right_rect) = topbar_rects(rect);
        let left_w = left_rect.width();

        let mut left_ui = ui.new_child(egui::UiBuilder::new().max_rect(left_rect));
        left_ui.set_clip_rect(left_rect);
        left_ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let compact = left_w < 620.0;
            ui.add_space(20.0);

            // Brand.
            let (mark, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
            ui.painter().rect_filled(mark, egui::Rounding::same(8.0), acc(40));
            paint_icon(ui.painter(), mark.center(), Icon::Note, accent());
            ui.add_space(3.0);
            ui.label(egui::RichText::new("UNREAL DEVTOOL").font(body(11.5)).color(text_primary()));

            ui.add_space(8.0);
            let (sep, _) = ui.allocate_exact_size(egui::vec2(1.0, 18.0), egui::Sense::hover());
            ui.painter().rect_filled(sep, egui::Rounding::ZERO, LINE);
            ui.add_space(8.0);

            // Live context. Read-only — this is state, not navigation.
            let state = self.run_state();
            let (light, name) = match (&self.project_path, state) {
                (Some(p), RunState::Running) => (AMBER, p.file_stem().unwrap_or_default().to_string_lossy().to_string()),
                (Some(p), _) => (GREEN, p.file_stem().unwrap_or_default().to_string_lossy().to_string()),
                (None, _) => (RED, "no project".to_string()),
            };
            dot(ui, light, 9.0);
            ui.add_space(3.0);
            // Natural width, truncating only when the bar actually runs out.
            //
            // These were `add_sized` with fixed widths (150 / 100 / 128), which
            // reserves the full box whatever the text measures — so "UE 5.7" in
            // a 100px slot left a visible hole, and the three context items sat
            // scattered across the bar with gaps between them. `Label::truncate`
            // takes only the space it needs and shortens when crowded.
            ui.add(egui::Label::new(
                egui::RichText::new(name).font(body(13.0)).color(text_primary())).truncate());

            if !compact && let Some(engine) = &self.engine_dir {
                let v = engine.file_name().map(|n| n.to_string_lossy().replace('_', " "))
                    .unwrap_or_else(|| "engine".into());
                ui.add_space(8.0);
                ui.add(egui::Label::new(
                    egui::RichText::new(v).font(body(12.0)).color(text_muted())).truncate());
            }

            if !compact && !self.git_current_branch.is_empty() && self.git_project_dir().is_some() && ui.available_width() >= 110.0 {
                ui.add_space(8.0);
                let branch = self.git_current_branch.clone();
                // A pill again. As a bare label it read as one more piece of
                // static text, giving no sign it is the way into the git flow.
                // `chip` is the current pill equivalent, and being a real
                // button it also carries the hover/press affordance a plain
                // label was missing.
                let chip_w = ui.available_width().clamp(70.0, 150.0);
                let clicked = ui.add_sized([chip_w, 24.0], chip(&branch, false))
                    .on_hover_text("Source control")
                    .clicked();
                if clicked {
                    // The pill is the shortcut into the git flow, so branch
                    // state is both the indicator and the way in.
                    self.open_git_menu();
                    self.sheet = None;
                    self.show_git_sheet = true;
                }
            }

        });

        // Reserve this rectangle before laying out context so the controls
        // remain reachable at the minimum supported window width.
        let mut right_ui = ui.new_child(egui::UiBuilder::new().max_rect(right_rect));
        right_ui.set_clip_rect(right_rect);
        right_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(14.0);
                let open = self.sheet;
                let extras = icon_button(ui, Icon::Dots, open == Some(Sheet::Extras))
                    .on_hover_text("Extras");
                self.guide_anchor(crate::ui::guide::step::EXTRAS, extras.rect);
                if extras.clicked() {
                        self.toggle_sheet(Sheet::Extras);
                    }
                let chat = icon_button(ui, Icon::Chat, open == Some(Sheet::Chat))
                    .on_hover_text("Dev Assistant");
                self.guide_anchor(crate::ui::guide::step::CHAT, chat.rect);
                if chat.clicked() {
                        self.toggle_sheet(Sheet::Chat);
                    }
                let browser = icon_button(ui, Icon::Globe, open == Some(Sheet::Browser))
                    .on_hover_text("Browser");
                self.guide_anchor(crate::ui::guide::step::BROWSER, browser.rect);
                if browser.clicked() {
                    self.toggle_sheet(Sheet::Browser);
                }
                let settings = icon_button(ui, Icon::Gear, open == Some(Sheet::Settings))
                    .on_hover_text("Settings");
                self.guide_anchor(crate::ui::guide::step::SETTINGS, settings.rect);
                if settings.clicked() {
                    self.toggle_sheet(Sheet::Settings);
                }
                let checks = icon_button(ui, Icon::Check, open == Some(Sheet::Diagnostics))
                    .on_hover_text("Project setup & checks");
                // On Setup this is the way to change the detected project;
                // on Ready it is also the route to the detailed prechecks.
                self.guide_anchor(crate::ui::guide::step::PROJECT, checks.rect);
                self.guide_anchor(crate::ui::guide::step::CHECKS, checks.rect);
                if checks.clicked() {
                    self.toggle_sheet(Sheet::Diagnostics);
                }
                let tools = icon_button(ui, Icon::Wrench, open == Some(Sheet::Tools))
                    .on_hover_text("Unreal tools — commandlets, launch, size analysis, plugins");
                self.guide_anchor(crate::ui::guide::step::TOOLS, tools.rect);
                if tools.clicked() {
                    self.toggle_sheet(Sheet::Tools);
                }
                let monitor = icon_button(ui, Icon::Pulse, open == Some(Sheet::Monitor))
                    .on_hover_text("Project monitor — live processes, editor log and project health");
                self.guide_anchor(crate::ui::guide::step::MONITOR, monitor.rect);
                if monitor.clicked() {
                    self.toggle_sheet(Sheet::Monitor);
                }
                let help = icon_button(ui, Icon::Help, self.guide_active)
                    .on_hover_text("Manual — a guided tour of every page");
                if help.clicked() {
                    if self.guide_active {
                        self.close_guide();
                    } else {
                        self.open_guide();
                    }
                }

            });
    }

    fn toggle_sheet(&mut self, sheet: Sheet) {
        if self.sheet == Some(sheet) {
            self.close_sheet();
        } else {
            self.open_sheet(sheet);
        }
    }

    /// The sheet layer: a dimmed backdrop over the run surface and a panel on
    /// top of it, animated in.
    ///
    /// Sheets sit over the work rather than replacing it because none of them
    /// are places you stay — you glance at a chat reply or a doc page and go
    /// back to the build, which is still visible behind.
    pub fn show_sheet_layer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(sheet) = self.sheet else { return };

        let id = egui::Id::new("sheet_fade").with(sheet_key(sheet));
        let t = ease_out(ctx.animate_bool_with_time(id, true, anim_secs(T_BASE, self.frame_dt)));

        let screen = ui.ctx().screen_rect();
        // Claim the whole screen for this layer.
        //
        // The sheet is drawn inside a foreground `Area` that sizes itself to
        // its content. Without this the area's rect did not cover the pointer,
        // so egui routed wheel events to the page *underneath* — the sheet
        // painted correctly but would not scroll, which left everything below
        // the fold (Clean project, the log scanner) unreachable.
        ui.set_min_size(screen.size());
        ui.expand_to_include_rect(screen);

        // Painted into *this* ui, not a layer of its own.
        //
        // A separate `layer_painter` at the same Foreground order was created
        // after the Area, so it painted on top of the sheet instead of behind
        // it — the panel's own chrome came out dimmed while the embedded
        // WebView2 control (a native child window, which egui cannot dim at
        // all) stayed bright. Drawing it here puts it under the panel in the
        // one layer, which is where a backdrop belongs.
        ui.painter().rect_filled(
            screen,
            egui::Rounding::ZERO,
            egui::Color32::from_black_alpha((190.0 * t) as u8),
        );

        // Clicking the backdrop closes, the way a sheet should.
        let backdrop = ui.interact(screen, egui::Id::new("sheet_backdrop_hit"), egui::Sense::click());

        let margin = egui::vec2(SHEET_GUTTER, SHEET_GUTTER);
        let panel = egui::Rect::from_min_max(
            screen.min + margin + egui::vec2(0.0, (1.0 - t) * 14.0),
            screen.max - margin + egui::vec2(0.0, (1.0 - t) * 14.0),
        );

        let mut close = backdrop.clicked();
        // No explicit layer: the caller already draws this inside a
        // foreground `Area`, so a plain child ui paints above the surface.
        let mut area_ui = ui.new_child(egui::UiBuilder::new().max_rect(panel));
        area_ui.set_clip_rect(panel);
        area_ui.multiply_opacity(t);

        egui::Frame::none()
            .fill(surface_card())
            .stroke(egui::Stroke::new(1.0, surface_line()))
            .rounding(egui::Rounding::same(20.0))
            .inner_margin(egui::Margin::ZERO)
            .show(&mut area_ui, |ui| {
                ui.set_min_size(panel.size());
                ui.vertical(|ui| {
                    // Sheet header.
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 46.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(18.0);
                            ui.label(heading(sheet.title(), 15.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(14.0);
                                if ui.add_sized([70.0, 28.0], ghost("Close")).clicked() {
                                    close = true;
                                }
                            });
                        },
                    );
                    let line = ui.max_rect();
                    ui.painter().hline(
                        line.x_range().shrink(0.0),
                        ui.min_rect().bottom(),
                        egui::Stroke::new(1.0, LINE_SOFT),
                    );
                    ui.add_space(6.0);

                    // Sheet body.
                    let body_rect = egui::vec2(ui.available_width(), ui.available_height());
                    ui.allocate_ui_with_layout(
                        body_rect,
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_size(body_rect);
                            egui::Frame::none()
                                .inner_margin(egui::Margin::same(SHEET_PAD))
                                .show(ui, |ui| self.show_sheet_body(ui, sheet));
                        },
                    );
                });
            });

        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_sheet();
        }
        if t < 1.0 {
            ctx.request_repaint();
        }
    }

    fn show_sheet_body(&mut self, ui: &mut egui::Ui, sheet: Sheet) {
        match sheet {
            Sheet::Chat        => self.show_chat_panel_ui(ui),
            Sheet::Browser     => self.show_browser_tab(ui),
            Sheet::Extras      => self.show_extras_tab(ui),
            Sheet::Monitor     => self.show_monitor_sheet(ui),
            Sheet::Tools       => self.show_tools_sheet(ui),
            Sheet::Diagnostics => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.show_project_checks_sheet(ui);
                });
            }
            Sheet::Settings    => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.show_media_config_panel(ui);
                });
            }
        }
    }
}

fn sheet_key(s: Sheet) -> usize {
    match s {
        Sheet::Chat => 1,
        Sheet::Browser => 2,
        Sheet::Extras => 3,
        Sheet::Diagnostics => 4,
        Sheet::Settings => 5,
        Sheet::Monitor => 6,
        Sheet::Tools => 7,
    }
}

/// `mm:ss`, or `h:mm:ss` past an hour.
pub fn clock(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Splits the bar into the identity region and the icon region.
///
/// Wide, they sit side by side. Narrow, they stack into two rows — identity
/// above, icons below — which is what the taller compact bar is for.
///
/// The narrow case used to split horizontally anyway and hand the icons the
/// whole width, which left the identity region zero-wide: the brand, the
/// project name and the status dot all disappeared, and the icons floated in
/// a 90px-tall bar with an empty row above them.
fn topbar_rects(rect: egui::Rect) -> (egui::Rect, egui::Rect) {
    if topbar_stacked(rect.width()) {
        let split_y = rect.min.y + rect.height() * 0.5;
        return (
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split_y)),
            egui::Rect::from_min_max(egui::pos2(rect.min.x, split_y), rect.max),
        );
    }
    let split_x = (rect.max.x - ICONS_W).max(rect.min.x);
    (
        egui::Rect::from_min_max(rect.min, egui::pos2(split_x, rect.max.y)),
        egui::Rect::from_min_max(egui::pos2(split_x, rect.min.y), rect.max),
    )
}

#[cfg(test)]
mod tests {
    use super::{topbar_height, topbar_rects, TOPBAR_H};
    use eframe::egui;

    #[test]
    fn topbar_children_cover_width_without_vertical_collapse() {
        for width in [620.0, 1040.0] {
            let rect = egui::Rect::from_min_max(
                egui::pos2(17.0, 23.0),
                egui::pos2(17.0 + width, 77.0),
            );
            let (left, right) = topbar_rects(rect);
            assert!(left.height() > 0.0 && right.height() > 0.0);
            assert_eq!(left.height(), rect.height());
            assert_eq!(right.height(), rect.height());
            assert!(left.max.x <= right.min.x);
            assert!((left.width() + right.width() - rect.width()).abs() < f32::EPSILON);
            assert!(right.width() >= super::ICONS_W);
        }
    }

    /// Narrow bars stack, and both rows stay usable.
    ///
    /// This previously asserted `left.width() == 0.0` — it encoded the bug as
    /// the intended behaviour, which is why the brand and project name simply
    /// vanished below the threshold.
    #[test]
    fn compact_topbar_stacks_into_two_usable_rows() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(255.0, 90.0));
        let (identity, icons) = topbar_rects(rect);
        assert_eq!(identity.width(), rect.width(), "identity row keeps the full width");
        assert_eq!(icons.width(), rect.width(), "icon row keeps the full width");
        assert!(identity.height() > 0.0 && icons.height() > 0.0, "both rows are visible");
        assert!(identity.max.y <= icons.min.y, "identity sits above the icons");
        assert!(
            (identity.height() + icons.height() - rect.height()).abs() < f32::EPSILON,
            "the two rows cover the bar exactly",
        );
        assert_eq!(topbar_height(rect.width()), 90.0);
        assert_eq!(topbar_height(620.0), TOPBAR_H);
    }
}
