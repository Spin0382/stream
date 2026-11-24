//! Stream Goat desktop app.
//!
//! Rust owns the window, the lists, Live TV and the player; the JVM extension host
//! (CloudStream extensions) is a separate process this app talks to. If the host
//! is not running, everything except extensions still works.

use eframe::egui;

mod app;
mod panels;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 720.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title("Stream Goat"),
        ..Default::default()
    };

    eframe::run_native(
        "Stream Goat",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
