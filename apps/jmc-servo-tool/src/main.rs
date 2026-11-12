//! JMC Servo Tool: the window. Reads and writes JMC JASD/JAND model 512 drives and plots live values.
//! The drive work lives in `jmc-core`; this crate is only the window around it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod drawing;
mod live;
mod markdown;
mod more;
mod plot;
mod settings;
mod side;
mod table;
mod theme;
mod worker;

#[cfg(test)]
mod shots;
#[cfg(test)]
mod tests;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("JMC Servo Tool")
            // Start maximized: the tables want the room, and 1360x840 runs off a 1080p screen at 125%.
            .with_maximized(true)
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "JMC Servo Tool",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc.egui_ctx.clone(), settings::Settings::default_path())))
        }),
    )
}
