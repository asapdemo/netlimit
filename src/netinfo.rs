//! Network interface discovery.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

/// Non-loopback, non-IFB interfaces, default-route first, then up, then name.
pub fn list_interfaces() -> Vec<String> {
    let net = Path::new("/sys/class/net");
    let Ok(entries) = fs::read_dir(net) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "lo" || name.starts_with("ifb") {
                None
            } else {
                Some(name)
            }
        })
        .collect();

    let default = default_interface().ok().flatten();

    names.sort_by(|a, b| {
        let a_def = Some(a.as_str()) == default.as_deref();
        let b_def = Some(b.as_str()) == default.as_deref();
        let a_up = interface_state(a) == "up";
        let b_up = interface_state(b) == "up";
        b_def
            .cmp(&a_def)
            .then(b_up.cmp(&a_up))
            .then(a.cmp(b))
    });

    names
}

pub fn default_interface() -> Result<Option<String>> {
    let out = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .context("failed to run `ip route`")?;

    if !out.status.success() {
        return Ok(list_interfaces().into_iter().next());
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut tokens = stdout.split_whitespace();
    while let Some(t) = tokens.next() {
        if t == "dev" {
            if let Some(dev) = tokens.next() {
                return Ok(Some(dev.to_string()));
            }
        }
    }

    Ok(list_interfaces().into_iter().next())
}

pub fn interface_state(iface: &str) -> String {
    let path = format!("/sys/class/net/{iface}/operstate");
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "missing".into())
}

pub fn interface_exists(iface: &str) -> bool {
    Path::new(&format!("/sys/class/net/{iface}")).exists()
}
