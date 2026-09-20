use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::preflight::CheckStatus;
use crate::theme::*;

impl DevToolApp {
    pub fn show_app_check_panel(&mut self, ui: &mut egui::Ui) {
        // Was its own bespoke `Frame::none()` — every Extras sub-panel now
        // shares the one `card()` from theme.rs instead of each
        // hand-rolling its own fill/stroke/rounding/margins.
        card().show(ui, |ui| {
                ui.label(heading("App Self-Check", 15.0));
                ui.label(
                    egui::RichText::new("The DevTool app's own install/config/update health — \
                                          see Check PC Setup for your Unreal project/engine.")
                        .size(10.0).color(MUTED),
                );
                ui.add_space(10.0);

                let mut has_leftover_binary = false;
                for item in &self.app_check_items {
                    if item.label == "Leftover update file" && matches!(item.status, CheckStatus::Warn) {
                        has_leftover_binary = true;
                    }
                    Self::show_check_item(ui, item);
                }

                if has_leftover_binary
                    && ui.add_sized([180.0, 26.0], egui::Button::new("🗑  Clean up now")).clicked() {
                        self.cleanup_leftover_binary_now();
                    }
                ui.add_space(6.0);

                // GitHub reachability runs on a background thread (network
                // call) — show a pending state until it reports back.
                match &*self.app_check_github.lock().unwrap_or_else(|e| e.into_inner()) {
                    Some(item) => Self::show_check_item(ui, item),
                    None => {
                        ui.colored_label(MUTED, "[..]  GitHub connectivity");
                        ui.label(egui::RichText::new("Checking…").size(10.5).color(MUTED));
                        ui.add_space(6.0);
                        ui.ctx().request_repaint();
                    }
                }

                ui.add_space(6.0);
                self.show_fingerprint(ui);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_sized([100.0, 28.0], egui::Button::new("↻  Refresh")).clicked() {
                        self.refresh_app_check();
                    }
                    if ui.add_sized([100.0, 28.0], egui::Button::new("< Back")).clicked() {
                        self.extras_tab = crate::types::ExtrasTab::Miku;
                    }
                });
            });
    }

    /// The executable's hash, with the two things worth doing with it when an
    /// antivirus complains: compare it, and look it up.
    fn show_fingerprint(&mut self, ui: &mut egui::Ui) {
        type Slot = std::sync::Arc<std::sync::Mutex<Option<Option<String>>>>;
        let id = egui::Id::new("exe_fingerprint");
        let slot: Slot = ui.ctx().data_mut(|d| d.get_temp::<Slot>(id)).unwrap_or_else(|| {
            let slot: Slot = Default::default();
            let worker = slot.clone();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let hash = crate::ops::selfcheck::exe_sha256();
                *worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(hash);
                ctx.request_repaint();
            });
            ui.ctx().data_mut(|d| d.insert_temp(id, slot.clone()));
            slot
        });

        ui.label(eyebrow("THIS FILE"));
        ui.add_space(4.0);
        let state = slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match state {
            None => { ui.label(hint("Hashing…")); }
            Some(None) => { ui.label(hint("Could not read the executable.")); }
            Some(Some(hash)) => {
                ui.add(egui::Label::new(
                    egui::RichText::new(&hash).font(mono(10.5)).color(SOFT)).wrap());
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(quiet("Copy SHA-256")).clicked() {
                        ui.ctx().copy_text(hash.clone());
                    }
                    if ui.add(quiet("Look up on VirusTotal"))
                        .on_hover_text("Opens a lookup by hash. The file itself is not uploaded.")
                        .clicked() {
                        let _ = crate::ops::cmd("explorer")
                            .arg(format!("https://www.virustotal.com/gui/file/{hash}")).spawn();
                    }
                });
            }
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(
            "Flagged by antivirus? Compare this hash with the .sha256 on the release page. The app runs without administrator rights, runs no hidden scripts and types into no other program.")
            .size(10.0).color(MUTED));
    }
}
