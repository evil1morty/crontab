#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod config;
mod cron_parse;
mod scheduler;

use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let start_hidden = args.iter().any(|a| a == "--hidden");

    let cfg = config::load().unwrap_or_default();
    let state = scheduler::SharedState::new(cfg);
    scheduler::spawn(state.clone());
    scheduler::fire_startup_jobs(state.clone());

    tauri::Builder::default()
        // Must be the FIRST plugin per the plugin docs: a second launch
        // forwards argv + cwd to the running instance and exits, so we
        // never end up with two scheduler threads or two tray icons.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_always_on_top(true);
                let _ = w.set_always_on_top(false);
                let _ = w.set_focus();
                let _ = w.emit("window-visibility", true);
            }
        }))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::list_jobs,
            commands::save_job,
            commands::delete_job,
            commands::toggle_job,
            commands::run_job_now,
            commands::list_runs,
            commands::set_master_enabled,
            commands::get_master_enabled,
            commands::is_autostart,
            commands::set_autostart,
            commands::open_logs_folder,
            commands::open_config_file,
            commands::config_path_str,
            commands::cron_validate,
            commands::cron_next,
            commands::cron_presets,
            commands::reset_runs_count,
            commands::get_max_run_history,
            commands::set_max_run_history,
            commands::view_job_log,
            commands::quit_app,
            commands::hide_window,
            commands::is_window_visible,
        ])
        .setup(move |app| {
            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let pause = MenuItemBuilder::with_id("pause", "Toggle pause all").build(app)?;
            let logs = MenuItemBuilder::with_id("logs", "Open logs folder").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[&show, &hide])
                .separator()
                .items(&[&pause, &logs])
                .separator()
                .item(&quit)
                .build()?;

            // Windows' SetForegroundWindow refuses to steal focus from the
            // currently active process. The reliable workaround is to flip
            // always_on_top on and back off — that bypasses the restriction
            // without leaving the window pinned. Used by both the tray "Show"
            // and the tray icon double-click.
            fn bring_to_front(w: &tauri::WebviewWindow) {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_always_on_top(true);
                let _ = w.set_always_on_top(false);
                let _ = w.set_focus();
            }

            // Register menu events at the App level rather than on the tray
            // builder. Both are documented as valid, but the app-level
            // handler is the more reliable one in Tauri 2.x — events from
            // tray menus fire here too, and we get a single place that owns
            // the routing.
            app.on_menu_event(|app, ev| match ev.id().as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        bring_to_front(&w);
                        let _ = w.emit("window-visibility", true);
                    }
                }
                "hide" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                        let _ = w.emit("window-visibility", false);
                    }
                }
                "pause" => {
                    let state = app.state::<Arc<scheduler::SharedState>>();
                    let snap = {
                        let mut cfg = state.config.lock().expect("state mutex poisoned");
                        cfg.master_enabled = !cfg.master_enabled;
                        cfg.clone()
                    };
                    let _ = config::save(&snap);
                    *state.config_mtime.lock().expect("state mutex poisoned") =
                        config::mtime(&config::config_path());
                    let _ = app.emit("config-changed", ());
                }
                "logs" => {
                    let state = app.state::<Arc<scheduler::SharedState>>();
                    let dir = {
                        let cfg = state.config.lock().expect("state mutex poisoned");
                        if cfg.log_dir.is_empty() {
                            config::default_log_dir()
                        } else {
                            std::path::PathBuf::from(&cfg.log_dir)
                        }
                    };
                    std::fs::create_dir_all(&dir).ok();
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            });

            let _tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("Window Crontab")
                .icon(app.default_window_icon().cloned().unwrap())
                .on_tray_icon_event(|tray, ev| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = ev {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            bring_to_front(&w);
                            let _ = w.emit("window-visibility", true);
                        }
                    }
                })
                .build(app)?;

            if let Some(w) = app.get_webview_window("main") {
                let w_clone = w.clone();
                w.on_window_event(move |ev| {
                    if let WindowEvent::CloseRequested { api, .. } = ev {
                        api.prevent_close();
                        let _ = w_clone.hide();
                        // Tell the webview to stop its 5s refresh timer.
                        // document.hidden isn't reliable on Windows when a
                        // Tauri window is hidden to tray, so we signal it
                        // explicitly. Without this the webview keeps doing
                        // IPC every 5s while invisible → idle CPU spike.
                        let _ = w_clone.emit("window-visibility", false);
                    }
                });
                // Window is created hidden (visible:false in tauri.conf.json)
                // so autostart with --hidden has nothing to race against. When
                // launched without --hidden we show it explicitly here.
                // Previously the window was created visible and then hidden in
                // this branch, which on Windows produced a half-initialized
                // handle: tray events fired but show()/set_focus() did nothing
                // until the process was killed and relaunched.
                if !start_hidden {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri error");
}
