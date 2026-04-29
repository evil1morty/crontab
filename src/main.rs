#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod config;
mod cron_parse;
mod scheduler;
mod tray;
mod ui;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load().unwrap_or_default();
    let state = scheduler::SharedState::new(cfg);
    scheduler::spawn(state.clone());

    let show_window = Arc::new(AtomicBool::new(false));
    let quit_flag = Arc::new(AtomicBool::new(false));

    // Build tray on main thread BEFORE eframe takes the event loop
    let tray = match tray::build() {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("tray init failed: {e}");
            None
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([720.0, 640.0])
            .with_min_inner_size([520.0, 480.0])
            .with_visible(false)
            .with_title("Claude Cron"),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let show_for_app = show_window.clone();
    let quit_for_app = quit_flag.clone();

    eframe::run_native(
        "Claude Cron",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(ui::App::new(
                state_for_app,
                show_for_app,
                quit_for_app,
                tray,
            )) as Box<dyn eframe::App>)
        }),
    )?;

    Ok(())
}
