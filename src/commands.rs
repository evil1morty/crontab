use crate::{autostart, config, cron_parse, scheduler::SharedState};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct JobView {
    pub index: usize,
    pub name: String,
    pub schedule: String,
    pub command: String,
    pub enabled: bool,
    pub next_display: Option<String>,
    pub next_human: Option<String>,
    pub schedule_valid: bool,
}

#[derive(Deserialize)]
pub struct JobInput {
    pub name: String,
    pub schedule: String,
    pub command: String,
    pub enabled: Option<bool>,
}

#[tauri::command]
pub fn list_jobs(state: State<'_, Arc<SharedState>>) -> Vec<JobView> {
    let now = Local::now();
    let cfg = state.config.lock().unwrap();
    cfg.jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let next = cron_parse::next_run(&j.schedule, now);
            JobView {
                index: i,
                name: j.name.clone(),
                schedule: j.schedule.clone(),
                command: j.command.clone(),
                enabled: j.enabled,
                next_display: next.map(|n| n.format("%a %H:%M").to_string()),
                next_human: next.map(|n| cron_parse::human_until(n, now)),
                schedule_valid: cron_parse::validate(&j.schedule).is_ok(),
            }
        })
        .collect()
}

#[tauri::command]
pub fn save_job(
    app: AppHandle,
    state: State<'_, Arc<SharedState>>,
    index: Option<usize>,
    job: JobInput,
) -> Result<(), String> {
    cron_parse::validate(&job.schedule)?;
    if job.name.trim().is_empty() {
        return Err("name is required".into());
    }
    if job.command.trim().is_empty() {
        return Err("command is required".into());
    }
    let new_job = config::Job {
        name: job.name.trim().to_string(),
        schedule: job.schedule.trim().to_string(),
        command: job.command.trim().to_string(),
        enabled: job.enabled.unwrap_or(true),
    };
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        match index {
            Some(i) if i < cfg.jobs.len() => {
                let prev_enabled = cfg.jobs[i].enabled;
                cfg.jobs[i] = config::Job {
                    enabled: prev_enabled,
                    ..new_job
                };
            }
            _ => cfg.jobs.push(new_job),
        }
        cfg.clone()
    };
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    let _ = app.emit("jobs-changed", ());
    Ok(())
}

#[tauri::command]
pub fn delete_job(
    app: AppHandle,
    state: State<'_, Arc<SharedState>>,
    index: usize,
) -> Result<(), String> {
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        if index >= cfg.jobs.len() {
            return Err("index out of range".into());
        }
        cfg.jobs.remove(index);
        cfg.clone()
    };
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    let _ = app.emit("jobs-changed", ());
    Ok(())
}

#[tauri::command]
pub fn toggle_job(
    app: AppHandle,
    state: State<'_, Arc<SharedState>>,
    index: usize,
    enabled: bool,
) -> Result<(), String> {
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        if index >= cfg.jobs.len() {
            return Err("index out of range".into());
        }
        cfg.jobs[index].enabled = enabled;
        cfg.clone()
    };
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    let _ = app.emit("jobs-changed", ());
    Ok(())
}

#[tauri::command]
pub fn run_job_now(state: State<'_, Arc<SharedState>>, index: usize) -> Result<String, String> {
    let (job, log_dir) = {
        let cfg = state.config.lock().unwrap();
        let job = cfg
            .jobs
            .get(index)
            .cloned()
            .ok_or_else(|| "index out of range".to_string())?;
        let log_dir = if cfg.log_dir.is_empty() {
            config::default_log_dir()
        } else {
            std::path::PathBuf::from(&cfg.log_dir)
        };
        (job, log_dir)
    };
    std::fs::create_dir_all(&log_dir).ok();
    let name = job.name.clone();
    crate::scheduler::fire(&job, &log_dir, state.inner().clone());
    Ok(name)
}

#[derive(Serialize)]
pub struct RunView {
    pub name: String,
    pub when: String,
    pub exit_code: i32,
    pub ok: bool,
}

#[tauri::command]
pub fn list_runs(state: State<'_, Arc<SharedState>>) -> Vec<RunView> {
    let runs = state.last_runs.lock().unwrap();
    runs.iter()
        .rev()
        .map(|(name, when, code)| RunView {
            name: name.clone(),
            when: when.format("%Y-%m-%d %H:%M:%S").to_string(),
            exit_code: *code,
            ok: *code == 0,
        })
        .collect()
}

#[tauri::command]
pub fn set_master_enabled(
    app: AppHandle,
    state: State<'_, Arc<SharedState>>,
    enabled: bool,
) -> Result<(), String> {
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        cfg.master_enabled = enabled;
        cfg.clone()
    };
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    let _ = app.emit("config-changed", ());
    Ok(())
}

#[tauri::command]
pub fn get_master_enabled(state: State<'_, Arc<SharedState>>) -> bool {
    state.config.lock().unwrap().master_enabled
}

#[tauri::command]
pub fn is_autostart() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string();
    if enabled {
        autostart::enable(&exe).map_err(|e| e.to_string())
    } else {
        autostart::disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn open_logs_folder(state: State<'_, Arc<SharedState>>) {
    let dir = {
        let cfg = state.config.lock().unwrap();
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

#[tauri::command]
pub fn open_config_file() {
    let path = config::config_path();
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("notepad").arg(&path).spawn();
    }
}

#[tauri::command]
pub fn config_path_str() -> String {
    config::config_path().to_string_lossy().to_string()
}

#[tauri::command]
pub fn cron_validate(expr: String) -> Result<(), String> {
    cron_parse::validate(&expr)
}

#[derive(Serialize)]
pub struct NextRun {
    pub display: String,
    pub human: String,
}

#[tauri::command]
pub fn cron_next(expr: String) -> Result<NextRun, String> {
    cron_parse::validate(&expr)?;
    let now = Local::now();
    let next = cron_parse::next_run(&expr, now).ok_or_else(|| "no upcoming runs".to_string())?;
    Ok(NextRun {
        display: next.format("%a %Y-%m-%d %H:%M").to_string(),
        human: cron_parse::human_until(next, now),
    })
}

#[derive(Serialize)]
pub struct Preset {
    pub label: &'static str,
    pub expr: &'static str,
}

#[tauri::command]
pub fn cron_presets() -> Vec<Preset> {
    cron_parse::PRESETS
        .iter()
        .map(|(label, expr)| Preset { label, expr })
        .collect()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}
