use eframe::egui;
use std::sync::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// Design tokens.
//
// These are the values from the approved canvas, one name per value, and
// nothing outside this file invents a fill, stroke, radius or font size.
//
// The palette is Miku's: a navy-ink ground, hair teal as the one accent every
// interactive affordance derives from, tie pink as the only secondary, and a
// five-step text ramp so a dense screen still has hierarchy without rules or
// boxes everywhere.
// ─────────────────────────────────────────────────────────────────────────────

const MIKU_TEAL_DEFAULT: egui::Color32 = egui::Color32::from_rgb(57, 197, 187);

// ── Surfaces ────────────────────────────────────────────────────────────────
/// App ground.
pub const BG:        egui::Color32 = egui::Color32::from_rgb(  7,  13,  20);
/// Top chrome — one step up from the ground so the bar reads as attached.
pub const BG_TOP:    egui::Color32 = egui::Color32::from_rgb( 11,  21,  32);
/// Section / card.
pub const CARD:      egui::Color32 = egui::Color32::from_rgb( 12,  23,  35);
/// Inset control: input, chip, secondary button.
pub const WELL:      egui::Color32 = egui::Color32::from_rgb( 18,  32,  47);
/// Deepest inset: log surface, media stage.
pub const DEEP:      egui::Color32 = egui::Color32::from_rgb(  5,  10,  16);
/// Track behind a progress segment.
pub const TRACK:     egui::Color32 = egui::Color32::from_rgb( 22,  40,  58);

// ── Lines ───────────────────────────────────────────────────────────────────
pub const LINE:      egui::Color32 = egui::Color32::from_rgb( 26,  46,  64);
/// Quieter divider, for use inside an already-bordered surface.
pub const LINE_SOFT: egui::Color32 = egui::Color32::from_rgb( 20,  36,  47);

// ── Text ramp ───────────────────────────────────────────────────────────────
pub const TEXT:  egui::Color32 = egui::Color32::from_rgb(230, 246, 250);
pub const SOFT:  egui::Color32 = egui::Color32::from_rgb(169, 198, 210);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(127, 160, 176);
pub const DIM:   egui::Color32 = egui::Color32::from_rgb( 95, 126, 142);
pub const FAINT: egui::Color32 = egui::Color32::from_rgb( 78, 107, 124);

// ── State ───────────────────────────────────────────────────────────────────
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(102, 224, 189);
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(243, 191, 104);
pub const RED:   egui::Color32 = egui::Color32::from_rgb(255, 123, 137);

// ── Gutters ─────────────────────────────────────────────────────────────────
/// Space between the window edge and any content, on every side.
///
/// One value rather than four. It used to be 22/26/20/22 — the right side was
/// widened by hand to clear a floating scrollbar. Scrollbars now reserve their
/// own space inside the content area, so the asymmetry no longer bought
/// anything and only made the frame look lopsided.
pub const GUTTER: f32 = 24.0;

/// Inset from the window edge to a sheet panel.
pub const SHEET_GUTTER: f32 = 40.0;

/// Padding inside a sheet, between its border and its content.
pub const SHEET_PAD: f32 = 18.0;

// ── Radii ───────────────────────────────────────────────────────────────────
pub const R_SECTION: f32 = 18.0;
pub const R_CARD:    f32 = 16.0;
pub const R_CTL:     f32 = 11.0;
pub const R_PILL:    f32 =  9.0;

// ── Motion ──────────────────────────────────────────────────────────────────
/// Rumi's `--t-base`. Long enough to read as motion, short enough never to
/// delay a click.
pub const T_BASE: f32 = 0.26;
/// Hover and other micro-transitions.
pub const T_FAST: f32 = 0.14;

/// Snaps a duration to a whole number of display frames.
///
/// Durations stay expressed in seconds, which is what makes a transition take
/// the same real time on a 60 Hz and a 144 Hz panel. What this adds is landing
/// the *end* of the animation on a refresh boundary: an interval that is not a
/// multiple of the frame time finishes mid-frame, so the last step is a
/// partial one and the motion ends with a visible hitch. `frame_dt` comes from
/// egui's smoothed frame time, so this follows the real panel rather than an
/// assumed 60 Hz.
pub fn anim_secs(base: f32, frame_dt: f32) -> f32 {
    if !(frame_dt.is_finite() && frame_dt > 0.0) {
        return base;
    }
    let frames = (base / frame_dt).round().max(1.0);
    frames * frame_dt
}

/// Fast departure, soft landing. A linear interpolation is most of what makes
/// an animation feel cheap.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

// ── Accent ──────────────────────────────────────────────────────────────────

/// Process-wide, user-customizable accent. Every accent-tinted surface in the
/// app derives from this rather than hard-coding a second copy of the colour,
/// so a colour picked at runtime propagates everywhere at once.
static ACCENT: Mutex<egui::Color32> = Mutex::new(MIKU_TEAL_DEFAULT);

pub fn accent() -> egui::Color32 {
    *ACCENT.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn default_accent() -> egui::Color32 {
    MIKU_TEAL_DEFAULT
}

/// The accent at `alpha`/255.
pub fn acc(alpha: u8) -> egui::Color32 {
    tint(accent(), alpha)
}

/// Any colour at `alpha`/255.
pub fn tint(c: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// Linear blend, `t` in 0..1.
pub fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Sets the accent without re-applying `Visuals`. Used at startup to seed the
/// saved colour before the first `apply_theme` call.
pub fn set_accent_value(color: egui::Color32) {
    *ACCENT.lock().unwrap_or_else(|e| e.into_inner()) = color;
}

/// Sets the accent and re-applies the theme. `Visuals` fields are snapshotted
/// by `set_visuals` at call time, so a later accent change needs a fresh
/// `apply_theme` to show up in hover/selection colours.
pub fn set_accent(ctx: &egui::Context, color: egui::Color32) {
    set_accent_value(color);
    apply_theme(ctx);
}

// ── Fonts ───────────────────────────────────────────────────────────────────

/// Heading / numeric family — Space Grotesk Bold.
pub fn display_family() -> egui::FontFamily {
    egui::FontFamily::Name("display".into())
}

pub fn display(size: f32) -> egui::FontId {
    egui::FontId::new(size, display_family())
}

pub fn body(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Proportional)
}

pub fn mono(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Monospace)
}

/// Installs the bundled typefaces.
///
/// egui's stock face is a generic grotesque that made every screen read as an
/// unstyled debug tool no matter what the layout did — the typography was
/// doing none of the work. Space Grotesk carries the UI and its numerals,
/// JetBrains Mono every path, version and log line.
///
/// The built-in families stay in the list *behind* ours rather than being
/// replaced: Space Grotesk is Latin-only, so the defaults remain the fallback
/// for emoji and anything else outside its coverage. Dropping them would turn
/// every uncovered glyph into a tofu box.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "sg_medium".to_owned(),
        egui::FontData::from_static(include_bytes!("../Fonts/SpaceGrotesk-Medium.ttf")),
    );
    fonts.font_data.insert(
        "sg_bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../Fonts/SpaceGrotesk-Bold.ttf")),
    );
    fonts.font_data.insert(
        "jb_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("../Fonts/JetBrainsMono-Regular.ttf")),
    );

    fonts.families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "sg_medium".to_owned());

    fonts.families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "jb_mono".to_owned());

    // The display family gets the proportional list appended behind it for the
    // same fallback reason.
    let mut display_stack = vec!["sg_bold".to_owned()];
    display_stack.extend(
        fonts.families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts.families.insert(egui::FontFamily::Name("display".into()), display_stack);

    ctx.set_fonts(fonts);
}

// ── Frames ──────────────────────────────────────────────────────────────────

/// The primary container: a titled region of the page.
pub fn section() -> egui::Frame {
    egui::Frame::none()
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(R_SECTION))
        .inner_margin(egui::Margin::symmetric(26.0, 24.0))
}

/// A smaller card, used in the rail.
pub fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(CARD)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(R_CARD))
        .inner_margin(egui::Margin::symmetric(17.0, 16.0))
}

/// Inset well: an input, a stat tile, a nested block.
pub fn well() -> egui::Frame {
    egui::Frame::none()
        .fill(WELL)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(15.0, 13.0))
}

/// Log / media surface — darker than the ground it sits on.
pub fn deep() -> egui::Frame {
    egui::Frame::none()
        .fill(DEEP)
        .stroke(egui::Stroke::new(1.0, LINE_SOFT))
        .rounding(egui::Rounding::same(R_CARD))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
}

/// Tinted callout, for a warning or an accent-toned note.
pub fn callout(color: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(tint(color, 11))
        .stroke(egui::Stroke::new(1.0, tint(color, 44)))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(15.0, 12.0))
}

// ── Text ────────────────────────────────────────────────────────────────────

/// The small letterspaced label above every section. Cloned from the canvas:
/// 10.5px, muted, and always upper case at the call site.
pub fn eyebrow(text: &str) -> egui::RichText {
    egui::RichText::new(text).font(body(10.5)).color(MUTED)
}

/// Page / section heading.
pub fn heading(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(display(size)).color(TEXT)
}

/// A number meant to be read as a number — version, size, duration.
pub fn numeral(text: &str, size: f32, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text).font(mono(size)).color(color)
}

pub fn hint(text: &str) -> egui::RichText {
    egui::RichText::new(text).font(body(11.5)).color(MUTED)
}

// ── Widgets ─────────────────────────────────────────────────────────────────

/// A filled dot with a glow ring. The app's one status indicator.
pub fn dot(ui: &mut egui::Ui, color: egui::Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let c = rect.center();
    ui.painter().circle_filled(c, size * 0.5, tint(color, 70));
    ui.painter().circle_filled(c, size * 0.29, color);
}

/// The single primary action on a screen.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).font(body(14.0)).color(egui::Color32::from_rgb(214, 251, 247)))
        .fill(acc(34))
        .stroke(egui::Stroke::new(1.0, acc(115)))
        .rounding(egui::Rounding::same(13.0))
}

/// Everything else. Same geometry as `primary` so mixed rows align.
pub fn ghost(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).font(body(12.5)).color(SOFT))
        .fill(WELL)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(R_CTL))
}

/// Quieter still: no fill, for a tertiary action inside a card.
pub fn quiet(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).font(body(12.0)).color(MUTED))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(R_CTL))
}

/// Destructive — cancelling a run, and nothing else.
pub fn danger(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).font(body(13.0)).color(RED))
        .fill(tint(RED, 20))
        .stroke(egui::Stroke::new(1.0, tint(RED, 62)))
        .rounding(egui::Rounding::same(12.0))
}

/// A chip that can be pressed — the config selectors on the run surface.
pub fn chip(text: &str, active: bool) -> egui::Button<'static> {
    if active {
        egui::Button::new(egui::RichText::new(text).font(body(12.5)).color(egui::Color32::from_rgb(191, 239, 234)))
            .fill(acc(26))
            .stroke(egui::Stroke::new(1.0, acc(77)))
            .rounding(egui::Rounding::same(R_PILL))
    } else {
        egui::Button::new(egui::RichText::new(text).font(body(12.5)).color(SOFT))
            .fill(WELL)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(R_PILL))
    }
}

/// A horizontal progress segment, as used by the pipeline and the timing rail.
pub fn segment(ui: &mut egui::Ui, width: f32, frac: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
    let r = egui::Rounding::same(3.0);
    ui.painter().rect_filled(rect, r, TRACK);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        let filled = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * f, rect.height()));
        ui.painter().rect_filled(filled, r, color);
    }
}

/// Square icon button for the top bar. `glyph` is drawn from a tiny painted
/// vocabulary rather than a font glyph — egui's bundled faces cover almost
/// none of the symbols this needs, and a missing glyph renders as a tofu box.
pub fn icon_button(ui: &mut egui::Ui, kind: Icon, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), T_FAST);
    let sel = ui.ctx().animate_bool_with_time(resp.id.with("s"), active, T_FAST);

    let bg = (sel * 30.0 + hov * (1.0 - sel) * 16.0) as u8;
    if bg > 0 {
        ui.painter().rect_filled(rect, egui::Rounding::same(R_PILL), acc(bg));
    }
    let col = mix(mix(MUTED, SOFT, hov), accent(), sel);
    paint_icon(ui.painter(), rect.center(), kind, col);
    if (hov > 0.0 && hov < 1.0) || (sel > 0.0 && sel < 1.0) {
        ui.ctx().request_repaint();
    }
    resp
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Globe,
    Chat,
    Dots,
    Gear,
    Folder,
    Copy,
    Check,
    Help,
    Play,
    Note,
    Cross,
    Pulse,
    Wrench,
    ArrowLeft,
    ArrowRight,
    Reload,
    Home,
}

/// Icons are stroked paths on a 16px box, centred on `c`.
pub fn paint_icon(p: &egui::Painter, c: egui::Pos2, kind: Icon, color: egui::Color32) {
    let s = egui::Stroke::new(1.6, color);
    let r = 7.0;
    match kind {
        Icon::Globe => {
            p.circle_stroke(c, r, s);
            p.line_segment([c - egui::vec2(r, 0.0), c + egui::vec2(r, 0.0)], s);
            // Two meridians, drawn as thin ellipses via line fans.
            for k in [-1.0f32, 1.0] {
                let mut pts = Vec::new();
                for i in 0..=10 {
                    let t = i as f32 / 10.0;
                    let y = -r + t * 2.0 * r;
                    let x = k * (r * 0.52) * (1.0 - (y / r).powi(2)).max(0.0).sqrt();
                    pts.push(c + egui::vec2(x, y));
                }
                p.add(egui::Shape::line(pts, s));
            }
        }
        Icon::Chat => {
            let rect = egui::Rect::from_center_size(c - egui::vec2(0.0, 1.0), egui::vec2(r * 2.0, r * 1.7));
            p.rect_stroke(rect, egui::Rounding::same(4.0), s);
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-r * 0.45, r * 0.85 - 1.0),
                    c + egui::vec2(-r * 0.85, r * 1.5),
                    c + egui::vec2(-r * 0.05, r * 0.85 - 1.0),
                ],
                s,
            ));
        }
        Icon::Dots => {
            for dx in [-5.0f32, 0.0, 5.0] {
                p.circle_filled(c + egui::vec2(dx, 0.0), 1.6, color);
            }
        }
        Icon::Gear => {
            p.circle_stroke(c, r * 0.42, s);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = egui::vec2(a.cos(), a.sin());
                p.line_segment([c + d * (r * 0.68), c + d * r], s);
            }
        }
        Icon::Folder => {
            let rect = egui::Rect::from_center_size(c + egui::vec2(0.0, 1.0), egui::vec2(r * 2.0, r * 1.5));
            p.rect_stroke(rect, egui::Rounding::same(3.0), s);
            p.line_segment([
                rect.left_top() + egui::vec2(0.5, -3.0),
                rect.left_top() + egui::vec2(r * 0.8, -3.0),
            ], s);
        }
        Icon::Copy => {
            let a = egui::Rect::from_center_size(c + egui::vec2(1.5, 1.5), egui::vec2(r * 1.5, r * 1.5));
            let b = egui::Rect::from_center_size(c - egui::vec2(1.5, 1.5), egui::vec2(r * 1.5, r * 1.5));
            p.rect_stroke(b, egui::Rounding::same(2.0), s);
            p.rect_stroke(a, egui::Rounding::same(2.0), s);
        }
        Icon::Wrench => {
            // An open-ended spanner: a diagonal shaft with a notched head.
            let a = c + egui::vec2(-r * 0.75, r * 0.75);
            let b = c + egui::vec2(r * 0.25, -r * 0.25);
            p.line_segment([a, b], egui::Stroke::new(2.4, color));
            p.circle_stroke(c + egui::vec2(r * 0.42, -r * 0.42), r * 0.45, s);
            p.line_segment([c + egui::vec2(r * 0.42, -r * 0.42), c + egui::vec2(r * 0.85, -r * 0.85)], egui::Stroke::new(3.0, BG));
        }
        Icon::Pulse => {
            // A heartbeat trace.
            let pts = [(-1.0, 0.0), (-0.45, 0.0), (-0.25, -0.75), (0.05, 0.85), (0.3, -0.2), (0.45, 0.0), (1.0, 0.0)];
            p.add(egui::Shape::line(
                pts.iter().map(|(x, y)| c + egui::vec2(x * r, y * r * 0.9)).collect(),
                egui::Stroke::new(1.6, color),
            ));
        }
        Icon::Cross => {
            let k = r * 0.55;
            let st = egui::Stroke::new(2.0, color);
            p.line_segment([c + egui::vec2(-k, -k), c + egui::vec2(k, k)], st);
            p.line_segment([c + egui::vec2(-k, k), c + egui::vec2(k, -k)], st);
        }
        Icon::Check => {
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-r * 0.7, 0.0),
                    c + egui::vec2(-r * 0.15, r * 0.55),
                    c + egui::vec2(r * 0.75, -r * 0.6),
                ],
                egui::Stroke::new(2.0, color),
            ));
        }
        Icon::Help => {
            p.circle_stroke(c, r, s);
            p.text(c, egui::Align2::CENTER_CENTER, "?", body(14.0), color);
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-r * 0.45, -r * 0.7),
                    c + egui::vec2(r * 0.7, 0.0),
                    c + egui::vec2(-r * 0.45, r * 0.7),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        Icon::ArrowLeft | Icon::ArrowRight => {
            // A chevron, drawn from the centre so it is optically centred in
            // its button. The old "<" and ">" were text glyphs whose own
            // side-bearings pushed them off-centre inside a square button.
            // `d` is the direction the chevron points, so the apex sits at
            // `d * apex` and the open ends trail behind it. Getting this
            // backwards drew Back as ">" and Forward as "<".
            let d = if kind == Icon::ArrowLeft { -1.0 } else { 1.0 };
            let apex = 2.6;
            let tail = 2.2;
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-d * tail, -r * 0.7),
                    c + egui::vec2(d * apex, 0.0),
                    c + egui::vec2(-d * tail, r * 0.7),
                ],
                egui::Stroke::new(2.0, color),
            ));
        }
        Icon::Reload => {
            // An open circle with a gap and an arrow head on the leading end.
            let mut pts = Vec::new();
            let start = -0.55 * std::f32::consts::TAU;
            let sweep = 0.78 * std::f32::consts::TAU;
            for i in 0..=18 {
                let a = start + sweep * (i as f32 / 18.0);
                pts.push(c + egui::vec2(a.cos(), a.sin()) * (r * 0.78));
            }
            let tip = *pts.last().unwrap();
            p.add(egui::Shape::line(pts, s));
            let a = start + sweep;
            let tangent = egui::vec2(-a.sin(), a.cos());
            let perp = egui::vec2(-tangent.y, tangent.x);
            p.add(egui::Shape::convex_polygon(
                vec![
                    tip + tangent * 4.0,
                    tip - tangent * 1.5 + perp * 3.2,
                    tip - tangent * 1.5 - perp * 3.2,
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        Icon::Home => {
            // Roof drawn wider than the walls so the two read as one house
            // rather than a triangle resting on a box.
            let roof_y = -r * 0.15;
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-r * 0.95, roof_y),
                    c + egui::vec2(0.0, -r * 0.9),
                    c + egui::vec2(r * 0.95, roof_y),
                ],
                s,
            ));
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-r * 0.62, roof_y),
                    c + egui::vec2(-r * 0.62, r * 0.8),
                    c + egui::vec2(r * 0.62, r * 0.8),
                    c + egui::vec2(r * 0.62, roof_y),
                ],
                s,
            ));
        }
        Icon::Note => {
            p.line_segment([c + egui::vec2(2.0, -r), c + egui::vec2(2.0, r * 0.45)], s);
            p.line_segment([c + egui::vec2(2.0, -r), c + egui::vec2(r, -r * 0.6)], s);
            p.circle_stroke(c + egui::vec2(-1.0, r * 0.5), 3.0, s);
        }
    }
}

// ── Visuals ─────────────────────────────────────────────────────────────────

/// A square icon button sized for a toolbar row: the same surface as `ghost`,
/// with a painted glyph centred in it rather than a text label.
pub fn icon_ghost(ui: &mut egui::Ui, kind: Icon, size: egui::Vec2) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), T_FAST);
    let fill = mix(WELL, acc(30), hov);
    let stroke = mix(LINE, acc(110), hov);
    ui.painter().rect_filled(rect, egui::Rounding::same(R_CTL), fill);
    ui.painter().rect_stroke(rect, egui::Rounding::same(R_CTL), egui::Stroke::new(1.0, stroke));
    paint_icon(ui.painter(), rect.center(), kind, mix(SOFT, TEXT, hov));
    if hov > 0.0 && hov < 1.0 { ui.ctx().request_repaint(); }
    resp
}


pub fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.window_fill         = BG;
    v.panel_fill          = BG;
    v.extreme_bg_color    = WELL;
    v.override_text_color = Some(TEXT);

    let r = egui::Rounding::same(R_CTL);
    v.widgets.noninteractive.bg_fill   = CARD;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, MUTED);
    v.widgets.noninteractive.rounding  = r;

    v.widgets.inactive.bg_fill      = WELL;
    v.widgets.inactive.weak_bg_fill = WELL;
    v.widgets.inactive.fg_stroke    = egui::Stroke::new(1.0, SOFT);
    v.widgets.inactive.bg_stroke    = egui::Stroke::new(1.0, LINE);
    v.widgets.inactive.rounding     = r;

    // Hover tints with the accent hue instead of swapping to a solid fill,
    // which is what used to make every button flash a bright teal slab.
    v.widgets.hovered.bg_fill      = acc(30);
    v.widgets.hovered.weak_bg_fill = acc(30);
    v.widgets.hovered.fg_stroke    = egui::Stroke::new(1.0, TEXT);
    v.widgets.hovered.bg_stroke    = egui::Stroke::new(1.0, acc(110));
    v.widgets.hovered.rounding     = r;

    v.widgets.active.bg_fill      = acc(56);
    v.widgets.active.weak_bg_fill = acc(56);
    v.widgets.active.fg_stroke    = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.bg_stroke    = egui::Stroke::new(1.0, accent());
    v.widgets.active.rounding     = r;

    v.widgets.open.bg_fill   = WELL;
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, acc(90));
    v.widgets.open.rounding  = r;

    v.selection.bg_fill = acc(70);
    v.selection.stroke  = egui::Stroke::new(1.5, accent());

    v.window_stroke   = egui::Stroke::new(1.0, LINE);
    v.window_rounding = egui::Rounding::same(R_CARD);
    v.window_fill     = CARD;
    v.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 10.0),
        blur:   30.0,
        spread: 0.0,
        color:  egui::Color32::from_black_alpha(120),
    };
    v.window_shadow = v.popup_shadow;

    ctx.set_visuals(v);

    ctx.style_mut(|style| {
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.item_spacing   = egui::vec2(8.0, 8.0);
        style.spacing.menu_margin    = egui::Margin::same(8.0);
        // Scrollbars reserve space instead of floating over the content.
        //
        // egui defaults `floating` to true, which draws the bar as an overlay
        // hard against the right edge of the scroll viewport — on top of
        // whatever card is there. That is what made card borders look like
        // they were touching the window edge: the bar was outside the card's
        // padding, flush with the frame.
        style.spacing.scroll.floating = false;
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.bar_inner_margin = 4.0;
        style.spacing.scroll.bar_outer_margin = 4.0;

        // Text styles are set from the bundled faces so a bare `ui.label`
        // lands on the right size and family without every call site saying so.
        use egui::TextStyle::*;
        style.text_styles = [
            (Heading,  display(19.0)),
            (Body,     body(12.5)),
            (Button,   body(12.5)),
            (Small,    body(11.0)),
            (Monospace, mono(11.5)),
        ].into();
    });
}


/// A small filled trace of recent values, newest at the right edge.
///
/// `max` fixes the top of the scale so a quiet machine does not look busy: the
/// line is drawn against 0..max rather than being stretched to fill the box.
pub fn sparkline(ui: &mut egui::Ui, size: egui::Vec2, values: &std::collections::VecDeque<f32>, max: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, egui::Rounding::same(6.0), WELL);
    let n = values.len();
    if n < 2 || max <= 0.0 { return; }
    let inner = rect.shrink2(egui::vec2(3.0, 4.0));
    // A young trace is spread over at least 30 slots, so the first seconds show
    // a short line at the right instead of a dot; a full one uses all of them.
    let slots = n.clamp(30, crate::ops::monitor::HISTORY) as f32;
    let step = inner.width() / (slots - 1.0);
    let x0 = inner.right() - step * (n as f32 - 1.0);
    let pts: Vec<egui::Pos2> = values.iter().enumerate()
        .map(|(i, v)| egui::pos2(x0 + step * i as f32, inner.bottom() - (v / max).clamp(0.0, 1.0) * inner.height()))
        .collect();
    // Fill under the line, then the line itself.
    let mut fill = pts.clone();
    fill.push(egui::pos2(pts[n - 1].x, inner.bottom()));
    fill.push(egui::pos2(pts[0].x, inner.bottom()));
    p.add(egui::Shape::convex_polygon(fill, tint(color, 28), egui::Stroke::NONE));
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.5, color)));
}
