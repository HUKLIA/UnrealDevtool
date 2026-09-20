use eframe::egui;

use crate::app::DevToolApp;
use crate::theme::*;
use crate::types::Sheet;

/// One stop on the tour.
///
/// `sheet` is the surface the step is about. The guide switches to it before
/// drawing, which is what lets the manual walk the whole app rather than only
/// the build screen: it opens the page, then points at a control on it.
struct Step {
    title: &'static str,
    body:  &'static str,
    /// Surface to show behind the callout. `None` means the run surface.
    sheet: Option<Sheet>,
}

/// The manual, in the order someone would actually meet these things.
const STEPS: &[Step] = &[
    Step {
        title: "Welcome",
        body:  "This is the whole app: one screen that follows your build. \
                Use Next or the arrow keys to walk through it; Escape leaves at any point.",
        sheet: None,
    },
    Step {
        title: "Choose a project",
        body:  "Everything starts here. Pick a .uproject and the matching Unreal Engine \
                is detected for you. The project and engine stay shown in the top bar.",
        sheet: None,
    },
    Step {
        title: "Pick a platform",
        body:  "Windows, Android or Linux. Mac is greyed out because Unreal cannot \
                cross-compile it from Windows — that build has to run on a Mac.",
        sheet: None,
    },
    Step {
        title: "Configure the build",
        body:  "Shipping or Development, the packaging method (full, compile first, or \
                restage the last cook), and the package and executable names. \
                Turn on Iterate to reuse the previous cook and rebuild only what changed — \
                much faster, but use a full cook for anything you ship.",
        sheet: None,
    },
    Step {
        title: "Check before you build",
        body:  "Engine, project, disk and editor status at a glance. Open Details \
                whenever one of them is amber or red, rather than starting a build \
                that is going to fail 20 minutes in.",
        sheet: None,
    },
    Step {
        title: "Start the build",
        body:  "The screen becomes a live console: a stage pipeline driven by Unreal's \
                own progress, per-stage timing, and the real build log as it is written.",
        sheet: None,
    },
    Step {
        title: "Your past builds",
        body:  "Every build you have made, with its size and age. Click any row to open \
                that build's folder in Explorer.",
        sheet: None,
    },
    Step {
        title: "Source control",
        body:  "Branch, ahead/behind and whether the tree is clean — the things worth \
                knowing before you build. Git menu opens the full commit, sync and \
                merge flow.",
        sheet: None,
    },
    Step {
        title: "Project setup & checks",
        body:  "Change the project or engine, read the full preflight results, scan a \
                pasted log, and clean the regenerable folders when a build fails for \
                no obvious reason.",
        sheet: Some(Sheet::Diagnostics),
    },
    Step {
        title: "Project monitor",
        body:  "A live view of the project: which Unreal processes are running and what \
                they cost, the editor's log as it is written (filter it to warnings or \
                errors), folder sizes, your latest edits and any crash. It only reads — \
                it never touches the editor.",
        sheet: Some(Sheet::Monitor),
    },
    Step {
        title: "Dev Assistant",
        body:  "A local LLM (Ollama or LM Studio) with your project, engine and branch \
                sent as context on every message. Nothing leaves your machine.",
        sheet: Some(Sheet::Chat),
    },
    Step {
        title: "Built-in browser",
        body:  "ChatGPT, Claude, Gemini and the Unreal docs, without leaving the app. \
                Sign-ins persist between restarts like a normal browser profile.",
        sheet: Some(Sheet::Browser),
    },
    Step {
        title: "Extras",
        body:  "The Miku visualiser, mini-games, quick links, the app's own self-check \
                and the Discord composer.",
        sheet: Some(Sheet::Extras),
    },
    Step {
        title: "Settings",
        body:  "Swap the visualiser image and sound, and pick the accent colour — every \
                tinted surface in the app follows it. That is the end of the tour.",
        sheet: Some(Sheet::Settings),
    },
];

pub const GUIDE_STEPS: usize = STEPS.len();

/// Step indices by name.
///
/// Anchors used to pass bare numbers, which silently pointed at the wrong
/// control the moment a step was inserted. Naming them makes a reorder a
/// compile-time edit in one place.
pub mod step {
    pub const PROJECT:     usize = 1;
    pub const PLATFORM:    usize = 2;
    pub const CONFIGURE:   usize = 3;
    pub const PRECHECKS:   usize = 4;
    pub const START:       usize = 5;
    pub const BUILDS:      usize = 6;
    pub const SOURCE:      usize = 7;
    pub const CHECKS:      usize = 8;
    pub const MONITOR:     usize = 9;
    pub const CHAT:        usize = 10;
    pub const BROWSER:     usize = 11;
    pub const EXTRAS:      usize = 12;
    pub const SETTINGS:    usize = 13;
}

impl DevToolApp {
    /// Records the rectangle of the live control the current step describes.
    ///
    /// Call sites pass their own step index; the guide paints after the
    /// surface, so arrows follow the real control through resizes and state
    /// changes rather than pointing at a remembered position.
    pub(crate) fn guide_anchor(&mut self, step: usize, rect: egui::Rect) {
        if self.guide_active && self.guide_step == step {
            self.guide_target = Some(rect);
        }
    }

    pub fn open_guide(&mut self) {
        self.guide_active = true;
        self.guide_step = 0;
        self.guide_target = None;
        self.apply_guide_sheet();
    }

    pub fn close_guide(&mut self) {
        self.guide_active = false;
        self.guide_target = None;
        self.sheet = None;
    }

    /// Shows whatever surface the current step is about.
    fn apply_guide_sheet(&mut self) {
        let wanted = STEPS.get(self.guide_step).and_then(|s| s.sheet);
        if self.sheet != wanted {
            match wanted {
                Some(sheet) => self.open_sheet(sheet),
                None => self.sheet = None,
            }
        }
    }

    fn set_guide_step(&mut self, step: usize) {
        self.guide_step = step.min(GUIDE_STEPS - 1);
        self.guide_target = None;
        self.apply_guide_sheet();
    }

    /// Draws the tour over whatever is currently on screen.
    pub fn show_guide_overlay(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        // Swallow clicks outside the callout so the dimmed surface behind
        // cannot accidentally start a build or open something mid-tour.
        let _ = ui.interact(screen, egui::Id::new("guide_backdrop_hit"), egui::Sense::click());
        // Keyboard: a tour you have to mouse through is a tour people abandon.
        let (esc, fwd, back_key) = ctx.input(|i| (
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::ArrowRight)
                || i.key_pressed(egui::Key::Enter)
                || i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::ArrowLeft),
        ));
        if esc {
            self.close_guide();
            return;
        }
        if back_key && self.guide_step > 0 {
            self.set_guide_step(self.guide_step - 1);
        } else if fwd {
            if self.guide_step + 1 >= GUIDE_STEPS {
                self.close_guide();
                return;
            }
            self.set_guide_step(self.guide_step + 1);
        }

        let step = &STEPS[self.guide_step.min(GUIDE_STEPS - 1)];

        // A step with no anchored control dims the whole screen and centres
        // its card, instead of drawing a spotlight around a stale rectangle.
        let target = self.guide_target.map(|r| r.intersect(screen).expand(7.0).intersect(screen));

        let dim = egui::Color32::from_black_alpha(208);
        let painter = ui.painter();
        match target {
            Some(t) if t.is_positive() => {
                for rect in spotlight_regions(screen, t) {
                    if rect.is_positive() {
                        painter.rect_filled(rect, egui::Rounding::ZERO, dim);
                    }
                }
                painter.rect_stroke(t, egui::Rounding::same(9.0), egui::Stroke::new(2.0, accent()));
            }
            _ => {
                painter.rect_filled(screen, egui::Rounding::ZERO, dim);
            }
        }

        let callout_w = if screen.width() < 272.0 {
            (screen.width() - 32.0).max(120.0)
        } else {
            (screen.width() - 32.0).clamp(260.0, 380.0)
        };
        // Height is measured from the wrapped copy rather than assumed. A flat
        // height clipped the Back/Next row on any step whose body ran past two
        // lines, which made those steps impossible to advance.
        let body_h = {
            let wrap = callout_w - 32.0;
            ui.fonts(|f| f.layout(step.body.to_owned(), body(11.5), MUTED, wrap).size().y)
        };
        // 26 padding + 30 header + 24 title + 6/5/13 gaps + 28 buttons, plus
        // the four 8px `item_spacing.y` gaps egui puts between them.
        const CHROME_H: f32 = 26.0 + 30.0 + 24.0 + 6.0 + 5.0 + 13.0 + 28.0 + 32.0;
        let callout_h = (CHROME_H + body_h).clamp(150.0, (screen.height() - 32.0).max(150.0));
        let callout_size = egui::vec2(callout_w, callout_h);

        let callout = callout_rect(screen, target.unwrap_or_else(|| {
            egui::Rect::from_center_size(screen.center(), egui::vec2(2.0, 2.0))
        }), callout_size);
        if let Some(t) = target {
            draw_guide_arrow(painter, t, callout, accent());
        }

        let mut next = false;
        let mut back = false;
        let mut close = false;

        // Painted at one explicit size: `Frame::show` is content-sized, so a
        // wrapped description could otherwise draw a second, taller outline
        // escaping the intended rectangle.
        ui.painter().rect_filled(callout, egui::Rounding::same(14.0), egui::Color32::from_rgb(10, 20, 30));
        let inner = egui::Rect::from_min_max(
            callout.min + egui::vec2(16.0, 13.0),
            callout.max - egui::vec2(16.0, 13.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        // A couple of pixels of slack for the rounded stroke: clipping exactly
        // to the child rect shaves the bottom corners at Windows DPI scales
        // and makes the card look like it has no lower edge.
        child.set_clip_rect(callout.shrink(1.0));
        child.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("MANUAL  {}/{}", self.guide_step + 1, GUIDE_STEPS))
                        .font(mono(10.5))
                        .color(accent()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(step.title).font(body(16.0)).color(TEXT).strong());
            ui.add_space(5.0);
            ui.add(egui::Label::new(hint(step.body)).wrap());
            ui.add_space(13.0);
            ui.horizontal(|ui| {
                if self.guide_step > 0 && ui.add_sized([68.0, 28.0], ghost("Back")).clicked() {
                    back = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let last = self.guide_step + 1 == GUIDE_STEPS;
                    if ui.add_sized([72.0, 28.0], primary(if last { "Done" } else { "Next" })).clicked() {
                        next = true;
                    }
                });
            });
        });

        // The edge is drawn in the parent so it cannot be affected by the
        // child layout or by text wrapping differently at another DPI scale.
        ui.painter().rect_stroke(callout, egui::Rounding::same(14.0), egui::Stroke::new(1.0, accent()));

        if close {
            self.close_guide();
        } else if back {
            self.set_guide_step(self.guide_step.saturating_sub(1));
        } else if next {
            if self.guide_step + 1 >= GUIDE_STEPS {
                self.close_guide();
            } else {
                self.set_guide_step(self.guide_step + 1);
            }
        }
    }
}

fn spotlight_regions(screen: egui::Rect, target: egui::Rect) -> [egui::Rect; 4] {
    [
        egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, target.top())),
        egui::Rect::from_min_max(
            egui::pos2(screen.min.x, target.top()),
            egui::pos2(target.left(), target.bottom()),
        ),
        egui::Rect::from_min_max(
            egui::pos2(target.right(), target.top()),
            egui::pos2(screen.max.x, target.bottom()),
        ),
        egui::Rect::from_min_max(egui::pos2(screen.min.x, target.bottom()), screen.max),
    ]
}

fn callout_rect(screen: egui::Rect, target: egui::Rect, size: egui::Vec2) -> egui::Rect {
    let gap = 18.0;
    let right = target.right() + gap + size.x <= screen.right();
    let left = target.left() - gap - size.x >= screen.left();
    let below = target.bottom() + gap + size.y <= screen.bottom();
    let mut pos = if right {
        egui::pos2(target.right() + gap, target.center().y - size.y * 0.5)
    } else if left {
        egui::pos2(target.left() - gap - size.x, target.center().y - size.y * 0.5)
    } else if below {
        egui::pos2(target.center().x - size.x * 0.5, target.bottom() + gap)
    } else {
        egui::pos2(target.center().x - size.x * 0.5, target.top() - gap - size.y)
    };
    pos.x = pos.x.clamp(
        screen.left() + 16.0,
        (screen.right() - size.x - 16.0).max(screen.left() + 16.0),
    );
    pos.y = pos.y.clamp(
        screen.top() + 16.0,
        (screen.bottom() - size.y - 16.0).max(screen.top() + 16.0),
    );
    egui::Rect::from_min_size(pos, size)
}

/// A leader line from the callout to the highlighted control, with a head at
/// the control end so the direction is unambiguous.
fn draw_guide_arrow(
    painter: &egui::Painter,
    target: egui::Rect,
    callout: egui::Rect,
    color: egui::Color32,
) {
    let from = callout.center();
    let to = target.center();
    let dir = (to - from).normalized();
    if !dir.is_finite() {
        return;
    }
    // Start and end on the two rectangles' edges rather than their centres, so
    // the line never runs underneath either box.
    let start = edge_point(callout, dir);
    let end = edge_point(target, -dir);
    painter.line_segment([start, end], egui::Stroke::new(2.0, color));

    let head = 9.0;
    let perp = egui::vec2(-dir.y, dir.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            end,
            end - dir * head + perp * (head * 0.5),
            end - dir * head - perp * (head * 0.5),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

/// Where a ray from the centre of `rect` in direction `dir` leaves the rect.
fn edge_point(rect: egui::Rect, dir: egui::Vec2) -> egui::Pos2 {
    let c = rect.center();
    let hx = rect.width() * 0.5;
    let hy = rect.height() * 0.5;
    let tx = if dir.x.abs() > 1e-3 { hx / dir.x.abs() } else { f32::INFINITY };
    let ty = if dir.y.abs() > 1e-3 { hy / dir.y.abs() } else { f32::INFINITY };
    c + dir * tx.min(ty)
}

#[cfg(test)]
mod tests {
    use super::{callout_rect, edge_point, GUIDE_STEPS, STEPS};
    use eframe::egui;

    /// A target hard against an edge must not push the callout off screen.
    ///
    /// This is the case the placement logic exists for: the top-bar icons the
    /// later steps point at sit in the top-right corner, so the preferred
    /// "to the right of the target" position does not fit and the callout has
    /// to fall back and then clamp.
    #[test]
    fn callout_stays_on_screen_for_a_corner_target() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(558.0, 373.0));
        let target = egui::Rect::from_min_size(egui::pos2(515.0, 30.0), egui::vec2(32.0, 32.0));
        let size = egui::vec2(360.0, 200.0);

        let callout = callout_rect(screen, target, size);

        assert!(callout.left() >= screen.left(), "left {} < {}", callout.left(), screen.left());
        assert!(callout.right() <= screen.right(), "right {} > {}", callout.right(), screen.right());
        assert!(callout.top() >= screen.top(), "top {} < {}", callout.top(), screen.top());
        assert!(callout.bottom() <= screen.bottom(), "bottom {} > {}", callout.bottom(), screen.bottom());
        assert_eq!(callout.size(), size, "placement must not resize the callout");
    }

    /// A callout wider than the screen still clamps to the left edge rather
    /// than landing at a negative x.
    #[test]
    fn callout_clamps_when_it_cannot_fit() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 200.0));
        let target = egui::Rect::from_center_size(screen.center(), egui::vec2(10.0, 10.0));
        let callout = callout_rect(screen, target, egui::vec2(400.0, 300.0));
        assert_eq!(callout.left(), screen.left() + 16.0);
        assert_eq!(callout.top(), screen.top() + 16.0);
    }

    /// The arrow has to start on the box's edge, not its centre, or the line
    /// is drawn underneath the callout it comes from.
    #[test]
    fn edge_point_lands_on_the_rect_boundary() {
        let r = egui::Rect::from_center_size(egui::pos2(100.0, 100.0), egui::vec2(40.0, 20.0));
        let right = edge_point(r, egui::vec2(1.0, 0.0));
        assert!((right.x - r.right()).abs() < 0.01, "{right:?}");
        let down = edge_point(r, egui::vec2(0.0, 1.0));
        assert!((down.y - r.bottom()).abs() < 0.01, "{down:?}");
    }

    /// Every named step index must address a real step.
    #[test]
    fn named_step_indices_are_in_range() {
        use super::step::*;
        for (name, idx) in [
            ("PROJECT", PROJECT), ("PLATFORM", PLATFORM), ("CONFIGURE", CONFIGURE),
            ("PRECHECKS", PRECHECKS), ("START", START), ("BUILDS", BUILDS),
            ("SOURCE", SOURCE), ("CHECKS", CHECKS), ("CHAT", CHAT),
            ("BROWSER", BROWSER), ("EXTRAS", EXTRAS), ("SETTINGS", SETTINGS),
        ] {
            assert!(idx < GUIDE_STEPS, "{name} = {idx} is past the last step ({GUIDE_STEPS})");
        }
        assert_eq!(STEPS.len(), GUIDE_STEPS);
    }
}
