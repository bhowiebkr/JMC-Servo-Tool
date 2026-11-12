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
    eframe::run_native(
        "JMC Servo Tool",
        native_options(),
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc.egui_ctx.clone(), settings::Settings::default_path())))
        }),
    )
}

fn native_options() -> eframe::NativeOptions {
    #[cfg_attr(not(windows), expect(unused_mut))]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(concat!("JMC Servo Tool ", env!("CARGO_PKG_VERSION")))
            // Start maximized: the tables want the room, and 1360x840 runs off a 1080p screen at 125%.
            .with_maximized(true)
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    // On Windows, use DX12. With every backend allowed, wgpu can pick Vulkan, and some Intel Vulkan
    // drivers crash at start (UHD 620, issue 1). WGPU_BACKEND still selects another backend.
    #[cfg(windows)]
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        use eframe::egui_wgpu::wgpu::Backends;
        setup.instance_descriptor.backends = Backends::from_env().unwrap_or(Backends::DX12);
    }
    options
}
