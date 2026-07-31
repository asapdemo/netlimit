"""Custom Textual widgets for the NetLimit TUI."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.message import Message
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Button, Label, Static


class MetricCard(Static):
    """Prominent card showing one network metric with +/- controls."""

    value: reactive[float] = reactive(0.0)
    selected: reactive[bool] = reactive(False)

    class Adjusted(Message):
        """Posted when the user requests a value change via buttons."""

        def __init__(self, metric: str, direction: int, coarse: bool = False) -> None:
            self.metric = metric
            self.direction = direction
            self.coarse = coarse
            super().__init__()

    def __init__(
        self,
        metric_id: str,
        title: str,
        unit: str,
        *,
        icon: str = "●",
        accent: str = "cyan",
        value: float = 0.0,
        unlimited_at_zero: bool = True,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)
        self.metric_id = metric_id
        self.card_title = title
        self.unit = unit
        self.icon = icon
        self.accent = accent
        self.unlimited_at_zero = unlimited_at_zero
        self.value = value

    def compose(self) -> ComposeResult:
        with Vertical(classes="metric-inner"):
            yield Label(f"{self.icon}  {self.card_title}", classes="metric-title")
            yield Static(
                self._format_value(),
                id=f"val-{self.metric_id}",
                classes="metric-value",
            )
            yield Label(self.unit, classes="metric-unit")
            with Horizontal(classes="metric-controls"):
                yield Button("−", id=f"dec-{self.metric_id}", classes="metric-btn")
                yield Button("+", id=f"inc-{self.metric_id}", classes="metric-btn")

    def watch_value(self, value: float) -> None:
        try:
            label = self.query_one(f"#val-{self.metric_id}", Static)
            label.update(self._format_value())
        except Exception:
            pass

    def watch_selected(self, selected: bool) -> None:
        self.set_class(selected, "selected")

    def _format_value(self) -> str:
        if self.unlimited_at_zero and self.value <= 0:
            return "∞"
        if self.value == int(self.value):
            return str(int(self.value))
        return f"{self.value:.1f}"

    def on_button_pressed(self, event: Button.Pressed) -> None:
        button_id = event.button.id or ""
        if button_id.startswith("inc-"):
            self.post_message(self.Adjusted(self.metric_id, +1))
        elif button_id.startswith("dec-"):
            self.post_message(self.Adjusted(self.metric_id, -1))


class StatusBanner(Static):
    """Status / confirmation banner at the bottom of the main panel."""

    level: reactive[str] = reactive("info")  # info | success | error | warn

    def __init__(self, message: str = "Ready", **kwargs: object) -> None:
        super().__init__(message, **kwargs)
        self._message = message

    def set_status(self, message: str, level: str = "info") -> None:
        self._message = message
        self.level = level
        self.update(message)
        self.remove_class("success", "error", "warn", "info")
        self.add_class(level)

    def watch_level(self, level: str) -> None:
        self.remove_class("success", "error", "warn", "info")
        self.add_class(level)


class KeyHelp(Static):
    """Footer keybinding help strip."""

    def __init__(self, **kwargs: object) -> None:
        text = (
            "[bold]↑↓[/] select   "
            "[bold]+/−[/] adjust   "
            "[bold]Shift+±[/] coarse   "
            "[bold]a[/] apply   "
            "[bold]r[/] reset   "
            "[bold]i[/] interface   "
            "[bold]q[/] quit"
        )
        super().__init__(text, **kwargs)


class InterfaceBar(Widget):
    """Shows the active interface and allows cycling."""

    interface: reactive[str] = reactive("")
    state: reactive[str] = reactive("unknown")

    class Cycle(Message):
        """Request to cycle to the next interface."""

        def __init__(self, direction: int = 1) -> None:
            self.direction = direction
            super().__init__()

    def compose(self) -> ComposeResult:
        with Horizontal(classes="iface-bar-inner"):
            yield Label("INTERFACE", classes="iface-label")
            yield Static("—", id="iface-name", classes="iface-name")
            yield Static("", id="iface-state", classes="iface-state")
            yield Button("⟨", id="iface-prev", classes="iface-btn")
            yield Button("⟩", id="iface-next", classes="iface-btn")

    def watch_interface(self, interface: str) -> None:
        try:
            self.query_one("#iface-name", Static).update(interface or "—")
        except Exception:
            pass

    def watch_state(self, state: str) -> None:
        try:
            widget = self.query_one("#iface-state", Static)
            widget.update(f"[{state}]" if state else "")
            widget.set_class(state == "up", "state-up")
            widget.set_class(state != "up", "state-down")
        except Exception:
            pass

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "iface-next":
            self.post_message(self.Cycle(+1))
        elif event.button.id == "iface-prev":
            self.post_message(self.Cycle(-1))
