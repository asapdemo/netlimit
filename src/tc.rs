//! Linux traffic control via `tc`, `netem`, and IFB.

use std::process::Command;

use thiserror::Error;

use crate::netinfo::{default_interface, interface_exists};

const IFB: &str = "ifb0";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Limits {
    /// Mbps; 0 / None = unlimited
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub loss_percent: f64,
    pub interface: String,
}

impl Limits {
    pub fn normalized(mut self) -> Self {
        if self.download_mbps < 0.0 {
            self.download_mbps = 0.0;
        }
        if self.upload_mbps < 0.0 {
            self.upload_mbps = 0.0;
        }
        self.loss_percent = self.loss_percent.clamp(0.0, 100.0);
        self
    }

    pub fn is_active(&self) -> bool {
        self.download_mbps > 0.0 || self.upload_mbps > 0.0 || self.loss_percent > 0.0
    }

    pub fn summary(&self) -> String {
        format!(
            "iface={}  ↓ {}  ↑ {}  loss={}",
            self.interface,
            format_rate(self.download_mbps),
            format_rate(self.upload_mbps),
            format_loss(self.loss_percent),
        )
    }
}

#[derive(Debug, Error)]
pub enum TcError {
    #[error("{0}")]
    Message(String),
    #[error("root privileges required")]
    NotRoot,
    #[error("command failed ({cmd}): {detail}")]
    Command { cmd: String, detail: String },
}

pub struct TrafficController {
    pub interface: String,
    ifb: String,
}

impl TrafficController {
    pub fn new(interface: Option<String>) -> Result<Self, TcError> {
        ensure_cmds(&["tc", "ip"])?;
        let interface = match interface {
            Some(i) if interface_exists(&i) => i,
            Some(i) => {
                return Err(TcError::Message(format!("interface not found: {i}")));
            }
            None => default_interface()
                .map_err(|e| TcError::Message(e.to_string()))?
                .ok_or_else(|| TcError::Message("no network interface found".into()))?,
        };
        if interface.starts_with("ifb") {
            return Err(TcError::Message("cannot shape an IFB device".into()));
        }
        Ok(Self {
            interface,
            ifb: IFB.into(),
        })
    }

    pub fn set_interface(&mut self, interface: String) -> Result<(), TcError> {
        if !interface_exists(&interface) {
            return Err(TcError::Message(format!(
                "interface not found: {interface}"
            )));
        }
        if interface.starts_with("ifb") {
            return Err(TcError::Message("cannot shape an IFB device".into()));
        }
        self.interface = interface;
        Ok(())
    }

    pub fn apply(&self, limits: Limits) -> Result<Limits, TcError> {
        require_root()?;
        let limits = limits.normalized();
        if !limits.is_active() {
            self.reset()?;
            return Ok(Limits {
                interface: self.interface.clone(),
                ..Default::default()
            });
        }

        // Clean slate first; on failure try cleanup again.
        let _ = self.cleanup();

        let result = (|| -> Result<(), TcError> {
            // Download path (ingress → IFB)
            if limits.download_mbps > 0.0 {
                self.setup_ifb()?;
                self.setup_ingress_redirect()?;
                self.apply_shaping(&self.ifb, limits.download_mbps, limits.loss_percent)?;
            } else if limits.loss_percent > 0.0 {
                self.setup_ifb()?;
                self.setup_ingress_redirect()?;
                self.apply_loss_only(&self.ifb, limits.loss_percent)?;
            }

            // Upload path (egress)
            if limits.upload_mbps > 0.0 {
                self.apply_shaping(&self.interface, limits.upload_mbps, limits.loss_percent)?;
            } else if limits.loss_percent > 0.0 {
                self.apply_loss_only(&self.interface, limits.loss_percent)?;
            }
            Ok(())
        })();

        if let Err(e) = result {
            let _ = self.cleanup();
            return Err(e);
        }

        Ok(Limits {
            interface: self.interface.clone(),
            ..limits
        })
    }

    pub fn reset(&self) -> Result<(), TcError> {
        require_root()?;
        self.cleanup()
    }

    pub fn status(&self) -> Limits {
        let has_ifb = has_custom_qdisc(&self.ifb);
        let has_egress = has_custom_qdisc(&self.interface);
        let download = if has_ifb {
            parse_rate(&self.ifb).unwrap_or(0.0)
        } else {
            0.0
        };
        let upload = if has_egress {
            parse_rate(&self.interface).unwrap_or(0.0)
        } else {
            0.0
        };
        let loss = parse_loss(&self.interface)
            .or_else(|| parse_loss(&self.ifb))
            .unwrap_or(0.0);

        Limits {
            download_mbps: download,
            upload_mbps: upload,
            loss_percent: loss,
            interface: self.interface.clone(),
        }
    }

    fn setup_ifb(&self) -> Result<(), TcError> {
        let _ = run_ok(&["modprobe", "ifb", "numifbs=1"]);
        if !interface_exists(&self.ifb) {
            run(&["ip", "link", "add", &self.ifb, "type", "ifb"])?;
        }
        run(&["ip", "link", "set", "dev", &self.ifb, "up"])?;
        Ok(())
    }

    fn setup_ingress_redirect(&self) -> Result<(), TcError> {
        run(&[
            "tc",
            "qdisc",
            "add",
            "dev",
            &self.interface,
            "handle",
            "ffff:",
            "ingress",
        ])?;
        run(&[
            "tc",
            "filter",
            "add",
            "dev",
            &self.interface,
            "parent",
            "ffff:",
            "protocol",
            "all",
            "u32",
            "match",
            "u32",
            "0",
            "0",
            "action",
            "mirred",
            "egress",
            "redirect",
            "dev",
            &self.ifb,
        ])?;
        Ok(())
    }

    fn apply_shaping(&self, device: &str, rate_mbps: f64, loss: f64) -> Result<(), TcError> {
        let rate = mbps_to_tc(rate_mbps);
        run(&[
            "tc",
            "qdisc",
            "add",
            "dev",
            device,
            "root",
            "handle",
            "1:",
            "htb",
            "default",
            "10",
        ])?;
        run(&[
            "tc",
            "class",
            "add",
            "dev",
            device,
            "parent",
            "1:",
            "classid",
            "1:10",
            "htb",
            "rate",
            &rate,
            "ceil",
            &rate,
        ])?;
        let mut args = vec![
            "tc",
            "qdisc",
            "add",
            "dev",
            device,
            "parent",
            "1:10",
            "handle",
            "10:",
            "netem",
        ];
        let loss_s;
        if loss > 0.0 {
            loss_s = fmt_loss(loss);
            args.push("loss");
            args.push(&loss_s);
        }
        run(&args)?;
        Ok(())
    }

    fn apply_loss_only(&self, device: &str, loss: f64) -> Result<(), TcError> {
        if loss <= 0.0 {
            return Ok(());
        }
        let loss_s = fmt_loss(loss);
        run(&[
            "tc", "qdisc", "add", "dev", device, "root", "handle", "10:", "netem", "loss", &loss_s,
        ])?;
        Ok(())
    }

    fn cleanup(&self) -> Result<(), TcError> {
        let _ = run_ok(&["tc", "qdisc", "del", "dev", &self.interface, "root"]);
        let _ = run_ok(&["tc", "qdisc", "del", "dev", &self.interface, "ingress"]);
        let _ = run_ok(&["tc", "qdisc", "del", "dev", &self.ifb, "root"]);
        if interface_exists(&self.ifb) {
            let _ = run_ok(&["ip", "link", "set", "dev", &self.ifb, "down"]);
        }
        Ok(())
    }
}

// ── helpers ──────────────────────────────────────────────────────────

pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn require_root() -> Result<(), TcError> {
    if is_root() {
        Ok(())
    } else {
        Err(TcError::NotRoot)
    }
}

fn ensure_cmds(cmds: &[&str]) -> Result<(), TcError> {
    let mut missing = Vec::new();
    for c in cmds {
        if which(c).is_none() {
            missing.push(*c);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(TcError::Message(format!(
            "missing command(s): {}. install iproute2",
            missing.join(", ")
        )))
    }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(cmd);
            if p.is_file() {
                Some(p)
            } else {
                None
            }
        })
    })
}

fn run(args: &[&str]) -> Result<(), TcError> {
    let output = Command::new(args[0])
        .args(&args[1..])
        .output()
        .map_err(|e| TcError::Command {
            cmd: args.join(" "),
            detail: e.to_string(),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = if detail.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            detail.trim().to_string()
        };
        Err(TcError::Command {
            cmd: args.join(" "),
            detail,
        })
    }
}

fn run_ok(args: &[&str]) -> Result<(), TcError> {
    run(args)
}

fn has_custom_qdisc(device: &str) -> bool {
    if !interface_exists(device) {
        return false;
    }
    let Ok(output) = Command::new("tc")
        .args(["qdisc", "show", "dev", device])
        .output()
    else {
        return false;
    };
    let s = String::from_utf8_lossy(&output.stdout);
    s.contains("htb") || s.contains("netem") || s.contains("tbf") || s.contains("ingress")
}

fn parse_rate(device: &str) -> Option<f64> {
    let output = Command::new("tc")
        .args(["class", "show", "dev", device])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    // rate 10Mbit
    let re = regex_rate(&s)?;
    Some(re)
}

fn regex_rate(s: &str) -> Option<f64> {
    let mut it = s.split_whitespace().peekable();
    while let Some(w) = it.next() {
        if w == "rate" {
            if let Some(val) = it.next() {
                return tc_rate_to_mbps(val);
            }
        }
    }
    None
}

fn parse_loss(device: &str) -> Option<f64> {
    let output = Command::new("tc")
        .args(["qdisc", "show", "dev", device])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    let mut it = s.split_whitespace().peekable();
    while let Some(w) = it.next() {
        if w == "loss" {
            if let Some(val) = it.next() {
                let v = val.trim_end_matches('%');
                return v.parse().ok();
            }
        }
    }
    None
}

fn mbps_to_tc(mbps: f64) -> String {
    if mbps >= 1000.0 {
        let g = mbps / 1000.0;
        if (g - g.round()).abs() < f64::EPSILON {
            format!("{}Gbit", g as i64)
        } else {
            format!("{g:.3}Gbit")
        }
    } else if (mbps - mbps.round()).abs() < f64::EPSILON {
        format!("{}Mbit", mbps as i64)
    } else {
        format!("{mbps:.3}Mbit")
    }
}

fn tc_rate_to_mbps(s: &str) -> Option<f64> {
    let s = s.trim();
    let (num, unit) = if let Some(n) = s.strip_suffix("Tbit") {
        (n, "tbit")
    } else if let Some(n) = s.strip_suffix("Gbit") {
        (n, "gbit")
    } else if let Some(n) = s.strip_suffix("Mbit") {
        (n, "mbit")
    } else if let Some(n) = s.strip_suffix("Kbit") {
        (n, "kbit")
    } else if let Some(n) = s.strip_suffix("bit") {
        (n, "bit")
    } else {
        return None;
    };
    let v: f64 = num.parse().ok()?;
    Some(match unit {
        "bit" => v / 1_000_000.0,
        "kbit" => v / 1000.0,
        "mbit" => v,
        "gbit" => v * 1000.0,
        "tbit" => v * 1_000_000.0,
        _ => v,
    })
}

fn fmt_loss(p: f64) -> String {
    if (p - p.round()).abs() < f64::EPSILON {
        format!("{}%", p as i64)
    } else {
        format!("{p:.1}%")
    }
}

pub fn format_rate(mbps: f64) -> String {
    if mbps <= 0.0 {
        "Unlimited".into()
    } else if (mbps - mbps.round()).abs() < f64::EPSILON {
        format!("{} Mbps", mbps as i64)
    } else {
        format!("{mbps:.1} Mbps")
    }
}

pub fn format_loss(p: f64) -> String {
    if p <= 0.0 {
        "0%".into()
    } else if (p - p.round()).abs() < f64::EPSILON {
        format!("{}%", p as i64)
    } else {
        format!("{p:.1}%")
    }
}

pub fn format_value(metric: Metric, value: f64) -> String {
    match metric {
        Metric::Download | Metric::Upload => {
            if value <= 0.0 {
                "∞".into()
            } else if (value - value.round()).abs() < f64::EPSILON {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            }
        }
        Metric::Loss => {
            if (value - value.round()).abs() < f64::EPSILON {
                format!("{}", value as i64)
            } else {
                format!("{value:.1}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Download,
    Upload,
    Loss,
}

impl Metric {
    pub const ALL: [Metric; 3] = [Metric::Download, Metric::Upload, Metric::Loss];

    pub fn label(self) -> &'static str {
        match self {
            Metric::Download => "DOWNLOAD",
            Metric::Upload => "UPLOAD",
            Metric::Loss => "PACKET LOSS",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Metric::Download => "↓",
            Metric::Upload => "↑",
            Metric::Loss => "⚠",
        }
    }

    pub fn small_step(self) -> f64 {
        match self {
            Metric::Download | Metric::Upload => 1.0,
            Metric::Loss => 0.5,
        }
    }

    pub fn large_step(self) -> f64 {
        match self {
            Metric::Download | Metric::Upload => 10.0,
            Metric::Loss => 5.0,
        }
    }

    pub fn max(self) -> f64 {
        match self {
            Metric::Download | Metric::Upload => 10_000.0,
            Metric::Loss => 100.0,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Metric::Download => 0,
            Metric::Upload => 1,
            Metric::Loss => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_roundtrip_basic() {
        assert_eq!(mbps_to_tc(10.0), "10Mbit");
        assert_eq!(mbps_to_tc(2.5), "2.500Mbit");
        assert_eq!(tc_rate_to_mbps("10Mbit"), Some(10.0));
        assert_eq!(tc_rate_to_mbps("1Gbit"), Some(1000.0));
    }

    #[test]
    fn format_helpers() {
        assert_eq!(format_rate(0.0), "Unlimited");
        assert_eq!(format_rate(10.0), "10 Mbps");
        assert_eq!(format_loss(0.0), "0%");
        assert_eq!(format_loss(3.5), "3.5%");
        assert_eq!(format_value(Metric::Download, 0.0), "∞");
    }

    #[test]
    fn limits_active() {
        assert!(!Limits::default().is_active());
        assert!(Limits {
            download_mbps: 1.0,
            ..Default::default()
        }
        .is_active());
    }
}
