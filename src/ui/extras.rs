use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::ExtrasTab;

impl DevToolApp {
    pub fn show_extras_tab(&mut self, ui: &mut egui::Ui) {
        card_frame().inner_margin(egui::Margin::symmetric(6.0, 6.0)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let tabs: &[(ExtrasTab, &str)] = &[
                    (ExtrasTab::Miku,      "💗 Miku"),
                    (ExtrasTab::Games,     "🎮 Games"),
                    (ExtrasTab::SelfCheck, "⚙ Self-Check"),
                    (ExtrasTab::Discord,   "💬 Discord"),
                    (ExtrasTab::Customize, "🎨 Customize"),
                ];
                for (tab, label) in tabs {
                    let selected = self.extras_tab == *tab;
                    let btn = egui::Button::new(
                        egui::RichText::new(*label).size(10.5)
                            .color(if selected { egui::Color32::WHITE } else { HINT_GRAY }),
                    )
                    .fill(if selected { PANEL_BG } else { egui::Color32::TRANSPARENT })
                    .stroke(if selected { egui::Stroke::new(1.0, accent()) } else { egui::Stroke::NONE });
                    if ui.add_sized([96.0, 26.0], btn).clicked() && !selected {
                        self.extras_tab = *tab;
                        if *tab == ExtrasTab::SelfCheck { self.refresh_app_check(); }
                    }
                }
            });
        });
        ui.add_space(10.0);

        match self.extras_tab {
            ExtrasTab::Miku      => self.show_miku_extra(ui),
            ExtrasTab::Games     => self.show_games_extra(ui),
            ExtrasTab::SelfCheck => self.show_app_check_panel(ui),
            ExtrasTab::Discord   => self.show_dm_spencer_panel(ui),
            ExtrasTab::Customize => self.show_media_config_panel(ui),
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_quick_links(ui);
    }

    fn show_miku_extra(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Miku Visualizer").size(13.0).color(accent()));
                ui.add_space(10.0);

                let ctx = ui.ctx().clone();
                let dt  = ctx.input(|i| i.stable_dt);
                if let Some(gif) = &mut self.gif_player { gif.advance(&ctx, dt); }

                egui::Frame::none()
                    .fill(GIF_BG)
                    .stroke(egui::Stroke::new(1.0, accent()))
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        if let Some(gif) = &self.gif_player {
                            gif.show(ui, egui::vec2(180.0, 180.0));
                        } else {
                            ui.allocate_exact_size(egui::vec2(180.0, 180.0), egui::Sense::hover());
                        }
                    });

                ui.add_space(10.0);
                if ui.add_sized([170.0, 28.0], egui::Button::new("🧊  View 3D Model")).clicked() {
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
            let w = [ui.available_width(), 40.0];
            if ui.add_sized(w, egui::Button::new("🍪  Cookie Clicker")).clicked() {
                self.active_web_panel = Some(crate::webview::WebPanel::CookieClicker);
            }
            ui.add_space(8.0);
            if ui.add_sized(w, egui::Button::new("🐦  Sponder Bird")).clicked() {
                self.active_web_panel = Some(crate::webview::WebPanel::SponderBird);
            }
        });
    }

    fn show_quick_links(&mut self, ui: &mut egui::Ui) {
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
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("📘  Unreal Docs")).clicked() {
            crate::ops::open_url("https://dev.epicgames.com/community/assistant/unreal-engine");
        }
    }
}
