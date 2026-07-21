use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::llm::LlmProvider;
use crate::theme::*;

impl DevToolApp {
    fn show_chat_bubble(ui: &mut egui::Ui, role: &str, content: &str) {
        let (label, color) = match role {
            "user" => ("You", accent()),
            _      => ("Assistant", egui::Color32::WHITE),
        };
        ui.colored_label(color, label);
        ui.label(egui::RichText::new(content).size(11.5).color(egui::Color32::from_rgb(220, 220, 220)));
        ui.add_space(8.0);
    }

    pub fn show_chat_panel_ui(&mut self, ui: &mut egui::Ui) {
        // Every other tab anchors its heading inside a bordered card (Git's
        // "Git" label, Package's "Package Configuration", Dashboard's
        // "ACTIVE PROJECT", Extras' "EXTRAS") — this one used to float
        // directly on the busy grid/glow background with nothing behind
        // it, which read as washed-out/inconsistent next to the rest.
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💬  Dev Assistant").size(14.0).color(accent()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Used to be "< Back", but the persistent tab bar right
                    // above this card already jumps to Dashboard in one
                    // click — a second button doing the exact same thing
                    // was redundant and confusing. "Clear chat" actually
                    // does something the header couldn't do before;
                    // disabled once there's nothing left to clear.
                    let can_clear = !self.chat_history.is_empty();
                    ui.add_enabled_ui(can_clear, |ui| {
                        if ui.add_sized([104.0, 24.0], egui::Button::new("🗑  Clear chat")).clicked() {
                            self.chat_history.clear();
                        }
                    });
                });
            });
        });
        ui.add_space(10.0);

        let providers = self.chat_providers.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let detecting = *self.chat_detecting.lock().unwrap_or_else(|e| e.into_inner());

        if providers.is_empty() {
            card_frame().show(ui, |ui| {
                if detecting {
                    ui.colored_label(HINT_GRAY, "Looking for Ollama / LM Studio…");
                    ui.ctx().request_repaint();
                } else {
                    ui.colored_label(WARN_AMBER, "⚠  No local LLM server found.");
                    ui.label(
                        egui::RichText::new(
                            "Start Ollama (ollama.com) or LM Studio (lmstudio.ai) with a \
                             model loaded, then click Refresh."
                        ).size(10.5).color(HINT_GRAY),
                    );
                }
                ui.add_space(6.0);
                if ui.add_sized([90.0, 24.0], egui::Button::new("↻  Refresh")).clicked() {
                    self.detect_chat_providers();
                }
            });
            return;
        }

        // Auto-select a provider/model the first time we have options, or if
        // the previously selected provider disappeared (server stopped)
        // between refreshes.
        if !providers.iter().any(|(p, _)| Some(*p) == self.chat_provider) {
            if let Some((p, models)) = providers.first() {
                self.chat_provider = Some(*p);
                self.chat_model    = models.first().cloned().unwrap_or_default();
            }
        }

        // `ui.horizontal` is `Layout::left_to_right(Align::Center)` — with
        // that, these two `allocate_ui_with_layout` children get vertically
        // *centered* relative to each other, so whichever column (sidebar
        // vs. main) ends up shorter this frame visibly drifts down instead
        // of both starting flush at the top. `horizontal_top` is the same
        // layout with `Align::Min`, which top-aligns them instead.
        ui.horizontal_top(|ui| {
            let total     = ui.available_width();
            let gap       = ui.spacing().item_spacing.x;
            let sidebar_w = (total * 0.28).clamp(180.0, 260.0);
            let main_w    = total - sidebar_w - gap;

            // `ui.scope` inherits the *ambient* layout direction — inside
            // this `ui.horizontal_top`, that's left-to-right, so a plain
            // scope would flow its children sideways instead of stacking
            // them. `allocate_ui_with_layout` resets the layout explicitly,
            // which is what actually makes this a sidebar instead of a
            // second row of horizontally-packed widgets.
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(sidebar_w, 0.0), top_down, |ui| {
                self.show_chat_sidebar(ui, &providers);
            });
            ui.allocate_ui_with_layout(egui::vec2(main_w, 0.0), top_down, |ui| {
                self.show_chat_main(ui);
            });
        });
    }

    fn show_chat_sidebar(&mut self, ui: &mut egui::Ui, providers: &[(LlmProvider, Vec<String>)]) {
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("LLM SERVER").size(10.5).color(HINT_GRAY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_sized([22.0, 20.0], egui::Button::new("↻"))
                        .on_hover_text("Re-scan for servers/models").clicked() {
                            self.detect_chat_providers();
                        }
                });
            });
            ui.add_space(8.0);

            for (p, models) in providers {
                let selected = self.chat_provider == Some(*p);
                let btn = egui::Button::new(
                    egui::RichText::new(p.label()).size(11.5)
                        .color(if selected { egui::Color32::WHITE } else { egui::Color32::LIGHT_GRAY }),
                )
                .fill(if selected { PANEL_BG } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::new(1.0, if selected { accent() } else { CARD_BORDER }));
                if ui.add_sized([ui.available_width(), 30.0], btn).clicked() {
                    self.chat_provider = Some(*p);
                    self.chat_model    = models.first().cloned().unwrap_or_default();
                }
                ui.add_space(4.0);
            }

            ui.add_space(6.0);
            ui.label(egui::RichText::new("MODEL").size(10.5).color(HINT_GRAY));
            let models = providers.iter()
                .find(|(p, _)| Some(*p) == self.chat_provider)
                .map(|(_, m)| m.clone())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("chat_model")
                .width(ui.available_width())
                .selected_text(if self.chat_model.is_empty() { "—" } else { self.chat_model.as_str() })
                .show_ui(ui, |ui| {
                    for m in &models {
                        ui.selectable_value(&mut self.chat_model, m.clone(), m);
                    }
                });
        });
        ui.add_space(10.0);

        card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("CONTEXT INJECTED").size(10.5).color(HINT_GRAY));
            ui.add_space(6.0);
            let engine = self.engine_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "not found".into());
            let project = self.project_path.as_ref()
                .and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "not set".into());
            let branch = if self.git_current_branch.is_empty() { "unknown".to_string() } else { self.git_current_branch.clone() };
            let space_flag = self.engine_dir.as_ref().is_some_and(|p| crate::ops::preflight::has_space(p));

            for (k, v, warn) in [
                ("PROJECT", project, false),
                ("ENGINE", engine, false),
                ("SPACE_IN_PATH", if space_flag { "YES".into() } else { "NO".into() }, space_flag),
                ("GIT_BRANCH", branch, false),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(k).size(9.5).monospace().color(HINT_GRAY));
                    ui.label(
                        egui::RichText::new(v).size(9.5).monospace()
                            .color(if warn { WARN_AMBER } else { egui::Color32::LIGHT_GRAY }),
                    );
                });
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Sent automatically with every message.")
                    .size(9.0).color(HINT_GRAY),
            );
        });
    }

    fn show_chat_main(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            let busy = *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner());

            // Bumped from 360 to use more of the vertical dead space below
            // the message list on anything taller than a small laptop
            // screen. A dynamic "fill remaining height" version was tried
            // and reverted: this card sits inside a sidebar/main split
            // that's itself inside the tab's fade-in scope, and
            // `ui.available_height()` reports 0 at that nesting depth (an
            // egui layout quirk, not a real constraint) — sizing off of it
            // put the input row below the visible window instead of fixing
            // anything. A larger fixed height is a safe, modest win without
            // that risk.
            // `stick_to_bottom(true)` pins the scroll position to whatever
            // was last added — right for a running conversation, but with
            // an empty history it pins the empty-state placeholder line to
            // the *bottom* of the 440px region instead, leaving a tall dead
            // gap above it where messages will eventually appear. Only
            // stick to the bottom once there's actually content to stick
            // to (existing messages, or a reply streaming in); otherwise
            // the placeholder renders at the natural top of the region.
            let has_content = !self.chat_history.is_empty() || busy;
            egui::ScrollArea::vertical()
                .max_height(440.0)
                .stick_to_bottom(has_content)
                .show(ui, |ui| {
                    if self.chat_history.is_empty() && !busy {
                        ui.label(
                            egui::RichText::new("Ask about this project — build failures, git state, \
                                                  engine setup, anything.")
                                .size(10.5).color(HINT_GRAY),
                        );
                    }
                    for msg in &self.chat_history {
                        Self::show_chat_bubble(ui, &msg.role, &msg.content);
                    }
                    if busy {
                        let partial = self.chat_streaming.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        if partial.is_empty() {
                            ui.colored_label(HINT_GRAY, "…thinking");
                        } else {
                            Self::show_chat_bubble(ui, "assistant", &partial);
                        }
                        ui.ctx().request_repaint();
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - 90.0).max(120.0);
                let resp = ui.add_enabled(
                    !busy,
                    egui::TextEdit::multiline(&mut self.chat_input)
                        .hint_text("Ask about this project, a build failure, git state…")
                        .desired_rows(2)
                        .desired_width(input_w),
                );
                let enter_sent = resp.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);

                ui.vertical(|ui| {
                    if busy {
                        if ui.add_sized([80.0, 26.0], egui::Button::new("Stop")).clicked() {
                            self.cancel_chat_message();
                        }
                    } else if ui.add_sized([80.0, 26.0], egui::Button::new("Send")).clicked() || enter_sent {
                        self.send_chat_message();
                    }
                });
            });
        });
    }
}
