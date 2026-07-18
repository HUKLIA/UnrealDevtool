use eframe::egui;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wry::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

/// Web pages that can be embedded as a child WebView2 control inside the app window.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebPanel {
    Miku3D,
    CookieClicker,
    SponderBird,
    /// The TACHYON packaging ad — plays a local mp4 once. Unlike the other
    /// panels its URL isn't a fixed remote string (see `ads::ad_video_url`).
    Ad,
}

impl WebPanel {
    pub fn url(self) -> &'static str {
        match self {
            WebPanel::Miku3D        => "https://huklia.github.io/MikuTest/",
            WebPanel::CookieClicker => "https://orteil.dashnet.org/cookieclicker/",
            WebPanel::SponderBird   => "https://nicktam1.github.io/SponderBirdNew/",
            WebPanel::Ad            => crate::ops::ads::ad_video_url().unwrap_or("about:blank"),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            WebPanel::Miku3D        => "3D Miku",
            WebPanel::CookieClicker => "Cookie Clicker",
            WebPanel::SponderBird   => "Sponder Bird",
            WebPanel::Ad            => "Advertisement",
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

/// Suppresses blocking JS dialogs. Some embedded pages (e.g. Unity WebGL's
/// error handler) call `alert()` on uncaught errors such as the Pointer Lock
/// cooldown SecurityError. A blocking native alert inside a child webview is
/// bad UX, so silence `alert`/`confirm`/`prompt` and let the page keep running.
const SUPPRESS_DIALOGS_SCRIPT: &str = "\
    window.alert = function(){}; \
    window.confirm = function(){ return true; }; \
    window.prompt = function(){ return null; };";

/// The 3D Miku page is a Unity WebGL build whose `#unity-container`/`canvas`
/// have a fixed pixel size (so it shows up tiny with scrollbars in our
/// embedded panel). This stretches the container and canvas to fill the
/// embedded view, resizes the canvas's backing resolution to match (so it
/// isn't blurry), and re-runs whenever the panel is resized. It also makes
/// the canvas focusable and focuses it on click, so the Pointer Lock API
/// (mouse-look) has a real user gesture + focused element to work with.
const MIKU3D_FIT_SCRIPT: &str = r#"
(function() {
  if (window.__mikuFitInstalled) return;
  window.__mikuFitInstalled = true;

  var style = document.createElement('style');
  style.textContent =
    'html,body{margin:0!important;padding:0!important;overflow:hidden!important;width:100%!important;height:100%!important;}' +
    '#unity-container,.unity-desktop,.unity-mobile,#gameContainer,#game{position:fixed!important;inset:0!important;width:100%!important;height:100%!important;transform:none!important;}' +
    'canvas{width:100%!important;height:100%!important;display:block!important;outline:none!important;}';
  document.documentElement.appendChild(style);

  function fitCanvas() {
    var canvas = document.querySelector('canvas');
    if (!canvas) return false;
    var rect = canvas.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return false;
    var dpr = window.devicePixelRatio || 1;
    var w = Math.round(rect.width * dpr);
    var h = Math.round(rect.height * dpr);
    if (Math.abs(canvas.width - w) > 2 || Math.abs(canvas.height - h) > 2) {
      canvas.width = w;
      canvas.height = h;
      window.dispatchEvent(new Event('resize'));
    }
    if (!canvas.__mikuClickBound) {
      canvas.__mikuClickBound = true;
      if (canvas.tabIndex < 0) canvas.tabIndex = 0;
      canvas.addEventListener('mousedown', function() { canvas.focus(); });
    }
    return true;
  }

  var tries = 0;
  var iv = setInterval(function() {
    if (fitCanvas() || ++tries > 40) clearInterval(iv);
  }, 250);
  window.addEventListener('resize', fitCanvas);
})();
"#;

/// A raw local media file navigated to directly (no HTML wrapper) gets a
/// Chromium-synthesized `<video>` element that doesn't exist yet when this
/// init script runs, so poll for it, then report the `ended` event back to
/// Rust via `window.ipc.postMessage` (auto-injected by wry on every page).
const AD_ENDED_SCRIPT: &str = r#"
(function() {
  function attach(v) {
    if (v.__adEndedBound) return;
    v.__adEndedBound = true;
    v.addEventListener('ended', function() { window.ipc.postMessage('ended'); });
  }
  var tries = 0;
  var iv = setInterval(function() {
    var v = document.querySelector('video');
    if (v) { attach(v); clearInterval(iv); }
    else if (++tries > 100) { clearInterval(iv); }
  }, 100);
})();
"#;

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
    parent:         ParentWindow,
    views:          HashMap<WebPanel, ViewEntry>,
    active:         Option<WebPanel>,
    web_context:    WebContext,
    // The Ad panel gets its own WebContext (own WebView2 user-data folder),
    // which forces its own browser process rather than sharing the one the
    // other three panels use. This matters because WebView2 only honors
    // `--autoplay-policy=no-user-gesture-required` (set via `with_autoplay`)
    // the first time a browser process is created for a given user-data
    // folder — if the Ad panel shared `web_context` and any of Miku3D /
    // CookieClicker / SponderBird had already been opened first, the shared
    // process would already be running without that flag, and the ad would
    // silently load paused, waiting for a user gesture that never comes.
    ad_web_context: WebContext,
    ad_ended:       Arc<AtomicBool>,
}

fn webview_data_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(appdata).join("UnrealDevtool").join("webview2");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn webview_ad_data_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(appdata).join("UnrealDevtool").join("webview2-ad");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl WebViewManager {
    pub fn new(window: RawWindowHandle, display: RawDisplayHandle) -> Self {
        // Persistent data dir so localStorage / cookies survive app restarts.
        // Cookie Clicker saves its game here instead of starting fresh each time.
        let web_context    = WebContext::new(webview_data_dir());
        let ad_web_context = WebContext::new(webview_ad_data_dir());
        Self {
            parent: ParentWindow { window, display },
            views: HashMap::new(),
            active: None,
            web_context,
            ad_web_context,
            ad_ended: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Call once per frame. `wanted` is the panel that should be visible right
    /// now (with its bounds in egui points), or `None` to hide everything.
    /// Returns an error message if the panel failed to load (e.g. WebView2
    /// runtime missing).
    pub fn update(&mut self, wanted: Option<(WebPanel, egui::Rect)>, pixels_per_point: f32) -> Option<String> {
        let wanted_panel = wanted.map(|(p, _)| p);

        if self.active != wanted_panel {
            if let Some(prev) = self.active
                && let Some(entry) = self.views.get_mut(&prev) {
                    if let Ok(v) = &entry.view {
                        let _ = v.set_visible(false);
                    }
                    entry.visible = false;
                }
            self.active = wanted_panel;
        }

        let (panel, rect) = wanted?;
        let bounds = to_physical_bounds(rect, pixels_per_point);

        // Lazily create the webview the first time this panel is requested.
        // Use an explicit `contains_key` + `insert` rather than `or_insert_with`
        // so we can mutably borrow `self.web_context` without conflicting with
        // the mutable borrow of `self.views`.
        if !self.views.contains_key(&panel) {
            let init_script = match panel {
                WebPanel::Miku3D => format!("{SUPPRESS_DIALOGS_SCRIPT}{MIKU3D_FIT_SCRIPT}"),
                WebPanel::Ad     => AD_ENDED_SCRIPT.to_string(),
                _                => SUPPRESS_DIALOGS_SCRIPT.to_string(),
            };
            let mut builder = WebViewBuilder::new_as_child(&self.parent)
                .with_url(panel.url())
                .with_bounds(make_rect(bounds))
                .with_initialization_script(&init_script);

            if panel == WebPanel::Ad {
                let ended = Arc::clone(&self.ad_ended);
                builder = builder.with_autoplay(true).with_ipc_handler(move |req: wry::http::Request<String>| {
                    if req.body() == "ended" {
                        ended.store(true, Ordering::Relaxed);
                    }
                });
            }

            let view = if panel == WebPanel::Ad {
                builder.with_web_context(&mut self.ad_web_context).build()
            } else {
                builder.with_web_context(&mut self.web_context).build()
            }.map_err(|e| e.to_string());
            self.views.insert(panel, ViewEntry { view, bounds, visible: false });
        }
        let entry = self.views.get_mut(&panel).unwrap();

        match &entry.view {
            Ok(v) => {
                if entry.bounds != bounds {
                    let _ = v.set_bounds(make_rect(bounds));
                    entry.bounds = bounds;
                }
                if !entry.visible {
                    let _ = v.set_visible(true);
                    // Give the embedded WebView2 control real OS keyboard
                    // focus so clicking inside it (e.g. for Pointer Lock /
                    // mouse-look in 3D Miku) works immediately. Only done on
                    // activation, not every frame — repeated MoveFocus calls
                    // would otherwise repeatedly blur the page.
                    let _ = v.focus();
                    entry.visible = true;
                }
                None
            }
            Err(e) => Some(format!("[ERROR] Could not load {}: {}", panel.title(), e)),
        }
    }

    /// Forces the Ad panel to re-navigate from scratch — fresh `<video>`
    /// element, autoplay fires again, `ended` listener re-attaches — so
    /// triggering TACHYON a second time in the same session replays the ad
    /// instead of showing an already-ended, frozen last frame. A no-op if
    /// the panel hasn't been created yet (its first-ever creation above
    /// already starts it at time zero, so no reload is needed then).
    pub fn restart_ad(&mut self) {
        self.ad_ended.store(false, Ordering::Relaxed);
        if let Some(entry) = self.views.get_mut(&WebPanel::Ad)
            && let Ok(v) = &entry.view {
                let _ = v.load_url(WebPanel::Ad.url());
            }
    }

    /// Edge-triggered: `true` exactly once, the first call after the ad's
    /// `ended` event fired since the last time this was checked.
    pub fn take_ad_ended(&mut self) -> bool {
        self.ad_ended.swap(false, Ordering::Relaxed)
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
