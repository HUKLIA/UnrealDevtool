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
            .with_inner_size([520.0, 560.0])
            .with_min_inner_size([520.0, 400.0])
            .with_title("Unreal DevTool"),
        ..Default::default()
    };
    eframe::run_native(
        "Unreal DevTool",
        options,
        Box::new(|cc| Box::new(app::DevToolApp::new(cc)) as Box<dyn eframe::App>),
    )
}
