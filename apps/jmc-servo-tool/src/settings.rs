//! Window settings, kept in `%LOCALAPPDATA%\JMC-Servo-Tool\gui.json`.

use std::path::{Path, PathBuf};

use jmc_core::link::{DEFAULT_BAUD, Framing};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub port: String,
    pub baud: u32,
    pub framing: Framing,
    pub slave: u8,
    pub max_read: u16,
    /// Use the simulated drive instead of a serial port.
    pub sim: bool,
    /// Snapshot or JMC export loaded into the simulated drive.
    pub sim_from: Option<PathBuf>,
    /// X, YL, YR or Z; empty until chosen.
    pub axis: String,
    pub baselines_dir: String,
    /// Empty means the default backup folder.
    pub backup_dir: String,
    pub monitor_ms: u64,
    pub window_s: f64,
    pub channels: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            port: std::env::var("JMC_PORT").unwrap_or_default(),
            baud: DEFAULT_BAUD,
            framing: Framing::Even1,
            slave: 1,
            max_read: jmc_core::link::DEFAULT_MAX_READ,
            sim: false,
            sim_from: None,
            axis: String::new(),
            baselines_dir: std::env::var("JMC_BASELINES").unwrap_or_default(),
            backup_dir: std::env::var("JMC_BACKUP_DIR").unwrap_or_default(),
            monitor_ms: 50,
            window_s: 10.0,
            channels: vec!["following_error".into(), "speed".into(), "torque".into()],
        }
    }
}

impl Settings {
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("JMC-Servo-Tool").join("gui.json"))
    }

    /// Missing or unreadable settings give the defaults.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    pub fn backup_dir(&self) -> PathBuf {
        if self.backup_dir.trim().is_empty() {
            jmc_core::guard::default_backup_dir()
        } else {
            PathBuf::from(self.backup_dir.trim())
        }
    }
}
