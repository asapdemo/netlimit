"""Helpers: interface detection, formatting, privilege checks."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
from pathlib import Path


class NetLimitError(Exception):
    """Base error for NetLimit operations."""


def is_root() -> bool:
    """Return True if the current process has root privileges."""
    return os.geteuid() == 0


def require_root() -> None:
    """Raise NetLimitError if not running as root."""
    if not is_root():
        raise NetLimitError(
            "Root privileges required. Re-run with sudo:\n  sudo netlimit"
        )


def ensure_commands(*commands: str) -> None:
    """Ensure required system binaries exist on PATH."""
    missing = [cmd for cmd in commands if shutil.which(cmd) is None]
    if missing:
        raise NetLimitError(
            f"Missing required command(s): {', '.join(missing)}. "
            "Install iproute2 (provides tc and ip)."
        )


def run_cmd(
    args: list[str],
    *,
    check: bool = True,
    capture: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run a system command and return the completed process."""
    try:
        return subprocess.run(
            args,
            check=check,
            capture_output=capture,
            text=True,
        )
    except FileNotFoundError as exc:
        raise NetLimitError(f"Command not found: {args[0]}") from exc
    except subprocess.CalledProcessError as exc:
        stderr = (exc.stderr or "").strip()
        stdout = (exc.stdout or "").strip()
        detail = stderr or stdout or str(exc)
        raise NetLimitError(f"Command failed ({' '.join(args)}): {detail}") from exc


def list_interfaces() -> list[str]:
    """List non-loopback, non-IFB network interfaces.

    Prefer: default route interface first, then interfaces that are up,
    then the rest alphabetically.
    """
    net_dir = Path("/sys/class/net")
    if not net_dir.is_dir():
        return []

    names: list[str] = []
    for path in net_dir.iterdir():
        name = path.name
        if name == "lo" or name.startswith("ifb"):
            continue
        names.append(name)

    default = None
    try:
        result = run_cmd(["ip", "route", "show", "default"], check=False)
        if result.returncode == 0:
            match = re.search(r"\bdev\s+(\S+)", result.stdout)
            if match:
                default = match.group(1)
    except NetLimitError:
        default = None

    def sort_key(name: str) -> tuple[int, int, str]:
        is_default = 0 if name == default else 1
        state = get_interface_state(name)
        is_up = 0 if state == "up" else 1
        return (is_default, is_up, name)

    return sorted(names, key=sort_key)


def get_default_interface() -> str | None:
    """Detect the default route network interface."""
    try:
        result = run_cmd(["ip", "route", "show", "default"], check=False)
    except NetLimitError:
        return None

    if result.returncode != 0 or not result.stdout.strip():
        # Fallback: first non-loopback interface
        ifaces = list_interfaces()
        return ifaces[0] if ifaces else None

    # "default via 192.168.1.1 dev eth0 proto dhcp metric 100"
    match = re.search(r"\bdev\s+(\S+)", result.stdout)
    if match:
        return match.group(1)

    ifaces = list_interfaces()
    return ifaces[0] if ifaces else None


def get_interface_state(interface: str) -> str:
    """Return operstate for an interface (up, down, unknown, ...)."""
    state_path = Path(f"/sys/class/net/{interface}/operstate")
    if not state_path.exists():
        return "missing"
    try:
        return state_path.read_text(encoding="utf-8").strip() or "unknown"
    except OSError:
        return "unknown"


def format_rate(mbps: float | None) -> str:
    """Format a rate in Mbps for display. None / 0 means unlimited."""
    if mbps is None or mbps <= 0:
        return "Unlimited"
    if mbps == int(mbps):
        return f"{int(mbps)} Mbps"
    return f"{mbps:.1f} Mbps"


def format_loss(percent: float) -> str:
    """Format packet loss percentage."""
    if percent <= 0:
        return "0%"
    if percent == int(percent):
        return f"{int(percent)}%"
    return f"{percent:.1f}%"


def clamp(value: float, minimum: float, maximum: float) -> float:
    """Clamp value into [minimum, maximum]."""
    return max(minimum, min(maximum, value))


def step_value(
    current: float,
    direction: int,
    *,
    small: float,
    large: float,
    coarse: bool = False,
    minimum: float = 0.0,
    maximum: float = 10_000.0,
) -> float:
    """Adjust a numeric value by a small or large step."""
    delta = large if coarse else small
    return clamp(current + direction * delta, minimum, maximum)
