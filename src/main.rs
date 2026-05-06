#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod config;
mod cron_parse;
mod scheduler;
mod theme;
mod tray;
mod ui;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let start_hidden = args.iter().any(|a| a == "--hidden");

    let cfg = config::load().unwrap_or_default();
    let state = scheduler::SharedState::new(cfg);
    scheduler::spawn(state.clone());

    let show_window = Arc::new(AtomicBool::new(!start_hidden));
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
            .with_title("Crontab")
            .with_visible(!start_hidden),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let show_for_app = show_window.clone();
    let quit_for_app = quit_flag.clone();

    eframe::run_native(
        "Crontab",
        native_options,
        Box::new(move |cc| {
            // Wire tray menu events to wake egui directly. This replaces the
            // old 250 ms repaint heartbeat that was burning ~20% CPU at idle.
            if let Some(t) = &tray {
                tray::install_handler(
                    t,
                    cc.egui_ctx.clone(),
                    show_for_app.clone(),
                    quit_for_app.clone(),
                    state_for_app.clone(),
                );
            }

            Ok(Box::new(ui::App::new(
                state_for_app,
                show_for_app,
                quit_for_app,
                tray,
                start_hidden,
            )) as Box<dyn eframe::App>)
        }),
    )?;

    Ok(())
}
