use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;

impl DevToolApp {
    /// Full-window boot splash shown once at launch: reveals `intro_log` line
    /// by line, then hands off to the main UI on its own.
    ///
    /// Purely cosmetic — the detection it narrates already happened in `new()`.
    /// There is no "continue" button: the splash held one for a click that
    /// only ever had a single possible answer, which made a two-second
    /// animation into an interaction. It now holds briefly on the final line
    /// and fades out (see `tick_intro` / `intro_fade`).
    pub fn show_intro_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.tick_intro(ctx);

        let fade = self.intro_fade();
        ui.multiply_opacity(fade);

        ui.add_space(ui.available_height() * 0.12);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("UNREAL DEVTOOL")
                    .size(22.0)
                    .strong()
                    .color(TEXT),
            );
            ui.label(
                egui::RichText::new("STUDY & RESEARCH PROJECT")
                    .size(10.0)
                    .color(MUTED),
            );
            ui.add_space(16.0);

            let box_width = (ui.available_width() - 24.0).min(460.0);
            ui.allocate_ui_with_layout(
                egui::vec2(box_width, 250.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    card().show(ui, |ui| {
                        ui.set_min_size(egui::vec2(box_width - 28.0, 220.0));

                        // Eased rather than stepped. The bar used to jump by a
                        // full 1/7th as each line landed; interpolating toward
                        // the target makes it travel continuously, which is
                        // what makes the whole splash read as smooth.
                        let target = self.intro_revealed as f32 / self.intro_log.len().max(1) as f32;
                        let shown  = ui.ctx().animate_value_with_time(
                            egui::Id::new("intro_progress"), target, 0.30);
                        ui.add(
                            egui::ProgressBar::new(shown)
                                .desired_width(ui.available_width())
                                .fill(accent())
                                .show_percentage(),
                        );
                        ui.add_space(8.0);

                        // `build_intro_log` never produces more than 7 lines, so
                        // this is sized to fit all of them at once — with
                        // `stick_to_bottom`, a shorter box would scroll such
                        // that the topmost line sits half-clipped by the
                        // viewport edge instead of fully visible.
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for (i, line) in self.intro_log.iter().take(self.intro_revealed).enumerate() {
                                    let is_warning = line.starts_with("WARNING");
                                    let color = if is_warning { AMBER } else if i == self.intro_log.len() - 1 {
                                        accent()
                                    } else {
                                        SOFT
                                    };
                                    // Each line fades in as it is revealed,
                                    // instead of appearing hard. The id is per
                                    // line index, so each one animates once.
                                    let a = ui.ctx().animate_bool_with_time(
                                        egui::Id::new("intro_line").with(i), true, 0.22);
                                    ui.scope(|ui| {
                                        ui.multiply_opacity(a);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!("[{}]", i + 1))
                                                    .monospace().size(10.0).color(MUTED),
                                            );
                                            ui.label(egui::RichText::new(line).monospace().size(10.5).color(color));
                                        });
                                    });
                                }
                            });
                    });
                },
            );

            ui.add_space(20.0);

            // Status line only — no control. It reports what is happening and
            // then that it is handing over, so the fade is never a surprise.
            let msg = if self.intro_done { "STARTING…" } else { "DETECTING LOCAL SDKs…" };
            ui.label(egui::RichText::new(msg).size(10.0).color(MUTED));
        });
    }
}
