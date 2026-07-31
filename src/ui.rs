//! btop-style dense dashboard rendering.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph, Sparkline, Wrap};
use ratatui::Frame;

use crate::app::{App, BannerLevel, MetricHits, PresetHits, Screen};
use crate::presets::{slider_max, value_to_slider_ratio};
use crate::tc::{format_value, Metric};
use crate::theme;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

    match app.screen {
        Screen::Main => draw_main(frame, area, app),
        Screen::SpeedTest => draw_speedtest_screen(frame, area, app),
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // interface
            Constraint::Length(5), // presets
            Constraint::Length(7), // metrics
            Constraint::Length(4), // applied
            Constraint::Min(7),    // ping quality (fills rest)
            Constraint::Length(3), // apply / reset / quit only
            Constraint::Length(2), // banner
            Constraint::Length(1), // keys
        ])
        .split(area);

    draw_interface(frame, root[0], app);
    draw_presets(frame, root[1], app);
    draw_metrics(frame, root[2], app);
    draw_applied(frame, root[3], app);
    draw_ping_panel(frame, root[4], app);
    draw_actions(frame, root[5], app);
    draw_banner(frame, root[6], app);
    draw_keys_main(frame, root[7]);
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
    // Chip height: prefer 1 row of content (+ borders drawn by chip itself → use 3 if room)
    let chip_h: u16 = if inner.height >= 5 { 3 } else { inner.height.min(3).max(1) };
    let row_stride = chip_h.saturating_add(0);
    let max_rows = (inner.height / row_stride.max(1)).max(1);

    // Fixed-ish width so names always fit: "N NameHere" (~12–14 cols)
    let content_w = inner.width.saturating_sub(save_w + gap);
    let min_chip: u16 = 12;
    let per_row = (content_w / min_chip).max(1);
    let chip_w = (content_w / per_row).clamp(min_chip, 18);

    let mut x = inner.x;
    let mut y = inner.y;
    let mut row = 0u16;
    app.hit_presets.clear();

    for (i, preset) in app.presets.iter().enumerate() {
        let del_extra: u16 = if preset.builtin { 0 } else { 3 };
        let need = chip_w.saturating_add(del_extra).saturating_add(1);

        // Wrap to next row when needed
        if x + need > inner.x + content_w {
            row += 1;
            if row >= max_rows {
                break;
            }
            x = inner.x;
            y = inner.y + row * row_stride;
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

        // leave room for × on custom presets
        let load_w = if preset.builtin {
            chip_w.saturating_sub(1).max(min_chip)
        } else {
            chip_w.saturating_sub(1).max(min_chip.saturating_sub(1))
        };

        let load_rect = Rect {
            x,
            y,
            width: load_w,
            height: chip_h,
        };

        // Always include the name; truncate only to the chip's inner width.
        let label = preset_chip_label(i + 1, &preset.name, load_w);
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
            x = x.saturating_add(chip_w);
            None
        };

        app.hit_presets.push(PresetHits {
            load: load_rect,
            delete,
        });
    }

    // Save chip: top-right of the presets panel
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

/// Build a chip label like `1 Unlimited` that fits in `outer_w` columns.
fn preset_chip_label(num: usize, name: &str, outer_w: u16) -> String {
    // borders take 2 cols when using bordered chips
    let inner = outer_w.saturating_sub(2).max(1) as usize;
    let prefix = format!("{num} ");
    if prefix.len() >= inner {
        return format!("{num}");
    }
    let name_budget = inner - prefix.len();
    let name_part: String = if name.chars().count() <= name_budget {
        name.to_string()
    } else if name_budget <= 1 {
        name.chars().take(1).collect()
    } else {
        let take = name_budget.saturating_sub(1);
        format!("{}…", name.chars().take(take).collect::<String>())
    };
    format!("{prefix}{name_part}")
}

fn draw_metrics(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    for (i, metric) in Metric::ALL.iter().enumerate() {
        let rect = pad(cols[i], if i == 0 { 0 } else { 1 }, 0, 0, 0);
        let hits = draw_metric_card(frame, rect, app, *metric, app.selected == *metric);
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
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut hits = MetricHits {
        card: area,
        ..Default::default()
    };

    if inner.height < 4 || inner.width < 10 {
        // Fallback ultra-compact view
        let value = format_value(metric, app.metric_value(metric));
        frame.render_widget(
            Paragraph::new(value)
                .alignment(Alignment::Center)
                .style(Style::default().fg(accent).bg(bg).add_modifier(Modifier::BOLD)),
            inner,
        );
        return hits;
    }

    // Compact layout (no spare Min stretch):
    //  [ − ]  VALUE  [ + ]
    //         unit
    //  ───slider────────
    //  0 … max
    let btn_h = if inner.height >= 6 { 3 } else { 1 };
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(btn_h), // − value +
            Constraint::Length(1),     // unit
            Constraint::Length(1),     // slider
            Constraint::Length(1),     // scale labels
            Constraint::Min(0),        // only if terminal is taller than 9
        ])
        .split(inner);

    let value_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(4),
            Constraint::Length(6),
        ])
        .split(body[0]);

    let dec = pad(value_row[0], 0, 1, 0, 0);
    let inc = pad(value_row[2], 1, 0, 0, 0);
    hits.dec = dec;
    hits.inc = inc;

    if btn_h >= 3 {
        render_step_button(frame, dec, "−", accent, bg, selected);
        render_step_button(frame, inc, "+", accent, bg, selected);
    } else {
        // Single-line ± when very short
        frame.render_widget(
            Paragraph::new(" − ")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(accent)
                        .bg(Color::Rgb(33, 38, 45))
                        .add_modifier(Modifier::BOLD),
                ),
            dec,
        );
        frame.render_widget(
            Paragraph::new(" + ")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(accent)
                        .bg(Color::Rgb(33, 38, 45))
                        .add_modifier(Modifier::BOLD),
                ),
            inc,
        );
    }

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

    let unit = match metric {
        Metric::Download | Metric::Upload => "Mbps · 0 = ∞",
        Metric::Loss => "% packet loss",
    };
    frame.render_widget(
        Paragraph::new(unit)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_MUTED).bg(bg)),
        body[1],
    );

    let slider_area = pad(body[2], 1, 1, 0, 0);
    hits.slider = slider_area;
    draw_slider(
        frame,
        slider_area,
        metric,
        app.metric_value(metric),
        accent,
        bg,
        selected || app.dragging == Some(metric),
    );

    let max_label = match metric {
        Metric::Download | Metric::Upload => {
            format!("0 ──────── {} Mbps", slider_max(metric) as i64)
        }
        Metric::Loss => "0% ────────────── 100%".into(),
    };
    frame.render_widget(
        Paragraph::new(max_label)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_MUTED).bg(bg)),
        body[3],
    );

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

fn render_step_button(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    accent: Color,
    bg: Color,
    selected: bool,
) {
    let border = if selected { accent } else { theme::BORDER };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(Color::Rgb(33, 38, 45)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(accent)
                    .bg(Color::Rgb(33, 38, 45))
                    .add_modifier(Modifier::BOLD),
            ),
        inner,
    );
    let _ = bg;
}

fn render_chip_button(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    fg: Color,
    bg: Color,
    border: Color,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)),
        inner,
    );
}

fn draw_applied(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.applied.is_active();
    let pending = app.draft_differs_from_applied();
    let border = if active {
        theme::SUCCESS
    } else {
        theme::BORDER
    };
    let title = if active {
        " APPLIED LIMITS  ·  ACTIVE "
    } else {
        " APPLIED LIMITS  ·  NONE "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if active { theme::SUCCESS } else { theme::TEXT_DIM })
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
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Big status line
    let status = if active {
        Line::from(vec![
            Span::styled(
                " ● ENFORCED ",
                Style::default()
                    .fg(theme::SUCCESS)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" on {iface}  "),
                Style::default().fg(theme::TEXT).bg(theme::SURFACE),
            ),
            Span::styled(
                format!("↓ {}  ", crate::tc::format_rate(app.applied.download_mbps)),
                Style::default()
                    .fg(theme::DOWNLOAD)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("↑ {}  ", crate::tc::format_rate(app.applied.upload_mbps)),
                Style::default()
                    .fg(theme::UPLOAD)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "loss {} ",
                    crate::tc::format_loss(app.applied.loss_percent)
                ),
                Style::default()
                    .fg(theme::LOSS)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ○ NO LIMITS ",
                Style::default()
                    .fg(theme::TEXT_MUTED)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" on {iface}  ·  traffic is unlimited "),
                Style::default().fg(theme::TEXT_DIM).bg(theme::SURFACE),
            ),
        ])
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(theme::SURFACE)),
        rows[0],
    );

    if rows[1].height > 0 {
        let draft_note = if pending {
            format!(
                " draft (not applied): ↓ {}  ↑ {}  loss {}   → press Apply [a]",
                crate::tc::format_rate(if app.download > 0.0 {
                    app.download
                } else {
                    0.0
                }),
                crate::tc::format_rate(if app.upload > 0.0 {
                    app.upload
                } else {
                    0.0
                }),
                crate::tc::format_loss(app.loss),
            )
        } else {
            " draft matches applied  ·  change values above then Apply".into()
        };
        frame.render_widget(
            Paragraph::new(draft_note).style(Style::default().fg(if pending {
                theme::WARN
            } else {
                theme::TEXT_MUTED
            }).bg(theme::SURFACE)),
            rows[1],
        );
    }
}

/// Path quality only (ICMP). Speed test lives on its own full screen.
fn draw_ping_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::SURFACE))
        .title(Span::styled(
            " PATH QUALITY  ·  ping ",
            Style::default()
                .fg(theme::TEXT_DIM)
                .add_modifier(Modifier::BOLD),
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
            Constraint::Min(2),    // loss spark + stats
            Constraint::Length(3), // open speed test button
        ])
        .split(inner);

    let spark_area = body[0];
    let spark_h = spark_area.height.saturating_sub(2).max(1);
    let spark_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(spark_h),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(spark_area);

    let loss_data = app.history.loss_slice();
    let spark_data: Vec<u64> = if loss_data.is_empty() {
        vec![0]
    } else {
        loss_data
    };
    let max = spark_data.iter().copied().max().unwrap_or(1).max(1);

    let spark_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(10),
            Constraint::Length(18),
        ])
        .split(spark_rows[0]);

    frame.render_widget(
        Paragraph::new(" LOSS")
            .style(
                Style::default()
                    .fg(theme::LOSS)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
        spark_row[0],
    );
    frame.render_widget(
        Sparkline::default()
            .data(&spark_data)
            .max(max)
            .style(Style::default().fg(theme::LOSS).bg(theme::SURFACE)),
        spark_row[1],
    );
    frame.render_widget(
        Paragraph::new(format!("{:.0}%", app.ping.loss_percent))
            .alignment(Alignment::Right)
            .style(
                Style::default()
                    .fg(theme::LOSS)
                    .bg(theme::SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
        spark_row[2],
    );

    let rtt = app
        .ping
        .last_rtt_ms
        .map(|m| format!("{m:.0} ms"))
        .unwrap_or_else(|| "—".into());
    let stats = format!(
        " host {}  ·  rtt {}  ·  window {} samples  ·  live ↓ {:.1} / ↑ {:.1} Mbps",
        app.ping.host,
        rtt,
        app.ping.samples,
        app.throughput.down_mbps,
        app.throughput.up_mbps,
    );
    frame.render_widget(
        Paragraph::new(stats).style(Style::default().fg(theme::TEXT_MUTED).bg(theme::SURFACE)),
        spark_rows[1],
    );

    // Dedicated speed-test entry (not next to Apply/Reset)
    let btn = pad(body[1], 1, 1, 0, 0);
    app.hit_open_speedtest = btn;
    let (label, fg) = if app.speedtest_running {
        ("⚡  Speed test running…  [t] open", theme::WARN)
    } else if app.last_speedtest.is_some() {
        ("⚡  Cloudflare Speed Test  [t]  ·  view report / retest", theme::ACCENT)
    } else {
        ("⚡  Cloudflare Speed Test  [t]", theme::ACCENT)
    };
    render_button(frame, btn, label, fg, theme::BG);
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
            " Esc/b back  ·  Enter/t run  ·  ←→ duration ",
            Style::default().fg(theme::TEXT_MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 6 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // controls
            Constraint::Length(1), // status
            Constraint::Length(1), // progress bar (compact)
            Constraint::Min(6),    // graphs (main focus)
            Constraint::Length(3), // compact results strip
        ])
        .split(inner);

    // ── controls ──────────────────────────────────────────────────
    let ctrl = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Length(2),
            Constraint::Length(10),
            Constraint::Min(0),
        ])
        .split(rows[0]);

    frame.render_widget(
        Paragraph::new(" Duration")
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
        Paragraph::new(format!("{}s", app.speedtest_duration_secs))
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

    app.hit_st_run = ctrl[5];
    app.hit_st_back = ctrl[7];
    render_button(
        frame,
        ctrl[5],
        if app.speedtest_running {
            "… Run"
        } else {
            "▶ Run"
        },
        if app.speedtest_running {
            theme::WARN
        } else {
            theme::SUCCESS
        },
        if app.speedtest_running {
            theme::SURFACE
        } else {
            Color::Rgb(35, 134, 54)
        },
    );
    render_button(frame, ctrl[7], "← Back", theme::TEXT_DIM, theme::BG);

    // ── status + thin progress ────────────────────────────────────
    let phase = if app.speedtest_running {
        format!(" {} — {} ", app.speedtest_phase, app.speedtest_detail)
    } else if let Some(err) = &app.speedtest_error {
        format!(" ✗ {err} ")
    } else if app.last_speedtest.is_some() {
        " complete — Run to retest ".into()
    } else {
        " set duration, then Run ".into()
    };
    frame.render_widget(
        Paragraph::new(phase).style(Style::default().fg(
            if app.speedtest_error.is_some() {
                theme::ERROR
            } else {
                theme::TEXT_MUTED
            },
        ).bg(theme::SURFACE)),
        rows[1],
    );

    let ratio = if app.speedtest_running {
        app.speedtest_progress.clamp(0.0, 1.0)
    } else if app.last_speedtest.is_some() && app.speedtest_error.is_none() {
        1.0
    } else {
        0.0
    };
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme::ACCENT).bg(Color::Rgb(30, 35, 42)))
            .ratio(ratio)
            .label(format!("{:.0}%", ratio * 100.0)),
        rows[2],
    );

    // ── graphs (main area) ────────────────────────────────────────
    let graphs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[3]);

    // While running (or after reset), only live samples — never mix with a previous report.
    let (down_s, up_s, lat_s, down_v, up_v, lat_v) = if app.speedtest_running
        || app.last_speedtest.is_none()
    {
        (
            app.st_down_samples.as_slice(),
            app.st_up_samples.as_slice(),
            app.st_lat_samples.as_slice(),
            app.st_down_samples
                .iter()
                .cloned()
                .fold(0.0_f64, f64::max),
            app.st_up_samples.iter().cloned().fold(0.0_f64, f64::max),
            app.st_lat_samples.last().copied().unwrap_or(0.0),
        )
    } else if let Some(r) = &app.last_speedtest {
        (
            if app.st_down_samples.is_empty() {
                r.down_samples.as_slice()
            } else {
                app.st_down_samples.as_slice()
            },
            if app.st_up_samples.is_empty() {
                r.up_samples.as_slice()
            } else {
                app.st_up_samples.as_slice()
            },
            if app.st_lat_samples.is_empty() {
                r.lat_samples.as_slice()
            } else {
                app.st_lat_samples.as_slice()
            },
            r.download_mbps,
            r.upload_mbps,
            r.latency_ms,
        )
    } else {
        (
            app.st_down_samples.as_slice(),
            app.st_up_samples.as_slice(),
            app.st_lat_samples.as_slice(),
            0.0,
            0.0,
            0.0,
        )
    };

    draw_st_graph(
        frame,
        graphs[0],
        "↓ DOWNLOAD",
        down_s,
        down_v,
        "Mbps",
        theme::DOWNLOAD,
        false,
    );
    draw_st_graph(
        frame,
        graphs[1],
        "↑ UPLOAD",
        up_s,
        up_v,
        "Mbps",
        theme::UPLOAD,
        false,
    );
    draw_st_graph(
        frame,
        graphs[2],
        "LATENCY",
        lat_s,
        lat_v,
        "ms",
        theme::ACCENT,
        true,
    );

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
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    let apply_area = pad(chunks[0], 0, 1, 0, 0);
    let reset_area = pad(chunks[1], 0, 1, 0, 0);
    let quit_area = pad(chunks[2], 0, 0, 0, 0);

    app.hit_apply = apply_area;
    app.hit_reset = reset_area;
    app.hit_quit = quit_area;
    // Clear speed-test hits from main action row
    app.hit_st_run = Rect::default();
    app.hit_st_back = Rect::default();

    render_button(
        frame,
        apply_area,
        "▶ Apply [a]",
        theme::SUCCESS,
        Color::Rgb(35, 134, 54),
    );
    render_button(
        frame,
        reset_area,
        "↺ Reset [r]",
        theme::ERROR,
        theme::SURFACE,
    );
    render_button(
        frame,
        quit_area,
        "✕ Quit [q]",
        theme::TEXT_DIM,
        theme::SURFACE,
    );
}

fn render_button(frame: &mut Frame, area: Rect, label: &str, fg: Color, bg: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(fg))
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)),
        inner,
    );
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
        key("−/+"),
        dim(" adj  "),
        key("t"),
        dim(" speed test  "),
        key("1-9"),
        dim(" preset  "),
        key("[/]"),
        dim(" iface  "),
        key("a"),
        dim(" apply  "),
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
