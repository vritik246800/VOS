use crate::modules::sysmon::{StorageKind, SystemMonitor};
use crate::ui::icons::{
    ICON_CPU, ICON_DISK, ICON_GPU, ICON_LAN, ICON_NETWORK, ICON_NETWORK_RX, ICON_NETWORK_TX,
    ICON_RAM, ICON_SWAP, ICON_UPTIME, ICON_USB,
};
use crate::ui::trunc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, LineGauge, Paragraph},
};

/// Bare progress bar — the percentage is always printed next to it, so the
/// widget's own inline label would only eat width.
fn bar(pct: f64, color: Color) -> LineGauge<'static> {
    LineGauge::default()
        .ratio((pct / 100.0).clamp(0.0, 1.0))
        .label("")
        .filled_style(Style::default().fg(color))
        .unfilled_style(Style::default().fg(DIM))
}

const ACCENT: Color = Color::Green;
const HIGHLIGHT: Color = Color::Yellow;
const DIM: Color = Color::DarkGray;

/// History buffer as `(x, y)` points for a [`Chart`], with the newest sample
/// pinned to the right edge of the x window while the buffer is still filling.
fn series<T: Copy + Into<f64>>(hist: &[T], window: usize) -> Vec<(f64, f64)> {
    let offset = window.saturating_sub(hist.len());
    hist.iter()
        .enumerate()
        .map(|(i, &v)| ((offset + i) as f64, v.into()))
        .collect()
}

/// One smooth braille line — much higher resolution than block sparklines.
fn line_dataset(data: &[(f64, f64)], color: Color) -> Dataset<'_> {
    Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(data)
}

/// Borderless time-series chart: unnamed datasets and label-less axes, so
/// ratatui draws only the lines themselves over the whole card area.
fn line_chart<'a>(datasets: Vec<Dataset<'a>>, x_window: usize, y_max: f64) -> Chart<'a> {
    Chart::new(datasets)
        .x_axis(Axis::default().bounds([0.0, x_window.saturating_sub(1) as f64]))
        .y_axis(Axis::default().bounds([0.0, y_max]))
}

/// One mini bar line: right-aligned label, a 6-cell block bar and the value.
fn mini_bar(label: &str, usage: f32) -> Line<'static> {
    const WIDTH: usize = 6;
    let filled = ((usage.clamp(0.0, 100.0) / 100.0) * WIDTH as f32).round() as usize;
    Line::from(vec![
        Span::styled(format!(" {label:>3} "), Style::default().fg(DIM)),
        Span::styled("█".repeat(filled), Style::default().fg(pct_color(usage))),
        Span::styled("░".repeat(WIDTH - filled), Style::default().fg(DIM)),
        Span::styled(format!(" {usage:>3.0}"), Style::default().fg(DIM)),
    ])
}

/// One line of the per-core bar chart.
fn core_bar(index: usize, usage: f32) -> Line<'static> {
    mini_bar(&index.to_string(), usage)
}

pub fn render_sysmon(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let storage_h = (sm.disk_info.len() as u16 + 2).clamp(3, 10);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // Storage is the only card with a fixed height (one row per volume);
            // the other three share the screen so every chart grows with it.
            Constraint::Percentage(30),    // CPU
            Constraint::Percentage(24),    // RAM + Swap
            Constraint::Min(8),            // Networking
            Constraint::Length(storage_h), // Storage
        ])
        .split(area);

    render_cpu_and_gpu(f, rows[0], sm);

    let mem = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);
    render_ram(f, mem[0], sm);
    render_swap(f, mem[1], sm);

    render_network(f, rows[2], sm);
    render_storage(f, rows[3], sm);
}

/// Rounded dark card with a small gray title — the dashboard look.
fn card(title: String) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(Color::Gray),
        ))
        .border_style(Style::default().fg(DIM))
}

/// The top row: CPU alone, or CPU | GPU side by side when a GPU is found.
fn render_cpu_and_gpu(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    if sm.gpu.is_none() {
        render_cpu(f, area, sm);
        return;
    }
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);
    render_cpu(f, top[0], sm);
    render_gpu(f, top[1], sm);
}

fn render_cpu(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let block = card(format!("{} CPU", ICON_CPU));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 1 {
        return;
    }

    // Per-core bars in two columns when there is room, one when the GPU card
    // shares the row. One bar line is 15 cells wide.
    let two_cols = inner.width >= 80;
    let n_cols: u16 = if two_cols { 2 } else { 1 };
    let bars_w = 15 * n_cols;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),     // big number
            Constraint::Length(bars_w), // per-core bars
            Constraint::Min(0),         // history chart
        ])
        .split(inner);

    let pct = sm.cpu_pct();
    let color = pct_color(pct);
    let cores = sm.cpu_cores.len();

    let left = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" {pct:.1}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" %", Style::default().fg(DIM)),
        ]),
        Line::from(vec![Span::styled(
            format!("  {cores} cores"),
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(Paragraph::new(left), chunks[0]);

    // One line per core, split across the column(s).
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(100 / n_cols); n_cols as usize])
        .split(chunks[1]);
    let per_col = cores.div_ceil(n_cols as usize);
    for (c, col) in cols.iter().enumerate() {
        let start = c * per_col;
        let lines: Vec<Line> = sm.cpu_cores[start..cores.min(start + per_col)]
            .iter()
            .enumerate()
            .map(|(i, &u)| core_bar(start + i, u))
            .collect();
        f.render_widget(Paragraph::new(lines), *col);
    }

    let points = series(&sm.cpu_history, sm.history_len);
    let chart = line_chart(vec![line_dataset(&points, ACCENT)], sm.history_len, 100.0);
    f.render_widget(chart, chunks[2]);
}

fn render_gpu(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let Some(gpu) = &sm.gpu else { return };
    let block = card(format!("{} GPU", ICON_GPU));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 1 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(0)])
        .split(inner);

    let pct = gpu.device_pct;
    let color = pct_color(pct);
    let name = trunc(&gpu.name, 12);

    // macOS exposes no per-core GPU counters, so the engine breakdown
    // (renderer / tiler) and the shared-memory usage stand in for them.
    let left = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" {pct:.0}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" %", Style::default().fg(DIM)),
        ]),
        Line::from(vec![Span::styled(
            format!(" {name} · {} cores", gpu.cores),
            Style::default().fg(DIM),
        )]),
        Line::from(""),
        mini_bar("Ren", gpu.renderer_pct),
        mini_bar("Til", gpu.tiler_pct),
        Line::from(vec![Span::styled(
            format!(
                " Mem {:.1}/{:.1} GB",
                gpu.mem_used_mb / 1024.0,
                gpu.mem_alloc_mb / 1024.0
            ),
            Style::default().fg(DIM),
        )]),
    ];
    f.render_widget(Paragraph::new(left), chunks[0]);

    let points = series(&sm.gpu_history, sm.history_len);
    let chart = line_chart(vec![line_dataset(&points, ACCENT)], sm.history_len, 100.0);
    f.render_widget(chart, chunks[1]);
}

fn render_ram(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let block = card(format!("{} Memory", ICON_RAM));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let pct = sm.ram_pct();
    let color = pct_color(pct);
    let used_gb = sm.used_ram_mb / 1024.0;
    let total_gb = sm.total_ram_mb / 1024.0;

    let big = Line::from(vec![
        Span::styled(
            format!(" {used_gb:.1}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" / {total_gb:.0} GB"), Style::default().fg(DIM)),
        Span::styled(format!("  {pct:.0}%"), Style::default().fg(color)),
    ]);
    f.render_widget(Paragraph::new(vec![Line::from(""), big]), rows[0]);

    f.render_widget(bar(pct as f64, color), rows[1]);

    let points = series(&sm.ram_history, sm.history_len);
    let chart = line_chart(vec![line_dataset(&points, ACCENT)], sm.history_len, 100.0);
    f.render_widget(chart, rows[2]);
}

fn render_swap(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let block = card(format!("{} Swap", ICON_SWAP));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // big number
            Constraint::Length(1), // bar
            Constraint::Min(1),    // history
            Constraint::Length(1), // status
        ])
        .split(inner);

    let pct = sm.swap_pct();
    let color = pct_color(pct);

    let big = if sm.total_swap_mb > 0.0 {
        Line::from(vec![
            Span::styled(
                format!(" {pct:.0}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" %", Style::default().fg(DIM)),
            Span::styled(
                format!("   {:.0}/{:.0} MB", sm.used_swap_mb, sm.total_swap_mb),
                Style::default().fg(DIM),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " 0",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" %   no swap", Style::default().fg(DIM)),
        ])
    };
    f.render_widget(Paragraph::new(vec![Line::from(""), big]), rows[0]);

    f.render_widget(bar(pct as f64, color), rows[1]);

    let points = series(&sm.swap_history, sm.history_len);
    let chart = line_chart(vec![line_dataset(&points, ACCENT)], sm.history_len, 100.0);
    f.render_widget(chart, rows[2]);

    // Status line, like the "On track!" card
    let status = if sm.total_swap_mb == 0.0 {
        ("Idle", DIM)
    } else if pct > 50.0 {
        ("Under pressure", HIGHLIGHT)
    } else {
        ("Healthy", ACCENT)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ●", Style::default().fg(status.1)),
            Span::styled(format!(" {}", status.0), Style::default().fg(DIM)),
        ])),
        rows[3],
    );
}

fn render_network(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let block = card(format!("{} Networking", ICON_NETWORK));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 3 {
        return;
    }

    // Busiest interfaces first — sized before the layout so the charts, not the
    // list, absorb the leftover height.
    let mut ifaces: Vec<_> = sm
        .net_info
        .iter()
        .filter(|n| n.rx_kbps > 0.0 || n.tx_kbps > 0.0)
        .collect();
    ifaces.sort_by(|a, b| {
        (b.rx_kbps + b.tx_kbps)
            .partial_cmp(&(a.rx_kbps + a.tx_kbps))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ifaces.truncate(4);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),                          // big totals
            Constraint::Length(1),                          // legend
            Constraint::Min(2),                             // chart
            Constraint::Length(ifaces.len().max(1) as u16), // interfaces
            Constraint::Length(1),                          // uptime
        ])
        .split(inner);

    // Big ↓/↑ numbers, like the dashboard headline stats
    let totals = Line::from(vec![
        Span::styled(
            format!(" {} ", ICON_NETWORK_RX),
            Style::default().fg(ACCENT),
        ),
        Span::styled(
            fmt_rate(sm.rx_kbps()),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("    {} ", ICON_NETWORK_TX),
            Style::default().fg(HIGHLIGHT),
        ),
        Span::styled(
            fmt_rate(sm.tx_kbps()),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(vec![Line::from(""), totals]), rows[0]);

    // Legend matching the two chart lines below.
    let legend = Line::from(vec![
        Span::styled(
            format!(" {} ", ICON_NETWORK_RX),
            Style::default().fg(ACCENT),
        ),
        Span::styled("Download   ", Style::default().fg(DIM)),
        Span::styled(
            format!("{} ", ICON_NETWORK_TX),
            Style::default().fg(HIGHLIGHT),
        ),
        Span::styled("Upload", Style::default().fg(DIM)),
    ]);
    f.render_widget(Paragraph::new(legend), rows[1]);

    // Download and upload share one chart and one scale, so spikes compare.
    let y_max = sm
        .rx_history
        .iter()
        .chain(sm.tx_history.iter())
        .copied()
        .fold(1.0f64, f64::max);
    let rx_points = series(&sm.rx_history, sm.history_len);
    let tx_points = series(&sm.tx_history, sm.history_len);
    let chart = line_chart(
        vec![
            line_dataset(&rx_points, ACCENT),
            line_dataset(&tx_points, HIGHLIGHT),
        ],
        sm.history_len,
        y_max,
    );
    f.render_widget(chart, rows[2]);

    let mut lines: Vec<Line> = ifaces
        .iter()
        .map(|n| {
            let name = trunc(&n.name, 12);
            Line::from(vec![
                Span::styled(
                    format!(" {name:<12}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} {:>9}  ", ICON_NETWORK_RX, fmt_rate(n.rx_kbps)),
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{} {:>9}", ICON_NETWORK_TX, fmt_rate(n.tx_kbps)),
                    Style::default().fg(HIGHLIGHT),
                ),
            ])
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " No active interfaces",
            Style::default().fg(DIM),
        )));
    }
    f.render_widget(Paragraph::new(lines), rows[3]);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} Uptime  ", ICON_UPTIME),
                Style::default().fg(DIM),
            ),
            Span::styled(
                SystemMonitor::uptime_str(),
                Style::default().fg(Color::White),
            ),
        ])),
        rows[4],
    );
}

fn render_storage(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let block = card(format!("{} Storage", ICON_DISK));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 10 || inner.height < 1 {
        return;
    }

    if sm.disk_info.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No storage found",
                Style::default().fg(DIM),
            ))),
            inner,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            sm.disk_info
                .iter()
                .take(inner.height as usize)
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(inner);

    for (d, row) in sm.disk_info.iter().zip(rows.iter()) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(24),
                Constraint::Length(9),
                Constraint::Min(12),
                Constraint::Length(26),
            ])
            .split(*row);

        let (icon, tag_color) = match d.kind {
            StorageKind::Main => (ICON_DISK, ACCENT),
            StorageKind::Usb => (ICON_USB, HIGHLIGHT),
            StorageKind::Network => (ICON_LAN, Color::Cyan),
        };
        let name = trunc(&d.name, 18);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {icon} "), Style::default().fg(tag_color)),
                Span::styled(format!("{name:<18}"), Style::default().fg(Color::White)),
            ])),
            cols[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                d.kind.tag(),
                Style::default().fg(tag_color),
            ))),
            cols[1],
        );

        let color = pct_color(d.pct as f32);
        f.render_widget(bar(d.pct, color), cols[2]);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {:>3.0}%", d.pct), Style::default().fg(color)),
                Span::styled(
                    format!("  {:.0}/{:.0} GB", d.used_gb, d.total_gb),
                    Style::default().fg(DIM),
                ),
            ])),
            cols[3],
        );
    }
}

fn fmt_rate(kbps: f64) -> String {
    if kbps >= 1024.0 {
        format!("{:.2} MB/s", kbps / 1024.0)
    } else {
        format!("{:.0} KB/s", kbps)
    }
}

fn pct_color(pct: f32) -> Color {
    if pct > 80.0 {
        Color::Red
    } else if pct > 50.0 {
        HIGHLIGHT
    } else {
        ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(w: u16, h: u16) -> String {
        let sm = SystemMonitor::new();
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render_sysmon(f, f.area(), &sm)).unwrap();
        println!("{}", term.backend());
        format!("{}", term.backend())
    }

    #[test]
    fn renders_every_card() {
        for (w, h) in [(110, 34), (160, 50)] {
            let out = draw(w, h);
            for card in ["CPU", "Memory", "Swap", "Networking", "Storage"] {
                assert!(out.contains(card), "missing {card} card at {w}x{h}:\n{out}");
            }
        }
    }

    #[test]
    fn renders_gpu_card_when_a_gpu_is_present() {
        let sm = SystemMonitor::new();
        if sm.gpu.is_none() {
            return; // no queryable GPU on this machine
        }
        let mut term = Terminal::new(TestBackend::new(110, 34)).unwrap();
        term.draw(|f| render_sysmon(f, f.area(), &sm)).unwrap();
        let out = format!("{}", term.backend());
        assert!(out.contains("GPU"), "missing GPU card:\n{out}");
    }

    #[test]
    fn survives_a_terminal_too_small_for_the_cards() {
        draw(24, 7);
    }
}
