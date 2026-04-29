use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

pub struct TrayHandles {
    pub _tray: TrayIcon,
    pub show_id: tray_icon::menu::MenuId,
    pub hide_id: tray_icon::menu::MenuId,
    pub pause_id: tray_icon::menu::MenuId,
    pub logs_id: tray_icon::menu::MenuId,
    pub quit_id: tray_icon::menu::MenuId,
}

pub fn build() -> Result<TrayHandles> {
    let menu = Menu::new();

    let show = MenuItem::new("Show", true, None);
    let hide = MenuItem::new("Hide", true, None);
    let pause = MenuItem::new("Toggle pause all", true, None);
    let logs = MenuItem::new("Open logs folder", true, None);
    let quit = MenuItem::new("Quit", true, None);

    menu.append(&show)?;
    menu.append(&hide)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&pause)?;
    menu.append(&logs)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    let icon = make_icon();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Claude Cron")
        .with_icon(icon)
        .build()?;

    Ok(TrayHandles {
        _tray: tray,
        show_id: show.id().clone(),
        hide_id: hide.id().clone(),
        pause_id: pause.id().clone(),
        logs_id: logs.id().clone(),
        quit_id: quit.id().clone(),
    })
}

fn make_icon() -> tray_icon::Icon {
    // 16x16 simple green-on-dark clock-ish icon
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let dx = x as i32 - 8;
            let dy = y as i32 - 8;
            let d2 = dx * dx + dy * dy;
            if d2 <= 49 {
                // body
                rgba[i] = 30;
                rgba[i + 1] = 30;
                rgba[i + 2] = 40;
                rgba[i + 3] = 255;
                // hands: vertical (12 o'clock) + 3 o'clock
                if (dx == 0 && dy <= 0 && dy >= -5) || (dy == 0 && dx >= 0 && dx <= 4) {
                    rgba[i] = 80;
                    rgba[i + 1] = 220;
                    rgba[i + 2] = 120;
                }
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("icon")
}

pub fn poll(
    handles: &TrayHandles,
    show_window: &Arc<AtomicBool>,
    quit_flag: &Arc<AtomicBool>,
    state: &Arc<crate::scheduler::SharedState>,
) {
    while let Ok(ev) = MenuEvent::receiver().try_recv() {
        if ev.id == handles.show_id {
            show_window.store(true, Ordering::SeqCst);
        } else if ev.id == handles.hide_id {
            show_window.store(false, Ordering::SeqCst);
        } else if ev.id == handles.pause_id {
            let mut cfg = state.config.lock().unwrap();
            cfg.master_enabled = !cfg.master_enabled;
            let snapshot = cfg.clone();
            drop(cfg);
            let _ = crate::config::save(&snapshot);
            *state.config_mtime.lock().unwrap() = crate::config::mtime(&crate::config::config_path());
        } else if ev.id == handles.logs_id {
            let cfg = state.config.lock().unwrap();
            let dir = if cfg.log_dir.is_empty() {
                crate::config::default_log_dir()
            } else {
                std::path::PathBuf::from(&cfg.log_dir)
            };
            drop(cfg);
            std::fs::create_dir_all(&dir).ok();
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
        } else if ev.id == handles.quit_id {
            quit_flag.store(true, Ordering::SeqCst);
        }
    }
}
