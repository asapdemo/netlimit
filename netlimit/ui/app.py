"""NetLimit interactive TUI — btop-inspired dark interface."""

from __future__ import annotations

from pathlib import Path

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Button, Header, Static

from netlimit.core.tc import NetworkLimits, TrafficController
from netlimit.core.utils import (
    NetLimitError,
    format_loss,
    format_rate,
    get_interface_state,
    is_root,
    list_interfaces,
    step_value,
)
from netlimit.ui.widgets import InterfaceBar, KeyHelp, MetricCard, StatusBanner

METRICS = ("download", "upload", "loss")
STEP_SMALL = {"download": 1.0, "upload": 1.0, "loss": 0.5}
STEP_LARGE = {"download": 10.0, "upload": 10.0, "loss": 5.0}
MAX_VALUES = {"download": 10_000.0, "upload": 10_000.0, "loss": 100.0}

_STYLES = Path(__file__).with_name("styles.tcss")


class NetLimitApp(App[None]):
    """Full-screen interactive network limiter."""

    TITLE = "NetLimit"
    SUB_TITLE = "system-wide network control"
    CSS_PATH = _STYLES

    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("escape", "quit", "Quit", show=False),
        Binding("a", "apply", "Apply", show=True),
        Binding("r", "reset", "Reset", show=True),
        Binding("i", "cycle_interface", "Interface", show=True),
        Binding("up", "select_prev", "Prev", show=False),
        Binding("down", "select_next", "Next", show=False),
        Binding("left", "adjust(-1, False)", "Dec", show=False),
        Binding("right", "adjust(1, False)", "Inc", show=False),
        Binding("minus", "adjust(-1, False)", "Dec", show=False),
        Binding("plus", "adjust(1, False)", "Inc", show=False),
        Binding("equal", "adjust(1, False)", "Inc", show=False),
        Binding("shift+left", "adjust(-1, True)", "Dec10", show=False),
        Binding("shift+right", "adjust(1, True)", "Inc10", show=False),
        Binding("shift+minus", "adjust(-1, True)", "Dec10", show=False),
        Binding("shift+plus", "adjust(1, True)", "Inc10", show=False),
        Binding("shift+equal", "adjust(1, True)", "Inc10", show=False),
        Binding("d", "focus_metric('download')", "↓", show=False),
        Binding("u", "focus_metric('upload')", "↑", show=False),
        Binding("l", "focus_metric('loss')", "Loss", show=False),
        Binding("tab", "select_next", "Next", show=False),
        Binding("shift+tab", "select_prev", "Prev", show=False),
    ]

    def __init__(
        self,
        interface: str | None = None,
        *,
        initial: NetworkLimits | None = None,
    ) -> None:
        super().__init__()
        self._interfaces = list_interfaces()
        self._controller: TrafficController | None = None
        self._controller_error: str | None = None

        try:
            self._controller = TrafficController(interface=interface)
            current_iface = self._controller.interface
        except NetLimitError as exc:
            self._controller_error = str(exc)
            current_iface = interface or (
                self._interfaces[0] if self._interfaces else ""
            )

        self._iface = current_iface
        if self._iface and self._iface not in self._interfaces:
            self._interfaces.insert(0, self._iface)

        self.download = 0.0
        self.upload = 0.0
        self.loss = 0.0
        self._selected = 0
        self._applied = NetworkLimits(interface=self._iface)

        if initial:
            self.download = float(initial.download_mbps or 0)
            self.upload = float(initial.upload_mbps or 0)
            self.loss = float(initial.loss_percent or 0)

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Container(id="main"):
            yield Static(
                "⚡  NETLIMIT  ·  real-time traffic control",
                id="title-row",
            )
            with Container(id="iface-section"):
                yield InterfaceBar()
            with Horizontal(id="metrics-row"):
                yield MetricCard(
                    "download",
                    "DOWNLOAD",
                    "Mbps  (0 = unlimited)",
                    icon="↓",
                    accent="cyan",
                    value=self.download,
                    id="card-download",
                )
                yield MetricCard(
                    "upload",
                    "UPLOAD",
                    "Mbps  (0 = unlimited)",
                    icon="↑",
                    accent="green",
                    value=self.upload,
                    id="card-upload",
                )
                yield MetricCard(
                    "loss",
                    "PACKET LOSS",
                    "percent",
                    icon="⚠",
                    accent="yellow",
                    value=self.loss,
                    unlimited_at_zero=False,
                    id="card-loss",
                )
            with Vertical(id="status-panel"):
                yield Static("APPLIED SETTINGS", id="status-panel-title")
                yield Static("No limits active", id="applied-summary")
            with Horizontal(id="actions"):
                yield Button("▶  Apply  [a]", id="btn-apply")
                yield Button("↺  Reset  [r]", id="btn-reset")
                yield Button("✕  Quit  [q]", id="btn-quit")
            yield StatusBanner(
                "Ready — adjust values, then press Apply", id="banner"
            )
        yield KeyHelp()

    def on_mount(self) -> None:
        self._sync_cards()
        self._highlight_selected()
        self._update_interface_bar()

        if not is_root():
            self._banner(
                "Not running as root — Apply/Reset need sudo. "
                "Launch with: sudo netlimit",
                "warn",
            )
        elif self._controller_error:
            self._banner(self._controller_error, "error")
        else:
            self._refresh_status_from_system()

    # ------------------------------------------------------------------
    # UI helpers
    # ------------------------------------------------------------------

    def _banner(self, message: str, level: str = "info") -> None:
        self.query_one("#banner", StatusBanner).set_status(message, level)

    def _sync_cards(self) -> None:
        self.query_one("#card-download", MetricCard).value = self.download
        self.query_one("#card-upload", MetricCard).value = self.upload
        self.query_one("#card-loss", MetricCard).value = self.loss

    def _highlight_selected(self) -> None:
        for idx, name in enumerate(METRICS):
            card = self.query_one(f"#card-{name}", MetricCard)
            card.selected = idx == self._selected

    def _update_interface_bar(self) -> None:
        bar = self.query_one(InterfaceBar)
        bar.interface = self._iface or "—"
        bar.state = get_interface_state(self._iface) if self._iface else "missing"

    def _update_applied_panel(self, limits: NetworkLimits) -> None:
        self._applied = limits
        summary = self.query_one("#applied-summary", Static)
        if not limits.is_active:
            summary.update(
                f"No limits active on [b]{limits.interface or self._iface or '?'}[/b]"
            )
        else:
            summary.update(
                f"[b]{limits.interface}[/b]   "
                f"↓ [cyan]{format_rate(limits.download_mbps)}[/]   "
                f"↑ [green]{format_rate(limits.upload_mbps)}[/]   "
                f"loss [yellow]{format_loss(limits.loss_percent)}[/]"
            )

    def _draft_limits(self) -> NetworkLimits:
        return NetworkLimits(
            download_mbps=self.download if self.download > 0 else None,
            upload_mbps=self.upload if self.upload > 0 else None,
            loss_percent=self.loss,
            interface=self._iface,
        )

    def _current_metric_name(self) -> str:
        return METRICS[self._selected]

    def _get_metric_value(self, name: str) -> float:
        return float(getattr(self, name))

    def _set_metric_value(self, name: str, value: float) -> None:
        setattr(self, name, value)
        self.query_one(f"#card-{name}", MetricCard).value = value

    def _refresh_status_from_system(self) -> None:
        if not self._controller:
            return
        try:
            limits = self._controller.status()
            self._update_applied_panel(limits)
            if self.download == 0 and self.upload == 0 and self.loss == 0:
                self.download = float(limits.download_mbps or 0)
                self.upload = float(limits.upload_mbps or 0)
                self.loss = float(limits.loss_percent or 0)
                self._sync_cards()
            if limits.is_active:
                self._banner(f"Loaded active rules: {limits.summary()}", "info")
            else:
                self._banner("Ready — adjust values, then press Apply", "info")
        except NetLimitError as exc:
            self._banner(str(exc), "error")

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    def action_select_next(self) -> None:
        self._selected = (self._selected + 1) % len(METRICS)
        self._highlight_selected()

    def action_select_prev(self) -> None:
        self._selected = (self._selected - 1) % len(METRICS)
        self._highlight_selected()

    def action_focus_metric(self, name: str) -> None:
        if name in METRICS:
            self._selected = METRICS.index(name)
            self._highlight_selected()

    def action_adjust(self, direction: int, coarse: bool = False) -> None:
        name = self._current_metric_name()
        current = self._get_metric_value(name)
        new_val = step_value(
            current,
            direction,
            small=STEP_SMALL[name],
            large=STEP_LARGE[name],
            coarse=coarse,
            minimum=0.0,
            maximum=MAX_VALUES[name],
        )
        self._set_metric_value(name, new_val)

    def action_cycle_interface(self) -> None:
        self._cycle_interface(+1)

    def action_apply(self) -> None:
        self._do_apply()

    def action_reset(self) -> None:
        self._do_reset()

    # ------------------------------------------------------------------
    # Events
    # ------------------------------------------------------------------

    @on(MetricCard.Adjusted)
    def _on_metric_adjusted(self, event: MetricCard.Adjusted) -> None:
        if event.metric in METRICS:
            self._selected = METRICS.index(event.metric)
            self._highlight_selected()
            current = self._get_metric_value(event.metric)
            new_val = step_value(
                current,
                event.direction,
                small=STEP_SMALL[event.metric],
                large=STEP_LARGE[event.metric],
                coarse=event.coarse,
                minimum=0.0,
                maximum=MAX_VALUES[event.metric],
            )
            self._set_metric_value(event.metric, new_val)

    @on(InterfaceBar.Cycle)
    def _on_iface_cycle(self, event: InterfaceBar.Cycle) -> None:
        self._cycle_interface(event.direction)

    def _cycle_interface(self, direction: int) -> None:
        if not self._interfaces:
            self._banner("No network interfaces found", "error")
            return
        try:
            idx = self._interfaces.index(self._iface)
        except ValueError:
            idx = 0
        idx = (idx + direction) % len(self._interfaces)
        new_iface = self._interfaces[idx]
        self._iface = new_iface
        if self._controller:
            try:
                self._controller.set_interface(new_iface)
            except NetLimitError as exc:
                self._banner(str(exc), "error")
                return
        self._update_interface_bar()
        self._banner(f"Interface set to {new_iface} (not yet applied)", "info")
        if self._controller and is_root():
            self._refresh_status_from_system()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        bid = event.button.id
        if bid == "btn-apply":
            self._do_apply()
        elif bid == "btn-reset":
            self._do_reset()
        elif bid == "btn-quit":
            self.exit()

    # ------------------------------------------------------------------
    # Apply / Reset (worker threads so UI stays responsive)
    # ------------------------------------------------------------------

    @work(exclusive=True, thread=True)
    def _do_apply(self) -> None:
        if not is_root():
            self.call_from_thread(
                self._banner,
                "Root required. Re-run: sudo netlimit",
                "error",
            )
            return
        if not self._controller:
            self.call_from_thread(
                self._banner,
                self._controller_error or "Traffic controller unavailable",
                "error",
            )
            return

        limits = self._draft_limits()
        self.call_from_thread(self._banner, "Applying limits…", "info")
        result = self._controller.apply(limits)

        def finish() -> None:
            if result.success:
                self._update_applied_panel(result.limits)
                self._banner(result.message, "success")
            else:
                self._banner(result.message, "error")

        self.call_from_thread(finish)

    @work(exclusive=True, thread=True)
    def _do_reset(self) -> None:
        if not is_root():
            self.call_from_thread(
                self._banner,
                "Root required. Re-run: sudo netlimit",
                "error",
            )
            return
        if not self._controller:
            self.call_from_thread(
                self._banner,
                self._controller_error or "Traffic controller unavailable",
                "error",
            )
            return

        self.call_from_thread(self._banner, "Resetting…", "info")
        result = self._controller.reset()

        def finish() -> None:
            if result.success:
                self.download = 0.0
                self.upload = 0.0
                self.loss = 0.0
                self._sync_cards()
                self._update_applied_panel(result.limits)
                self._banner(result.message, "success")
            else:
                self._banner(result.message, "error")

        self.call_from_thread(finish)


def run_tui(interface: str | None = None) -> None:
    """Launch the interactive NetLimit application."""
    NetLimitApp(interface=interface).run()
