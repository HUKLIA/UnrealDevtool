use eframe::egui;
use crate::theme::*;

/// Custom-painted circular progress ring, matching the reference mockup's
/// `CircularMeter.tsx`. Pure rendering — no `DevToolApp` state, just a
/// progress fraction and a label, so it can be reused anywhere a compact
/// progress readout is useful.
///
/// The live "actively packaging" progress still uses the existing full-
/// screen Miku+linear-bars busy view (`show_busy_view`) unchanged — that
/// view takes over the whole window before any tab ever renders, so this
/// ring is only ever seen in the Package tab's idle state: at 0% before a
/// run, or holding at 100% right after one finishes.
pub fn show_circular_meter(ui: &mut egui::Ui, progress: f32, label: &str) {
    let size   = 168.0;
    let radius = 68.0;
    let stroke_width = 12.0;

    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center  = rect.center();

    painter.circle_stroke(center, radius, egui::Stroke::new(stroke_width, egui::Color32::from_rgb(40, 40, 48)));

    let progress = progress.clamp(0.0, 1.0);
    if progress > 0.0 {
        let start_angle = -std::f32::consts::FRAC_PI_2;
        let end_angle   = start_angle + progress * std::f32::consts::TAU;
        let segments    = ((progress * 120.0) as usize).clamp(2, 120);
        let points: Vec<egui::Pos2> = (0..=segments)
            .map(|i| {
                let t = i as f32 / segments as f32;
                let angle = start_angle + t * (end_angle - start_angle);
                center + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(stroke_width, accent())));
    }

    painter.text(
        center + egui::vec2(0.0, -4.0),
        egui::Align2::CENTER_CENTER,
        format!("{}%", (progress * 100.0).round() as i32),
        egui::FontId::monospace(26.0),
        egui::Color32::WHITE,
    );
    painter.text(
        center + egui::vec2(0.0, 20.0),
        egui::Align2::CENTER_CENTER,
        if progress >= 1.0 { "COMPLETED" } else { "READY" },
        egui::FontId::monospace(10.0),
        HINT_GRAY,
    );

    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(label).size(10.5).color(HINT_GRAY));
    });
}
