#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use noteon::{app::NoteonApp, config};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([820.0, 580.0])
            .with_title(config::WINDOW_TITLE)
            .with_decorations(false),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        config::APP_NAME,
        options,
        Box::new(|creation| Ok(Box::new(NoteonApp::new(creation)))),
    )
}
