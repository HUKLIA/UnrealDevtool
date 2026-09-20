use eframe::egui;
use crate::app::DevToolApp;
use crate::theme::*;
use crate::webview::WebPanel;

/// Where the browser opens on first use.
pub const HOME_URL: &str = "https://chatgpt.com/";

/// Shortcut targets. Web AI first, since that is what this page is for, then
/// the two Unreal references worth having a click away while packaging.
///
/// These are ordinary sites in an ordinary browser — the point is not to
/// integrate with them, just to save alt-tabbing out of the tool.
const SHORTCUTS: &[(&str, &str)] = &[
    ("ChatGPT",     "https://chatgpt.com/"),
    ("Claude",      "https://claude.ai/"),
    ("Gemini",      "https://gemini.google.com/app"),
    ("Copilot",     "https://copilot.microsoft.com/"),
    ("Perplexity",  "https://www.perplexity.ai/"),
    ("Grok",        "https://grok.com/"),
    ("UE Docs",     "https://dev.epicgames.com/documentation/en-us/unreal-engine"),
    ("UE Forums",   "https://forums.unrealengine.com/"),
];

impl DevToolApp {
    /// Built-in browser — a full-height page wrapping the embedded WebView2
    /// control, with a shortcut row for web AI, a URL bar and history nav.
    ///
    /// Sessions persist: the whole app shares one `WebContext` pointed at
    /// `%APPDATA%\UnrealDevtool\webview2\`, so signing in to ChatGPT or Claude
    /// once keeps you signed in across restarts, exactly like a normal browser
    /// profile.
    pub fn show_browser_tab(&mut self, ui: &mut egui::Ui) {
        // Collected during layout and applied after, since the closures below
        // borrow `self` while the webview manager also needs `&mut self`.
        let mut nav: Option<String> = None;
        let mut script: Option<&'static str> = None;

        card()
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                // ── History + address row ──────────────────────────────────
                ui.horizontal(|ui| {
                    // wry exposes no back/forward, but the page's own
                    // `window.history` does exactly the same thing.
                    // Painted icons, not text labels. "<", ">" and "R" were
                    // glyphs whose own side-bearings sat them off-centre in a
                    // square button, and "R" for reload is not a symbol anyone
                    // reads as reload.
                    let btn = egui::vec2(32.0, 30.0);
                    if icon_ghost(ui, Icon::ArrowLeft, btn)
                        .on_hover_text("Back").clicked() {
                            script = Some("history.back()");
                        }
                    if icon_ghost(ui, Icon::ArrowRight, btn)
                        .on_hover_text("Forward").clicked() {
                            script = Some("history.forward()");
                        }
                    if icon_ghost(ui, Icon::Reload, btn)
                        .on_hover_text("Reload").clicked() {
                            script = Some("location.reload()");
                        }
                    if icon_ghost(ui, Icon::Home, btn)
                        .on_hover_text(HOME_URL).clicked() {
                            nav = Some(HOME_URL.to_string());
                        }

                    ui.add_space(4.0);
                    let go_w  = 52.0;
                    let url_w = (ui.available_width() - go_w - ui.spacing().item_spacing.x).max(120.0);
                    let resp = ui.add_sized(
                        [url_w, 28.0],
                        egui::TextEdit::singleline(&mut self.browser_url_input)
                            .hint_text("Search, or paste a URL"),
                    );
                    let entered = resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add_sized([go_w, 28.0], primary("Go")).clicked() || entered {
                        nav = Some(normalize_url(&self.browser_url_input));
                    }
                });

                ui.add_space(8.0);

                // ── Shortcuts ──────────────────────────────────────────────
                // `horizontal_wrapped` so a narrow window stacks these onto a
                // second row instead of pushing the last ones off the edge.
                ui.horizontal_wrapped(|ui| {
                    for (name, url) in SHORTCUTS {
                        let active = self.browser_current == *url;
                        let btn = if active { primary(name) } else { ghost(name) };
                        if ui.add_sized([88.0, 26.0], btn).on_hover_text(*url).clicked() {
                            nav = Some((*url).to_string());
                        }
                    }
                });
            });

        ui.add_space(8.0);

        // ── Viewport ───────────────────────────────────────────────────────
        // Everything left over goes to the webview. The rect is handed to
        // `WebViewManager` at the end of the frame, which positions the native
        // child control over it — nothing egui paints here would be visible,
        // so the space is allocated rather than drawn.
        let avail = ui.available_size();
        if avail.y > 40.0 {
            let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
            // A backing fill behind the control, so the area reads as part of
            // the page for the frame or two before WebView2 paints.
            ui.painter().rect_filled(rect, egui::Rounding::same(R_CARD), DEEP);
            self.pending_webview = Some((WebPanel::Browser, rect));
        }

        if let Some(s) = script {
            self.webview_manager.eval_in_browser(s);
        }
        if let Some(url) = nav {
            self.browser_current   = url.clone();
            self.browser_url_input = url.clone();
            self.webview_manager.navigate(&url);
        }
    }
}

/// Turns whatever was typed into something navigable.
///
/// Anything that already names a scheme is passed through. A bare token with a
/// dot and no spaces is treated as a hostname; everything else is treated as a
/// search query, which is what a browser address bar does and what anyone
/// typing into one expects.
fn normalize_url(input: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return HOME_URL.to_string();
    }
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("file://") {
        return t.to_string();
    }
    if !t.contains(' ') && t.contains('.') {
        return format!("https://{t}");
    }
    format!(
        "https://duckduckgo.com/?q={}",
        t.replace('&', "%26").replace('#', "%23").replace(' ', "+")
    )
}
