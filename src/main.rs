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
    let args: Vec<String> = std::env::args().collect();
    let start_hidden = args.iter().any(|a| a == "--hidden");

    let cfg = config::load().unwrap_or_default();
    let state = scheduler::SharedState::new(cfg);
    scheduler::spawn(state.clone());

    // Start visible unless --hidden was passed (autostart uses --hidden)
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
            .with_title("Claude Cron"),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let show_for_app = show_window.clone();
    let quit_for_app = quit_flag.clone();

    eframe::run_native(
        "Claude Cron",
        native_options,
        Box::new(move |cc| {
            // Spawn a heartbeat that requests a repaint every 250ms even while
            // the window is hidden, so update() keeps polling tray events.
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || loop {
                ctx.request_repaint();
                std::thread::sleep(std::time::Duration::from_millis(250));
            });

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
