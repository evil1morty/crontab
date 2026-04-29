use crate::config::{self, Config};
use crate::cron_parse;
use chrono::{DateTime, Local, Timelike};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

pub struct SharedState {
    pub config: Mutex<Config>,
    pub config_mtime: Mutex<Option<SystemTime>>,
    pub last_runs: Mutex<Vec<(String, DateTime<Local>, i32)>>, // (name, when, exit_code or -1)
}

impl SharedState {
    pub fn new(cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            config: Mutex::new(cfg),
            config_mtime: Mutex::new(config::mtime(&config::config_path())),
            last_runs: Mutex::new(Vec::new()),
        })
    }
}

pub fn spawn(state: Arc<SharedState>) {
    thread::spawn(move || run_loop(state));
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
            let saved = state.config_mtime.lock().unwrap();
            *saved != current_mtime
        };
        if need_reload {
            if let Ok(cfg) = config::load() {
                *state.config.lock().unwrap() = cfg;
                *state.config_mtime.lock().unwrap() = current_mtime;
            }
        }

        let cfg = state.config.lock().unwrap().clone();
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
                fire(job, &log_dir, state.clone());
            }
        }
    }
}

pub fn fire(job: &crate::config::Job, log_dir: &std::path::Path, state: Arc<SharedState>) {
    let log_path = log_dir.join(format!("{}.log", sanitize(&job.name)));
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(ref f) = log_file {
        use std::io::Write;
        let mut f = f;
        let _ = writeln!(f, "\n[{stamp}] === run: {} ===", job.command);
    }

    let cmd_str = job.command.clone();
    let name = job.name.clone();
    let log_clone = log_path.clone();
    thread::spawn(move || {
        let stdout = OpenOptions::new().create(true).append(true).open(&log_clone).ok();
        let stderr = OpenOptions::new().create(true).append(true).open(&log_clone).ok();

        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&cmd_str);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&cmd_str);
            c
        };

        if let Some(o) = stdout { cmd.stdout(Stdio::from(o)); }
        if let Some(e) = stderr { cmd.stderr(Stdio::from(e)); }

        // CREATE_NO_WINDOW = 0x08000000 — prevents flashing console on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        let exit_code = match cmd.spawn() {
            Ok(mut child) => match child.wait() {
                Ok(s) => s.code().unwrap_or(-1),
                Err(_) => -1,
            },
            Err(e) => {
                if let Ok(f) = OpenOptions::new().create(true).append(true).open(&log_clone) {
                    use std::io::Write;
                    let mut f = f;
                    let _ = writeln!(f, "[spawn error] {e}");
                }
                -1
            }
        };

        let mut runs = state.last_runs.lock().unwrap();
        runs.push((name, Local::now(), exit_code));
        // keep only last 50
        let len = runs.len();
        if len > 50 {
            runs.drain(0..len - 50);
        }
    });
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn sleep_to_next_minute() {
    let now = Local::now();
    let secs = now.second() as u64;
    let nanos = now.nanosecond() as u64;
    let wait_ms = (60 - secs) * 1000 - nanos / 1_000_000;
    thread::sleep(Duration::from_millis(wait_ms.max(100)));
}
