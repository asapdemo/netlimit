//! Live interface throughput and ICMP path-quality sampling.

use std::collections::VecDeque;
use std::fs;
use std::process::Command;
use std::time::Instant;

/// Number of samples kept for sparklines / charts.
pub const HISTORY_LEN: usize = 60;

#[derive(Debug, Clone, Default)]
pub struct SampleHistory {
    pub down_mbps: VecDeque<u64>,
    pub up_mbps: VecDeque<u64>,
    /// Packet loss percent × 10 (so 0.5% → 5) for sparkline resolution.
    pub loss_x10: VecDeque<u64>,
}

impl SampleHistory {
    pub fn push(&mut self, down: f64, up: f64, loss_pct: f64) {
        push_capped(&mut self.down_mbps, rate_to_spark(down));
        push_capped(&mut self.up_mbps, rate_to_spark(up));
        push_capped(
            &mut self.loss_x10,
            (loss_pct.clamp(0.0, 100.0) * 10.0).round() as u64,
        );
    }

    pub fn down_slice(&self) -> Vec<u64> {
        self.down_mbps.iter().copied().collect()
    }

    pub fn up_slice(&self) -> Vec<u64> {
        self.up_mbps.iter().copied().collect()
    }

    pub fn loss_slice(&self) -> Vec<u64> {
        self.loss_x10.iter().copied().collect()
    }
}

fn push_capped(q: &mut VecDeque<u64>, v: u64) {
    q.push_back(v);
    while q.len() > HISTORY_LEN {
        q.pop_front();
    }
}

/// Map Mbps to sparkline units (kbps, capped) so low rates stay visible.
fn rate_to_spark(mbps: f64) -> u64 {
    if mbps <= 0.0 {
        return 0;
    }
    // store as kbps, cap at ~10 Gbit for display scale
    (mbps * 1000.0).clamp(0.0, 10_000_000.0).round() as u64
}

pub fn spark_to_mbps(v: u64) -> f64 {
    v as f64 / 1000.0
}

#[derive(Debug, Clone)]
struct Counters {
    rx: u64,
    tx: u64,
    at: Instant,
}

/// Tracks interface byte counters and computes instantaneous rates.
#[derive(Debug, Default)]
pub struct ThroughputMonitor {
    last: Option<Counters>,
    pub down_mbps: f64,
    pub up_mbps: f64,
}

impl ThroughputMonitor {
    pub fn tick(&mut self, iface: &str) {
        let Some((rx, tx)) = read_iface_bytes(iface) else {
            return;
        };
        let now = Instant::now();
        if let Some(prev) = &self.last {
            let dt = now.duration_since(prev.at).as_secs_f64();
            if dt > 0.05 {
                let drx = rx.saturating_sub(prev.rx) as f64;
                let dtx = tx.saturating_sub(prev.tx) as f64;
                // bytes/s → Mbps
                self.down_mbps = (drx * 8.0) / dt / 1_000_000.0;
                self.up_mbps = (dtx * 8.0) / dt / 1_000_000.0;
            }
        }
        self.last = Some(Counters { rx, tx, at: now });
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.down_mbps = 0.0;
        self.up_mbps = 0.0;
    }
}

/// Read rx/tx byte counters for `iface` from `/proc/net/dev`.
pub fn read_iface_bytes(iface: &str) -> Option<(u64, u64)> {
    let data = fs::read_to_string("/proc/net/dev").ok()?;
    for line in data.lines().skip(2) {
        let line = line.trim();
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if name.trim() != iface {
            continue;
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        // rx_bytes is col 0, tx_bytes is col 8
        if cols.len() < 10 {
            return None;
        }
        let rx: u64 = cols[0].parse().ok()?;
        let tx: u64 = cols[8].parse().ok()?;
        return Some((rx, tx));
    }
    None
}

#[derive(Debug, Clone)]
pub struct PingUpdate {
    pub loss_percent: f64,
    pub last_rtt_ms: Option<f64>,
    pub last_error: Option<String>,
    pub samples: usize,
}

/// Rolling ICMP quality toward a host (default 1.1.1.1), sampled off the UI thread.
#[derive(Debug)]
pub struct PingMonitor {
    pub host: String,
    pub loss_percent: f64,
    pub last_rtt_ms: Option<f64>,
    pub last_error: Option<String>,
    pub samples: usize,
    rx: Option<std::sync::mpsc::Receiver<PingUpdate>>,
}

impl Default for PingMonitor {
    fn default() -> Self {
        Self::start("1.1.1.1")
    }
}

impl PingMonitor {
    pub fn start(host: impl Into<String>) -> Self {
        let host = host.into();
        let (tx, rx) = std::sync::mpsc::channel();
        let host_bg = host.clone();
        std::thread::spawn(move || {
            let mut results: VecDeque<bool> = VecDeque::new();
            let window = 20usize;
            loop {
                let output = Command::new("ping")
                    .args(["-c", "1", "-W", "1", &host_bg])
                    .output();

                let (ok, rtt, err) = match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        (true, parse_rtt_ms(&stdout), None)
                    }
                    Ok(o) => {
                        let e = String::from_utf8_lossy(&o.stderr).trim().to_string();
                        (
                            false,
                            None,
                            if e.is_empty() { None } else { Some(e) },
                        )
                    }
                    Err(e) => (false, None, Some(format!("ping: {e}"))),
                };

                results.push_back(ok);
                while results.len() > window {
                    results.pop_front();
                }
                let lost = results.iter().filter(|r| !**r).count();
                let loss = if results.is_empty() {
                    0.0
                } else {
                    (lost as f64 / results.len() as f64) * 100.0
                };

                if tx
                    .send(PingUpdate {
                        loss_percent: loss,
                        last_rtt_ms: rtt,
                        last_error: err,
                        samples: results.len(),
                    })
                    .is_err()
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });

        Self {
            host,
            loss_percent: 0.0,
            last_rtt_ms: None,
            last_error: None,
            samples: 0,
            rx: Some(rx),
        }
    }

    /// Drain any pending updates from the background sampler.
    pub fn poll(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        while let Ok(u) = rx.try_recv() {
            self.loss_percent = u.loss_percent;
            self.last_rtt_ms = u.last_rtt_ms;
            self.last_error = u.last_error;
            self.samples = u.samples;
        }
    }
}

fn parse_rtt_ms(stdout: &str) -> Option<f64> {
    // time=12.3 ms  or time=12.3ms
    for part in stdout.split_whitespace() {
        if let Some(rest) = part.strip_prefix("time=") {
            let num = rest.trim_end_matches("ms").trim();
            if let Ok(v) = num.parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}
