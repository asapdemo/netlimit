//! Cloudflare HTTP speed test (same endpoints as speed.cloudflare.com).

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::{Body, Client};
use reqwest::header::{CONTENT_TYPE, USER_AGENT};

const DOWN_URL: &str = "https://speed.cloudflare.com/__down";
const UP_URL: &str = "https://speed.cloudflare.com/__up";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKind {
    Download,
    Upload,
    Latency,
}

#[derive(Debug, Clone)]
pub enum SpeedTestEvent {
    Progress {
        phase: String,
        detail: String,
        /// 0.0 ..= 1.0 overall progress estimate
        progress: f64,
    },
    /// One probe result for live / report graphs (Mbps or ms).
    Sample {
        kind: SampleKind,
        value: f64,
    },
    Finished {
        download_mbps: f64,
        upload_mbps: f64,
        latency_ms: f64,
        jitter_ms: f64,
        duration_secs: u32,
    },
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct SpeedTestResult {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub duration_secs: u32,
    pub down_samples: Vec<f64>,
    pub up_samples: Vec<f64>,
    pub lat_samples: Vec<f64>,
    #[allow(dead_code)]
    pub at: Option<Instant>,
}

/// Spawn Cloudflare speed test on a background thread.
/// `duration_secs` targets overall wall time (clamped 5..=120).
pub fn start(duration_secs: u32) -> Receiver<SpeedTestEvent> {
    let duration_secs = duration_secs.clamp(5, 120);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Err(e) = run_test(&tx, duration_secs) {
            let _ = tx.send(SpeedTestEvent::Failed(e));
        }
    });
    rx
}

fn run_test(tx: &Sender<SpeedTestEvent>, duration_secs: u32) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(4)
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let started = Instant::now();
    let budget = Duration::from_secs(duration_secs as u64);

    // Budget split: ~20% latency, ~50% download, ~30% upload
    let lat_budget = budget.mul_f64(0.20);
    let down_budget = budget.mul_f64(0.50);
    let up_budget = budget.mul_f64(0.30);

    // ── Latency / jitter ────────────────────────────────────────────
    let _ = tx.send(SpeedTestEvent::Progress {
        phase: "latency".into(),
        detail: "measuring RTT to Cloudflare…".into(),
        progress: 0.02,
    });

    let mut rtts = Vec::new();
    let lat_deadline = Instant::now() + lat_budget;
    let mut i = 0u32;
    while Instant::now() < lat_deadline || rtts.len() < 4 {
        i += 1;
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "latency".into(),
            detail: format!("latency sample #{i}"),
            progress: 0.05 + 0.15 * (i as f64 / 12.0).min(1.0),
        });
        match measure_latency_ms(&client) {
            Ok(ms) => {
                rtts.push(ms);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Latency,
                    value: ms,
                });
            }
            Err(e) if rtts.is_empty() && i >= 3 => {
                return Err(format!("latency probe failed: {e}"));
            }
            Err(_) => {}
        }
        if i >= 20 {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    if rtts.is_empty() {
        return Err("could not measure latency to Cloudflare".into());
    }
    rtts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let latency_ms = percentile(&rtts, 0.5);
    let jitter_ms = if rtts.len() >= 2 {
        let mean = rtts.iter().sum::<f64>() / rtts.len() as f64;
        let var = rtts.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / rtts.len() as f64;
        var.sqrt()
    } else {
        0.0
    };

    // ── Download ────────────────────────────────────────────────────
    let down_sizes: &[u64] = match duration_secs {
        0..=9 => &[500_000, 1_000_000, 5_000_000],
        10..=19 => &[1_000_000, 5_000_000, 10_000_000, 25_000_000],
        20..=39 => &[1_000_000, 5_000_000, 10_000_000, 25_000_000, 50_000_000],
        _ => &[
            1_000_000,
            5_000_000,
            10_000_000,
            25_000_000,
            50_000_000,
            100_000_000,
        ],
    };

    let mut down_rates = Vec::new();
    let down_deadline = Instant::now() + down_budget;
    for (idx, &bytes) in down_sizes.iter().enumerate() {
        if Instant::now() >= down_deadline && !down_rates.is_empty() {
            break;
        }
        let label = format_bytes(bytes);
        let prog = 0.20 + 0.50 * ((idx + 1) as f64 / down_sizes.len() as f64);
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "download".into(),
            detail: format!("downloading {label}…"),
            progress: prog,
        });
        match measure_download_mbps(&client, bytes) {
            Ok(mbps) => {
                down_rates.push(mbps);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Download,
                    value: mbps,
                });
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "download".into(),
                    detail: format!("{label} → {mbps:.1} Mbps"),
                    progress: prog,
                });
            }
            Err(e) => {
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "download".into(),
                    detail: format!("{label} failed: {e}"),
                    progress: prog,
                });
            }
        }
    }
    if down_rates.is_empty() {
        return Err("all download probes failed".into());
    }
    let download_mbps = max_f64(&down_rates);

    // ── Upload ──────────────────────────────────────────────────────
    let up_sizes: &[usize] = match duration_secs {
        0..=9 => &[500_000, 1_000_000],
        10..=19 => &[1_000_000, 5_000_000, 10_000_000],
        20..=39 => &[1_000_000, 5_000_000, 10_000_000, 25_000_000],
        _ => &[1_000_000, 5_000_000, 10_000_000, 25_000_000, 50_000_000],
    };

    let mut up_rates = Vec::new();
    let up_deadline = Instant::now() + up_budget;
    for (idx, &bytes) in up_sizes.iter().enumerate() {
        if Instant::now() >= up_deadline && !up_rates.is_empty() {
            break;
        }
        let label = format_bytes(bytes as u64);
        let prog = 0.70 + 0.28 * ((idx + 1) as f64 / up_sizes.len() as f64);
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "upload".into(),
            detail: format!("uploading {label}…"),
            progress: prog,
        });
        match measure_upload_mbps(&client, bytes) {
            Ok(mbps) => {
                up_rates.push(mbps);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Upload,
                    value: mbps,
                });
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "upload".into(),
                    detail: format!("{label} → {mbps:.1} Mbps"),
                    progress: prog,
                });
            }
            Err(e) => {
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "upload".into(),
                    detail: format!("{label} failed: {e}"),
                    progress: prog,
                });
            }
        }
    }
    if up_rates.is_empty() {
        return Err("all upload probes failed".into());
    }
    let upload_mbps = max_f64(&up_rates);

    let _ = tx.send(SpeedTestEvent::Finished {
        download_mbps,
        upload_mbps,
        latency_ms,
        jitter_ms,
        duration_secs,
    });

    let _ = started; // wall clock available if we want later
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.0} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    }
}

fn max_f64(v: &[f64]) -> f64 {
    v.iter()
        .copied()
        .fold(0.0_f64, |a, b| if b > a { b } else { a })
}

fn measure_latency_ms(client: &Client) -> Result<f64, String> {
    let url = format!("{DOWN_URL}?bytes=0");
    let start = Instant::now();
    let resp = client
        .get(&url)
        .header(USER_AGENT, "netlimit/0.1")
        .send()
        .map_err(|e| e.to_string())?;
    let _ = resp.bytes().map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn measure_download_mbps(client: &Client, bytes: u64) -> Result<f64, String> {
    let url = format!("{DOWN_URL}?bytes={bytes}");
    let start = Instant::now();
    let resp = client
        .get(&url)
        .header(USER_AGENT, "netlimit/0.1")
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body = resp.bytes().map_err(|e| e.to_string())?;
    let elapsed = start.elapsed().as_secs_f64().max(1e-6);
    let got = body.len() as f64;
    Ok((got * 8.0) / elapsed / 1_000_000.0)
}

fn measure_upload_mbps(client: &Client, bytes: usize) -> Result<f64, String> {
    let payload = vec![0u8; bytes];
    let len = payload.len() as f64;
    let start = Instant::now();
    let resp = client
        .post(UP_URL)
        .header(USER_AGENT, "netlimit/0.1")
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(payload))
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let _ = resp.bytes().map_err(|e| e.to_string())?;
    let elapsed = start.elapsed().as_secs_f64().max(1e-6);
    Ok((len * 8.0) / elapsed / 1_000_000.0)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
