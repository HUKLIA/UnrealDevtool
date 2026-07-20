use eframe::egui;
use crate::app::DevToolApp;
use crate::ops::preflight::{has_space, CheckStatus};
use crate::theme::*;

impl DevToolApp {
    /// True while there's an unresolved space-in-path issue that the
    /// one-click fix (directory junction) can address.
    fn has_unfixed_space_issue(&self) -> bool {
        if self.use_space_free_link { return false; }
        self.engine_dir.as_ref().is_some_and(|p| has_space(p))
            || self.project_path.as_ref()
                .and_then(|p| p.parent())
                .is_some_and(has_space)
    }

    /// Warning box + one-click fix button for the UAT-breaks-on-spaces issue.
    /// Shared by the standalone "Check PC Setup" panel and the inline warning
    /// in the package-config panel, so the fix is visible right where the
    /// failure would otherwise happen.
    pub fn show_space_warning_inline(&mut self, ui: &mut egui::Ui) {
        if !self.has_unfixed_space_issue() { return; }

        egui::Frame::none()
            .fill(egui::Color32::from_rgb(45, 35, 15))
            .stroke(egui::Stroke::new(1.0, WARN_AMBER))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.colored_label(WARN_AMBER, "⚠  Engine or project path contains a space");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Unreal's own build scripts (UAT/UBT) have a long-standing bug with spaces \
                         in paths — most often hit via the default \"C:\\Program Files\\Epic Games\\...\" \
                         install. It shows up as a cryptic \"'C:\\Program' is not recognized...\" failure \
                         after a long build. The fix below links the affected folder(s) to a space-free \
                         path via an NTFS junction — it doesn't move or copy anything."
                    ).size(10.5).color(egui::Color32::from_rgb(210, 190, 140)),
                );
                ui.add_space(6.0);
                if ui.add_sized([220.0, 26.0], egui::Button::new("🔧  Fix automatically (link to space-free path)")).clicked() {
                    self.apply_space_free_fix();
                }
            });
        ui.add_space(8.0);
    }

    pub fn show_pc_check_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(PANEL_DARK)
            .stroke(egui::Stroke::new(1.0, accent()))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("🩺  Check PC Setup").size(13.0).color(accent()));
                ui.add_space(10.0);

                for item in &self.pc_check_items {
                    let (prefix, color) = match item.status {
                        CheckStatus::Ok   => ("[OK]",   accent()),
                        CheckStatus::Warn => ("[WARN]", WARN_AMBER),
                        CheckStatus::Fail => ("[FAIL]", ERR_RED),
                    };
                    ui.colored_label(color, format!("{prefix}  {}", item.label));
                    ui.label(
                        egui::RichText::new(&item.detail).size(10.5).color(HINT_GRAY),
                    );
                    ui.add_space(6.0);
                }

                self.show_space_warning_inline(ui);

                ui.horizontal(|ui| {
                    if ui.add_sized([100.0, 28.0], egui::Button::new("↻  Refresh")).clicked() {
                        self.refresh_pc_check();
                    }
                    if ui.add_sized([100.0, 28.0], egui::Button::new("< Back")).clicked() {
                        self.show_pc_check = false;
                    }
                });
            });
    }
}
