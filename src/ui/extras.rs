use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::ExtrasTab;

impl DevToolApp {
    /// Extras tab: a left sidebar (sub-nav + Quick Links, matching the
    /// reference mockup's `FunExtras.tsx`) and a wider content area on the
    /// right, now that there's room for a real sidebar instead of a
    /// horizontal tab strip.
    pub fn show_extras_tab(&mut self, ui: &mut egui::Ui) {
        // `ui.horizontal` defaults to `Layout::left_to_right(Align::Center)`,
        // which vertically centers the sidebar and main-content children
        // relative to *each other* — whichever one is shorter this frame
        // drifts down instead of both starting flush at the top edge.
        // `horizontal_top` is the same layout with `Align::Min` instead,
        // so both columns start at the same top edge regardless of height.
        ui.horizontal_top(|ui| {
            let total     = ui.available_width();
            let gap       = ui.spacing().item_spacing.x;
            let sidebar_w = (total * 0.24).clamp(170.0, 230.0);
            let main_w    = total - sidebar_w - gap;

            // `ui.scope` inherits the ambient layout direction, which inside
            // this `ui.horizontal_top` is left-to-right — that flattened the
            // sidebar into a single horizontal row instead of stacking it.
            // `allocate_ui_with_layout` resets the layout explicitly.
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(sidebar_w, 0.0), top_down, |ui| {
                self.show_extras_sidebar(ui);
            });
            ui.allocate_ui_with_layout(egui::vec2(main_w, 0.0), top_down, |ui| {
                match self.extras_tab {
                    ExtrasTab::Miku      => self.show_miku_extra(ui),
                    ExtrasTab::Games     => self.show_games_extra(ui),
                    ExtrasTab::SelfCheck => self.show_app_check_panel(ui),
                    ExtrasTab::Discord   => self.show_dm_spencer_panel(ui),
                    ExtrasTab::Customize => self.show_media_config_panel(ui),
                }
            });
        });
    }

    fn show_extras_sidebar(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("EXTRAS").size(10.5).color(HINT_GRAY));
            ui.add_space(8.0);

            let tabs: &[(ExtrasTab, &str)] = &[
                (ExtrasTab::Miku,      "💗  Miku Visualizer"),
                (ExtrasTab::Games,     "🎮  Mini-Games"),
                (ExtrasTab::SelfCheck, "⚙  App Self-Check"),
                (ExtrasTab::Discord,   "💬  DM on Discord"),
                (ExtrasTab::Customize, "🎨  Customize"),
            ];
            for (tab, label) in tabs {
                let selected = self.extras_tab == *tab;
                let btn = egui::Button::new(
                    egui::RichText::new(*label).size(11.0)
                        .color(if selected { egui::Color32::WHITE } else { egui::Color32::LIGHT_GRAY }),
                )
                .fill(if selected { PANEL_BG } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::new(1.0, if selected { accent() } else { CARD_BORDER }));
                if ui.add_sized([ui.available_width(), 30.0], btn).clicked() && !selected {
                    self.extras_tab = *tab;
                    if *tab == ExtrasTab::SelfCheck { self.refresh_app_check(); }
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(10.0);

        card_frame().show(ui, |ui| {
            self.show_quick_links(ui);
        });
    }

    fn show_miku_extra(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Miku Visualizer").size(13.0).color(accent()));
                ui.add_space(10.0);

                let ctx = ui.ctx().clone();
                let dt  = ctx.input(|i| i.stable_dt);
                if let Some(gif) = &mut self.gif_player { gif.advance(&ctx, dt); }

                // Was a fixed 180x180 in a card that's often 700px+ wide —
                // tiny preview adrift in a lot of empty fill color. Sizing
                // relative to the available column (capped so it doesn't
                // get absurd on an ultrawide window) uses that space
                // instead of just leaving it blank.
                let gif_size = ui.available_width().min(420.0).max(180.0);
                egui::Frame::none()
                    .fill(GIF_BG)
                    .stroke(egui::Stroke::new(1.0, accent()))
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        if let Some(gif) = &self.gif_player {
                            gif.show(ui, egui::vec2(gif_size, gif_size));
                        } else {
                            ui.allocate_exact_size(egui::vec2(gif_size, gif_size), egui::Sense::hover());
                        }
                    });

                ui.add_space(10.0);
                if ui.add_sized([220.0, 32.0], egui::Button::new("🧊  View 3D Model")).clicked() {
                    self.active_web_panel = Some(crate::webview::WebPanel::Miku3D);
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Full mouse-look pointer-lock support in 3D mode.")
                        .size(10.0).color(HINT_GRAY),
                );
            });
        });
    }

    fn show_games_extra(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Embedded Mini-Games").size(13.0).color(accent()));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Take a break — quick browser games embedded right in the app.")
                    .size(10.5).color(HINT_GRAY),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                let w   = [(ui.available_width() - gap) / 2.0, 60.0];
                if ui.add_sized(w, egui::Button::new("🍪  Cookie Clicker")).clicked() {
                    self.active_web_panel = Some(crate::webview::WebPanel::CookieClicker);
                }
                if ui.add_sized(w, egui::Button::new("🐦  Sponder Bird")).clicked() {
                    self.active_web_panel = Some(crate::webview::WebPanel::SponderBird);
                }
            });
        });
    }

    /// Quick Links: a user-editable list of label+URL buttons (persisted to
    /// `links.json`). Links with no URL set (e.g. Trello/Jira — there's no
    /// universal default for a team's own board) open the editor instead of
    /// navigating nowhere when clicked.
    pub(crate) fn show_quick_links(&mut self, ui: &mut egui::Ui) {
        // Cap on the scrollable list's height — see the long comment further
        // down (`ui.allocate_ui`) for why this needs to be an explicit
        // allocated height rather than relying on `ScrollArea::max_height`
        // alone.
        const QUICK_LINKS_MAX_H: f32 = 260.0;

        // Pin this card's width up front. Without it, the nested
        // ScrollArea/allocate_ui below (needed to cap the list's height —
        // see the comment further down) measured its own used width as
        // slightly *wider* than the sidebar column once a vertical
        // scrollbar appeared in edit mode, and `Frame::show` grew the
        // card to match — visibly wider than the nav card above it.
        // `set_max_width` gives every nested widget a hard ceiling so that
        // can't happen regardless of the exact cause.
        ui.set_max_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Quick Links").size(11.0).color(HINT_GRAY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.links_edit_mode { "Done" } else { "✏ Edit" };
                if ui.add_sized([64.0, 22.0], egui::Button::new(label)).clicked() {
                    self.links_edit_mode = !self.links_edit_mode;
                }
            });
        });
        ui.add_space(6.0);

        // The sidebar column has no fixed height, and the list can grow
        // without bound (the user can keep hitting "+ Add Link"). Left to
        // its natural size, a long list pushed the nav card and this whole
        // card taller than the window, silently shoving later entries
        // (Jira, Task List, Requirement Check, "+ Add Link") below the
        // visible area with no obvious sign there was more to scroll to —
        // that's what read as "the UI breaking". Capping this list in its
        // own scroll region keeps the card a predictable height and makes
        // the overflow indicator (the scrollbar) show up right where the
        // cut-off content is, instead of at the bottom of the whole page.
        //
        // `ScrollArea::max_height` alone isn't enough here: it clamps to
        // `min(max_height, ui.available_height())`, and this sidebar column
        // is itself inside a container allocated with a *zero* height hint
        // (so content isn't forced to reserve unused space) — which left
        // `available_height()` reporting only a sliver, not 260. Explicitly
        // allocating a concrete 260-tall child `Ui` first sidesteps that;
        // the ScrollArea then just fills the space it was actually given.
        ui.allocate_ui(egui::vec2(ui.available_width(), QUICK_LINKS_MAX_H), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
        if self.links_edit_mode {
            // The sidebar column is only ~170-230px wide (minus the card's
            // own padding). Packing a 110px label field + a URL field + a
            // delete button into one horizontal row (the original layout)
            // left the URL field with a *negative* desired width once the
            // label field and padding were subtracted — it rendered
            // squashed to nothing. Stacking label+button on one row and the
            // URL on its own full-width row below fits comfortably instead.
            // `TextEdit::desired_width` turned out to be a *minimum*, not a
            // cap — once a field held a long-enough label/URL, the widget
            // measured itself wider than the hint and pushed everything
            // after it (the delete button, then the whole card) further
            // right each row, growing with label length ("Epic Games"
            // drifted further than "Claude"). `add_sized` allocates a fixed
            // rect up front and fits the widget *into* it instead, which
            // actually caps it.
            let mut to_remove = None;
            for i in 0..self.custom_links.len() {
                let mut changed = false;
                ui.horizontal(|ui| {
                    let btn_w = 24.0;
                    let label_w = (ui.available_width() - btn_w - ui.spacing().item_spacing.x).max(30.0);
                    changed |= ui.add_sized(
                        [label_w, 22.0],
                        egui::TextEdit::singleline(&mut self.custom_links[i].label).hint_text("Label"),
                    ).changed();
                    if ui.add_sized([btn_w, 22.0], egui::Button::new("🗑")).clicked() {
                        to_remove = Some(i);
                    }
                });
                let url_w = ui.available_width();
                changed |= ui.add_sized(
                    [url_w, 22.0],
                    egui::TextEdit::singleline(&mut self.custom_links[i].url).hint_text("https://…"),
                ).changed();
                if changed { self.save_links(); }
                ui.add_space(6.0);
                if i + 1 < self.custom_links.len() {
                    // A manually-painted line instead of `ui.separator()`:
                    // the separator widget was the original source of the
                    // width-growth bug above (it measures itself unbounded
                    // in a vertical layout), and `allocate_exact_size` can't
                    // repeat that mistake since the size isn't a hint.
                    let w = ui.available_width();
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, CARD_BORDER));
                    ui.add_space(6.0);
                }
            }
            if let Some(i) = to_remove { self.remove_custom_link(i); }
            ui.add_space(2.0);
            if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("+ Add Link")).clicked() {
                self.add_custom_link();
            }
        } else {
            for i in 0..self.custom_links.len() {
                let has_url = !self.custom_links[i].url.trim().is_empty();
                let label = if has_url {
                    self.custom_links[i].label.clone()
                } else {
                    format!("{}  (not set)", self.custom_links[i].label)
                };
                let btn = egui::Button::new(
                    egui::RichText::new(label).size(11.0)
                        .color(if has_url { egui::Color32::LIGHT_GRAY } else { HINT_GRAY }),
                );
                if ui.add_sized([ui.available_width(), 26.0], btn).clicked() {
                    if has_url {
                        crate::ops::open_url(&self.custom_links[i].url.clone());
                    } else {
                        self.links_edit_mode = true;
                    }
                }
                ui.add_space(3.0);
            }
        }
        });
        });
    }
}
