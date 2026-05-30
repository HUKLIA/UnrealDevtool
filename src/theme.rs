use eframe::egui;

pub const MIKU_TEAL:  egui::Color32 = egui::Color32::from_rgb(57,  197, 187);
pub const MIKU_PINK:  egui::Color32 = egui::Color32::from_rgb(225,  40, 133);
pub const DARK_BG:    egui::Color32 = egui::Color32::from_rgb( 20,  20,  25);
pub const PANEL_BG:   egui::Color32 = egui::Color32::from_rgb( 35,  35,  45);
pub const GIF_BG:     egui::Color32 = egui::Color32::from_rgb( 12,  12,  18);
pub const WARN_AMBER: egui::Color32 = egui::Color32::from_rgb(180, 140,  60);
pub const ERR_RED:    egui::Color32 = egui::Color32::from_rgb(210,  80,  80);
pub const HINT_GRAY:  egui::Color32 = egui::Color32::from_rgb(100, 100, 120);
pub const PANEL_DARK: egui::Color32 = egui::Color32::from_rgb( 25,  25,  35);

pub fn apply_miku_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.window_fill = DARK_BG;
    v.panel_fill  = DARK_BG;
    v.widgets.inactive.bg_fill   = PANEL_BG;
    v.widgets.inactive.rounding  = egui::Rounding::same(4.0);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY);
    v.widgets.hovered.bg_fill    = MIKU_TEAL;
    v.widgets.hovered.fg_stroke  = egui::Stroke::new(1.5, egui::Color32::BLACK);
    v.widgets.active.bg_fill     = MIKU_PINK;
    v.widgets.active.fg_stroke   = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.rounding    = egui::Rounding::same(8.0);
    v.selection.bg_fill          = MIKU_TEAL;
    v.selection.stroke           = egui::Stroke::new(1.0, MIKU_TEAL);
    ctx.set_visuals(v);
}
