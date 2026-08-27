use crate::modules::process::{ProcessViewer, SortBy};
use crate::modules::sysmon::SystemMonitor;
use crate::ui::trunc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline},
};

/// Returns the process list area so callers can cache it for mouse hit-testing.
pub fn render_monitor(
    f: &mut Frame,
    area: Rect,
    pv: &mut ProcessViewer,
    sm: &SystemMonitor,
) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    render_sys_row(f, rows[0], sm);
    render_disk_net_row(f, rows[1], sm);
    render_processes(f, rows[2], pv)
}

fn render_sys_row(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cpu_pct = sm.cpu_pct();
    let cpu_color = pct_color(cpu_pct);
    let cpu_data: Vec<u64> = sm.cpu_history.iter().map(|&v| v as u64).collect();
    let cpu_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(cols[0]);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" ≈ CPU  {cpu_pct:.1}% "))
                .border_style(Style::default().fg(cpu_color)),
        )
        .gauge_style(Style::default().fg(cpu_color).bg(Color::DarkGray))
        .percent(cpu_pct as u16);
    f.render_widget(gauge, cpu_chunks[0]);
    let spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CPU History ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(&cpu_data)
        .style(Style::default().fg(cpu_color));
    f.render_widget(spark, cpu_chunks[1]);

    let ram_pct = sm.ram_pct();
    let ram_color = pct_color(ram_pct);
    let ram_data: Vec<u64> = sm.ram_history.iter().map(|&v| v as u64).collect();
    let ram_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(cols[1]);
    let ram_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " ▣ RAM  {:.0}/{:.0} MB ",
                    sm.used_ram_mb, sm.total_ram_mb
                ))
                .border_style(Style::default().fg(ram_color)),
        )
        .gauge_style(Style::default().fg(ram_color).bg(Color::DarkGray))
        .percent(ram_pct as u16);
    f.render_widget(ram_gauge, ram_chunks[0]);
    let ram_spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" RAM History ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(&ram_data)
        .style(Style::default().fg(ram_color));
    f.render_widget(ram_spark, ram_chunks[1]);
}

fn render_disk_net_row(f: &mut Frame, area: Rect, sm: &SystemMonitor) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let disk_spans: Vec<Span> = sm
        .disk_info
        .iter()
        .flat_map(|d| {
            let color = pct_color(d.pct as f32);
            vec![
                Span::styled(
                    format!(" {:<12}", trunc(&d.name, 12)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!(" {:.1}/{:.1}GB  ", d.used_gb, d.total_gb),
                    Style::default().fg(color),
                ),
            ]
        })
        .collect();
    let disk = Paragraph::new(Line::from(disk_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" ▤ Disks ")
            .border_style(Style::default().fg(Color::Blue)),
    );
    f.render_widget(disk, cols[0]);

    let uptime = SystemMonitor::uptime_str();
    // Only the interfaces actually moving bytes, busiest first — otherwise idle
    // pseudo-interfaces push the real one off the end of the row.
    let mut busiest: Vec<_> = sm
        .net_info
        .iter()
        .filter(|n| n.rx_kbps > 0.0 || n.tx_kbps > 0.0)
        .collect();
    busiest.sort_by(|a, b| {
        (b.rx_kbps + b.tx_kbps)
            .partial_cmp(&(a.rx_kbps + a.tx_kbps))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let net_spans: Vec<Span> = busiest
        .iter()
        .flat_map(|n| {
            vec![
                Span::styled(
                    format!(" {:<10}", trunc(&n.name, 10)),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("↓{:.1}KB ↑{:.1}KB  ", n.rx_kbps, n.tx_kbps),
                    Style::default().fg(Color::Green),
                ),
            ]
        })
        .collect();
    let net = Paragraph::new(Line::from(net_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" ~ Network  uptime: {uptime} "))
            .border_style(Style::default().fg(Color::Blue)),
    );
    f.render_widget(net, cols[1]);
}

fn render_processes(f: &mut Frame, area: Rect, pv: &mut ProcessViewer) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(30)])
        .split(area);

    let list_area = render_list(f, chunks[0], pv);
    render_sidebar(f, chunks[1], pv);
    list_area
}

fn render_list(f: &mut Frame, area: Rect, pv: &mut ProcessViewer) -> Rect {
    let sort_label = match pv.sort_by {
        SortBy::Cpu => "CPU",
        SortBy::Memory => "RAM",
        SortBy::Pid => "PID",
        SortBy::Name => "NAME",
    };
    let title = format!(
        " Processes — {} total  sort: {} ",
        pv.filtered.len(),
        sort_label
    );

    let header = Rect { height: 1, ..area };
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(3),
        ..area
    };

    let header_line = Line::from(vec![
        Span::styled(
            format!(" {:<8}", "PID"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<22}", "NAME"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>6}", "CPU%"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>8}", "MEM MB"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "STATE"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(Color::DarkGray)),
        header,
    );

    let items: Vec<ListItem> = pv
        .visible_processes()
        .iter()
        .map(|p| {
            let cpu_color = if p.cpu > 50.0 {
                Color::Red
            } else if p.cpu > 20.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            let mem_color = if p.mem_mb > 500.0 {
                Color::Red
            } else if p.mem_mb > 100.0 {
                Color::Yellow
            } else {
                Color::White
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<8}", p.pid),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!(" {:<22}", trunc(&p.name, 22)), Style::default()),
                Span::styled(
                    format!(" {:>5.1}%", p.cpu),
                    Style::default().fg(cpu_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {:>7.1}", p.mem_mb),
                    Style::default().fg(mem_color),
                ),
                Span::styled(
                    format!(" {}", p.status),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, list_area, &mut pv.list_state);
    list_area
}

fn render_sidebar(f: &mut Frame, area: Rect, pv: &ProcessViewer) {
    let selected = pv.selected_process();
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Selected process",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(p) = selected {
        lines.push(Line::from(vec![
            Span::styled(" # PID   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                p.pid.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" · Name  ", Style::default().fg(Color::DarkGray)),
            Span::styled(p.name.clone(), Style::default().fg(Color::White)),
        ]));
        let cpu_color = if p.cpu > 50.0 {
            Color::Red
        } else if p.cpu > 20.0 {
            Color::Yellow
        } else {
            Color::Green
        };
        lines.push(Line::from(vec![
            Span::styled(" ≈ CPU   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1}%", p.cpu),
                Style::default().fg(cpu_color).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ▣ RAM   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1} MB", p.mem_mb),
                Style::default().fg(Color::White),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            " None",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            " Commands",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " k  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" kill process"),
        ]),
        Line::from(vec![
            Span::styled(
                " F5 ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" refresh"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Sort",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " 1  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("by CPU"),
        ]),
        Line::from(vec![
            Span::styled(
                " 2  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("by RAM"),
        ]),
        Line::from(vec![
            Span::styled(
                " 3  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("by PID"),
        ]),
        Line::from(vec![
            Span::styled(
                " 4  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("by Name"),
        ]),
    ]);

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Info ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(widget, area);
}

fn pct_color(pct: f32) -> Color {
    if pct > 80.0 {
        Color::Red
    } else if pct > 50.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}
