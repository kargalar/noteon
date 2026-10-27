#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use eframe::egui;
use noteon::{app::NoteonApp, config};

fn load_app_icon() -> Option<Arc<egui::IconData>> {
    let icon_bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(icon_bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }))
}

fn main() -> eframe::Result {
    if std::env::args().any(|argument| argument == noteon::assistant::MCP_FLAG) {
        if let Err(error) = noteon::assistant::mcp::serve() {
            eprintln!("noteon mcp server failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 860.0])
        .with_min_inner_size([820.0, 580.0])
        .with_title(config::WINDOW_TITLE)
        .with_decorations(false);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        config::APP_NAME,
        options,
        Box::new(|creation| Ok(Box::new(NoteonApp::new(creation)))),
    )
}
