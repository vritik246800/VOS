use crate::app::App;
use crate::modules::services::{ServiceManager, SortBy};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

/// Full-screen Service Manager. On non-Linux / no-systemctl hosts it renders a
/// centered unavailability notice instead of the table.
pub fn render_services(f: &mut Frame, app: &mut App, area: Rect) {
    if !ServiceManager::available() {
        render_unavailable(f, area);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(34)])
        .split(area);

    render_list(f, cols[0], &mut app.service_manager);
    render_sidebar(f, cols[1], &app.service_manager);
}

fn render_unavailable(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Service Manager ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "systemd not available on this system",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The Service Manager requires Linux with systemctl in PATH.",
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            "Esc — back",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    // Vertically center-ish the message.
    let pad = inner.height.saturating_sub(lines.len() as u16) / 2;
    let msg_area = Rect {
        y: inner.y + pad,
        height: inner.height.saturating_sub(pad),
        ..inner
    };
    let para = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(para, msg_area);
}

fn active_color(active: &str) -> Color {
    match active {
        "active" => Color::Green,
        "failed" => Color::Red,
        "inactive" | "dead" => Color::DarkGray,
        "activating" | "reloading" | "deactivating" => Color::Yellow,
        _ => Color::Gray,
    }
}

fn render_list(f: &mut Frame, area: Rect, sm: &mut ServiceManager) {
    let sort_label = match sm.sort_by {
        SortBy::Name => "NAME",
        SortBy::Active => "STATE",
    };
    let title = if sm.filter.is_empty() {
        format!(
            " Services — {} units  sort: {} ",
            sm.filtered.len(),
            sort_label
        )
    } else {
        format!(
            " Services — {} units  sort: {}  filter: {} ",
            sm.filtered.len(),
            sort_label,
            sm.filter
        )
    };

    let header = Rect { height: 1, ..area };
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    let header_line = Line::from(vec![
        Span::styled(
            format!(" {:<34}", "UNIT"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<8}", "ACTIVE"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<10}", "SUB"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "DESCRIPTION"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(Color::DarkGray)),
        header,
    );

    let items: Vec<ListItem> = sm
        .visible_units()
        .iter()
        .map(|u| {
            let color = active_color(&u.active);
            let name = clip(&u.name, 34);
            let desc = clip(&u.description, 60);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {name:<34}"), Style::default().fg(Color::White)),
                Span::styled(
                    format!(" {:<8}", u.active),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {:<10}", u.sub), Style::default().fg(Color::Gray)),
                Span::styled(format!(" {desc}"), Style::default().fg(Color::DarkGray)),
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

    f.render_stateful_widget(list, list_area, &mut sm.list_state);
}

fn render_sidebar(f: &mut Frame, area: Rect, sm: &ServiceManager) {
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Selected service",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if let Some(u) = sm.selected_unit() {
        push_kv(&mut lines, " Unit  ", &u.name, Color::White);
        push_kv(&mut lines, " Load  ", &u.load, Color::Gray);
        let ac = active_color(&u.active);
        lines.push(Line::from(vec![
            Span::styled(" State ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                u.active.clone(),
                Style::default().fg(ac).add_modifier(Modifier::BOLD),
            ),
        ]));
        push_kv(&mut lines, " Sub   ", &u.sub, Color::Gray);
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            wrap_desc(&u.description),
            Style::default().fg(Color::White),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " (no service selected)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    if let Some(err) = &sm.last_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(" ! {err}"),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Actions",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    for (key, label) in [
        ("s", "start"),
        ("t", "stop *"),
        ("r", "restart *"),
        ("e", "enable"),
        ("d", "disable *"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {key}  "),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(label.to_string(), Style::default().fg(Color::Gray)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "  (* confirms)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  1/2 sort · F5 refresh",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+F filter · type to find",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Details ")
            .border_style(Style::default().fg(Color::Blue)),
    );
    f.render_widget(para, area);
}

fn push_kv(lines: &mut Vec<Line>, key: &str, val: &str, color: Color) {
    lines.push(Line::from(vec![
        Span::styled(key.to_string(), Style::default().fg(Color::DarkGray)),
        Span::styled(val.to_string(), Style::default().fg(color)),
    ]));
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn wrap_desc(s: &str) -> String {
    // Single-line clip for the narrow sidebar.
    clip(s, 30)
}
