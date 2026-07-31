"""NetLimit entry point — launch the interactive TUI."""

from __future__ import annotations

import argparse
import sys

from netlimit import __version__
from netlimit.core.utils import NetLimitError, is_root
from netlimit.elevate import elevate_or_exit, sudo_command_hint


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="netlimit",
        description="Interactive TUI for system-wide Linux network traffic control.",
    )
    parser.add_argument(
        "-i",
        "--interface",
        metavar="IFACE",
        help="Network interface to control (default: auto-detect).",
    )
    parser.add_argument(
        "--no-sudo",
        action="store_true",
        help="Do not re-exec with sudo (TUI opens read-only without root).",
    )
    parser.add_argument(
        "-V",
        "--version",
        action="version",
        version=f"netlimit {__version__}",
    )
    return parser


def main(argv: list[str] | None = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)

    forwarded: list[str] = []
    if args.interface:
        forwarded.extend(["--interface", args.interface])

    if not is_root() and not args.no_sudo:
        elevate_or_exit(*forwarded)
        return

    from netlimit.ui import run_tui

    try:
        run_tui(interface=args.interface)
    except NetLimitError as exc:
        print(f"error: {exc}", file=sys.stderr)
        print(f"hint: try  {sudo_command_hint(*forwarded)}", file=sys.stderr)
        raise SystemExit(1) from exc
    except KeyboardInterrupt:
        raise SystemExit(0) from None


if __name__ == "__main__":
    main()
