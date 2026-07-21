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

fn main() -> eframe::Result<()> {
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
            .with_title("Unreal DevTool"),
        ..Default::default()
    };
    eframe::run_native(
        "Unreal DevTool",
        options,
        Box::new(|cc| Ok(Box::new(app::DevToolApp::new(cc)) as Box<dyn eframe::App>)),
    )
}
