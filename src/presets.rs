//! Built-in and user-saved quick presets.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub loss_percent: f64,
    /// Built-in presets cannot be deleted from the UI.
    #[serde(default)]
    pub builtin: bool,
}

impl Preset {
    pub fn new(name: impl Into<String>, download: f64, upload: f64, loss: f64) -> Self {
        Self {
            name: name.into(),
            download_mbps: download,
            upload_mbps: upload,
            loss_percent: loss,
            builtin: false,
        }
    }

    pub fn builtin(name: impl Into<String>, download: f64, upload: f64, loss: f64) -> Self {
        Self {
            name: name.into(),
            download_mbps: download,
            upload_mbps: upload,
            loss_percent: loss,
            builtin: true,
        }
    }

    pub fn short_label(&self) -> String {
        // Keep chip labels compact for the bar.
        if self.name.chars().count() <= 12 {
            self.name.clone()
        } else {
            format!("{}…", self.name.chars().take(11).collect::<String>())
        }
    }

    pub fn summary(&self) -> String {
        let dl = if self.download_mbps <= 0.0 {
            "∞".into()
        } else {
            format!("{}↓", trim_num(self.download_mbps))
        };
        let ul = if self.upload_mbps <= 0.0 {
            "∞".into()
        } else {
            format!("{}↑", trim_num(self.upload_mbps))
        };
        let loss = if self.loss_percent <= 0.0 {
            String::new()
        } else {
            format!(" {}%", trim_num(self.loss_percent))
        };
        format!("{dl}/{ul}{loss}")
    }
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < f64::EPSILON {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

/// Default factory presets shown for everyone.
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        Preset::builtin("Unlimited", 0.0, 0.0, 0.0),
        Preset::builtin("4G", 25.0, 10.0, 0.0),
        Preset::builtin("3G", 5.0, 2.0, 1.0),
        Preset::builtin("Slow", 2.0, 1.0, 0.0),
        Preset::builtin("Stream", 15.0, 5.0, 0.0),
        Preset::builtin("Flaky", 40.0, 10.0, 5.0),
        Preset::builtin("Harsh", 10.0, 2.0, 15.0),
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PresetFile {
    #[serde(default)]
    presets: Vec<Preset>,
}

pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("netlimit")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("netlimit")
    } else {
        PathBuf::from("/tmp/netlimit")
    }
}

pub fn presets_path() -> PathBuf {
    config_dir().join("presets.json")
}

/// Load user custom presets from disk (builtin flag forced false).
pub fn load_user_presets() -> Vec<Preset> {
    let path = presets_path();
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_str::<PresetFile>(&data) else {
        return Vec::new();
    };
    file.presets
        .into_iter()
        .map(|mut p| {
            p.builtin = false;
            p
        })
        .collect()
}

pub fn save_user_presets(presets: &[Preset]) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    let customs: Vec<Preset> = presets
        .iter()
        .filter(|p| !p.builtin)
        .cloned()
        .map(|mut p| {
            p.builtin = false;
            p
        })
        .collect();
    let file = PresetFile { presets: customs };
    let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(presets_path(), data).map_err(|e| format!("write presets: {e}"))?;
    Ok(())
}

/// Merge builtins + user customs (customs after builtins).
pub fn all_presets() -> Vec<Preset> {
    let mut all = builtin_presets();
    all.extend(load_user_presets());
    all
}

/// Practical max for the on-screen slider (buttons/keys can go higher).
pub fn slider_max(metric: crate::tc::Metric) -> f64 {
    match metric {
        crate::tc::Metric::Download | crate::tc::Metric::Upload => 200.0,
        crate::tc::Metric::Loss => 100.0,
    }
}

pub fn value_to_slider_ratio(metric: crate::tc::Metric, value: f64) -> f64 {
    let max = slider_max(metric);
    (value / max).clamp(0.0, 1.0)
}

pub fn slider_ratio_to_value(metric: crate::tc::Metric, ratio: f64) -> f64 {
    let ratio = ratio.clamp(0.0, 1.0);
    let max = slider_max(metric);
    let raw = ratio * max;
    match metric {
        crate::tc::Metric::Loss => (raw * 2.0).round() / 2.0, // 0.5 steps
        crate::tc::Metric::Download | crate::tc::Metric::Upload => raw.round(), // 1 Mbps
    }
}
