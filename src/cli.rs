//! Command-line interface (clap subcommands) alongside the interactive TUI.

use std::io::{self, Write};

use clap::{Parser, Subcommand, ValueEnum};

use crate::elevate;
use crate::history;
use crate::netinfo::{interface_state, list_interfaces};
use crate::presets::{self, all_presets};
use crate::speedtest::{self, SampleKind, SpeedTestEvent, TestScope};
use crate::tc::{is_root, format_loss, format_ms, format_rate, Limits, TrafficController};

/// NetLimit — shape and measure Linux network traffic.
#[derive(Debug, Parser)]
#[command(
    name = "netlimit",
    version,
    about = "Shape Linux traffic (tc/netem) and run Cloudflare speed tests",
    long_about = "NetLimit applies system-wide download/upload/loss/delay/jitter limits \
via tc/netem/IFB, and can run Cloudflare speed tests.\n\n\
With no subcommand, opens the interactive TUI.\n\n\
Examples:\n  \
  sudo netlimit\n  \
  sudo netlimit apply --download 10 --upload 2 --loss 1\n  \
  sudo netlimit apply --preset 4G\n  \
  sudo netlimit reset\n  \
  netlimit status\n  \
  netlimit speedtest --duration 5\n  \
  netlimit interfaces"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Network interface (default: auto-detect via default route)
    #[arg(short, long, global = true)]
    pub interface: Option<String>,

    /// Do not re-exec with sudo when root is required
    #[arg(long, global = true)]
    pub no_sudo: bool,

    /// Machine-readable JSON where supported (status, history, interfaces, presets)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Open the interactive TUI (default when no subcommand is given)
    Tui,

    /// Apply traffic limits (requires root)
    Apply {
        /// Download limit in Mbps (0 = unlimited)
        #[arg(long, short = 'd', value_name = "MBPS")]
        download: Option<f64>,

        /// Upload limit in Mbps (0 = unlimited)
        #[arg(long, short = 'u', value_name = "MBPS")]
        upload: Option<f64>,

        /// Packet loss percent (0–100)
        #[arg(long, short = 'l', value_name = "PERCENT")]
        loss: Option<f64>,

        /// Base delay in milliseconds
        #[arg(long, value_name = "MS")]
        delay: Option<f64>,

        /// Jitter (delay variation) in milliseconds
        #[arg(long, short = 'j', value_name = "MS")]
        jitter: Option<f64>,

        /// Load a named preset (overridable by the flags above)
        #[arg(long, short = 'p', value_name = "NAME")]
        preset: Option<String>,
    },

    /// Remove all netlimit qdiscs / IFB shaping (requires root)
    Reset,

    /// Show currently applied limits
    Status,

    /// List network interfaces
    #[command(visible_alias = "ifaces")]
    Interfaces,

    /// List built-in and user presets
    Presets,

    /// Run a Cloudflare speed test (non-interactive)
    Speedtest {
        /// Seconds per phase (latency / download / upload)
        #[arg(long, short = 't', default_value_t = 5)]
        duration: u32,

        /// Which phases to run
        #[arg(long, short = 's', value_enum, default_value_t = ScopeArg::Full)]
        scope: ScopeArg,

        /// Quiet: only print the final summary
        #[arg(long, short = 'q')]
        quiet: bool,
    },

    /// Show saved speed-test history
    History {
        /// Max entries to print (newest first)
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    Full,
    Latency,
    Download,
    Upload,
}

impl From<ScopeArg> for TestScope {
    fn from(s: ScopeArg) -> Self {
        match s {
            ScopeArg::Full => TestScope::Full,
            ScopeArg::Latency => TestScope::Latency,
            ScopeArg::Download => TestScope::Download,
            ScopeArg::Upload => TestScope::Upload,
        }
    }
}

/// Run the parsed CLI. Returns process exit code.
pub fn run(cli: Cli) -> anyhow::Result<i32> {
    let needs_root = command_needs_root(&cli.command);
    if needs_root && !is_root() && !cli.no_sudo {
        // Re-exec under sudo with the same argv (absolute binary path).
        let forwarded: Vec<String> = std::env::args().skip(1).collect();
        elevate::elevate(&forwarded)?;
        return Ok(0);
    }

    match cli.command {
        None | Some(Commands::Tui) => {
            run_tui(cli.interface)?;
            Ok(0)
        }
        Some(Commands::Apply {
            download,
            upload,
            loss,
            delay,
            jitter,
            preset,
        }) => {
            cmd_apply(
                cli.interface,
                download,
                upload,
                loss,
                delay,
                jitter,
                preset,
                cli.json,
            )?;
            Ok(0)
        }
        Some(Commands::Reset) => {
            cmd_reset(cli.interface, cli.json)?;
            Ok(0)
        }
        Some(Commands::Status) => {
            cmd_status(cli.interface, cli.json)?;
            Ok(0)
        }
        Some(Commands::Interfaces) => {
            cmd_interfaces(cli.json)?;
            Ok(0)
        }
        Some(Commands::Presets) => {
            cmd_presets(cli.json)?;
            Ok(0)
        }
        Some(Commands::Speedtest {
            duration,
            scope,
            quiet,
        }) => {
            cmd_speedtest(duration, scope.into(), quiet, cli.json)?;
            Ok(0)
        }
        Some(Commands::History { limit }) => {
            cmd_history(limit, cli.json)?;
            Ok(0)
        }
    }
}

fn command_needs_root(cmd: &Option<Commands>) -> bool {
    match cmd {
        // TUI elevates so Apply/Reset work inside the UI.
        None | Some(Commands::Tui) | Some(Commands::Apply { .. }) | Some(Commands::Reset) => true,
        Some(Commands::Status)
        | Some(Commands::Interfaces)
        | Some(Commands::Presets)
        | Some(Commands::Speedtest { .. })
        | Some(Commands::History { .. }) => false,
    }
}

fn run_tui(interface: Option<String>) -> anyhow::Result<()> {
    use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    use crossterm::execute;
    use std::io::stdout;

    use crate::app::App;

    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;

    let app = App::new(interface);
    let result = app.run(&mut terminal);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn controller(interface: Option<String>) -> anyhow::Result<TrafficController> {
    TrafficController::new(interface).map_err(|e| anyhow::anyhow!("{e}"))
}

#[allow(clippy::too_many_arguments)]
fn cmd_apply(
    interface: Option<String>,
    download: Option<f64>,
    upload: Option<f64>,
    loss: Option<f64>,
    delay: Option<f64>,
    jitter: Option<f64>,
    preset: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    if !is_root() {
        anyhow::bail!("root required for apply (run with sudo, or drop --no-sudo)");
    }

    let mut limits = Limits::default();

    if let Some(name) = &preset {
        let found = all_presets()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name));
        let Some(p) = found else {
            let names: Vec<_> = all_presets().iter().map(|p| p.name.clone()).collect();
            anyhow::bail!(
                "unknown preset {name:?}. Available: {}",
                names.join(", ")
            );
        };
        limits.download_mbps = p.download_mbps;
        limits.upload_mbps = p.upload_mbps;
        limits.loss_percent = p.loss_percent;
        limits.delay_ms = p.delay_ms;
        limits.jitter_ms = p.jitter_ms;
    }

    if let Some(v) = download {
        limits.download_mbps = v;
    }
    if let Some(v) = upload {
        limits.upload_mbps = v;
    }
    if let Some(v) = loss {
        limits.loss_percent = v;
    }
    if let Some(v) = delay {
        limits.delay_ms = v;
    }
    if let Some(v) = jitter {
        limits.jitter_ms = v;
    }

    if preset.is_none()
        && download.is_none()
        && upload.is_none()
        && loss.is_none()
        && delay.is_none()
        && jitter.is_none()
    {
        anyhow::bail!(
            "specify at least one of --download/--upload/--loss/--delay/--jitter or --preset"
        );
    }

    let ctrl = controller(interface)?;
    limits.interface = ctrl.interface.clone();
    let applied = ctrl
        .apply(limits)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&applied)?);
    } else {
        println!("Applied: {}", applied.summary());
    }
    Ok(())
}

fn cmd_reset(interface: Option<String>, json: bool) -> anyhow::Result<()> {
    if !is_root() {
        anyhow::bail!("root required for reset (run with sudo, or drop --no-sudo)");
    }
    let ctrl = controller(interface)?;
    ctrl.reset().map_err(|e| anyhow::anyhow!("{e}"))?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "interface": ctrl.interface,
                "action": "reset"
            })
        );
    } else {
        println!("Reset traffic control on {}", ctrl.interface);
    }
    Ok(())
}

fn cmd_status(interface: Option<String>, json: bool) -> anyhow::Result<()> {
    let ctrl = controller(interface)?;
    let status = ctrl.status();
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else if status.is_active() {
        println!("{}", status.summary());
        println!(
            "  download: {:>10}    upload: {:>10}",
            format_rate(status.download_mbps),
            format_rate(status.upload_mbps)
        );
        println!(
            "  loss:     {:>10}    delay:  {:>6} ± {} ms",
            format_loss(status.loss_percent),
            format_ms(status.delay_ms),
            format_ms(status.jitter_ms)
        );
    } else {
        println!(
            "interface={}  (no netlimit shaping active)",
            status.interface
        );
    }
    Ok(())
}

fn cmd_interfaces(json: bool) -> anyhow::Result<()> {
    let ifaces = list_interfaces();
    let default = crate::netinfo::default_interface()
        .ok()
        .flatten()
        .unwrap_or_default();

    if json {
        let rows: Vec<_> = ifaces
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "state": interface_state(name),
                    "default": *name == default,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if ifaces.is_empty() {
        println!("No network interfaces found");
        return Ok(());
    }
    println!("{:<16} {:<8} {}", "INTERFACE", "STATE", "NOTES");
    for name in &ifaces {
        let state = interface_state(name);
        let note = if *name == default { "default route" } else { "" };
        println!("{name:<16} {state:<8} {note}");
    }
    Ok(())
}

fn cmd_presets(json: bool) -> anyhow::Result<()> {
    let presets = all_presets();
    if json {
        println!("{}", serde_json::to_string_pretty(&presets)?);
        return Ok(());
    }
    if presets.is_empty() {
        println!("No presets");
        return Ok(());
    }
    println!("{:<14} {:<28} {}", "NAME", "SUMMARY", "KIND");
    for p in &presets {
        let kind = if p.builtin { "builtin" } else { "custom" };
        println!("{:<14} {:<28} {kind}", p.name, p.summary());
    }
    println!(
        "\nConfig: {}",
        presets::presets_path().display()
    );
    Ok(())
}

fn cmd_speedtest(
    duration: u32,
    scope: TestScope,
    quiet: bool,
    json: bool,
) -> anyhow::Result<()> {
    let duration = duration.clamp(1, 120);
    if !quiet && !json {
        eprintln!(
            "Cloudflare speed test · scope={} · {}s per phase",
            scope.label(),
            duration
        );
        eprintln!();
    }

    let (rx, _cancel) = speedtest::start(duration, scope);

    let mut down_samples = Vec::new();
    let mut up_samples = Vec::new();
    let mut lat_samples = Vec::new();
    let mut last_phase = String::new();

    loop {
        match rx.recv() {
            Ok(SpeedTestEvent::Progress {
                phase,
                detail,
                phase_progress,
            }) => {
                if !quiet && !json {
                    if phase != last_phase {
                        if !last_phase.is_empty() {
                            eprintln!();
                        }
                        last_phase = phase.clone();
                    }
                    eprint!(
                        "\r  [{phase:<8}] {detail:<40} {:>3.0}%",
                        phase_progress * 100.0
                    );
                    let _ = io::stderr().flush();
                }
            }
            Ok(SpeedTestEvent::Sample { kind, value }) => match kind {
                SampleKind::Download => down_samples.push(value),
                SampleKind::Upload => up_samples.push(value),
                SampleKind::Latency => lat_samples.push(value),
            },
            Ok(SpeedTestEvent::Finished {
                scope,
                download_mbps,
                upload_mbps,
                latency_ms,
                jitter_ms,
                duration_secs,
            }) => {
                if !quiet && !json {
                    eprintln!();
                }

                let dl = download_mbps.unwrap_or(0.0);
                let ul = upload_mbps.unwrap_or(0.0);
                let lat = latency_ms.unwrap_or(0.0);
                let jit = jitter_ms.unwrap_or(0.0);

                // Persist history (best-effort)
                let iface = crate::netinfo::default_interface()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "—".into());
                let entry = history::HistoryEntry {
                    at: history::now_human(),
                    interface: iface,
                    download_mbps: dl,
                    upload_mbps: ul,
                    latency_ms: lat,
                    jitter_ms: jit,
                    duration_secs,
                    limits: Limits::default(),
                };
                let _ = history::push_history(entry);

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "scope": scope.label(),
                            "duration_secs": duration_secs,
                            "download_mbps": download_mbps,
                            "upload_mbps": upload_mbps,
                            "latency_ms": latency_ms,
                            "jitter_ms": jitter_ms,
                            "samples": {
                                "download": down_samples,
                                "upload": up_samples,
                                "latency": lat_samples,
                            }
                        })
                    );
                } else {
                    println!("Results ({})", scope.label());
                    if download_mbps.is_some() {
                        println!("  Download: {dl:.2} Mbps");
                    }
                    if upload_mbps.is_some() {
                        println!("  Upload:   {ul:.2} Mbps");
                    }
                    if latency_ms.is_some() {
                        println!("  Latency:  {lat:.1} ms  (jitter {jit:.1} ms)");
                    }
                    println!("  Duration: {duration_secs}s / phase");
                }
                return Ok(());
            }
            Ok(SpeedTestEvent::Cancelled) => {
                if !quiet && !json {
                    eprintln!("\nSpeed test cancelled");
                }
                anyhow::bail!("speed test cancelled");
            }
            Ok(SpeedTestEvent::Failed(err)) => {
                if !quiet && !json {
                    eprintln!();
                }
                anyhow::bail!("speed test failed: {err}");
            }
            Err(_) => {
                anyhow::bail!("speed test worker ended unexpectedly");
            }
        }
    }
}

fn cmd_history(limit: usize, json: bool) -> anyhow::Result<()> {
    let mut entries = history::load_history();
    if limit > 0 && entries.len() > limit {
        entries.truncate(limit);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("No speed-test history yet.");
        println!("Path: {}", history::history_path().display());
        return Ok(());
    }

    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>8}  {}",
        "WHEN", "↓ Mbps", "↑ Mbps", "LAT ms", "JIT ms", "IFACE"
    );
    for e in &entries {
        println!(
            "{:<20} {:>8.1} {:>8.1} {:>8.1} {:>8.1}  {}",
            e.at, e.download_mbps, e.upload_mbps, e.latency_ms, e.jitter_ms, e.interface
        );
    }
    println!("\nPath: {}", history::history_path().display());
    Ok(())
}
