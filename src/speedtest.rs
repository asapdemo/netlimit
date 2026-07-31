//! Cloudflare HTTP speed test (same endpoints as speed.cloudflare.com).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
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

/// Which part(s) of the test to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestScope {
    #[default]
    Full,
    Latency,
    Download,
    Upload,
}

impl TestScope {
    pub fn label(self) -> &'static str {
        match self {
            TestScope::Full => "full",
            TestScope::Latency => "latency",
            TestScope::Download => "download",
            TestScope::Upload => "upload",
        }
    }
}

#[derive(Debug, Clone)]
pub enum SpeedTestEvent {
    Progress {
        phase: String,
        detail: String,
        /// 0.0 ..= 1.0 progress **within the current phase**
        phase_progress: f64,
    },
    /// One probe result for live / report graphs (Mbps or ms).
    Sample {
        kind: SampleKind,
        value: f64,
    },
    Finished {
        scope: TestScope,
        /// Filled only for phases that ran.
        download_mbps: Option<f64>,
        upload_mbps: Option<f64>,
        latency_ms: Option<f64>,
        jitter_ms: Option<f64>,
        /// Per-phase duration setting (not total wall time).
        duration_secs: u32,
    },
    /// User requested stop. Partial samples already sent remain valid.
    Cancelled,
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

/// Error used inside the worker to distinguish cancel from failure.
enum RunError {
    Cancelled,
    Failed(String),
}

impl From<String> for RunError {
    fn from(s: String) -> Self {
        RunError::Failed(s)
    }
}

/// Spawn Cloudflare speed test on a background thread.
///
/// Returns `(events, cancel)`. Set `cancel` to true to stop between probes
/// (in-flight HTTP requests finish first).
///
/// `duration_secs` is the wall-clock budget **for each phase** that runs
/// (latency, download, upload). A full test ≈ 3 × duration_secs.
pub fn start(duration_secs: u32, scope: TestScope) -> (Receiver<SpeedTestEvent>, Arc<AtomicBool>) {
    let duration_secs = duration_secs.clamp(1, 120);
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);
    thread::spawn(move || {
        match run_test(&tx, duration_secs, scope, &cancel_worker) {
            Ok(()) => {}
            Err(RunError::Cancelled) => {
                let _ = tx.send(SpeedTestEvent::Cancelled);
            }
            Err(RunError::Failed(e)) => {
                let _ = tx.send(SpeedTestEvent::Failed(e));
            }
        }
    });
    (rx, cancel)
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), RunError> {
    if is_cancelled(cancel) {
        Err(RunError::Cancelled)
    } else {
        Ok(())
    }
}

fn run_test(
    tx: &Sender<SpeedTestEvent>,
    duration_secs: u32,
    scope: TestScope,
    cancel: &AtomicBool,
) -> Result<(), RunError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(4)
        .build()
        .map_err(|e| RunError::Failed(format!("http client: {e}")))?;

    let phase_budget = Duration::from_secs(duration_secs as u64);

    let mut download_mbps = None;
    let mut upload_mbps = None;
    let mut latency_ms = None;
    let mut jitter_ms = None;

    // ── Latency ─────────────────────────────────────────────────────
    if matches!(scope, TestScope::Full | TestScope::Latency) {
        check_cancel(cancel)?;
        let (lat, jit) = run_latency_phase(tx, &client, phase_budget, cancel)?;
        latency_ms = Some(lat);
        jitter_ms = Some(jit);
    }

    // ── Download ────────────────────────────────────────────────────
    if matches!(scope, TestScope::Full | TestScope::Download) {
        check_cancel(cancel)?;
        download_mbps = Some(run_download_phase(tx, &client, phase_budget, cancel)?);
    }

    // ── Upload ──────────────────────────────────────────────────────
    if matches!(scope, TestScope::Full | TestScope::Upload) {
        check_cancel(cancel)?;
        upload_mbps = Some(run_upload_phase(tx, &client, phase_budget, cancel)?);
    }

    check_cancel(cancel)?;

    let _ = tx.send(SpeedTestEvent::Finished {
        scope,
        download_mbps,
        upload_mbps,
        latency_ms,
        jitter_ms,
        duration_secs,
    });
    Ok(())
}

fn run_latency_phase(
    tx: &Sender<SpeedTestEvent>,
    client: &Client,
    budget: Duration,
    cancel: &AtomicBool,
) -> Result<(f64, f64), RunError> {
    let _ = tx.send(SpeedTestEvent::Progress {
        phase: "latency".into(),
        detail: format!("measuring RTT for {}s…", budget.as_secs()),
        phase_progress: 0.0,
    });

    let mut rtts = Vec::new();
    let start = Instant::now();
    let deadline = start + budget;
    let mut i = 0u32;

    // Run for the full phase budget (at least 4 samples if possible).
    while Instant::now() < deadline || rtts.len() < 4 {
        check_cancel(cancel)?;
        if Instant::now() >= deadline && rtts.len() >= 4 {
            break;
        }
        // Safety cap so we never spin forever if clock skews
        if i >= 500 {
            break;
        }
        i += 1;
        let frac = (start.elapsed().as_secs_f64() / budget.as_secs_f64()).min(1.0);
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "latency".into(),
            detail: format!("latency #{i}"),
            phase_progress: frac,
        });
        match measure_latency_ms(client) {
            Ok(ms) => {
                rtts.push(ms);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Latency,
                    value: ms,
                });
            }
            Err(e) if rtts.is_empty() && i >= 5 => {
                return Err(RunError::Failed(format!("latency probe failed: {e}")));
            }
            Err(_) => {}
        }
        // Short sleep, but wake early if cancelled.
        for _ in 0..4 {
            check_cancel(cancel)?;
            thread::sleep(Duration::from_millis(5));
        }
    }
    if rtts.is_empty() {
        return Err(RunError::Failed(
            "could not measure latency to Cloudflare".into(),
        ));
    }

    let _ = tx.send(SpeedTestEvent::Progress {
        phase: "latency".into(),
        detail: format!("latency done · {} probes", rtts.len()),
        phase_progress: 1.0,
    });

    let mut sorted = rtts.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let latency_ms = percentile(&sorted, 0.5);
    let jitter_ms = if rtts.len() >= 2 {
        let mean = rtts.iter().sum::<f64>() / rtts.len() as f64;
        let var = rtts.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / rtts.len() as f64;
        var.sqrt()
    } else {
        0.0
    };
    Ok((latency_ms, jitter_ms))
}

fn run_download_phase(
    tx: &Sender<SpeedTestEvent>,
    client: &Client,
    budget: Duration,
    cancel: &AtomicBool,
) -> Result<f64, RunError> {
    let pool: &[u64] = &[
        256_000, 512_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000,
    ];
    let mut rates = Vec::new();
    let start = Instant::now();
    let deadline = start + budget;
    let mut step = 0usize;

    while Instant::now() < deadline || rates.len() < 3 {
        check_cancel(cancel)?;
        if Instant::now() >= deadline && rates.len() >= 3 {
            break;
        }
        if step > 200 {
            break;
        }
        let bytes = pick_payload(pool, step, rates.len());
        let label = format_bytes(bytes);
        let n = rates.len() + 1;
        let frac = (start.elapsed().as_secs_f64() / budget.as_secs_f64()).min(1.0);
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "download".into(),
            detail: format!("↓ #{n} {label}…"),
            phase_progress: frac,
        });
        match measure_download_mbps(client, bytes) {
            Ok(mbps) => {
                rates.push(mbps);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Download,
                    value: mbps,
                });
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "download".into(),
                    detail: format!("↓ #{n} {label} → {mbps:.1} Mbps"),
                    phase_progress: frac,
                });
            }
            Err(e) => {
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "download".into(),
                    detail: format!("↓ #{n} failed: {e}"),
                    phase_progress: frac,
                });
                if rates.is_empty() && step >= 4 {
                    return Err(RunError::Failed(format!("download probes failed: {e}")));
                }
            }
        }
        step += 1;
    }
    if rates.is_empty() {
        return Err(RunError::Failed("all download probes failed".into()));
    }
    let _ = tx.send(SpeedTestEvent::Progress {
        phase: "download".into(),
        detail: format!("download done · {} probes", rates.len()),
        phase_progress: 1.0,
    });
    Ok(max_f64(&rates))
}

fn run_upload_phase(
    tx: &Sender<SpeedTestEvent>,
    client: &Client,
    budget: Duration,
    cancel: &AtomicBool,
) -> Result<f64, RunError> {
    let pool: &[usize] = &[256_000, 512_000, 1_000_000, 2_000_000, 5_000_000];
    let mut rates = Vec::new();
    let start = Instant::now();
    let deadline = start + budget;
    let mut step = 0usize;

    while Instant::now() < deadline || rates.len() < 3 {
        check_cancel(cancel)?;
        if Instant::now() >= deadline && rates.len() >= 3 {
            break;
        }
        if step > 200 {
            break;
        }
        let bytes = pick_payload_usize(pool, step, rates.len());
        let label = format_bytes(bytes as u64);
        let n = rates.len() + 1;
        let frac = (start.elapsed().as_secs_f64() / budget.as_secs_f64()).min(1.0);
        let _ = tx.send(SpeedTestEvent::Progress {
            phase: "upload".into(),
            detail: format!("↑ #{n} {label}…"),
            phase_progress: frac,
        });
        match measure_upload_mbps(client, bytes) {
            Ok(mbps) => {
                rates.push(mbps);
                let _ = tx.send(SpeedTestEvent::Sample {
                    kind: SampleKind::Upload,
                    value: mbps,
                });
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "upload".into(),
                    detail: format!("↑ #{n} {label} → {mbps:.1} Mbps"),
                    phase_progress: frac,
                });
            }
            Err(e) => {
                let _ = tx.send(SpeedTestEvent::Progress {
                    phase: "upload".into(),
                    detail: format!("↑ #{n} failed: {e}"),
                    phase_progress: frac,
                });
                if rates.is_empty() && step >= 4 {
                    return Err(RunError::Failed(format!("upload probes failed: {e}")));
                }
            }
        }
        step += 1;
    }
    if rates.is_empty() {
        return Err(RunError::Failed("all upload probes failed".into()));
    }
    let _ = tx.send(SpeedTestEvent::Progress {
        phase: "upload".into(),
        detail: format!("upload done · {} probes", rates.len()),
        phase_progress: 1.0,
    });
    Ok(max_f64(&rates))
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        let mb = bytes as f64 / 1_000_000.0;
        if (mb - mb.round()).abs() < 0.05 {
            format!("{:.0} MB", mb)
        } else {
            format!("{mb:.1} MB")
        }
    } else {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    }
}

/// Prefer smaller sizes early (dense graph), larger later (better peak Mbps).
fn pick_payload(pool: &[u64], step: usize, have: usize) -> u64 {
    if pool.is_empty() {
        return 1_000_000;
    }
    if have < 4 {
        let half = (pool.len() / 2).max(1);
        pool[step % half]
    } else {
        let idx = (step % pool.len()).max(pool.len() / 3);
        pool[idx.min(pool.len() - 1)]
    }
}

fn pick_payload_usize(pool: &[usize], step: usize, have: usize) -> usize {
    if pool.is_empty() {
        return 1_000_000;
    }
    if have < 4 {
        let half = (pool.len() / 2).max(1);
        pool[step % half]
    } else {
        let idx = (step % pool.len()).max(pool.len() / 3);
        pool[idx.min(pool.len() - 1)]
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
