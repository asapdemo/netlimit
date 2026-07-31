//! NetLimit — interactive TUI for system-wide Linux network traffic control.

mod app;
mod elevate;
mod netinfo;
mod presets;
mod tc;
mod theme;
mod ui;

use clap::Parser;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use std::io::stdout;

use crate::app::App;
use crate::tc::is_root;

#[derive(Debug, Parser)]
#[command(
    name = "netlimit",
    version,
    about = "Interactive TUI for system-wide Linux network traffic control"
)]
struct Cli {
    /// Network interface (default: auto-detect via default route)
    #[arg(short, long)]
    interface: Option<String>,

    /// Do not re-exec with sudo (open TUI read-only without root)
    #[arg(long)]
    no_sudo: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut forwarded = Vec::new();
    if let Some(ref iface) = cli.interface {
        forwarded.push("--interface".into());
        forwarded.push(iface.clone());
    }

    if !is_root() && !cli.no_sudo {
        elevate::elevate(&forwarded)?;
        // elevate uses exec; if we return, something went wrong
        return Ok(());
    }

    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;

    let app = App::new(cli.interface.clone());
    let result = app.run(&mut terminal);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
