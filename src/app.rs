//! Application state and event handling.

use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::DefaultTerminal;

use std::sync::mpsc::Receiver;

use crate::monitor::{PingMonitor, SampleHistory, ThroughputMonitor};
use crate::netinfo::list_interfaces;
use crate::presets::{
    self, all_presets, save_user_presets, slider_max, slider_ratio_to_value, Preset,
};
use crate::speedtest::{self, SpeedTestEvent, SpeedTestResult};
use crate::tc::{is_root, Limits, Metric, TcError, TrafficController};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerLevel {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Banner {
    pub message: String,
    pub level: BannerLevel,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricHits {
    pub card: Rect,
    pub dec: Rect,
    pub inc: Rect,
    pub slider: Rect,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PresetHits {
    /// Click to load this preset.
    pub load: Rect,
    /// Click to delete (custom presets only).
    pub delete: Option<Rect>,
}

pub struct App {
    pub should_quit: bool,
    pub selected: Metric,
    pub download: f64,
    pub upload: f64,
    pub loss: f64,
    pub interfaces: Vec<String>,
    pub iface_idx: usize,
    pub applied: Limits,
    pub banner: Banner,
    pub is_root: bool,
    pub busy: bool,
    pub presets: Vec<Preset>,
    /// Which preset chip is highlighted (−1 = none)
    pub selected_preset: Option<usize>,
    controller: Option<TrafficController>,
    controller_error: Option<String>,
    /// Hit boxes updated each frame for mouse support.
    pub hit_metrics: [MetricHits; 3],
    pub hit_apply: Rect,
    pub hit_reset: Rect,
    pub hit_speedtest: Rect,
    pub hit_quit: Rect,
    /// Clickable rows in the interface list (parallel to `interfaces`).
    pub hit_ifaces: Vec<Rect>,
    pub hit_presets: Vec<PresetHits>,
    pub hit_save_preset: Rect,
    /// Scroll offset when the interface list is taller than the panel.
    pub iface_scroll: usize,
    /// Active slider drag (metric being scrubbed).
    pub dragging: Option<Metric>,
    /// Live ↓/↑ from /proc/net/dev + path loss from ping.
    pub throughput: ThroughputMonitor,
    pub ping: PingMonitor,
    pub history: SampleHistory,
    pub speedtest_running: bool,
    pub speedtest_phase: String,
    pub speedtest_detail: String,
    pub last_speedtest: Option<SpeedTestResult>,
    speedtest_rx: Option<Receiver<SpeedTestEvent>>,
    last_sample_at: Instant,
    tick: u64,
}

impl App {
    pub fn new(interface: Option<String>) -> Self {
        let interfaces = list_interfaces();
        let mut controller_error = None;
        let controller = match TrafficController::new(interface.clone()) {
            Ok(c) => Some(c),
            Err(e) => {
                controller_error = Some(e.to_string());
                None
            }
        };

        let iface = controller
            .as_ref()
            .map(|c| c.interface.clone())
            .or(interface)
            .or_else(|| interfaces.first().cloned())
            .unwrap_or_default();

        let mut interfaces = interfaces;
        if !iface.is_empty() && !interfaces.iter().any(|i| i == &iface) {
            interfaces.insert(0, iface.clone());
        }
        let iface_idx = interfaces.iter().position(|i| i == &iface).unwrap_or(0);

        let mut app = Self {
            should_quit: false,
            selected: Metric::Download,
            download: 0.0,
            upload: 0.0,
            loss: 0.0,
            interfaces,
            iface_idx,
            applied: Limits {
                interface: iface,
                ..Default::default()
            },
            banner: Banner {
                message: "Ready — use − / + , sliders, or presets".into(),
                level: BannerLevel::Info,
            },
            is_root: is_root(),
            busy: false,
            presets: all_presets(),
            selected_preset: None,
            controller,
            controller_error,
            hit_metrics: [MetricHits::default(); 3],
            hit_apply: Rect::default(),
            hit_reset: Rect::default(),
            hit_speedtest: Rect::default(),
            hit_quit: Rect::default(),
            hit_ifaces: Vec::new(),
            hit_presets: Vec::new(),
            hit_save_preset: Rect::default(),
            iface_scroll: 0,
            dragging: None,
            throughput: ThroughputMonitor::default(),
            ping: PingMonitor::default(),
            history: SampleHistory::default(),
            speedtest_running: false,
            speedtest_phase: String::new(),
            speedtest_detail: String::new(),
            last_speedtest: None,
            speedtest_rx: None,
            last_sample_at: Instant::now(),
            tick: 0,
        };

        if !app.is_root {
            app.set_banner(
                "Not root — Apply/Reset need privileges. Launch with sudo.",
                BannerLevel::Warn,
            );
        } else if let Some(err) = app.controller_error.clone() {
            app.set_banner(err, BannerLevel::Error);
        } else {
            app.refresh_from_system();
        }

        app
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        let tick_rate = Duration::from_millis(50);
        let mut last_tick = Instant::now();

        while !self.should_quit {
            self.poll_speedtest();
            self.sample_monitors();

            terminal.draw(|frame| ui::draw(frame, &mut self))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
                    Event::Mouse(mouse) => self.on_mouse(mouse),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_rate {
                self.tick = self.tick.wrapping_add(1);
                last_tick = Instant::now();
            }
        }
        Ok(())
    }

    fn sample_monitors(&mut self) {
        self.ping.poll();
        let now = Instant::now();
        // Throughput + history every 500ms
        if now.duration_since(self.last_sample_at) >= Duration::from_millis(500) {
            let iface = self.current_iface().to_string();
            self.throughput.tick(&iface);
            self.history.push(
                self.throughput.down_mbps,
                self.throughput.up_mbps,
                self.ping.loss_percent,
            );
            self.last_sample_at = now;
        }
    }

    fn poll_speedtest(&mut self) {
        loop {
            let event = {
                let Some(rx) = self.speedtest_rx.as_ref() else {
                    return;
                };
                match rx.try_recv() {
                    Ok(ev) => ev,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if self.speedtest_running {
                            self.speedtest_running = false;
                            self.speedtest_rx = None;
                            self.set_banner(
                                "Speed test worker ended unexpectedly",
                                BannerLevel::Error,
                            );
                        }
                        return;
                    }
                }
            };

            match event {
                SpeedTestEvent::Progress { phase, detail } => {
                    self.speedtest_phase = phase.clone();
                    self.speedtest_detail = detail.clone();
                    self.set_banner(
                        format!("Speed test [{phase}]: {detail}"),
                        BannerLevel::Info,
                    );
                }
                SpeedTestEvent::Finished {
                    download_mbps,
                    upload_mbps,
                    latency_ms,
                    jitter_ms,
                } => {
                    self.speedtest_running = false;
                    self.speedtest_rx = None;
                    self.speedtest_phase = "done".into();
                    self.speedtest_detail = format!(
                        "↓ {download_mbps:.1}  ↑ {upload_mbps:.1}  lat {latency_ms:.0}ms  jit {jitter_ms:.0}ms"
                    );
                    self.last_speedtest = Some(SpeedTestResult {
                        download_mbps,
                        upload_mbps,
                        latency_ms,
                        jitter_ms,
                        at: Some(Instant::now()),
                    });
                    self.set_banner(
                        format!(
                            "✓ Cloudflare: ↓ {download_mbps:.1} Mbps  ↑ {upload_mbps:.1} Mbps  ·  {latency_ms:.0} ms ± {jitter_ms:.0}"
                        ),
                        BannerLevel::Success,
                    );
                    return;
                }
                SpeedTestEvent::Failed(err) => {
                    self.speedtest_running = false;
                    self.speedtest_rx = None;
                    self.speedtest_phase = "error".into();
                    self.speedtest_detail = err.clone();
                    self.set_banner(format!("✗ Speed test failed: {err}"), BannerLevel::Error);
                    return;
                }
            }
        }
    }

    fn start_speedtest(&mut self) {
        if self.speedtest_running {
            self.set_banner("Speed test already running…", BannerLevel::Warn);
            return;
        }
        self.speedtest_running = true;
        self.speedtest_phase = "starting".into();
        self.speedtest_detail = "connecting to Cloudflare…".into();
        self.speedtest_rx = Some(speedtest::start());
        self.set_banner(
            "Cloudflare speed test started (runs in background)…",
            BannerLevel::Info,
        );
    }

    pub fn current_iface(&self) -> &str {
        self.interfaces
            .get(self.iface_idx)
            .map(|s| s.as_str())
            .unwrap_or("—")
    }

    pub fn metric_value(&self, m: Metric) -> f64 {
        match m {
            Metric::Download => self.download,
            Metric::Upload => self.upload,
            Metric::Loss => self.loss,
        }
    }

    pub fn set_metric_value(&mut self, m: Metric, v: f64) {
        // Allow keys/buttons above slider max; clamp to metric absolute max.
        let v = v.clamp(0.0, m.max());
        let v = match m {
            Metric::Loss => (v * 10.0).round() / 10.0,
            Metric::Download | Metric::Upload => {
                if (v - v.round()).abs() < 1e-9 {
                    v.round()
                } else {
                    (v * 10.0).round() / 10.0
                }
            }
        };
        match m {
            Metric::Download => self.download = v,
            Metric::Upload => self.upload = v,
            Metric::Loss => self.loss = v,
        }
        self.selected_preset = None;
    }

    fn set_banner(&mut self, message: impl Into<String>, level: BannerLevel) {
        self.banner = Banner {
            message: message.into(),
            level,
        };
    }

    fn draft_limits(&self) -> Limits {
        Limits {
            download_mbps: self.download,
            upload_mbps: self.upload,
            loss_percent: self.loss,
            interface: self.current_iface().to_string(),
        }
        .normalized()
    }

    fn refresh_from_system(&mut self) {
        let Some(ctrl) = self.controller.as_ref() else {
            return;
        };
        let limits = ctrl.status();
        self.applied = limits.clone();
        if self.download == 0.0 && self.upload == 0.0 && self.loss == 0.0 {
            self.download = limits.download_mbps;
            self.upload = limits.upload_mbps;
            self.loss = limits.loss_percent;
        }
        if limits.is_active() {
            self.set_banner(
                format!("Loaded active rules: {}", limits.summary()),
                BannerLevel::Info,
            );
        } else {
            self.set_banner(
                "Ready — use − / + , sliders, or presets",
                BannerLevel::Info,
            );
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        let coarse = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('a') => self.do_apply(),
            KeyCode::Char('r') => self.do_reset(),
            KeyCode::Char('t') => self.start_speedtest(),
            KeyCode::Char('i') => self.cycle_iface(1),
            KeyCode::Char('d') => self.selected = Metric::Download,
            KeyCode::Char('u') => self.selected = Metric::Upload,
            KeyCode::Char('l') => self.selected = Metric::Loss,
            KeyCode::Char('s') => self.save_current_as_preset(),
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                self.delete_selected_preset();
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = Metric::from_index(self.selected.index() + 2);
            }
            KeyCode::Down | KeyCode::Tab => {
                self.selected = Metric::from_index(self.selected.index() + 1);
            }
            KeyCode::Left | KeyCode::Char('-') | KeyCode::Char('_') => {
                self.adjust(-1, coarse || key.code == KeyCode::Char('_'));
            }
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.adjust(1, coarse);
            }
            KeyCode::Char(']') => self.cycle_iface(1),
            KeyCode::Char('[') => self.cycle_iface(-1),
            KeyCode::PageDown => self.cycle_iface(1),
            KeyCode::PageUp => self.cycle_iface(-1),
            KeyCode::Char('0') => self.set_metric_value(self.selected, 0.0),
            // Number keys 1–9 load presets
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let idx = (c as u8 - b'1') as usize;
                self.apply_preset(idx);
            }
            _ => {}
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        let col = mouse.column;
        let row = mouse.row;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // ± buttons first (more specific hit targets)
                for (i, hits) in self.hit_metrics.iter().enumerate() {
                    let metric = Metric::from_index(i);
                    if contains(hits.dec, col, row) {
                        self.selected = metric;
                        self.adjust(-1, false);
                        return;
                    }
                    if contains(hits.inc, col, row) {
                        self.selected = metric;
                        self.adjust(1, false);
                        return;
                    }
                    if contains(hits.slider, col, row) {
                        self.selected = metric;
                        self.dragging = Some(metric);
                        self.set_from_slider(metric, col, hits.slider);
                        return;
                    }
                    if contains(hits.card, col, row) {
                        self.selected = metric;
                        return;
                    }
                }

                for (i, hits) in self.hit_presets.iter().enumerate() {
                    if let Some(del) = hits.delete {
                        if contains(del, col, row) {
                            self.delete_preset_at(i);
                            return;
                        }
                    }
                    if contains(hits.load, col, row) {
                        self.apply_preset(i);
                        return;
                    }
                }
                if contains(self.hit_save_preset, col, row) {
                    self.save_current_as_preset();
                    return;
                }
                for (i, rect) in self.hit_ifaces.iter().enumerate() {
                    if contains(*rect, col, row) {
                        // hit_ifaces are only visible rows; map via scroll
                        let idx = self.iface_scroll + i;
                        self.select_iface(idx);
                        return;
                    }
                }
                if contains(self.hit_apply, col, row) {
                    self.do_apply();
                } else if contains(self.hit_reset, col, row) {
                    self.do_reset();
                } else if contains(self.hit_speedtest, col, row) {
                    self.start_speedtest();
                } else if contains(self.hit_quit, col, row) {
                    self.should_quit = true;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(metric) = self.dragging {
                    let track = self.hit_metrics[metric.index()].slider;
                    if track.width > 0 {
                        self.set_from_slider(metric, col, track);
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging = None;
            }
            // Scroll wheel over selected metric card
            MouseEventKind::ScrollUp => {
                if let Some(m) = self.metric_at(col, row) {
                    self.selected = m;
                    self.adjust(1, false);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(m) = self.metric_at(col, row) {
                    self.selected = m;
                    self.adjust(-1, false);
                }
            }
            _ => {}
        }
    }

    fn metric_at(&self, col: u16, row: u16) -> Option<Metric> {
        for (i, hits) in self.hit_metrics.iter().enumerate() {
            if contains(hits.card, col, row) {
                return Some(Metric::from_index(i));
            }
        }
        None
    }

    fn set_from_slider(&mut self, metric: Metric, col: u16, track: Rect) {
        if track.width == 0 {
            return;
        }
        let rel = if col <= track.x {
            0.0
        } else if col >= track.x.saturating_add(track.width.saturating_sub(1)) {
            1.0
        } else {
            (col.saturating_sub(track.x) as f64) / (track.width.saturating_sub(1) as f64)
        };
        let value = slider_ratio_to_value(metric, rel);
        // If user drags to max of slider, keep ability to go higher via buttons —
        // at full right set exactly slider_max.
        let value = value.min(slider_max(metric));
        self.set_metric_value(metric, value);
    }

    fn adjust(&mut self, direction: i32, coarse: bool) {
        let m = self.selected;
        let step = if coarse {
            m.large_step()
        } else {
            m.small_step()
        };
        let cur = self.metric_value(m);
        self.set_metric_value(m, cur + direction as f64 * step);
    }

    fn apply_preset(&mut self, idx: usize) {
        let Some(preset) = self.presets.get(idx).cloned() else {
            self.set_banner(format!("No preset #{}", idx + 1), BannerLevel::Warn);
            return;
        };
        self.download = preset.download_mbps;
        self.upload = preset.upload_mbps;
        self.loss = preset.loss_percent;
        self.selected_preset = Some(idx);
        self.set_banner(
            format!(
                "Preset “{}” loaded ({}) — press Apply to enforce",
                preset.name,
                preset.summary()
            ),
            BannerLevel::Info,
        );
    }

    fn save_current_as_preset(&mut self) {
        let n = self.presets.iter().filter(|p| !p.builtin).count() + 1;
        let name = format!("Custom {n}");
        let preset = Preset::new(name.clone(), self.download, self.upload, self.loss);
        self.presets.push(preset);
        match save_user_presets(&self.presets) {
            Ok(()) => {
                let idx = self.presets.len() - 1;
                self.selected_preset = Some(idx);
                self.set_banner(
                    format!(
                        "Saved preset “{name}” → {}  (x/Del to remove)",
                        presets::presets_path().display()
                    ),
                    BannerLevel::Success,
                );
            }
            Err(e) => {
                // roll back push
                self.presets.pop();
                self.set_banner(format!("Failed to save preset: {e}"), BannerLevel::Error);
            }
        }
    }

    fn delete_selected_preset(&mut self) {
        match self.selected_preset {
            Some(idx) => self.delete_preset_at(idx),
            None => self.set_banner(
                "Select a custom preset first (click it), then press x / Del",
                BannerLevel::Warn,
            ),
        }
    }

    fn delete_preset_at(&mut self, idx: usize) {
        let Some(preset) = self.presets.get(idx) else {
            self.set_banner("Preset not found", BannerLevel::Warn);
            return;
        };
        if preset.builtin {
            self.set_banner(
                format!("“{}” is built-in and cannot be deleted", preset.name),
                BannerLevel::Warn,
            );
            return;
        }
        let name = preset.name.clone();
        self.presets.remove(idx);
        // Fix selection index after removal
        self.selected_preset = match self.selected_preset {
            Some(s) if s == idx => None,
            Some(s) if s > idx => Some(s - 1),
            other => other,
        };
        match save_user_presets(&self.presets) {
            Ok(()) => {
                self.set_banner(format!("Deleted preset “{name}”"), BannerLevel::Success);
            }
            Err(e) => {
                self.set_banner(format!("Deleted in UI but failed to save: {e}"), BannerLevel::Error);
            }
        }
    }

    fn cycle_iface(&mut self, dir: i32) {
        if self.interfaces.is_empty() {
            self.set_banner("No network interfaces found", BannerLevel::Error);
            return;
        }
        let n = self.interfaces.len() as i32;
        let idx = ((self.iface_idx as i32 + dir).rem_euclid(n)) as usize;
        self.select_iface(idx);
    }

    pub fn select_iface(&mut self, idx: usize) {
        if idx >= self.interfaces.len() {
            return;
        }
        self.iface_idx = idx;
        self.throughput.reset();
        let name = self.current_iface().to_string();
        if let Some(ctrl) = self.controller.as_mut() {
            if let Err(e) = ctrl.set_interface(name.clone()) {
                self.set_banner(e.to_string(), BannerLevel::Error);
                return;
            }
        }
        self.set_banner(
            format!("Interface set to {name} (not yet applied)"),
            BannerLevel::Info,
        );
        if self.is_root {
            self.refresh_from_system();
        }
    }

    /// Ensure `iface_idx` is within the visible window of `visible_rows`.
    pub fn ensure_iface_visible(&mut self, visible_rows: usize) {
        if visible_rows == 0 || self.interfaces.is_empty() {
            return;
        }
        if self.iface_idx < self.iface_scroll {
            self.iface_scroll = self.iface_idx;
        } else if self.iface_idx >= self.iface_scroll + visible_rows {
            self.iface_scroll = self.iface_idx + 1 - visible_rows;
        }
        let max_scroll = self.interfaces.len().saturating_sub(visible_rows);
        if self.iface_scroll > max_scroll {
            self.iface_scroll = max_scroll;
        }
    }

    fn do_apply(&mut self) {
        if !self.is_root {
            self.set_banner(
                "Root required. Re-run with: sudo netlimit",
                BannerLevel::Error,
            );
            return;
        }
        if self.controller.is_none() {
            self.set_banner(
                self.controller_error
                    .clone()
                    .unwrap_or_else(|| "controller unavailable".into()),
                BannerLevel::Error,
            );
            return;
        }

        self.busy = true;
        self.set_banner("Applying limits…", BannerLevel::Info);
        let limits = self.draft_limits();
        let result = self.controller.as_ref().unwrap().apply(limits);
        match result {
            Ok(applied) => {
                self.applied = applied.clone();
                self.set_banner(
                    format!("✓ Applied: {}", applied.summary()),
                    BannerLevel::Success,
                );
            }
            Err(TcError::NotRoot) => {
                self.set_banner("Root required", BannerLevel::Error);
            }
            Err(e) => {
                self.set_banner(format!("✗ {e}"), BannerLevel::Error);
            }
        }
        self.busy = false;
    }

    fn do_reset(&mut self) {
        if !self.is_root {
            self.set_banner(
                "Root required. Re-run with: sudo netlimit",
                BannerLevel::Error,
            );
            return;
        }
        if self.controller.is_none() {
            self.set_banner(
                self.controller_error
                    .clone()
                    .unwrap_or_else(|| "controller unavailable".into()),
                BannerLevel::Error,
            );
            return;
        }

        self.busy = true;
        self.set_banner("Resetting…", BannerLevel::Info);
        let result = self.controller.as_ref().unwrap().reset();
        match result {
            Ok(()) => {
                self.download = 0.0;
                self.upload = 0.0;
                self.loss = 0.0;
                self.selected_preset = self
                    .presets
                    .iter()
                    .position(|p| p.download_mbps == 0.0 && p.upload_mbps == 0.0 && p.loss_percent == 0.0);
                self.applied = Limits {
                    interface: self.current_iface().to_string(),
                    ..Default::default()
                };
                self.set_banner(
                    format!(
                        "✓ Reset complete on {} (all limits removed)",
                        self.current_iface()
                    ),
                    BannerLevel::Success,
                );
            }
            Err(e) => self.set_banner(format!("✗ {e}"), BannerLevel::Error),
        }
        self.busy = false;
    }

    /// True when draft (UI) values differ from last successfully applied rules.
    pub fn draft_differs_from_applied(&self) -> bool {
        let draft = self.draft_limits();
        let a = &self.applied;
        // Compare rates with small epsilon; treat 0 as unlimited both sides.
        let same_dl = (draft.download_mbps - a.download_mbps).abs() < 0.05;
        let same_ul = (draft.upload_mbps - a.upload_mbps).abs() < 0.05;
        let same_loss = (draft.loss_percent - a.loss_percent).abs() < 0.05;
        let same_iface = draft.interface == a.interface
            || (a.interface.is_empty() && draft.interface == self.current_iface());
        !(same_dl && same_ul && same_loss && same_iface)
    }
}

fn contains(r: Rect, col: u16, row: u16) -> bool {
    r.width > 0
        && r.height > 0
        && col >= r.x
        && col < r.x.saturating_add(r.width)
        && row >= r.y
        && row < r.y.saturating_add(r.height)
}
