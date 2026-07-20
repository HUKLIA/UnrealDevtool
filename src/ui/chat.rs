use eframe::egui;
use crate::app::DevToolApp;
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
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("💬  Dev Assistant").size(13.0).color(accent()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([70.0, 24.0], egui::Button::new("< Back")).clicked() {
                            self.show_chat_panel = false;
                        }
                    });
                });
                ui.add_space(8.0);

                let providers = self.chat_providers.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let detecting = *self.chat_detecting.lock().unwrap_or_else(|e| e.into_inner());

                if providers.is_empty() {
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
                    return;
                }

                // Auto-select a provider/model the first time we have options,
                // or if the previously selected provider disappeared (server
                // stopped) between refreshes.
                if !providers.iter().any(|(p, _)| Some(*p) == self.chat_provider) {
                    if let Some((p, models)) = providers.first() {
                        self.chat_provider = Some(*p);
                        self.chat_model    = models.first().cloned().unwrap_or_default();
                    }
                }

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Server:").size(11.0).color(egui::Color32::GRAY));
                    egui::ComboBox::from_id_salt("chat_provider")
                        .selected_text(self.chat_provider.map(|p| p.label()).unwrap_or("—"))
                        .show_ui(ui, |ui| {
                            for (p, models) in &providers {
                                if ui.selectable_label(self.chat_provider == Some(*p), p.label()).clicked() {
                                    self.chat_provider = Some(*p);
                                    self.chat_model    = models.first().cloned().unwrap_or_default();
                                }
                            }
                        });

                    ui.label(egui::RichText::new("Model:").size(11.0).color(egui::Color32::GRAY));
                    let models = providers.iter()
                        .find(|(p, _)| Some(*p) == self.chat_provider)
                        .map(|(_, m)| m.clone())
                        .unwrap_or_default();
                    egui::ComboBox::from_id_salt("chat_model")
                        .selected_text(if self.chat_model.is_empty() { "—" } else { self.chat_model.as_str() })
                        .show_ui(ui, |ui| {
                            for m in &models {
                                ui.selectable_value(&mut self.chat_model, m.clone(), m);
                            }
                        });

                    if ui.add_sized([26.0, 22.0], egui::Button::new("↻"))
                        .on_hover_text("Re-scan for servers/models").clicked() {
                            self.detect_chat_providers();
                        }
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                let busy = *self.chat_busy.lock().unwrap_or_else(|e| e.into_inner());

                egui::ScrollArea::vertical().max_height(280.0).stick_to_bottom(true).show(ui, |ui| {
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
                    let resp = ui.add_enabled(
                        !busy,
                        egui::TextEdit::multiline(&mut self.chat_input)
                            .hint_text("Ask about this project, a build failure, git state…")
                            .desired_rows(2)
                            .desired_width(ui.available_width() - 90.0),
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
