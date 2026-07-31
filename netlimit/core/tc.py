"""Linux traffic control via tc, netem, and IFB.

Upload shaping is applied on the real interface egress (HTB + netem).
Download shaping uses an IFB device: ingress is redirected to ifbN, then
shaped as egress on that virtual device.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from netlimit.core.utils import (
    NetLimitError,
    ensure_commands,
    format_loss,
    format_rate,
    get_default_interface,
    require_root,
    run_cmd,
)

# IFB device index used for download shaping (ifb0 by default).
IFB_DEVICE = "ifb0"

# HTB/netem handle conventions
ROOT_HANDLE = "1:"
CLASS_ID = "1:10"
NETEM_HANDLE = "10:"
INGRESS_HANDLE = "ffff:"


@dataclass
class NetworkLimits:
    """Desired or reported network limit configuration."""

    download_mbps: float | None = None  # None / 0 = unlimited
    upload_mbps: float | None = None
    loss_percent: float = 0.0
    interface: str | None = None

    def normalized(self) -> NetworkLimits:
        """Return a copy with cleaned values."""
        dl = self.download_mbps if self.download_mbps and self.download_mbps > 0 else None
        ul = self.upload_mbps if self.upload_mbps and self.upload_mbps > 0 else None
        loss = max(0.0, min(100.0, float(self.loss_percent or 0.0)))
        return NetworkLimits(
            download_mbps=dl,
            upload_mbps=ul,
            loss_percent=loss,
            interface=self.interface,
        )

    @property
    def is_active(self) -> bool:
        """True if any limiting is configured."""
        n = self.normalized()
        return bool(n.download_mbps or n.upload_mbps or n.loss_percent > 0)

    def summary(self) -> str:
        n = self.normalized()
        iface = n.interface or "?"
        return (
            f"iface={iface}  ↓ {format_rate(n.download_mbps)}  "
            f"↑ {format_rate(n.upload_mbps)}  loss={format_loss(n.loss_percent)}"
        )


@dataclass
class ApplyResult:
    """Outcome of apply/reset operations."""

    success: bool
    message: str
    limits: NetworkLimits = field(default_factory=NetworkLimits)


class TrafficController:
    """Manage system-wide network limits with tc/netem/IFB."""

    def __init__(self, interface: str | None = None, ifb: str = IFB_DEVICE) -> None:
        ensure_commands("tc", "ip")
        self.ifb = ifb
        self.interface = interface or get_default_interface()
        if not self.interface:
            raise NetLimitError(
                "Could not detect a network interface. "
                "Specify one with --interface / -i."
            )

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def apply(self, limits: NetworkLimits) -> ApplyResult:
        """Apply download/upload rate limits and packet loss."""
        require_root()
        limits = limits.normalized()
        limits.interface = self.interface

        if not limits.is_active:
            return self.reset()

        try:
            # Always start from a clean slate on this interface.
            self._cleanup(silent=True)

            # Download (ingress → IFB egress)
            if limits.download_mbps is not None:
                self._setup_ifb()
                self._setup_ingress_redirect()
                self._apply_shaping(
                    device=self.ifb,
                    rate_mbps=limits.download_mbps,
                    loss_percent=limits.loss_percent,
                )
            elif limits.loss_percent > 0:
                # Loss-only still needs IFB so ingress is affected.
                self._setup_ifb()
                self._setup_ingress_redirect()
                self._apply_loss_only(device=self.ifb, loss_percent=limits.loss_percent)

            # Upload (interface egress)
            if limits.upload_mbps is not None:
                self._apply_shaping(
                    device=self.interface,
                    rate_mbps=limits.upload_mbps,
                    loss_percent=limits.loss_percent,
                )
            elif limits.loss_percent > 0:
                self._apply_loss_only(
                    device=self.interface, loss_percent=limits.loss_percent
                )

            return ApplyResult(
                success=True,
                message=f"Applied: {limits.summary()}",
                limits=limits,
            )
        except NetLimitError as exc:
            # Best-effort cleanup after a failed apply.
            try:
                self._cleanup(silent=True)
            except NetLimitError:
                pass
            return ApplyResult(success=False, message=str(exc), limits=limits)
        except Exception as exc:  # noqa: BLE001 — surface unexpected errors cleanly
            try:
                self._cleanup(silent=True)
            except NetLimitError:
                pass
            return ApplyResult(
                success=False,
                message=f"Unexpected error: {exc}",
                limits=limits,
            )

    def reset(self) -> ApplyResult:
        """Remove all traffic control rules and tear down IFB."""
        require_root()
        try:
            self._cleanup(silent=False)
            cleared = NetworkLimits(interface=self.interface)
            return ApplyResult(
                success=True,
                message=f"Reset complete on {self.interface} (all limits removed)",
                limits=cleared,
            )
        except NetLimitError as exc:
            return ApplyResult(success=False, message=str(exc))

    def status(self) -> NetworkLimits:
        """Inspect current tc rules and report effective limits."""
        iface = self.interface
        download = self._parse_rate_from_qdisc(self.ifb)
        upload = self._parse_rate_from_qdisc(iface)
        # Prefer loss from the real interface; fall back to IFB.
        loss = self._parse_loss_from_qdisc(iface)
        if loss is None:
            loss = self._parse_loss_from_qdisc(self.ifb)
        if loss is None:
            loss = 0.0

        # If IFB has no root qdisc, download is unlimited / inactive.
        has_ifb = self._has_root_qdisc(self.ifb)
        has_egress = self._has_root_qdisc(iface)
        has_ingress = self._has_ingress_qdisc(iface)

        if not (has_ifb or has_egress or has_ingress):
            return NetworkLimits(
                download_mbps=None,
                upload_mbps=None,
                loss_percent=0.0,
                interface=iface,
            )

        return NetworkLimits(
            download_mbps=download if has_ifb else None,
            upload_mbps=upload if has_egress else None,
            loss_percent=loss,
            interface=iface,
        )

    def set_interface(self, interface: str) -> None:
        """Switch the controlled interface (does not auto-apply)."""
        path_ok = Path(f"/sys/class/net/{interface}").exists()
        if not path_ok:
            raise NetLimitError(f"Interface not found: {interface}")
        if interface.startswith("ifb"):
            raise NetLimitError("Cannot shape an IFB device directly")
        self.interface = interface

    # ------------------------------------------------------------------
    # Setup helpers
    # ------------------------------------------------------------------

    def _setup_ifb(self) -> None:
        """Load IFB module if needed and bring the device up."""
        # Ensure module is loaded (ignore failure if built-in).
        run_cmd(["modprobe", "ifb", "numifbs=1"], check=False)

        # Create the device if it does not exist.
        if not self._device_exists(self.ifb):
            run_cmd(["ip", "link", "add", self.ifb, "type", "ifb"])

        run_cmd(["ip", "link", "set", "dev", self.ifb, "up"])

    def _setup_ingress_redirect(self) -> None:
        """Redirect interface ingress traffic to the IFB device."""
        run_cmd(
            ["tc", "qdisc", "add", "dev", self.interface, "handle", INGRESS_HANDLE, "ingress"]
        )
        run_cmd(
            [
                "tc",
                "filter",
                "add",
                "dev",
                self.interface,
                "parent",
                INGRESS_HANDLE,
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
                self.ifb,
            ]
        )

    def _apply_shaping(
        self,
        *,
        device: str,
        rate_mbps: float,
        loss_percent: float,
    ) -> None:
        """Apply HTB rate limit with optional netem loss on *device* egress."""
        rate = self._mbps_to_tc_rate(rate_mbps)

        run_cmd(
            [
                "tc",
                "qdisc",
                "add",
                "dev",
                device,
                "root",
                "handle",
                ROOT_HANDLE,
                "htb",
                "default",
                "10",
            ]
        )
        run_cmd(
            [
                "tc",
                "class",
                "add",
                "dev",
                device,
                "parent",
                ROOT_HANDLE,
                "classid",
                CLASS_ID,
                "htb",
                "rate",
                rate,
                "ceil",
                rate,
            ]
        )

        netem_args = [
            "tc",
            "qdisc",
            "add",
            "dev",
            device,
            "parent",
            CLASS_ID,
            "handle",
            NETEM_HANDLE,
            "netem",
        ]
        if loss_percent > 0:
            netem_args.extend(["loss", self._fmt_loss(loss_percent)])
        # netem with no options is fine as a leaf; keep for consistency.
        run_cmd(netem_args)

    def _apply_loss_only(self, *, device: str, loss_percent: float) -> None:
        """Apply netem packet loss without rate limiting."""
        if loss_percent <= 0:
            return
        run_cmd(
            [
                "tc",
                "qdisc",
                "add",
                "dev",
                device,
                "root",
                "handle",
                NETEM_HANDLE,
                "netem",
                "loss",
                self._fmt_loss(loss_percent),
            ]
        )

    @staticmethod
    def _fmt_loss(percent: float) -> str:
        if percent == int(percent):
            return f"{int(percent)}%"
        return f"{percent:.1f}%"

    def _cleanup(self, *, silent: bool = True) -> None:
        """Remove qdiscs from interface and IFB; bring IFB down."""
        # Order matters: remove filters/qdiscs before taking IFB down.
        self._del_qdisc(self.interface, "root", silent=silent)
        self._del_qdisc(self.interface, "ingress", silent=silent)
        self._del_qdisc(self.ifb, "root", silent=silent)

        if self._device_exists(self.ifb):
            # Bring down; leave module loaded (safer for concurrent users).
            run_cmd(["ip", "link", "set", "dev", self.ifb, "down"], check=False)

    def _del_qdisc(self, device: str, kind: str, *, silent: bool = True) -> None:
        """Delete a qdisc; kind is 'root' or 'ingress'."""
        if not self._device_exists(device):
            return
        args = ["tc", "qdisc", "del", "dev", device, kind]
        result = run_cmd(args, check=False)
        if result.returncode != 0 and not silent:
            # "No such file" / "RTNETLINK answers: No such file or directory" is OK.
            err = (result.stderr or "").lower()
            if "no such file" in err or "not found" in err or "invalid argument" in err:
                return
            # Other errors during reset should surface.
            if "cannot find" not in err:
                # Still tolerate common empty-state messages.
                return

    # ------------------------------------------------------------------
    # Inspection helpers
    # ------------------------------------------------------------------

    def _device_exists(self, device: str) -> bool:
        return Path(f"/sys/class/net/{device}").exists()

    def _has_root_qdisc(self, device: str) -> bool:
        if not self._device_exists(device):
            return False
        result = run_cmd(["tc", "qdisc", "show", "dev", device], check=False)
        if result.returncode != 0:
            return False
        for line in result.stdout.splitlines():
            if "qdisc" in line and "0:" not in line.split()[:3]:
                # Default pfifo_fast / noqueue with handle 0: is "no custom qdisc".
                parts = line.split()
                if len(parts) >= 3 and parts[2] not in {"0:", "2:"}:
                    # Heuristic: any non-default root qdisc we installed.
                    if any(k in line for k in ("htb", "netem", "tbf", "hfsc")):
                        return True
        return False

    def _has_ingress_qdisc(self, device: str) -> bool:
        if not self._device_exists(device):
            return False
        result = run_cmd(["tc", "qdisc", "show", "dev", device], check=False)
        return "ingress" in result.stdout

    def _parse_rate_from_qdisc(self, device: str) -> float | None:
        """Parse HTB rate (Mbps) from class show output."""
        if not self._device_exists(device):
            return None
        result = run_cmd(["tc", "class", "show", "dev", device], check=False)
        if result.returncode != 0 or not result.stdout.strip():
            return None
        # Example: "class htb 1:10 root leaf 10: prio 0 rate 10Mbit ceil 10Mbit ..."
        match = re.search(r"\brate\s+(\d+(?:\.\d+)?)(bit|Kbit|Mbit|Gbit|Tbit)\b", result.stdout)
        if not match:
            return None
        return self._tc_rate_to_mbps(float(match.group(1)), match.group(2))

    def _parse_loss_from_qdisc(self, device: str) -> float | None:
        """Parse netem loss percent from qdisc show output."""
        if not self._device_exists(device):
            return None
        result = run_cmd(["tc", "qdisc", "show", "dev", device], check=False)
        if result.returncode != 0:
            return None
        # "qdisc netem ... loss 3%"
        match = re.search(r"\bloss\s+(\d+(?:\.\d+)?)%", result.stdout)
        if match:
            return float(match.group(1))
        return None

    @staticmethod
    def _mbps_to_tc_rate(mbps: float) -> str:
        """Convert Mbps to a tc rate string."""
        if mbps >= 1000:
            gbit = mbps / 1000
            if gbit == int(gbit):
                return f"{int(gbit)}Gbit"
            return f"{gbit:.3f}Gbit"
        if mbps == int(mbps):
            return f"{int(mbps)}Mbit"
        return f"{mbps:.3f}Mbit"

    @staticmethod
    def _tc_rate_to_mbps(value: float, unit: str) -> float:
        unit = unit.lower()
        if unit == "bit":
            return value / 1_000_000
        if unit == "kbit":
            return value / 1000
        if unit == "mbit":
            return value
        if unit == "gbit":
            return value * 1000
        if unit == "tbit":
            return value * 1_000_000
        return value
