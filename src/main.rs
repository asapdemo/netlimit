//! NetLimit — interactive TUI and CLI for system-wide Linux network traffic control.

mod app;
mod cli;
mod elevate;
mod history;
mod monitor;
mod netinfo;
mod presets;
mod speedtest;
mod tc;
mod theme;
mod ui;

use clap::Parser;

use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = cli::run(cli)?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
