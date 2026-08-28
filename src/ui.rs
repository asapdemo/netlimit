//! btop-style dense dashboard rendering.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph, Sparkline, Wrap};
use ratatui::Frame;

use crate::app::{App, BannerLevel, MetricHits, PresetHits, Screen};
use crate::tc::{format_loss, format_ms, format_rate};
use crate::presets::{slider_max, value_to_slider_ratio};
use crate::tc::{format_value, Metric};
use crate::theme;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

    match app.screen {
        Screen::Main => draw_main(frame, area, app),
        Screen::SpeedTest => draw_speedtest_screen(frame, area, app),
        Screen::History => draw_history_screen(frame, area, app),
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // top: interfaces (left 50%) + applied (right 50%)
            Constraint::Length(5),  // presets
            Constraint::Length(5),  // metrics
            Constraint::Min(10),    // path quality (taller)
            Constraint::Length(3),  // actions
            Constraint::Length(2),  // banner
            Constraint::Length(1),  // keys
        ])
        .split(area);

    // Top row: networks | applied limits
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[0]);
    draw_interface(frame, top[0], app);
    draw_applied(frame, top[1], app);

    draw_presets(frame, root[1], app);
    draw_metrics(frame, root[2], app);
    draw_ping_panel(frame, root[3], app);
    draw_actions(frame, root[4], app);
    draw_banner(frame, root[5], app);
    draw_keys_main(frame, root[6]);
}

fn draw_interface(frame: &mut Frame, area: Rect, app: &mut App) {
    let n = app.interfaces.len();
    let priv_label = if app.is_root { "root" } else { "user" };
    let title = if n == 0 {
        format!(" NetLimit  ·  INTERFACE  ·  {priv_label} ")
    } else {
        format!(" NetLimit  ·  INTERFACE  ·  {n} found  ·  {priv_label} ")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " click row to select  ·  [ / ] or i to cycle ",
            Style::default().fg(theme::TEXT_MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    app.hit_ifaces.clear();

    if inner.height == 0 || inner.width < 8 {
        return;
    }

    if app.interfaces.is_empty() {
        frame.render_widget(
            Paragraph::new("  No interfaces found")
                .style(Style::default().fg(theme::ERROR).bg(theme::SURFACE)),
            inner,
        );
        return;
    }

    let visible = inner.height as usize;
    app.ensure_iface_visible(visible);
    let scroll = app.iface_scroll;
    let end = (scroll + visible).min(app.interfaces.len());

    // Header column labels
    // name | state | flags
    let default_name = crate::netinfo::default_interface()
        .ok()
        .flatten()
        .unwrap_or_default();

    for (row_i, idx) in (scroll..end).enumerate() {
        let name = &app.interfaces[idx];
        let state = crate::netinfo::interface_state(name);
        let selected = idx == app.iface_idx;
        let is_default = name.as_str() == default_name;

        let row = Rect {
            x: inner.x,
            y: inner.y + row_i as u16,
            width: inner.width,
            height: 1,
        };
        app.hit_ifaces.push(row);

        let state_color = match state.as_str() {
            "up" => theme::SUCCESS,
            "down" => theme::ERROR,
            _ => theme::WARN,
        };

        let (bg, name_fg, marker) = if selected {
            (theme::SURFACE_ALT, theme::ACCENT, "▶")
        } else {
            (theme::SURFACE, theme::TEXT, " ")
        };

        let mut spans = vec![
            Span::styled(
                format!(" {marker} "),
                Style::default().fg(theme::ACCENT).bg(bg),
            ),
            Span::styled(
                format!("{name:<18}"),
                Style::default()
                    .fg(name_fg)
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(
                format!("● {state:<8}"),
                Style::default().fg(state_color).bg(bg),
            ),
        ];
        if is_default {
            spans.push(Span::styled(
                "  ★ default",
                Style::default().fg(theme::WARN).bg(bg),
            ));
        }
        if selected {
            spans.push(Span::styled(
                "  ← active",
                Style::default().fg(theme::ACCENT).bg(bg),
            ));
        }

        // Fill rest of row with background
        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(bg)),
            row,
        );
    }

    // Scroll indicators when list is longer than panel
    if scroll > 0 {
        frame.render_widget(
            Paragraph::new("▲ more")
                .alignment(Alignment::Right)
                .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
            Rect {
                x: inner.x + inner.width.saturating_sub(8),
                y: inner.y,
                width: 8.min(inner.width),
                height: 1,
            },
        );
    }
    if end < app.interfaces.len() {
        frame.render_widget(
            Paragraph::new("▼ more")
                .alignment(Alignment::Right)
                .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
            Rect {
                x: inner.x + inner.width.saturating_sub(8),
                y: inner.y + inner.height.saturating_sub(1),
                width: 8.min(inner.width),
                height: 1,
            },
        );
    }
}

fn draw_presets(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            " PRESETS ",
            Style::default()
                .fg(theme::TEXT_DIM)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " 1-9 load  ·  s save  ·  × / x / Del remove custom ",
            Style::default().fg(theme::TEXT_MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height == 0 {
        app.hit_presets.clear();
        app.hit_save_preset = Rect::default();
        return;
    }

    let save_w: u16 = 10;
    let gap: u16 = 1;
    let chip_h: u16 = inner.height.min(3).max(1);
    let content_w = inner.width.saturating_sub(save_w + gap + 1);
    let max_rows = (inner.height / chip_h.max(1)).max(1);

    let mut x = inner.x;
    let mut y = inner.y;
    let mut row = 0u16;
    app.hit_presets.clear();

    for (i, preset) in app.presets.iter().enumerate() {
        // Full name always: "1 No limits"
        let label = format!("{} {}", i + 1, preset.name);
        // Width = text + padding + borders (leave headroom so names never clip)
        let load_w = (label.chars().count() as u16 + 6).clamp(14, 28);
        let del_extra: u16 = if preset.builtin { 0 } else { 4 };
        let need = load_w.saturating_add(del_extra).saturating_add(1);

        if x + need > inner.x + content_w {
            row += 1;
            if row >= max_rows {
                break;
            }
            x = inner.x;
            y = inner.y + row * chip_h;
        }

        let selected = app.selected_preset == Some(i);
        let border = if selected {
            theme::ACCENT
        } else {
            theme::BORDER
        };
        let fg = if selected {
            theme::ACCENT
        } else if preset.builtin {
            theme::TEXT
        } else {
            theme::WARN
        };

        let load_rect = Rect {
            x,
            y,
            width: load_w,
            height: chip_h,
        };
        render_chip_button(frame, load_rect, &label, fg, theme::BG, border);

        let delete = if !preset.builtin {
            let del_rect = Rect {
                x: x + load_w,
                y,
                width: 3,
                height: chip_h,
            };
            render_chip_button(
                frame,
                del_rect,
                "×",
                theme::ERROR,
                theme::BG,
                if selected { theme::ERROR } else { theme::BORDER },
            );
            x = x.saturating_add(load_w + 3 + 1);
            Some(del_rect)
        } else {
            x = x.saturating_add(load_w + 1);
            None
        };

        app.hit_presets.push(PresetHits {
            load: load_rect,
            delete,
        });
    }

    let save_rect = Rect {
        x: inner.x + inner.width.saturating_sub(save_w),
        y: inner.y,
        width: save_w,
        height: chip_h,
    };
    render_chip_button(
        frame,
        save_rect,
        "+ Save",
        theme::SUCCESS,
        theme::BG,
        theme::SUCCESS,
    );
    app.hit_save_preset = save_rect;
}

fn draw_metrics(frame: &mut Frame, area: Rect, app: &mut App) {
    // Single dense row: all five metrics side by side
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    for (i, rect) in cols.iter().enumerate() {
        let metric = Metric::from_index(i);
        let r = pad(*rect, if i == 0 { 0 } else { 1 }, 0, 0, 0);
        let hits = draw_metric_card(frame, r, app, metric, app.selected == metric);
        app.hit_metrics[i] = hits;
    }
}

fn draw_metric_card(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    metric: Metric,
    selected: bool,
) -> MetricHits {
    let accent = match metric {
        Metric::Download => theme::DOWNLOAD,
        Metric::Upload => theme::UPLOAD,
        Metric::Loss => theme::LOSS,
        Metric::Delay => theme::ACCENT,
        Metric::Jitter => theme::WARN,
    };

    let border = if selected {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::BORDER)
    };
    let bg = if selected {
        theme::SURFACE_ALT
    } else {
        theme::SURFACE
    };

    let title = format!(" {} {} ", metric.icon(), metric.label());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .style(Style::default().bg(bg))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if selected { accent } else { theme::TEXT_DIM })
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            format!(" {} ", metric.unit_hint()),
            Style::default().fg(theme::TEXT_MUTED),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut hits = MetricHits {
        card: area,
        ..Default::default()
    };

    if inner.height == 0 || inner.width < 6 {
        return hits;
    }

    // Ultra-compact: one control row + optional slider
    //  [−]  value  [+] unit
    //  ──slider──
    let has_slider = inner.height >= 2 && inner.width >= 8;
    let body = if has_slider {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(inner)
    };

    let unit = metric.unit();
    // Give unit enough room for "Mbps"
    let unit_w = (unit.chars().count() as u16 + 1).max(3);
    let value_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(unit_w),
        ])
        .split(body[0]);

    hits.dec = value_row[0];
    hits.inc = value_row[2];

    frame.render_widget(
        Paragraph::new("−")
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if selected {
                        theme::TEXT_INVERSE
                    } else {
                        accent
                    })
                    .bg(if selected { accent } else { theme::BTN_STEP_BG })
                    .add_modifier(Modifier::BOLD),
            ),
        value_row[0],
    );
    frame.render_widget(
        Paragraph::new("+")
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(if selected {
                        theme::TEXT_INVERSE
                    } else {
                        accent
                    })
                    .bg(if selected { accent } else { theme::BTN_STEP_BG })
                    .add_modifier(Modifier::BOLD),
            ),
        value_row[2],
    );

    let value = format_value(metric, app.metric_value(metric));
    frame.render_widget(
        Paragraph::new(value)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(accent)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
        value_row[1],
    );

    frame.render_widget(
        Paragraph::new(unit)
            .alignment(Alignment::Left)
            .style(Style::default().fg(theme::TEXT_MUTED).bg(bg)),
        value_row[3],
    );

    if has_slider {
        hits.slider = body[1];
        draw_slider(
            frame,
            body[1],
            metric,
            app.metric_value(metric),
            accent,
            bg,
            selected || app.dragging == Some(metric),
        );
    }

    hits
}

fn draw_slider(
    frame: &mut Frame,
    area: Rect,
    metric: Metric,
    value: f64,
    accent: Color,
    bg: Color,
    active: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let ratio = value_to_slider_ratio(metric, value);
    let over = value > slider_max(metric) + 0.01;

    // Track + filled portion via Gauge, then overlay a thumb character.
    let track_bg = Color::Rgb(30, 35, 42);
    let fill = if over {
        theme::WARN
    } else {
        accent
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(fill).bg(track_bg))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, area);

    // Thumb marker
    let thumb_x = if area.width <= 1 {
        area.x
    } else {
        area.x + ((area.width - 1) as f64 * ratio).round() as u16
    };
    let thumb = if active { "●" } else { "◆" };
    let thumb_style = Style::default()
        .fg(if active { Color::White } else { accent })
        .bg(track_bg)
        .add_modifier(Modifier::BOLD);
    // Draw thumb as a 1-col paragraph at thumb position
    let thumb_rect = Rect {
        x: thumb_x.min(area.x + area.width.saturating_sub(1)),
        y: area.y,
        width: 1,
        height: 1,
    };
    frame.render_widget(Paragraph::new(thumb).style(thumb_style), thumb_rect);

    // Suppress unused bg warning in some layouts
    let _ = bg;
}

/// Visual style for on-screen buttons.
#[derive(Clone, Copy)]
enum BtnStyle {
    /// Filled green — Apply / Run
    Primary,
    /// Red outline on dark red tint — Reset
    Danger,
    /// Blue filled tint — Speed test / accent CTAs
    Accent,
    /// Neutral grey — Quit / Back
    Ghost,
    /// Preset chips / small toggles
    Chip { fg: Color, active: bool, danger: bool },
}

fn render_btn(frame: &mut Frame, area: Rect, label: &str, style: BtnStyle) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (fg, bg, border, bold) = match style {
        BtnStyle::Primary => (
            theme::TEXT_INVERSE,
            theme::BTN_PRIMARY_BG,
            theme::BTN_PRIMARY_BORDER,
            true,
        ),
        BtnStyle::Danger => (
            theme::TEXT_INVERSE,
            theme::ERROR,
            theme::BTN_DANGER_BORDER,
            true,
        ),
        BtnStyle::Accent => (
            theme::ACCENT,
            theme::BTN_ACCENT_BG,
            theme::BTN_ACCENT_BORDER,
            true,
        ),
        BtnStyle::Ghost => (
            theme::TEXT_DIM,
            theme::BTN_GHOST_BG,
            theme::BTN_GHOST_BORDER,
            true,
        ),
        BtnStyle::Chip { fg, active, danger } => {
            if danger {
                (theme::ERROR, theme::BTN_DANGER_BG, theme::ERROR, true)
            } else if active {
                (fg, theme::BTN_CHIP_ACTIVE_BG, fg, true)
            } else {
                (fg, theme::BTN_CHIP_BG, theme::BORDER, true)
            }
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border).add_modifier(if matches!(
            style,
            BtnStyle::Primary | BtnStyle::Accent
        ) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Pad short labels so chips don't look empty.
    let text = if area.width >= 6 && label.chars().count() <= 2 {
        format!(" {label} ")
    } else {
        format!(" {label} ")
    };

    let mut style_text = Style::default().fg(fg).bg(bg);
    if bold {
        style_text = style_text.add_modifier(Modifier::BOLD);
    }

    // Vertically center when button is tall.
    if inner.height >= 3 {
        let mid = Rect {
            x: inner.x,
            y: inner.y + inner.height / 2,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .style(style_text),
            mid,
        );
    } else {
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .style(style_text),
            inner,
        );
    }
}

fn render_chip_button(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    fg: Color,
    _bg: Color,
    border: Color,
) {
    // Infer chip state from colors callers already pass.
    let active = border == theme::ACCENT || border == fg && fg == theme::ACCENT;
    let danger = fg == theme::ERROR || border == theme::ERROR || label.trim() == "×";
    let success_chip = fg == theme::SUCCESS || label.contains("Save");
    let style = if success_chip {
        BtnStyle::Primary
    } else {
        BtnStyle::Chip {
            fg: if danger { theme::ERROR } else { fg },
            active: active && !danger,
            danger,
        }
    };
    render_btn(frame, area, label, style);
}

/// Current applied limits (right half of top row).
fn draw_applied(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.applied.is_active();
    let pending = app.draft_differs_from_applied();
    let border = if active {
        theme::SUCCESS
    } else {
        theme::BORDER
    };
    let title = if active {
        " CURRENT APPLIED LIMITS  ·  ACTIVE "
    } else {
        " CURRENT APPLIED LIMITS  ·  NONE "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if active {
                    theme::SUCCESS
                } else {
                    theme::TEXT_DIM
                })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let iface = if app.applied.interface.is_empty() {
        app.current_iface()
    } else {
        app.applied.interface.as_str()
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status
            Constraint::Length(1), // download / upload
            Constraint::Length(1), // loss / delay / jitter
            Constraint::Min(1),    // draft note
        ])
        .split(inner);

    let status = if active {
        Line::from(vec![
            Span::styled(
                " ● ENFORCED ",
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("on {iface}"),
                Style::default().fg(theme::TEXT),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ○ NO LIMITS ",
                Style::default()
                    .fg(theme::TEXT_MUTED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("on {iface}  ·  unlimited"),
                Style::default().fg(theme::TEXT_DIM),
            ),
        ])
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(theme::SURFACE)),
        rows[0],
    );

    if rows[1].height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ↓ Download  ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(
                    format!("{:<12}", format_rate(app.applied.download_mbps)),
                    Style::default()
                        .fg(theme::DOWNLOAD)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ↑ Upload  ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(
                    format_rate(app.applied.upload_mbps),
                    Style::default()
                        .fg(theme::UPLOAD)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
            .style(Style::default().bg(theme::SURFACE)),
            rows[1],
        );
    }

    if rows[2].height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ⚠ Loss  ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(
                    format!("{:<8}", format_loss(app.applied.loss_percent)),
                    Style::default().fg(theme::LOSS).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ⏱ Delay  ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(
                    format!("{}ms  ", format_ms(app.applied.delay_ms)),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ∿ Jitter  ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(
                    format!("{}ms", format_ms(app.applied.jitter_ms)),
                    Style::default().fg(theme::WARN).add_modifier(Modifier::BOLD),
                ),
            ]))
            .style(Style::default().bg(theme::SURFACE)),
            rows[2],
        );
    }

    if rows[3].height > 0 {
        let draft_note = if pending {
            format!(
                " draft differs → Apply [a]  (↓{} ↑{} loss{} dly{}±{})",
                format_rate(app.download),
                format_rate(app.upload),
                format_loss(app.loss),
                format_ms(app.delay),
                format_ms(app.jitter),
            )
        } else {
            " draft matches applied".into()
        };
        frame.render_widget(
            Paragraph::new(draft_note).style(
                Style::default()
                    .fg(if pending {
                        theme::WARN
                    } else {
                        theme::TEXT_MUTED
                    })
                    .bg(theme::SURFACE),
            ),
            rows[3],
        );
    }
}

/// Path quality (ICMP ping). Clear dual graphs: packet loss + latency.
fn draw_ping_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            " PATH QUALITY  ·  ICMP ping to 1.1.1.1 ",
            Style::default()
                .fg(theme::TEXT_DIM)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " left = packet loss over time  ·  right = latency (RTT) over time ",
            Style::default().fg(theme::TEXT_MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 2 || inner.width < 12 {
        app.hit_open_speedtest = Rect::default();
        return;
    }

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // dual graphs
            Constraint::Length(1), // summary line
            Constraint::Length(3), // speed test button
        ])
        .split(inner);

    // Two clear panels side by side
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body[0]);

    // ── Packet loss ───────────────────────────────────────────────
    let (loss_label, loss_color) = crate::monitor::loss_quality(app.ping.loss_percent);
    let ok = app.ping.ok_count();
    let n = app.ping.samples;
    draw_quality_graph(
        frame,
        halves[0],
        "Packet loss",
        &format!("{:.0}%", app.ping.loss_percent),
        &format!("{loss_label}  ·  {ok}/{n} pings ok"),
        "0% ──────── 100%",
        &app.history.loss_slice(),
        1000, // max = 100.0% * 10
        loss_color,
        true, // force max scale to 100%
    );

    // ── Latency (RTT) ─────────────────────────────────────────────
    let (rtt_label, rtt_color) = crate::monitor::rtt_quality(app.ping.last_rtt_ms);
    let rtt_txt = app
        .ping
        .last_rtt_ms
        .map(|m| format!("{m:.0} ms"))
        .unwrap_or_else(|| "— ms".into());
    let rtt_data = app.history.rtt_slice();
    let rtt_max = rtt_data.iter().copied().max().unwrap_or(1).max(1);
    draw_quality_graph(
        frame,
        halves[1],
        "Latency (RTT)",
        &rtt_txt,
        rtt_label,
        "lower is better · dips = timeouts",
        &rtt_data,
        rtt_max,
        rtt_color,
        false,
    );

    // ── Summary ───────────────────────────────────────────────────
    let summary = format!(
        " Live traffic on {}:  ↓ {:.1} Mbps download   ↑ {:.1} Mbps upload   ·   ping host {}",
        app.current_iface(),
        app.throughput.down_mbps,
        app.throughput.up_mbps,
        app.ping.host,
    );
    frame.render_widget(
        Paragraph::new(summary).style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
        body[1],
    );

    // ── Speed test CTA ────────────────────────────────────────────
    let btn = pad(body[2], 1, 1, 0, 0);
    app.hit_open_speedtest = btn;
    let (label, style) = if app.speedtest_running {
        (
            "⚡  Speed test in progress…  [t] open to stop",
            BtnStyle::Chip {
                fg: theme::WARN,
                active: true,
                danger: false,
            },
        )
    } else if app.last_speedtest.is_some() {
        (
            "⚡  Cloudflare Speed Test  [t]  ·  view / retest",
            BtnStyle::Accent,
        )
    } else {
        ("⚡  Cloudflare Speed Test  [t]", BtnStyle::Accent)
    };
    render_btn(frame, btn, label, style);
}

/// One labeled quality card: title, big value, status, sparkline, scale hint.
fn draw_quality_graph(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    big_value: &str,
    status: &str,
    scale_hint: &str,
    data: &[u64],
    max: u64,
    color: Color,
    fixed_max: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // big value + status
            Constraint::Min(1),    // sparkline
            Constraint::Length(1), // scale
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {big_value}  "),
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(status, Style::default().fg(theme::TEXT_MUTED)),
        ]))
        .style(Style::default().bg(theme::BG)),
        rows[0],
    );

    let spark_data: Vec<u64> = if data.is_empty() { vec![0] } else { data.to_vec() };
    let max = if fixed_max {
        max.max(1)
    } else {
        spark_data.iter().copied().max().unwrap_or(1).max(1)
    };
    frame.render_widget(
        Sparkline::default()
            .data(&spark_data)
            .max(max)
            .style(Style::default().fg(color).bg(theme::BG)),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(format!(" {scale_hint}"))
            .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::BG)),
        rows[2],
    );
}

fn draw_speedtest_screen(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            " Cloudflare Speed Test ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Esc/b back  ·  Enter run/stop  ·  s stop  ·  ←→ sec/phase  ·  ↺ re-run phase ",
            Style::default().fg(theme::TEXT_MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 6 {
        return;
    }

    let d = app.speedtest_duration_secs;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // controls
            Constraint::Length(1), // status
            Constraint::Length(3), // 3 phase progress gauges
            Constraint::Min(6),    // graphs + re-run
            Constraint::Length(3), // summary strip
        ])
        .split(inner);

    // ── controls ──────────────────────────────────────────────────
    let ctrl = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(14),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(16), // "~15s total"
            Constraint::Length(2),
            Constraint::Length(16), // Run all / STOP
            Constraint::Length(2),
            Constraint::Length(10),
            Constraint::Min(0),
        ])
        .split(rows[0]);

    frame.render_widget(
        Paragraph::new(" Sec/phase")
            .style(Style::default().fg(theme::TEXT_DIM).bg(theme::SURFACE)),
        ctrl[0],
    );
    app.hit_st_dur_dec = ctrl[1];
    app.hit_st_dur_inc = ctrl[3];
    render_chip_button(
        frame,
        ctrl[1],
        "−",
        theme::ACCENT,
        theme::BG,
        if app.speedtest_running {
            theme::BORDER
        } else {
            theme::ACCENT
        },
    );
    frame.render_widget(
        Paragraph::new(format!("{d}s"))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::BG)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER)),
            ),
        ctrl[2],
    );
    render_chip_button(
        frame,
        ctrl[3],
        "+",
        theme::ACCENT,
        theme::BG,
        if app.speedtest_running {
            theme::BORDER
        } else {
            theme::ACCENT
        },
    );
    frame.render_widget(
        Paragraph::new(format!("×3 ≈ {}s", d * 3))
            .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
        ctrl[4],
    );

    app.hit_st_run = ctrl[6];
    app.hit_st_back = ctrl[8];
    if app.speedtest_running {
        // Filled red STOP — replaces the old inert "… Running" chip.
        render_btn(frame, ctrl[6], "■ STOP [s]", BtnStyle::Danger);
    } else {
        render_btn(frame, ctrl[6], "▶  Run all", BtnStyle::Primary);
    }
    render_btn(frame, ctrl[8], "← Back", BtnStyle::Ghost);

    // ── status + thin progress ────────────────────────────────────
    let phase = if app.speedtest_running {
        format!(" {} — {} ", app.speedtest_phase, app.speedtest_detail)
    } else if let Some(err) = &app.speedtest_error {
        format!(" ✗ {err} ")
    } else if app.speedtest_phase == "stopped" {
        " stopped — partial samples kept · Run all to retry ".into()
    } else if app.last_speedtest.is_some() {
        " complete — Run all or ↺ under a graph ".into()
    } else {
        format!(" each phase runs {d}s (latency + download + upload ≈ {}s) ", d * 3)
    };
    frame.render_widget(
        Paragraph::new(phase).style(
            Style::default()
                .fg(if app.speedtest_error.is_some() {
                    theme::ERROR
                } else {
                    theme::TEXT_MUTED
                })
                .bg(theme::SURFACE),
        ),
        rows[1],
    );

    // Three independent progress bars (latency / download / upload)
    let prog_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[2]);

    draw_phase_progress(
        frame,
        prog_cols[0],
        "↓ down",
        app.st_prog_down,
        theme::DOWNLOAD,
        app.speedtest_running && app.speedtest_phase == "download",
    );
    draw_phase_progress(
        frame,
        prog_cols[1],
        "↑ up",
        app.st_prog_up,
        theme::UPLOAD,
        app.speedtest_running && app.speedtest_phase == "upload",
    );
    draw_phase_progress(
        frame,
        prog_cols[2],
        "lat",
        app.st_prog_lat,
        theme::ACCENT,
        app.speedtest_running && app.speedtest_phase == "latency",
    );

    // ── graphs + per-test re-run ──────────────────────────────────
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[3]);

    // Prefer live samples; fall back to stored report samples per series.
    let down_s = if !app.st_down_samples.is_empty() {
        app.st_down_samples.as_slice()
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.down_samples.as_slice())
            .unwrap_or(&[])
    };
    let up_s = if !app.st_up_samples.is_empty() {
        app.st_up_samples.as_slice()
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.up_samples.as_slice())
            .unwrap_or(&[])
    };
    let lat_s = if !app.st_lat_samples.is_empty() {
        app.st_lat_samples.as_slice()
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.lat_samples.as_slice())
            .unwrap_or(&[])
    };
    let down_v = if !app.st_down_samples.is_empty() {
        app.st_down_samples.iter().cloned().fold(0.0_f64, f64::max)
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.download_mbps)
            .unwrap_or(0.0)
    };
    let up_v = if !app.st_up_samples.is_empty() {
        app.st_up_samples.iter().cloned().fold(0.0_f64, f64::max)
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.upload_mbps)
            .unwrap_or(0.0)
    };
    let lat_v = if !app.st_lat_samples.is_empty() {
        // median-ish: use last sample while running; report uses true median on finish
        app.st_lat_samples.last().copied().unwrap_or(0.0)
    } else {
        app.last_speedtest
            .as_ref()
            .map(|r| r.latency_ms)
            .unwrap_or(0.0)
    };

    let col_parts = |col: Rect| {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(3)])
            .split(col)
    };
    let d_parts = col_parts(cols[0]);
    let u_parts = col_parts(cols[1]);
    let l_parts = col_parts(cols[2]);

    draw_st_graph(
        frame,
        d_parts[0],
        "↓ DOWNLOAD",
        down_s,
        down_v,
        "Mbps",
        theme::DOWNLOAD,
        false,
    );
    draw_st_graph(
        frame,
        u_parts[0],
        "↑ UPLOAD",
        up_s,
        up_v,
        "Mbps",
        theme::UPLOAD,
        false,
    );
    draw_st_graph(
        frame,
        l_parts[0],
        "LATENCY",
        lat_s,
        lat_v,
        "ms",
        theme::ACCENT,
        true,
    );

    let rerun_disabled = app.speedtest_running;
    let down_btn = pad(d_parts[1], 1, 1, 0, 0);
    let up_btn = pad(u_parts[1], 1, 1, 0, 0);
    let lat_btn = pad(l_parts[1], 1, 1, 0, 0);
    app.hit_st_rerun_down = if rerun_disabled {
        Rect::default()
    } else {
        down_btn
    };
    app.hit_st_rerun_up = if rerun_disabled {
        Rect::default()
    } else {
        up_btn
    };
    app.hit_st_rerun_lat = if rerun_disabled {
        Rect::default()
    } else {
        lat_btn
    };

    let rerun_style = if rerun_disabled {
        BtnStyle::Ghost
    } else {
        BtnStyle::Accent
    };
    render_btn(
        frame,
        down_btn,
        &format!("↺  ↓ {d}s"),
        if rerun_disabled {
            BtnStyle::Ghost
        } else {
            BtnStyle::Accent
        },
    );
    render_btn(frame, up_btn, &format!("↺  ↑ {d}s"), rerun_style);
    render_btn(frame, lat_btn, &format!("↺  lat {d}s"), rerun_style);

    // ── compact results strip ─────────────────────────────────────
    let strip = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.last_speedtest.is_some() {
            theme::SUCCESS
        } else {
            theme::BORDER
        }))
        .style(Style::default().bg(theme::BG));
    let strip_inner = strip.inner(rows[4]);
    frame.render_widget(strip, rows[4]);

    if let Some(err) = &app.speedtest_error {
        frame.render_widget(
            Paragraph::new(format!(" ✗ {err}"))
                .style(Style::default().fg(theme::ERROR).bg(theme::BG)),
            strip_inner,
        );
    } else if let Some(r) = &app.last_speedtest {
        let line = Line::from(vec![
            Span::styled(" ↓ ", Style::default().fg(theme::DOWNLOAD)),
            Span::styled(
                format!("{:.1} Mbps  ", r.download_mbps),
                Style::default()
                    .fg(theme::DOWNLOAD)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("↑ ", Style::default().fg(theme::UPLOAD)),
            Span::styled(
                format!("{:.1} Mbps  ", r.upload_mbps),
                Style::default()
                    .fg(theme::UPLOAD)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("lat ", Style::default().fg(theme::ACCENT)),
            Span::styled(
                format!("{:.0} ms  ", r.latency_ms),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("jit ", Style::default().fg(theme::WARN)),
            Span::styled(
                format!("{:.0} ms  ", r.jitter_ms),
                Style::default()
                    .fg(theme::WARN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({}s)", r.duration_secs),
                Style::default().fg(theme::TEXT_MUTED),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(theme::BG)),
            strip_inner,
        );
    } else if app.speedtest_running {
        frame.render_widget(
            Paragraph::new(" measuring… graphs update as probes finish ")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::BG)),
            strip_inner,
        );
    } else {
        frame.render_widget(
            Paragraph::new(" no results yet — press Run ")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::BG)),
            strip_inner,
        );
    }
}

fn draw_phase_progress(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    progress: f64,
    color: Color,
    active: bool,
) {
    let ratio = progress.clamp(0.0, 1.0);
    let border = if active {
        color
    } else if ratio >= 1.0 {
        theme::SUCCESS
    } else {
        theme::BORDER
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border).add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }))
                .title(Span::styled(
                    format!(" {label} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(color).bg(Color::Rgb(30, 35, 42)))
        .ratio(ratio)
        .label(format!("{:.0}%", ratio * 100.0));
    frame.render_widget(gauge, area);
}

/// Tall sparkline card for one speed-test series.
fn draw_st_graph(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    samples: &[f64],
    headline: f64,
    unit: &str,
    color: Color,
    is_latency: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 2 {
        return;
    }

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // big number
            Constraint::Min(2),    // graph
            Constraint::Length(1), // scale
        ])
        .split(inner);

    let value_txt = if is_latency {
        if headline <= 0.0 && samples.is_empty() {
            "—".into()
        } else {
            format!("{headline:.0}")
        }
    } else if headline <= 0.0 && samples.is_empty() {
        "—".into()
    } else {
        format!("{headline:.1}")
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {value_txt}"),
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {unit}"),
                Style::default().fg(theme::TEXT_MUTED),
            ),
        ]))
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme::BG)),
        parts[0],
    );

    // Sparkline data: scale floats → u64
    let data: Vec<u64> = if samples.is_empty() {
        vec![0]
    } else if is_latency {
        samples
            .iter()
            .map(|v| (v.max(0.0) * 10.0).round() as u64) // 0.1 ms
            .collect()
    } else {
        samples
            .iter()
            .map(|v| (v.max(0.0) * 10.0).round() as u64) // 0.1 Mbps
            .collect()
    };
    let max = data.iter().copied().max().unwrap_or(1).max(1);
    frame.render_widget(
        Sparkline::default()
            .data(&data)
            .max(max)
            .style(Style::default().fg(color).bg(theme::BG)),
        parts[1],
    );

    let scale = if samples.is_empty() {
        " waiting… ".into()
    } else {
        let mn = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = samples.iter().cloned().fold(0.0_f64, f64::max);
        if is_latency {
            format!(" {mn:.0}–{mx:.0} ms · {} probes ", samples.len())
        } else {
            format!(" {mn:.1}–{mx:.1} · {} probes ", samples.len())
        }
    };
    frame.render_widget(
        Paragraph::new(scale)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::BG)),
        parts[2],
    );
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let apply_area = pad(chunks[0], 0, 1, 0, 0);
    let reset_area = pad(chunks[1], 0, 1, 0, 0);
    let history_area = pad(chunks[2], 0, 1, 0, 0);
    let quit_area = pad(chunks[3], 0, 0, 0, 0);

    app.hit_apply = apply_area;
    app.hit_reset = reset_area;
    app.hit_history = history_area;
    app.hit_quit = quit_area;
    app.hit_st_run = Rect::default();
    app.hit_st_back = Rect::default();

    render_btn(frame, apply_area, "▶ Apply [a]", BtnStyle::Primary);
    render_btn(frame, reset_area, "↺ Reset [r]", BtnStyle::Danger);
    render_btn(frame, history_area, "Hist [h]", BtnStyle::Ghost);
    render_btn(frame, quit_area, "✕ Quit [q]", BtnStyle::Ghost);
}

fn draw_banner(frame: &mut Frame, area: Rect, app: &App) {
    let (fg, border) = match app.banner.level {
        BannerLevel::Info => (theme::TEXT_DIM, theme::BORDER),
        BannerLevel::Success => (theme::SUCCESS, theme::SUCCESS),
        BannerLevel::Warn => (theme::WARN, theme::WARN),
        BannerLevel::Error => (theme::ERROR, theme::ERROR),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let busy = if app.busy { "  …" } else { "" };
    frame.render_widget(
        Paragraph::new(format!(" {}{busy}", app.banner.message))
            .style(Style::default().fg(fg).bg(theme::SURFACE))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_keys_main(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key("a"),
        dim(" apply  "),
        key("t"),
        dim(" speed  "),
        key("h"),
        dim(" history  "),
        key("y/j"),
        dim(" dly/jit  "),
        key("r"),
        dim(" reset  "),
        key("q"),
        dim(" quit"),
    ]);
    let p = Paragraph::new(line)
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme::BG));
    frame.render_widget(Clear, area);
    frame.render_widget(p, area);
}

fn draw_history_screen(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .title(" Speed test history  ·  Esc back ")
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.speed_history.is_empty() {
        frame.render_widget(
            Paragraph::new("\n  No history yet. Run a Cloudflare speed test [t].")
                .style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
            inner,
        );
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        "  When                 ↓ Mbps   ↑ Mbps   Lat    Iface / limits",
        Style::default().fg(theme::TEXT_DIM),
    ))];
    for e in app.speed_history.iter().take(inner.height.saturating_sub(2) as usize) {
        let lim = if e.limits.is_active() {
            format!("  [{}]", e.limits.summary())
        } else {
            String::new()
        };
        lines.push(Line::from(format!(
            "  {:19}  {:>6.1}  {:>6.1}  {:>4.0}   {}{}",
            e.at, e.download_mbps, e.upload_mbps, e.latency_ms, e.interface, lim
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(theme::TEXT).bg(theme::SURFACE)),
        inner,
    );
}

fn key(s: &str) -> Span<'_> {
    Span::styled(
        s,
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    )
}

fn dim(s: &str) -> Span<'_> {
    Span::styled(s, Style::default().fg(theme::TEXT_MUTED))
}

fn pad(area: Rect, left: u16, right: u16, top: u16, bottom: u16) -> Rect {
    let x = area.x.saturating_add(left);
    let y = area.y.saturating_add(top);
    let width = area.width.saturating_sub(left.saturating_add(right));
    let height = area.height.saturating_sub(top.saturating_add(bottom));
    Rect {
        x,
        y,
        width,
        height,
    }
}
