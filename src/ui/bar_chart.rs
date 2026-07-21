use eframe::egui;
use crate::theme::*;

/// Custom-painted vertical bar chart for the Git tab's commit-activity
/// panel, styled to match `circular_meter.rs`: a free function over plain
/// data, no `DevToolApp` state, so it can be reused anywhere a compact
/// per-bucket count needs a visual instead of a wall of numbers.
///
/// `data` is `(label, count)` pairs, oldest-first — see
/// `ops::git::git_status_summary`'s `activity` field, the only producer of
/// real (non-fabricated) counts this is meant to render. Callers should
/// check for an empty slice themselves and show a "no data" message instead
/// of calling this with nothing to draw (an empty repo/no commit history is
/// a real, meaningful state — not the same as "zero commits every day").
pub fn show_bar_chart(ui: &mut egui::Ui, data: &[(String, usize)], height: f32) {
    let width = ui.available_width();
    // Allocate the space up front so layout below/after this call is
    // correct regardless of what the painter does inside it.
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Track background the bars sit on top of.
    painter.rect_filled(rect, egui::Rounding::same(4.0), GIF_BG);

    // Faint horizontal gridlines — a cheap "axis" cue without pulling in a
    // real charting dependency for one small panel.
    const GRID_LINES: usize = 3;
    for i in 1..=GRID_LINES {
        let y = rect.top() + rect.height() * (i as f32 / (GRID_LINES as f32 + 1.0));
        painter.hline(rect.x_range(), y, egui::Stroke::new(1.0_f32, CARD_BORDER));
    }

    if data.is_empty() {
        return;
    }

    // Reserve a thin strip at the bottom for the first/last date labels so
    // bars never paint over the text.
    let label_h      = 12.0;
    let chart_top    = rect.top();
    let chart_bottom = rect.bottom() - label_h;

    let n       = data.len();
    let bar_gap = 3.0;
    let bar_w   = ((rect.width() - bar_gap * (n as f32 - 1.0)) / n as f32).max(1.0);

    // A max of 0 (every day in range has zero commits) must render flat
    // baseline bars, not divide-by-zero — `frac` just stays 0.0 for all of
    // them in that case.
    let max = data.iter().map(|(_, c)| *c).max().unwrap_or(0);

    for (i, (label, count)) in data.iter().enumerate() {
        let x0 = rect.left() + i as f32 * (bar_w + bar_gap);
        let x1 = x0 + bar_w;

        let frac   = if max == 0 { 0.0 } else { *count as f32 / max as f32 };
        let bar_h  = ((chart_bottom - chart_top) * frac).max(2.0);
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x0, chart_bottom - bar_h),
            egui::pos2(x1, chart_bottom),
        );
        painter.rect_filled(bar_rect, egui::Rounding::same(2.0), accent());

        // Hit-test the bar's *full* column height, not just its filled
        // portion — a near-zero-commit bar is only ~2px tall and would be
        // an unreasonably small hover target otherwise.
        let hit_rect = egui::Rect::from_min_max(egui::pos2(x0, chart_top), egui::pos2(x1, chart_bottom));
        let id   = ui.id().with("bar_chart_bar").with(i);
        let resp = ui.interact(hit_rect, id, egui::Sense::hover());
        let _ = resp.on_hover_text(format!(
            "{}: {} commit{}",
            label, count, if *count == 1 { "" } else { "s" }
        ));

        // 14 date labels under 14 skinny bars would collide into an
        // unreadable smear — only the first and last (oldest/newest day)
        // are shown, which is enough to read the window's span.
        if i == 0 || i == n - 1 {
            painter.text(
                egui::pos2((x0 + x1) / 2.0, rect.bottom() - label_h / 2.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(9.0),
                HINT_GRAY,
            );
        }
    }
}
