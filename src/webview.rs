use eframe::egui;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle,
};
use std::collections::HashMap;
use wry::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use wry::{Rect, WebView, WebViewBuilder};

/// Web pages that can be embedded as a child WebView2 control inside the app window.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebPanel {
    Miku3D,
    CookieClicker,
    SponderBird,
}

impl WebPanel {
    pub fn url(self) -> &'static str {
        match self {
            WebPanel::Miku3D        => "https://huklia.github.io/MikuTest/",
            WebPanel::CookieClicker => "https://orteil.dashnet.org/cookieclicker/",
            WebPanel::SponderBird   => "https://nicktam1.github.io/SponderBirdNew/",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            WebPanel::Miku3D        => "3D Miku",
            WebPanel::CookieClicker => "Cookie Clicker",
            WebPanel::SponderBird   => "Sponder Bird",
        }
    }
}

/// Plain copy of the host window's raw handles. The eframe `CreationContext`
/// they come from only lives during app setup, but `RawWindowHandle` /
/// `RawDisplayHandle` are `Copy` and remain valid for the life of the window
/// (i.e. the life of the app).
struct ParentWindow {
    window:  RawWindowHandle,
    display: RawDisplayHandle,
}

impl HasWindowHandle for ParentWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for ParentWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

/// A created (or failed) webview plus the bounds/visibility we last applied
/// to it, so `update` can skip redundant `set_bounds`/`set_visible` calls.
/// Calling these every frame causes WebView2 to repeatedly drop focus, which
/// breaks the Pointer Lock API used by Unity WebGL content (e.g. 3D Miku)
/// with a "user has exited the lock" SecurityError.
struct ViewEntry {
    view:    Result<WebView, String>,
    bounds:  (i32, i32, u32, u32),
    visible: bool,
}

/// Lazily creates embedded WebView2 child windows for the app's web panels
/// and shows/hides/repositions them to follow an egui placeholder rect.
/// Only one panel is visible at a time.
pub struct WebViewManager {
    parent: ParentWindow,
    views:  HashMap<WebPanel, ViewEntry>,
    active: Option<WebPanel>,
}

impl WebViewManager {
    pub fn new(window: RawWindowHandle, display: RawDisplayHandle) -> Self {
        Self { parent: ParentWindow { window, display }, views: HashMap::new(), active: None }
    }

    /// Call once per frame. `wanted` is the panel that should be visible right
    /// now (with its bounds in egui points), or `None` to hide everything.
    /// Returns an error message if the panel failed to load (e.g. WebView2
    /// runtime missing).
    pub fn update(&mut self, wanted: Option<(WebPanel, egui::Rect)>, pixels_per_point: f32) -> Option<String> {
        let wanted_panel = wanted.map(|(p, _)| p);

        if self.active != wanted_panel {
            if let Some(prev) = self.active {
                if let Some(entry) = self.views.get_mut(&prev) {
                    if let Ok(v) = &entry.view {
                        let _ = v.set_visible(false);
                    }
                    entry.visible = false;
                }
            }
            self.active = wanted_panel;
        }

        let (panel, rect) = wanted?;
        let bounds = to_physical_bounds(rect, pixels_per_point);

        let entry = self.views.entry(panel).or_insert_with(|| ViewEntry {
            view: WebViewBuilder::new_as_child(&self.parent)
                .with_url(panel.url())
                .with_bounds(make_rect(bounds))
                .build()
                .map_err(|e| e.to_string()),
            bounds,
            visible: true,
        });

        match &entry.view {
            Ok(v) => {
                if entry.bounds != bounds {
                    let _ = v.set_bounds(make_rect(bounds));
                    entry.bounds = bounds;
                }
                if !entry.visible {
                    let _ = v.set_visible(true);
                    entry.visible = true;
                }
                None
            }
            Err(e) => Some(format!("[ERROR] Could not load {}: {}", panel.title(), e)),
        }
    }
}

fn to_physical_bounds(rect: egui::Rect, ppp: f32) -> (i32, i32, u32, u32) {
    (
        (rect.min.x * ppp).round() as i32,
        (rect.min.y * ppp).round() as i32,
        (rect.width()  * ppp).round().max(0.0) as u32,
        (rect.height() * ppp).round().max(0.0) as u32,
    )
}

fn make_rect((x, y, w, h): (i32, i32, u32, u32)) -> Rect {
    Rect {
        position: Position::Physical(PhysicalPosition::new(x, y)),
        size:     Size::Physical(PhysicalSize::new(w, h)),
    }
}
