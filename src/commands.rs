use crate::{
    autostart, config, cron_parse,
    scheduler::{self, SharedState},
};
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
    pub max_runs: Option<u64>,
    pub runs_count: u64,
    pub exhausted: bool,
    pub timeout_secs: Option<u64>,
    pub allow_concurrent: bool,
    pub run_on_startup: bool,
    pub is_running: bool,
}

#[derive(Deserialize)]
pub struct JobInput {
    pub name: String,
    pub schedule: String,
    pub command: String,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_runs: Option<u64>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub allow_concurrent: bool,
    #[serde(default)]
    pub run_on_startup: bool,
}

#[tauri::command]
pub fn list_jobs(state: State<'_, Arc<SharedState>>) -> Vec<JobView> {
    let now = Local::now();
    let cfg = state.config.lock().unwrap();
    let running = state.running.lock().unwrap();
    cfg.jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let next = cron_parse::next_run(&j.schedule, now);
            let exhausted = j.max_runs.is_some_and(|m| j.runs_count >= m);
            JobView {
                index: i,
                name: j.name.clone(),
                schedule: j.schedule.clone(),
                command: j.command.clone(),
                enabled: j.enabled,
                next_display: next.map(|n| n.format("%a %H:%M").to_string()),
                next_human: next.map(|n| cron_parse::human_until(n, now)),
                schedule_valid: cron_parse::validate(&j.schedule).is_ok(),
                max_runs: j.max_runs,
                runs_count: j.runs_count,
                exhausted,
                timeout_secs: j.timeout_secs,
                allow_concurrent: j.allow_concurrent,
                run_on_startup: j.run_on_startup,
                is_running: running.contains(&j.name),
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
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        match index {
            Some(i) if i < cfg.jobs.len() => {
                // preserve enabled + runs_count, but reset count if max_runs changes
                let prev = cfg.jobs[i].clone();
                let count = if prev.max_runs != job.max_runs {
                    0
                } else {
                    prev.runs_count
                };
                cfg.jobs[i] = config::Job {
                    name: job.name.trim().to_string(),
                    schedule: job.schedule.trim().to_string(),
                    command: job.command.trim().to_string(),
                    enabled: prev.enabled,
                    max_runs: job.max_runs,
                    runs_count: count,
                    timeout_secs: job.timeout_secs,
                    allow_concurrent: job.allow_concurrent,
                    run_on_startup: job.run_on_startup,
                };
            }
            _ => cfg.jobs.push(config::Job {
                name: job.name.trim().to_string(),
                schedule: job.schedule.trim().to_string(),
                command: job.command.trim().to_string(),
                enabled: job.enabled.unwrap_or(true),
                max_runs: job.max_runs,
                runs_count: 0,
                timeout_secs: job.timeout_secs,
                allow_concurrent: job.allow_concurrent,
                run_on_startup: job.run_on_startup,
            }),
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
    pub status: &'static str,
}

fn status_for(code: i32) -> &'static str {
    match code {
        0 => "ok",
        scheduler::EXIT_TIMEOUT => "timeout",
        scheduler::EXIT_SKIPPED => "skipped",
        _ => "fail",
    }
}

#[tauri::command]
pub fn list_runs(state: State<'_, Arc<SharedState>>) -> Vec<RunView> {
    let runs = state.last_runs.lock().unwrap();
    runs.iter()
        .rev()
        .map(|e| RunView {
            name: e.name.clone(),
            when: e.when.format("%Y-%m-%d %H:%M:%S").to_string(),
            exit_code: e.exit_code,
            status: status_for(e.exit_code),
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
pub fn reset_runs_count(
    app: AppHandle,
    state: State<'_, Arc<SharedState>>,
    index: usize,
) -> Result<(), String> {
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        if index >= cfg.jobs.len() {
            return Err("index out of range".into());
        }
        cfg.jobs[index].runs_count = 0;
        cfg.clone()
    };
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    let _ = app.emit("jobs-changed", ());
    Ok(())
}

#[tauri::command]
pub fn get_max_run_history(state: State<'_, Arc<SharedState>>) -> usize {
    state.config.lock().unwrap().max_run_history
}

#[tauri::command]
pub fn set_max_run_history(state: State<'_, Arc<SharedState>>, value: usize) -> Result<(), String> {
    if value == 0 {
        return Err("must be at least 1".into());
    }
    let snap = {
        let mut cfg = state.config.lock().unwrap();
        cfg.max_run_history = value;
        cfg.clone()
    };
    // Trim in-memory list immediately
    {
        let mut runs = state.last_runs.lock().unwrap();
        let len = runs.len();
        if len > value {
            runs.drain(0..len - value);
        }
    }
    config::save(&snap).map_err(|e| e.to_string())?;
    *state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
    Ok(())
}

#[tauri::command]
pub fn view_job_log(state: State<'_, Arc<SharedState>>, index: usize) -> Result<(), String> {
    let (job_name, log_dir) = {
        let cfg = state.config.lock().unwrap();
        let job = cfg
            .jobs
            .get(index)
            .ok_or_else(|| "index out of range".to_string())?;
        let log_dir = if cfg.log_dir.is_empty() {
            config::default_log_dir()
        } else {
            std::path::PathBuf::from(&cfg.log_dir)
        };
        (job.name.clone(), log_dir)
    };
    let log_path = log_dir.join(format!("{}.log", scheduler::sanitize(&job_name)));
    if !log_path.exists() {
        return Err("no log yet, this job has not run".into());
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("notepad").arg(&log_path).spawn();
    }
    Ok(())
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
