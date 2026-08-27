use crate::app::App;
use crate::modules::network::NetIface;
use crate::ui::icons::{ICON_ACTIVE, ICON_INACTIVE, ICON_NETWORK_RX, ICON_NETWORK_TX, ICON_UPTIME};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Sparkline},
};

pub fn render_network(f: &mut Frame, app: &mut App, area: Rect) {
    // Outer block
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Network Panel — ↑/↓ select   p ping   Esc close ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // Split: left = interface list, right = detail + chart + ping
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(inner);

    render_iface_list(f, app, cols[0]);
    render_detail(f, app, cols[1]);
}

fn render_iface_list(f: &mut Frame, app: &mut App, area: Rect) {
    let np = &app.network_panel;
    let items: Vec<ListItem> = np
        .ifaces
        .iter()
        .enumerate()
        .map(|(i, iface)| {
            let active = !iface.ips.is_empty();
            let color = if active {
                Color::Green
            } else {
                Color::DarkGray
            };
            let icon = if active { ICON_ACTIVE } else { ICON_INACTIVE };
            let label = format!(" {icon} {}", iface.name);
            let style = if i == np.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(if np.ifaces.is_empty() {
        None
    } else {
        Some(np.selected)
    });

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Interfaces ")
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let np = &app.network_panel;

    let pinging = np.is_pinging() || !np.ping_lines.is_empty();

    // Vertical split: detail+chart on top, ping panel below (if active)
    let rows = if pinging {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(100), Constraint::Min(0)])
            .split(area)
    };

    render_iface_detail(f, np.ifaces.get(np.selected), rows[0]);

    if pinging {
        render_ping_panel(f, app, rows[1]);
    }
}

fn render_iface_detail(f: &mut Frame, iface: Option<&NetIface>, area: Rect) {
    if let Some(iface) = iface {
        // Split detail area: info pane on top, two sparklines below
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),
                Constraint::Length(5),
                Constraint::Length(5),
            ])
            .split(area);

        // -- Info text
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Interface  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    iface.name.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  MAC        ", Style::default().fg(Color::DarkGray)),
                Span::styled(iface.mac.clone(), Style::default().fg(Color::White)),
            ]),
        ];

        if iface.ips.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  IPs        ", Style::default().fg(Color::DarkGray)),
                Span::styled("(none)", Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            for (j, ip) in iface.ips.iter().enumerate() {
                let label = if j == 0 {
                    "  IPs        ".to_string()
                } else {
                    "             ".to_string()
                };
                lines.push(Line::from(vec![
                    Span::styled(label, Style::default().fg(Color::DarkGray)),
                    Span::styled(ip.clone(), Style::default().fg(Color::Cyan)),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  RX total   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(iface.rx_total),
                Style::default().fg(Color::Green),
            ),
            Span::raw("   "),
            Span::styled(
                format!("{} rate ", ICON_NETWORK_RX),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format_rate(iface.rx_rate),
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  TX total   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(iface.tx_total),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("   "),
            Span::styled(
                format!("{} rate ", ICON_NETWORK_TX),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format_rate(iface.tx_rate),
                Style::default().fg(Color::Yellow),
            ),
        ]));

        let info = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", iface.name))
                .border_style(Style::default().fg(Color::Blue)),
        );
        f.render_widget(info, sections[0]);

        // -- RX sparkline
        let rx_data: Vec<u64> = iface.rx_history.iter().map(|&v| v as u64).collect();
        let rx_spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " {} RX  {} ",
                        ICON_NETWORK_RX,
                        format_rate(iface.rx_rate)
                    ))
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .data(&rx_data)
            .style(Style::default().fg(Color::Green));
        f.render_widget(rx_spark, sections[1]);

        // -- TX sparkline
        let tx_data: Vec<u64> = iface.tx_history.iter().map(|&v| v as u64).collect();
        let tx_spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " {} TX  {} ",
                        ICON_NETWORK_TX,
                        format_rate(iface.tx_rate)
                    ))
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .data(&tx_data)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(tx_spark, sections[2]);
    } else {
        let msg = Paragraph::new(Line::from(Span::styled(
            " No interfaces detected.",
            Style::default().fg(Color::DarkGray),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail ")
                .border_style(Style::default().fg(Color::Blue)),
        );
        f.render_widget(msg, area);
    }
}

fn render_ping_panel(f: &mut Frame, app: &App, area: Rect) {
    let np = &app.network_panel;
    let title = if np.is_pinging() {
        format!(" Ping {} (running…) ", np.ping_host)
    } else {
        format!(" Ping {} (done) ", np.ping_host)
    };

    let lines: Vec<Line> = np
        .ping_lines
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(Color::Cyan))))
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(paragraph, area);
}

// ── Formatting helpers ────────────────────────────────────────────────────────

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.2} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_rate(bytes_per_sec: f32) -> String {
    if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1_024.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1_024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}
