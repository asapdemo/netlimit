//! Persistent speed-test history.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::presets::config_dir;
use crate::tc::Limits;

const MAX_ENTRIES: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub at: String,
    pub interface: String,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub duration_secs: u32,
    /// Limits that were active when the test finished (if any).
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

pub fn history_path() -> PathBuf {
    config_dir().join("speedtest_history.json")
}

pub fn load_history() -> Vec<HistoryEntry> {
    let Ok(data) = fs::read_to_string(history_path()) else {
        return Vec::new();
    };
    serde_json::from_str::<HistoryFile>(&data)
        .map(|f| f.entries)
        .unwrap_or_default()
}

pub fn push_history(entry: HistoryEntry) -> Result<(), String> {
    let mut entries = load_history();
    entries.insert(0, entry);
    entries.truncate(MAX_ENTRIES);
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let file = HistoryFile { entries };
    let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(history_path(), data).map_err(|e| format!("write history: {e}"))?;
    Ok(())
}

pub fn now_iso() -> String {
    // Local time without external crate: YYYY-MM-DD HH:MM:SS
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format via libc localtime if available — keep simple ISO-ish UTC offset free:
    format!("unix:{secs}")
}

/// Better human timestamp using `date` when available.
pub fn now_human() -> String {
    if let Ok(o) = std::process::Command::new("date")
        .args(["+%Y-%m-%d %H:%M:%S"])
        .output()
    {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout).trim().to_string();
        }
    }
    now_iso()
}
