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
            // Modest bump from the original 700 to fit more of the button
            // list without scrolling — kept well under typical laptop-scale
            // logical screen heights (e.g. 800 on a 1920x1200 @ 150% display)
            // so the window can never open taller than the screen itself.
            .with_inner_size([540.0, 740.0])
            .with_min_inner_size([520.0, 400.0])
            .with_title("Unreal DevTool"),
        ..Default::default()
    };
    eframe::run_native(
        "Unreal DevTool",
        options,
        Box::new(|cc| Ok(Box::new(app::DevToolApp::new(cc)) as Box<dyn eframe::App>)),
    )
}
