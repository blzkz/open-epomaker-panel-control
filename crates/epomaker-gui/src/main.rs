//! Open control panel for EPOMAKER keyboards.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod editors;
mod hidmap;
mod i18n;
mod keyboard_view;
mod worker;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Open Epomaker Panel")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("open-epomaker-panel", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
