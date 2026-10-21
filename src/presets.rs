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
    #[serde(default)]
    pub delay_ms: f64,
    #[serde(default)]
    pub jitter_ms: f64,
    /// Built-in presets cannot be deleted from the UI.
    #[serde(default)]
    pub builtin: bool,
}

impl Preset {
    pub fn new(
        name: impl Into<String>,
        download: f64,
        upload: f64,
        loss: f64,
        delay_ms: f64,
        jitter_ms: f64,
    ) -> Self {
        Self {
            name: name.into(),
            download_mbps: download,
            upload_mbps: upload,
            loss_percent: loss,
            delay_ms,
            jitter_ms,
            builtin: false,
        }
    }

    pub fn builtin(
        name: impl Into<String>,
        download: f64,
        upload: f64,
        loss: f64,
        delay_ms: f64,
        jitter_ms: f64,
    ) -> Self {
        Self {
            name: name.into(),
            download_mbps: download,
            upload_mbps: upload,
            loss_percent: loss,
            delay_ms,
            jitter_ms,
            builtin: true,
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
        let mut s = format!("{dl}/{ul}");
        if self.loss_percent > 0.0 {
            s.push_str(&format!(" {}%", trim_num(self.loss_percent)));
        }
        if self.delay_ms > 0.0 || self.jitter_ms > 0.0 {
            s.push_str(&format!(
                " {}±{}ms",
                trim_num(self.delay_ms),
                trim_num(self.jitter_ms)
            ));
        }
        s
    }
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Default factory presets shown for everyone.
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        // No shaping
        Preset::builtin("No limits", 0.0, 0.0, 0.0, 0.0, 0.0),
        // Typical LTE-class mobile
        Preset::builtin("4G", 25.0, 10.0, 0.0, 40.0, 10.0),
        // Older mobile / congested cell
        Preset::builtin("3G", 5.0, 2.0, 1.0, 100.0, 30.0),
        // Starlink: solid throughput, high RTT + jitter
        Preset::builtin("Starlink", 100.0, 20.0, 0.5, 40.0, 15.0),
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
        crate::tc::Metric::Delay => 500.0,
        crate::tc::Metric::Jitter => 100.0,
    }
}

/// Left portion of a rate slider. Its first half is 0..=1 Mbps and snaps to
/// 0, 0.1, 0.25, 0.5, and 1 so each slow rate has its own slice. The rest of
/// this span is 1..=2 Mbps in 0.1 steps.
const RATE_LOW_FRAC: f64 = 0.30;
const RATE_LOW_END: f64 = 2.0;

fn rate_one_frac() -> f64 {
    RATE_LOW_FRAC * (1.0 / RATE_LOW_END)
}

/// `0` plus [`crate::tc::SLOW_RATE_STOPS`], which ends at 1 Mbps.
fn slow_slider_stops() -> [f64; 5] {
    let s = crate::tc::SLOW_RATE_STOPS;
    [0.0, s[0], s[1], s[2], s[3]]
}

fn slow_value_to_ratio(value: f64) -> f64 {
    let stops = slow_slider_stops();
    let one = rate_one_frac();
    let n = (stops.len() - 1) as f64;
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return one;
    }
    let mut i = 0;
    while i + 1 < stops.len() - 1 && value >= stops[i + 1] {
        i += 1;
    }
    let a = stops[i];
    let b = stops[i + 1];
    let u = ((value - a) / (b - a)).clamp(0.0, 1.0);
    let r0 = one * (i as f64) / n;
    let r1 = one * ((i + 1) as f64) / n;
    r0 + u * (r1 - r0)
}

fn slow_ratio_to_value(ratio: f64) -> f64 {
    let stops = slow_slider_stops();
    let one = rate_one_frac();
    let n = (stops.len() - 1) as f64;
    let t = (ratio / one).clamp(0.0, 1.0);
    let idx = (t * n).round() as usize;
    stops[idx.min(stops.len() - 1)]
}

pub fn value_to_slider_ratio(metric: crate::tc::Metric, value: f64) -> f64 {
    match metric {
        crate::tc::Metric::Download | crate::tc::Metric::Upload => {
            let max = slider_max(metric);
            if value <= 1.0 {
                slow_value_to_ratio(value)
            } else if value <= RATE_LOW_END {
                let t = ((value - 1.0) / (RATE_LOW_END - 1.0)).clamp(0.0, 1.0);
                rate_one_frac() + t * (RATE_LOW_FRAC - rate_one_frac())
            } else {
                let t = ((value - RATE_LOW_END) / (max - RATE_LOW_END)).clamp(0.0, 1.0);
                RATE_LOW_FRAC + t * (1.0 - RATE_LOW_FRAC)
            }
        }
        _ => {
            let max = slider_max(metric);
            (value / max).clamp(0.0, 1.0)
        }
    }
}

pub fn slider_ratio_to_value(metric: crate::tc::Metric, ratio: f64) -> f64 {
    let ratio = ratio.clamp(0.0, 1.0);
    match metric {
        crate::tc::Metric::Download | crate::tc::Metric::Upload => {
            let max = slider_max(metric);
            if ratio <= rate_one_frac() {
                slow_ratio_to_value(ratio)
            } else if ratio <= RATE_LOW_FRAC {
                let t = (ratio - rate_one_frac()) / (RATE_LOW_FRAC - rate_one_frac());
                let raw = 1.0 + t * (RATE_LOW_END - 1.0);
                (raw * 10.0).round() / 10.0
            } else {
                let t = (ratio - RATE_LOW_FRAC) / (1.0 - RATE_LOW_FRAC);
                (RATE_LOW_END + t * (max - RATE_LOW_END)).round()
            }
        }
        crate::tc::Metric::Loss => {
            let raw = ratio * slider_max(metric);
            (raw * 2.0).round() / 2.0
        }
        crate::tc::Metric::Delay | crate::tc::Metric::Jitter => {
            (ratio * slider_max(metric)).round()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tc::Metric;

    #[test]
    fn slider_reaches_slow_rates() {
        for rate in [0.0, 0.1, 0.25, 0.5, 1.0, 1.5, 10.0] {
            let ratio = value_to_slider_ratio(Metric::Download, rate);
            let back = slider_ratio_to_value(Metric::Download, ratio);
            assert!(
                (back - rate).abs() < 1e-9,
                "{rate} -> ratio {ratio} -> {back}"
            );
        }
        let quarter = value_to_slider_ratio(Metric::Upload, 0.25);
        let nudged = slider_ratio_to_value(Metric::Upload, quarter + 0.005);
        assert!(
            (nudged - 0.25).abs() < 1e-9,
            "0.25 slider slice collapsed to {nudged}"
        );
    }
}
