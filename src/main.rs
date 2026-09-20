// In release builds, hide the console window so only the GUI appears.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod engine;
mod gif;
mod ops;
mod theme;
mod types;
mod ui;
mod webview;

use eframe::egui;

/// Records a panic to `crash.log` in the config folder before the default hook
/// runs. A GUI app with the console hidden otherwise dies without a trace, and
/// "it just closed" is not something anyone can act on.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = config::config_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let when = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let entry = format!(
                "[unix {when}] v{} — {info}
",
                env!("CARGO_PKG_VERSION"),
            );
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true).open(dir.join("crash.log"))
            {
                let _ = f.write_all(entry.as_bytes());
            }
        }
        default(info);
    }));
}

fn main() -> eframe::Result<()> {
    install_crash_log();
    // The Windows resource icon covers Explorer and shortcuts, while the
    // viewport icon is what eframe/winit uses for the live window, taskbar,
    // and task switcher. Keep both paths on the same head-only artwork so
    // the app never falls back to eframe's default letter mark.
    let app_icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../Image/app-icon-head.png"
    ))
    .expect("Image/app-icon-head.png must be a valid RGBA PNG");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Widened for the multi-column "bento grid" desktop layout
            // (matching the unreal-devtool/ reference mockup) — still kept
            // well under typical laptop-scale logical screen heights (e.g.
            // 800 on a 1920x1200 @ 150% display) so it can't open taller
            // than the screen itself. min_inner_size lets it shrink back
            // down to something closer to the old compact size if needed.
            .with_inner_size([1040.0, 760.0])
            .with_min_inner_size([620.0, 420.0])
            .with_icon(app_icon)
            .with_title("Unreal DevTool"),
        ..Default::default()
    };
    eframe::run_native(
        "Unreal DevTool",
        options,
        Box::new(|cc| Ok(Box::new(app::DevToolApp::new(cc)) as Box<dyn eframe::App>)),
    )
}
