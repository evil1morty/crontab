//! TOML-backed config (jobs + global settings).
//!
//! Lives at `%APPDATA%\claude-cron\config\crontab.toml` on Windows. The
//! scheduler watches the file's mtime and hot-reloads when it changes, so
//! editing the TOML directly is supported and intentional.
//!
//! All new fields use `#[serde(default)]` so an older config keeps loading
//! after an app upgrade.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    pub schedule: String,
    pub command: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<u64>,
    #[serde(default)]
    pub runs_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_concurrent: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub run_on_startup: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn default_true() -> bool {
    true
}
fn default_max_history() -> usize {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_true")]
    pub master_enabled: bool,
    #[serde(default)]
    pub log_dir: String,
    #[serde(default = "default_max_history")]
    pub max_run_history: usize,
    /// User's "start with Windows" intent. The actual HKCU Run key lives
    /// outside our control (an MSI reinstall can wipe it, an upgrade can move
    /// the exe), so we treat this flag as the source of truth and re-assert
    /// the Run key from it on every launch.
    #[serde(default)]
    pub autostart: bool,
    #[serde(default, rename = "job")]
    pub jobs: Vec<Job>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            master_enabled: true,
            log_dir: default_log_dir().to_string_lossy().to_string(),
            max_run_history: default_max_history(),
            autostart: false,
            jobs: vec![],
        }
    }
}

pub fn config_path() -> PathBuf {
    if let Some(pd) = ProjectDirs::from("", "", "claude-cron") {
        let dir = pd.config_dir();
        let _ = fs::create_dir_all(dir);
        return dir.join("crontab.toml");
    }
    PathBuf::from("crontab.toml")
}

pub fn default_log_dir() -> PathBuf {
    if let Some(pd) = ProjectDirs::from("", "", "claude-cron") {
        let d = pd.data_local_dir().join("logs");
        let _ = fs::create_dir_all(&d);
        return d;
    }
    PathBuf::from("logs")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        let cfg = Config::default();
        save(&cfg)?;
        return Ok(cfg);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let cfg: Config = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let text = toml::to_string_pretty(cfg)?;
    fs::write(&path, text)?;
    Ok(())
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}
