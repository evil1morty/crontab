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
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_true")]
    pub master_enabled: bool,
    #[serde(default)]
    pub log_dir: String,
    #[serde(default, rename = "job")]
    pub jobs: Vec<Job>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            master_enabled: true,
            log_dir: default_log_dir().to_string_lossy().to_string(),
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
    let text = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let cfg: Config = toml::from_str(&text)
        .with_context(|| format!("parse {}", path.display()))?;
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
