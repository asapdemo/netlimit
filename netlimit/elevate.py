"""Privilege escalation helpers (sudo re-exec with absolute paths)."""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path


def python_executable() -> str:
    """Absolute path to the interpreter that has netlimit installed."""
    return str(Path(sys.executable).resolve())


def console_script() -> Path | None:
    """Path to the netlimit console script, if we were launched via it."""
    script = Path(sys.argv[0]).resolve()
    if script.is_file() and os.access(script, os.X_OK) and script.name.startswith(
        "netlimit"
    ):
        return script
    candidate = Path(python_executable()).parent / "netlimit"
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return candidate
    return None


def sudo_argv(*forwarded: str) -> list[str]:
    """Build a sudo argv that works when `netlimit` is not on secure_path."""
    script = console_script()
    if script is not None:
        return ["sudo", str(script), *forwarded]
    return ["sudo", python_executable(), "-m", "netlimit", *forwarded]


def sudo_command_hint(*forwarded: str) -> str:
    return " ".join(sudo_argv(*forwarded))


def elevate_or_exit(*forwarded: str) -> None:
    """Re-exec under sudo using absolute paths, or print a clear fallback and exit."""
    argv = sudo_argv(*forwarded)
    cmd = " ".join(argv)
    script = console_script()
    link_src = script or Path(python_executable()).parent / "netlimit"

    if shutil.which("sudo") is None:
        print("error: sudo not found. Re-run NetLimit as root:", file=sys.stderr)
        print(f"  {python_executable()} -m netlimit", file=sys.stderr)
        raise SystemExit(1)

    print("NetLimit needs root to change traffic rules.")
    print(f"Running: {cmd}")
    print()
    print("Tip: if sudo says “command not found”, install a system link once:")
    print(f"  sudo ln -sf {link_src} /usr/local/bin/netlimit")
    print()

    try:
        os.execvp("sudo", argv)
    except OSError as exc:
        print(f"error: failed to exec sudo: {exc}", file=sys.stderr)
        print(f"Run manually:\n  {cmd}", file=sys.stderr)
        raise SystemExit(1) from exc
