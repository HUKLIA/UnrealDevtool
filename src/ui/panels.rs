use eframe::egui;
use crate::app::DevToolApp;
use crate::config::{clear_project_path, save_project_path, save_ui_config, UiConfig};
use crate::theme::*;

// Panels carried over from the previous UI.
//
// These were already doing their job — a path picker, the media and accent
// settings, the Discord composer, the embedded web panel — so they moved here
// intact rather than being rewritten for the sake of it, restyled onto the new
// tokens along with everything else. The surfaces that needed rethinking (the
// run, the rail, setup, the shell) are new files.
impl DevToolApp {
    pub fn show_media_config_panel(&mut self, ui: &mut egui::Ui) {
        // Was its own bespoke `Frame::none()` (CARD fill, accent
        // stroke) — every Extras sub-panel used to build a slightly
        // different frame (Miku used `card()`, Discord had a custom
        // fill color, this one an accent-colored stroke), which read as
        // visually inconsistent flipping between them. `card()` is
        // the one shared frame the rest of the app already uses.
        card().show(ui, |ui| {
                ui.label(egui::RichText::new("🎨  Customize Miku & Sound").size(13.0).color(accent()));
                ui.add_space(10.0);

                ui.label(egui::RichText::new("2D Image / GIF").size(11.0).color(egui::Color32::GRAY));
                ui.add_space(4.0);
                let ctx = ui.ctx().clone();
                // Plain `ui.horizontal` (`Align::Center`) vertically centers
                // the 96px thumbnail frame against the multi-line details
                // column next to it — the shorter of the two drifted toward
                // the middle instead of both starting at the same top edge.
                // `horizontal_top` (`Align::Min`) fixes that.
                ui.horizontal_top(|ui| {
                    let thumb_max = 96.0;
                    egui::Frame::none()
                        .fill(surface_deep())
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
                        ui.label(egui::RichText::new(gif_label).size(10.0).color(MUTED).monospace());
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
                    ui.label(egui::RichText::new(hint).size(10.0).color(MUTED).monospace());
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

                // A compact hue/value palette inspired by the supplied color
                // wheel. One selection drives the shared accent and the
                // surface tint functions in theme.rs, so the whole app moves
                // together rather than changing a single button color.
                let mut picked = None;
                for row in 0..5 {
                    ui.horizontal(|ui| {
                        if row % 2 == 1 { ui.add_space(14.0); }
                        for col in 0..12 {
                            let color = crate::theme::theme_swatch(col, row);
                            let selected = accent() == color;
                            let button = egui::Button::new(if selected { "●" } else { "" })
                                .fill(color)
                                .min_size(egui::vec2(24.0, 22.0))
                                .rounding(egui::Rounding::same(7.0));
                            if ui.add(button).on_hover_text("Use this as the main theme color").clicked() {
                                picked = Some(color);
                            }
                        }
                    });
                }
                if let Some(color) = picked {
                    crate::theme::set_accent(ui.ctx(), color);
                    save_ui_config(&UiConfig {
                        accent_rgb: Some((color.r(), color.g(), color.b())),
                        theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }),
                    });
                }
                ui.add_space(6.0);

                const PRESETS: &[(&str, egui::Color32)] = &[
                    ("Miku Teal",    egui::Color32::from_rgb(0, 173, 181)),
                    ("Sakura Pink",  egui::Color32::from_rgb(236, 72, 153)),
                    ("Hyper Purple", egui::Color32::from_rgb(168, 85, 247)),
                    ("Cyber Orange", egui::Color32::from_rgb(249, 115, 22)),
                    ("Tachyon Yellow", egui::Color32::from_rgb(234, 179, 8)),
                ];
                ui.horizontal(|ui| {
                    for (name, color) in PRESETS {
                        let selected = accent() == *color;
                        let btn = egui::Button::new(if selected { "✓" } else { "" })
                            .fill(*color)
                            .min_size(egui::vec2(26.0, 22.0));
                        if ui.add(btn).on_hover_text(*name).clicked() {
                            crate::theme::set_accent(ui.ctx(), *color);
                            save_ui_config(&UiConfig { accent_rgb: Some((color.r(), color.g(), color.b())), theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }) });
                        }
                    }
                });
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(eyebrow("APP APPEARANCE"));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for (preset, label) in [
                        (ThemePreset::Miku, "Miku dark"),
                        (ThemePreset::Sakura, "Sakura light"),
                        (ThemePreset::Aurora, "Aurora dark"),
                        (ThemePreset::Mono, "Mono light"),
                    ] {
                        if ui.add(chip(label, false)).clicked() {
                            set_theme_preset(ui.ctx(), preset);
                            let c = accent();
                            save_ui_config(&UiConfig { accent_rgb: Some((c.r(), c.g(), c.b())), theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }) });
                        }
                    }
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let label = if is_light_mode() { "Light mode" } else { "Dark mode" };
                    if ui.add(chip(label, true)).clicked() {
                        set_light_mode(!is_light_mode());
                        apply_theme(ui.ctx());
                        let c = accent();
                        save_ui_config(&UiConfig { accent_rgb: Some((c.r(), c.g(), c.b())), theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }) });
                    }
                    ui.label(hint("Presets change the accent and surface contrast across the app."));
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    let mut color = accent();
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        crate::theme::set_accent(ui.ctx(), color);
                        save_ui_config(&UiConfig { accent_rgb: Some((color.r(), color.g(), color.b())), theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }) });
                    }
                    ui.add_space(8.0);
                    if ui.add_sized([160.0, 24.0], egui::Button::new("Reset to default teal")).clicked() {
                        crate::theme::set_accent(ui.ctx(), crate::theme::default_accent());
                        save_ui_config(&UiConfig { accent_rgb: None, theme_mode: Some(if is_light_mode() { "light".into() } else { "dark".into() }) });
                    }
                });
                // No trailing "< Back" here — the Extras left sidebar is
                // the navigation for these sub-panels; a Back button that
                // just jumps to Miku was a dead-end control duplicating
                // what the sidebar already does one click away.
            });
    }

    /// Renders an embedded web page (Cookie Clicker, Sponder Bird, 3D Miku)
    /// with a "< Back" button. The actual WebView2 control is positioned by
    /// `WebViewManager::update` after this frame's layout is known.
    pub fn show_web_panel_ui(&mut self, ui: &mut egui::Ui, panel: crate::webview::WebPanel) {
        ui.horizontal(|ui| {
            if ui.add_sized([90.0, 26.0], egui::Button::new("< Back")).clicked() {
                self.active_web_panel = None;
                self.open_sheet(crate::types::Sheet::Extras);
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
        // Copy-and-open, not remote control. Discord is opened through its own
        // URL handler and the message goes to the clipboard; nothing here types
        // into another program (see `ops::discord` for why that matters).
        card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new("Message on Discord").size(13.0).color(accent()));
            ui.add_space(10.0);

            ui.label(egui::RichText::new("Who to message (shown as a reminder):")
                .size(11.0).color(egui::Color32::GRAY));
            ui.add_space(4.0);
            ui.add(egui::TextEdit::singleline(&mut self.dm_target_name)
                .desired_width(f32::INFINITY)
                .hint_text("e.g. gonkindroid"));
            ui.add_space(10.0);

            let name = self.dm_target_name.trim().to_string();
            let how = if name.is_empty() {
                "In Discord press Ctrl+K, pick the person, then paste (Ctrl+V).".to_string()
            } else {
                format!("In Discord press Ctrl+K, type {name}, then paste (Ctrl+V).")
            };

            let btn_w = ui.available_width();
            if ui.add_sized([btn_w, 34.0], egui::Button::new("Open Discord")).clicked() {
                crate::ops::discord::open_discord();
                self.set_status(how.clone());
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(egui::RichText::new("Quick messages (click to copy and open Discord):")
                .size(11.0).color(egui::Color32::GRAY));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for preset in self.dm_message_presets.clone() {
                    if ui.button(&preset).clicked() {
                        ui.ctx().copy_text(preset.clone());
                        crate::ops::discord::open_discord();
                        self.set_status(format!("Copied. {how}"));
                    }
                }
            });
            ui.add_space(10.0);

            ui.label(egui::RichText::new("Custom message:")
                .size(11.0).color(egui::Color32::GRAY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let btn_w = 110.0;
                let text_w = (ui.available_width() - btn_w - ui.spacing().item_spacing.x).max(60.0);
                ui.add_sized(
                    [text_w, 22.0],
                    egui::TextEdit::singleline(&mut self.dm_custom_message).hint_text("Type a message…"),
                );
                let can_send = !self.dm_custom_message.trim().is_empty();
                ui.add_enabled_ui(can_send, |ui| {
                    if ui.add_sized([btn_w, 22.0], egui::Button::new("Copy & open")).clicked() {
                        ui.ctx().copy_text(self.dm_custom_message.clone());
                        crate::ops::discord::open_discord();
                        self.set_status(format!("Copied. {how}"));
                    }
                });
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Image to share:")
                .size(11.0).color(egui::Color32::GRAY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                let browse_w = 80.0;
                let show_w = 110.0;
                let text_w = (ui.available_width() - browse_w - show_w - gap * 2.0).max(60.0);
                ui.add_sized(
                    [text_w, 22.0],
                    egui::TextEdit::singleline(&mut self.dm_image_path).hint_text(r"C:\path\to\image.png"),
                );
                if ui.add_sized([browse_w, 22.0], egui::Button::new("Browse...")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]).pick_file() {
                        self.dm_image_path = path.to_string_lossy().to_string();
                    }
                let can_show = !self.dm_image_path.trim().is_empty();
                ui.add_enabled_ui(can_show, |ui| {
                    if ui.add_sized([show_w, 22.0], egui::Button::new("Show in folder")).clicked() {
                        crate::ops::discord::reveal_in_explorer(&self.dm_image_path);
                        crate::ops::discord::open_discord();
                        self.set_status("Drag the selected image into the Discord chat.".into());
                    }
                });
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new(
                "This app never types into Discord for you — it opens it and puts the message on your clipboard.")
                .size(10.0).color(MUTED));
        });
    }

    pub fn show_project_path_row(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Unreal Project  (.uproject)")
            .size(12.0).color(egui::Color32::GRAY));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let has_path  = self.project_path.is_some();
            let browse_w  = 78.0;
            let clear_w   = 80.0;
            let gap       = ui.spacing().item_spacing.x;
            // Reserve exactly what the trailing buttons actually consume —
            // one gap + Browse, plus another gap + Clear only when Clear is
            // even shown — instead of a separate hardcoded constant that
            // has to be kept in sync with the buttons by hand.
            let reserved  = gap + browse_w + if has_path { gap + clear_w } else { 0.0 };
            let text_w    = (ui.available_width() - reserved).max(60.0);

            // `add_sized`, not `desired_width`: the latter is only a minimum, so a
            // long path made this field — and the card around it — wider than
            // its column, and the two cards above the diagnostics overlapped.
            let resp = ui.add_sized(
                [text_w, 22.0],
                egui::TextEdit::singleline(&mut self.project_path_input)
                    .hint_text("Select or paste path to .uproject…"),
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
                    if path.is_file()
                        && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("uproject"))
                    {
                        save_project_path(&path);
                        self.project_path_input = path.to_string_lossy().to_string();
                        self.project_path       = Some(path);
                        self.redetect_engine();
                    } else {
                        self.set_status("[ERROR] Select an existing .uproject file.".into());
                    }
                }

            if has_path
                && ui.add_sized([clear_w, 22.0], egui::Button::new("x  Clear")).clicked() {
                    clear_project_path();
                    self.project_path = None;
                    self.project_path_input.clear();
                    self.redetect_engine();
                }
        });

        ui.add_space(2.0);
        match &self.project_path {
            Some(p) => {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                ui.colored_label(accent(), format!("[OK]  {}", name));
            }
            None if !self.project_path_input.trim().is_empty() => {
                ui.colored_label(RED, "[!]  File not found or not a .uproject");
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

            let browse_w = 78.0;
            let auto_w   = 86.0;
            let gap      = ui.spacing().item_spacing.x;
            // This used to be a bare `172.0`/`86.0` that didn't match what
            // the row below actually consumes (`78.0 + 86.0 + 2 *
            // item_spacing.x` = 180.0 when the Auto-detect button is shown,
            // vs. a reserved 172.0) — 8px short, so the row overflowed the
            // card's right edge and clipped "x Auto-detect" at the window
            // edge. Deriving the reservation from the real button widths +
            // real spacing (like `show_project_path_row` does) keeps it
            // correct regardless of button size changes, and `.max(60.0)`
            // stops the label from going negative-width on a narrow window.
            let reserved = gap + browse_w + if has_override { gap + auto_w } else { 0.0 };
            let label_w  = (ui.available_width() - reserved).max(60.0);

            // `Label` defaults to `TextWrapMode::Extend` — it does NOT clip to
            // its allocated size, it grows past it. Without `.truncate()` a
            // long engine path overlaps the Browse/Auto-detect buttons that
            // follow it in this row instead of eliding with "…".
            // Left-aligned in its slot: `add_sized` would centre the text.
            ui.allocate_ui_with_layout(
                egui::vec2(label_w, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(label_w);
                    ui.set_max_width(label_w);
                    ui.add(egui::Label::new(egui::RichText::new(&path_text).size(12.0).color(MUTED)).truncate())
                        .on_hover_text(&path_text);
                },
            );

            if ui.add_sized([browse_w, 22.0], egui::Button::new("Browse…")).clicked() {
                self.choose_engine_dir();
            }
            if has_override && ui.add_sized([auto_w, 22.0], egui::Button::new("x  Auto-detect")).clicked() {
                self.clear_engine_override();
            }
        });

        ui.add_space(2.0);
        match (&self.engine_dir, self.engine_override.is_some()) {
            (Some(_), true)  => { ui.colored_label(accent(), "[OK]  Manual override"); }
            (Some(_), false) => { ui.colored_label(accent(), "[OK]  Auto-detected"); }
            (None, _) => {
                ui.colored_label(RED, "[!]  Engine not found — select your Unreal Engine install folder");
            }
        }
    }

}
