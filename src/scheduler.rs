//! Cron scheduler thread and per-run executor.
//!
//! One background thread (`run_loop`) wakes every minute, hot-reloads the
//! config if its mtime changed, and fires any enabled job whose schedule
//! matches the current minute. Each fire spawns its own short-lived thread
//! so a slow job can't hold up the next tick.
//!
//! Concurrency invariants:
//! - `running` tracks job names with a live child process. Cleared by the
//!   per-run thread on exit. The tick loop checks this set and records an
//!   `EXIT_SKIPPED` entry instead of double-firing (unless `allow_concurrent`).
//! - `last_runs` is bounded by `Config::max_run_history` and persisted to
//!   `history.json` after every push, so the UI Logs tab survives restart.
//! - `runs_count` is incremented atomically with the config save that may
//!   auto-disable the job when its `max_runs` cap is reached.
//! - Lock order (always): `config` → `running` → `last_runs`. Anywhere we
//!   need more than one, we acquire in that order to avoid deadlock.
//! - Global cap `MAX_CONCURRENT_RUNS` prevents misconfigured `* * * * *`
//!   schedules with slow commands from spawning unbounded threads.

use crate::config::{self, Config};
use crate::cron_parse;
use chrono::{DateTime, Local, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// Sentinel exit code emitted when a job is killed for exceeding its timeout.
pub const EXIT_TIMEOUT: i32 = -2;
/// Sentinel exit code emitted when a job is skipped because a previous run is still alive.
pub const EXIT_SKIPPED: i32 = -3;

/// Hard cap on simultaneously running children. A misconfigured `* * * * *`
/// with a slow command would otherwise spawn unbounded threads and child
/// processes; once we hit this we record `EXIT_SKIPPED` instead.
const MAX_CONCURRENT_RUNS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEntry {
    pub name: String,
    pub when: DateTime<Local>,
    pub exit_code: i32,
}

pub struct SharedState {
    pub config: Mutex<Config>,
    pub config_mtime: Mutex<Option<SystemTime>>,
    pub last_runs: Mutex<Vec<RunEntry>>,
    pub running: Mutex<HashSet<String>>,
}

impl SharedState {
    pub fn new(cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            config: Mutex::new(cfg),
            config_mtime: Mutex::new(config::mtime(&config::config_path())),
            last_runs: Mutex::new(load_history()),
            running: Mutex::new(HashSet::new()),
        })
    }
}

fn history_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "claude-cron")
        .map(|pd| pd.data_local_dir().join("history.json"))
}

fn load_history() -> Vec<RunEntry> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_history(runs: &[RunEntry]) {
    if let Some(path) = history_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Ok(text) = serde_json::to_string(runs) {
            let _ = fs::write(&path, text);
        }
    }
}

pub fn spawn(state: Arc<SharedState>) {
    thread::spawn(move || run_loop(state));
}

/// Fire any jobs marked `run_on_startup` once, right at app launch.
pub fn fire_startup_jobs(state: Arc<SharedState>) {
    let cfg = state
        .config
        .lock()
        .expect("scheduler mutex poisoned")
        .clone();
    if !cfg.master_enabled {
        return;
    }
    let log_dir = if cfg.log_dir.is_empty() {
        config::default_log_dir()
    } else {
        PathBuf::from(&cfg.log_dir)
    };
    fs::create_dir_all(&log_dir).ok();
    for job in cfg.jobs.iter().filter(|j| j.enabled && j.run_on_startup) {
        fire(job, &log_dir, state.clone());
    }
}

fn run_loop(state: Arc<SharedState>) {
    loop {
        sleep_to_next_minute();
        let now = Local::now();
        let minute_start = now
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(now);

        // Hot-reload if file changed
        let path = config::config_path();
        let current_mtime = config::mtime(&path);
        let need_reload = {
            let saved = state.config_mtime.lock().expect("scheduler mutex poisoned");
            *saved != current_mtime
        };
        if need_reload {
            if let Ok(cfg) = config::load() {
                *state.config.lock().expect("scheduler mutex poisoned") = cfg;
                *state.config_mtime.lock().expect("scheduler mutex poisoned") = current_mtime;
            }
        }

        let cfg = state
            .config
            .lock()
            .expect("scheduler mutex poisoned")
            .clone();
        if !cfg.master_enabled {
            continue;
        }

        let log_dir = if cfg.log_dir.is_empty() {
            config::default_log_dir()
        } else {
            PathBuf::from(&cfg.log_dir)
        };
        let _ = fs::create_dir_all(&log_dir);

        for job in cfg.jobs.iter().filter(|j| j.enabled) {
            if cron_parse::fired_in_minute(&job.schedule, minute_start) {
                // Skip if a previous run is still alive (unless allow_concurrent).
                if !job.allow_concurrent {
                    let running = state.running.lock().expect("scheduler mutex poisoned");
                    if running.contains(&job.name) {
                        drop(running);
                        record_run(&state, &job.name, EXIT_SKIPPED);
                        continue;
                    }
                }
                fire(job, &log_dir, state.clone());
            }
        }
    }
}

fn record_run(state: &Arc<SharedState>, name: &str, exit_code: i32) {
    let entry = RunEntry {
        name: name.to_string(),
        when: Local::now(),
        exit_code,
    };
    // Read cap under the config lock first, then move to last_runs so we
    // never nest config inside last_runs (lock-order: config → last_runs).
    let cap = state
        .config
        .lock()
        .expect("config mutex poisoned")
        .max_run_history
        .max(1);
    let snapshot = {
        let mut runs = state.last_runs.lock().expect("last_runs mutex poisoned");
        runs.push(entry);
        let len = runs.len();
        if len > cap {
            runs.drain(0..len - cap);
        }
        runs.clone()
    };
    save_history(&snapshot);
}

pub fn fire(job: &crate::config::Job, log_dir: &std::path::Path, state: Arc<SharedState>) {
    let log_path = log_dir.join(format!("{}.log", sanitize(&job.name)));
    let log_file = OpenOptions::new().create(true).append(true).open(&log_path);

    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(ref f) = log_file {
        use std::io::Write;
        let mut f = f;
        let _ = writeln!(f, "\n[{stamp}] === run: {} ===", job.command);
    }

    let cmd_str = job.command.clone();
    let name = job.name.clone();
    let log_clone = log_path.clone();
    let timeout_secs = job.timeout_secs;

    // Mark as running BEFORE we spawn the thread so the next-minute tick
    // sees it, even if scheduling was racy. Also enforce the global cap
    // here so the funnel is single-source.
    {
        let mut running = state.running.lock().expect("running mutex poisoned");
        if running.len() >= MAX_CONCURRENT_RUNS {
            drop(running);
            record_run(&state, &name, EXIT_SKIPPED);
            return;
        }
        running.insert(name.clone());
    }

    thread::spawn(move || {
        // Open the log file once and clone the handle for stderr so we don't
        // hold two distinct kernel file descriptors racing each other in
        // append mode.
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_clone)
            .ok();
        let stderr = stdout.as_ref().and_then(|f| f.try_clone().ok());

        #[cfg(windows)]
        let mut cmd = {
            // raw_arg lets cmd.exe's own /C parser handle the wrapping quotes.
            // Without this, std's arg() backslash-escapes inner quotes and they
            // leak into the child's argv.
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("cmd");
            c.raw_arg("/C").raw_arg(format!("\"{}\"", &cmd_str));
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&cmd_str);
            c
        };

        if let Some(o) = stdout {
            cmd.stdout(Stdio::from(o));
        }
        if let Some(e) = stderr {
            cmd.stderr(Stdio::from(e));
        }

        // CREATE_NO_WINDOW = 0x08000000 prevents the flashing console on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        let exit_code = match cmd.spawn() {
            Ok(mut child) => {
                let started = Instant::now();
                loop {
                    match child.try_wait() {
                        Ok(Some(s)) => break s.code().unwrap_or(-1),
                        Ok(None) => {
                            if let Some(timeout) = timeout_secs {
                                if started.elapsed() >= Duration::from_secs(timeout) {
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    if let Ok(f) = OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&log_clone)
                                    {
                                        use std::io::Write;
                                        let mut f = f;
                                        let _ = writeln!(f, "[killed, timed out after {timeout}s]");
                                    }
                                    break EXIT_TIMEOUT;
                                }
                            }
                            thread::sleep(Duration::from_millis(200));
                        }
                        Err(_) => break -1,
                    }
                }
            }
            Err(e) => {
                if let Ok(f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_clone)
                {
                    use std::io::Write;
                    let mut f = f;
                    let _ = writeln!(f, "[spawn error] {e}");
                }
                -1
            }
        };

        // Drop the running marker first so the next tick can fire even before
        // we finish persisting.
        state
            .running
            .lock()
            .expect("scheduler mutex poisoned")
            .remove(&name);

        record_run(&state, &name, exit_code);

        // Increment per-job runs_count and auto-disable if max_runs reached
        let snap = {
            let mut cfg = state.config.lock().expect("scheduler mutex poisoned");
            let mut changed = false;
            if let Some(j) = cfg.jobs.iter_mut().find(|j| j.name == name) {
                j.runs_count = j.runs_count.saturating_add(1);
                changed = true;
                if let Some(max) = j.max_runs {
                    if j.runs_count >= max {
                        j.enabled = false;
                    }
                }
            }
            if changed {
                Some(cfg.clone())
            } else {
                None
            }
        };
        if let Some(snap) = snap {
            if config::save(&snap).is_ok() {
                *state.config_mtime.lock().expect("scheduler mutex poisoned") =
                    config::mtime(&config::config_path());
            }
        }
    });
}

pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn sleep_to_next_minute() {
    let now = Local::now();
    let secs = now.second() as u64;
    let nanos = now.nanosecond() as u64;
    let wait_ms = (60 - secs) * 1000 - nanos / 1_000_000;
    thread::sleep(Duration::from_millis(wait_ms.max(100)));
}
