//! Re-exec under sudo with absolute paths (sudo secure_path safe).

use std::env;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};

pub fn elevate(forwarded: &[String]) -> Result<()> {
    let sudo = which("sudo").context("sudo not found on PATH")?;
    let exe = env::current_exe().context("cannot resolve current executable")?;

    eprintln!("NetLimit needs root to change traffic rules.");
    eprintln!("Running: sudo {} {}", exe.display(), forwarded.join(" "));
    eprintln!();
    eprintln!("Tip: install a system link so `sudo netlimit` works:");
    eprintln!("  sudo ln -sf {} /usr/local/bin/netlimit", exe.display());
    eprintln!();

    let err = Command::new(sudo).arg(&exe).args(forwarded).exec();
    bail!("failed to exec sudo: {err}");
}

fn which(cmd: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(cmd);
            p.is_file().then_some(p)
        })
    })
}
