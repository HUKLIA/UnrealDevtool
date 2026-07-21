use eframe::egui;
use std::sync::Mutex;

// "Professional Polish" palette — near-black glass panels + teal accent,
// matched to the unreal-devtool/ reference mockup (see its src/index.css).
const MIKU_TEAL_DEFAULT: egui::Color32 = egui::Color32::from_rgb(0, 173, 181);

pub const MIKU_PINK:  egui::Color32 = egui::Color32::from_rgb(225,  40, 133);
pub const DARK_BG:    egui::Color32 = egui::Color32::from_rgb(  8,   8,  10);
pub const PANEL_BG:   egui::Color32 = egui::Color32::from_rgb( 24,  24,  28);
pub const GIF_BG:     egui::Color32 = egui::Color32::from_rgb(  6,   6,   8);
pub const WARN_AMBER: egui::Color32 = egui::Color32::from_rgb(217, 160,  40);
pub const ERR_RED:    egui::Color32 = egui::Color32::from_rgb(220,  70,  90);
pub const HINT_GRAY:  egui::Color32 = egui::Color32::from_rgb(130, 130, 145);
pub const PANEL_DARK: egui::Color32 = egui::Color32::from_rgb( 14,  14,  17);
pub const CARD_BORDER: egui::Color32 = egui::Color32::from_rgb( 34,  34,  34);

/// Process-wide, user-customizable accent color. Replaces what used to be a
/// `MIKU_TEAL` const — every former bare-const call site now calls `accent()`
/// instead, so a color picked at runtime propagates everywhere without
/// threading a value through every function signature.
static ACCENT: Mutex<egui::Color32> = Mutex::new(MIKU_TEAL_DEFAULT);

pub fn accent() -> egui::Color32 {
    *ACCENT.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn default_accent() -> egui::Color32 {
    MIKU_TEAL_DEFAULT
}

/// Sets the accent value only, without re-applying `egui::Visuals`. Used at
/// startup to seed the saved color before the first `apply_miku_theme` call.
pub fn set_accent_value(color: egui::Color32) {
    *ACCENT.lock().unwrap_or_else(|e| e.into_inner()) = color;
}

/// Sets the accent and immediately re-applies the theme. `Visuals` fields
/// like `widgets.hovered.bg_fill` are snapshotted by `ctx.set_visuals` at
/// call time, so a later `ACCENT` change needs a fresh `apply_miku_theme`
/// call to actually show up in hover/selection colors.
pub fn set_accent(ctx: &egui::Context, color: egui::Color32) {
    set_accent_value(color);
    apply_miku_theme(ctx);
}

/// "Glass card" frame matching the reference mockup's `.glass-card`: dark
/// panel fill, subtle border, generous rounding. Used throughout the tabbed
/// layout for visual consistency instead of every call site picking its own
/// fill/stroke/rounding.
pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(PANEL_DARK)
        .stroke(egui::Stroke::new(1.0, CARD_BORDER))
        .rounding(egui::Rounding::same(14.0))
        .inner_margin(egui::Margin::same(14.0))
}

pub fn apply_miku_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.window_fill = DARK_BG;
    v.panel_fill  = DARK_BG;
    v.widgets.inactive.bg_fill   = PANEL_BG;
    v.widgets.inactive.rounding  = egui::Rounding::same(10.0);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, CARD_BORDER);
    v.widgets.hovered.bg_fill    = accent();
    v.widgets.hovered.fg_stroke  = egui::Stroke::new(1.5, egui::Color32::BLACK);
    // `Visuals::dark()` leaves hovered/open rounding at its small (~3px)
    // default while inactive/active were bumped to 10 — every button
    // visibly snapped to a sharper corner radius the instant the cursor
    // touched it. Matching all four states stops that flicker.
    v.widgets.hovered.rounding   = egui::Rounding::same(10.0);
    v.widgets.active.bg_fill     = MIKU_PINK;
    v.widgets.active.fg_stroke   = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.rounding    = egui::Rounding::same(10.0);
    v.widgets.open.rounding      = egui::Rounding::same(10.0);
    v.selection.bg_fill          = accent();
    // Widened from 1.0: this stroke is also what egui draws around a
    // *focused* TextEdit, so a thicker accent line here doubles as the
    // "glowing" focus indicator the input fields were missing.
    v.selection.stroke           = egui::Stroke::new(1.8, accent());
    ctx.set_visuals(v);

    // Default `button_padding` (4, 1) and `item_spacing.y` (3) are tuned for
    // dense inspector-style UIs, not the roomier "glass card" look this app
    // is going for — text sat almost flush against input/button edges.
    ctx.style_mut(|style| {
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.item_spacing.y = 6.0;
    });
}
