//! Docker Panel UI.
//!
//! Renders container/image/volume tabs driven by `DockerPanel` state.
//! This function is read-only — no state mutations occur here.

use crate::app::App;
use crate::modules::docker::DockerTab;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

// ── Public entry point ────────────────────────────────────────────────────────

pub fn render_docker_panel(f: &mut Frame, app: &App, area: Rect) {
    let dp = &app.docker_panel;

    if !dp.available {
        render_unavailable(f, area);
        return;
    }

    // Layout: tab bar (1 line) + content (rest) + hint bar (1 line)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar (inside a block)
            Constraint::Min(0),    // table
            Constraint::Length(1), // hint
        ])
        .split(area);

    render_tab_bar(f, app, rows[0]);

    match dp.tab {
        DockerTab::Containers => render_containers(f, app, rows[1]),
        DockerTab::Images => render_images(f, app, rows[1]),
        DockerTab::Volumes => render_volumes_placeholder(f, rows[1]),
    }

    render_hint(f, rows[2]);

    // Error overlay at the very bottom of the content area (above hint)
    if let Some(err) = &dp.error {
        render_error_line(f, rows[1], err);
    }
}

// ── Tab bar ───────────────────────────────────────────────────────────────────

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let dp = &app.docker_panel;

    let tabs = [
        ("Containers", &DockerTab::Containers),
        ("Images", &DockerTab::Images),
        ("Volumes", &DockerTab::Volumes),
    ];

    let spans: Vec<Span> = tabs
        .iter()
        .flat_map(|(label, variant)| {
            let active = &dp.tab == *variant;
            let style = if active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            vec![Span::styled(format!(" {label} "), style), Span::raw("  ")]
        })
        .collect();

    let status = dp.status_msg.clone();
    let tab_line = Line::from(spans);
    let status_line = Line::from(Span::styled(
        format!(" {status}"),
        Style::default().fg(Color::DarkGray),
    ));

    let para = Paragraph::new(vec![tab_line, status_line]).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Docker ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(para, area);
}

// ── Containers table ──────────────────────────────────────────────────────────

fn render_containers(f: &mut Frame, app: &App, area: Rect) {
    let dp = &app.docker_panel;

    // Header row
    let header_area = Rect { height: 1, ..area };
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    let header = Line::from(vec![
        Span::styled(
            format!(" {:<14}", "ID"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<20}", "NAME"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<22}", "IMAGE"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<22}", "STATUS"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "PORTS"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header).style(Style::default().bg(Color::DarkGray)),
        header_area,
    );

    let items: Vec<ListItem> = dp
        .containers
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let selected = i == dp.selected;
            let status_color = container_status_color(&c.status);

            let id_short = clip(&c.id, 12);
            let name = clip(&c.name, 20);
            let image = clip(&c.image, 22);
            let status = clip(&c.status, 22);
            let ports = clip(&c.ports, 30);

            let row_style = if selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!(" {id_short:<14}"), row_style.fg(Color::Gray)),
                Span::styled(format!(" {name:<20}"), row_style.fg(Color::White)),
                Span::styled(format!(" {image:<22}"), row_style.fg(Color::Yellow)),
                Span::styled(
                    format!(" {status:<22}"),
                    row_style.fg(status_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {ports}"), row_style.fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let count = dp.containers.len();
    let title = format!(" Containers ({count}) ");
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

    // We use a plain stateless render since selection is tracked in dp.selected.
    f.render_widget(list, list_area);
}

// ── Images table ──────────────────────────────────────────────────────────────

fn render_images(f: &mut Frame, app: &App, area: Rect) {
    let dp = &app.docker_panel;

    let header_area = Rect { height: 1, ..area };
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    let header = Line::from(vec![
        Span::styled(
            format!(" {:<14}", "ID"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<28}", "REPOSITORY"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<14}", "TAG"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<10}", "SIZE"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "CREATED"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header).style(Style::default().bg(Color::DarkGray)),
        header_area,
    );

    let items: Vec<ListItem> = dp
        .images
        .iter()
        .enumerate()
        .map(|(i, img)| {
            let selected = i == dp.selected;

            // Short 12-char ID (strip "sha256:" prefix if present)
            let raw_id = img.id.strip_prefix("sha256:").unwrap_or(&img.id);
            let id_short = clip(raw_id, 12);
            let repo = clip(&img.repository, 28);
            let tag = clip(&img.tag, 14);
            let size = clip(&img.size, 10);
            let created = clip(&img.created, 30);

            let row_style = if selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!(" {id_short:<14}"), row_style.fg(Color::Gray)),
                Span::styled(format!(" {repo:<28}"), row_style.fg(Color::White)),
                Span::styled(format!(" {tag:<14}"), row_style.fg(Color::Green)),
                Span::styled(format!(" {size:<10}"), row_style.fg(Color::Yellow)),
                Span::styled(format!(" {created}"), row_style.fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let count = dp.images.len();
    let title = format!(" Images ({count}) ");
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

    f.render_widget(list, list_area);
}

// ── Volumes placeholder ───────────────────────────────────────────────────────

fn render_volumes_placeholder(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Volumes ")
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pad = inner.height.saturating_sub(3) / 2;
    let msg_area = Rect {
        y: inner.y + pad,
        height: inner.height.saturating_sub(pad),
        ..inner
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Volume management — not yet implemented",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Use [Tab] to switch to Containers or Images",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), msg_area);
}

// ── Unavailability notice ─────────────────────────────────────────────────────

fn render_unavailable(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Docker ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pad = inner.height.saturating_sub(5) / 2;
    let msg_area = Rect {
        y: inner.y + pad,
        height: inner.height.saturating_sub(pad),
        ..inner
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Docker not available.",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Install docker or start the daemon.",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Esc — back",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), msg_area);
}

// ── Hint bar ─────────────────────────────────────────────────────────────────

fn render_hint(f: &mut Frame, area: Rect) {
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("switch tabs  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[s]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("start  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[S]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("stop  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[r]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("restart  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[d]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("delete  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[l]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("logs  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[F5]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("refresh  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("back", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(hint, area);
}

// ── Error line ────────────────────────────────────────────────────────────────

fn render_error_line(f: &mut Frame, area: Rect, err: &str) {
    if area.height == 0 {
        return;
    }
    // Overlay at the bottom row of the content area.
    let err_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    let msg = clip(err, area.width.saturating_sub(4) as usize);
    let para = Paragraph::new(Line::from(Span::styled(
        format!(" ! {msg}"),
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )));
    f.render_widget(para, err_area);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a container status string to a display colour.
fn container_status_color(status: &str) -> Color {
    let lower = status.to_lowercase();
    if lower.starts_with("up") {
        Color::Green
    } else if lower.starts_with("exited") || lower.starts_with("stopped") {
        Color::Red
    } else {
        Color::Yellow
    }
}

/// Truncate a string to at most `max` characters, appending `…` when clipped.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
