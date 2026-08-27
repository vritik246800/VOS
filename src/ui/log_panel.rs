//! Log Viewer UI — renders log lines with level highlight and a status footer.
//! This file contains ZERO state mutations; it is read-only on `App`.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::modules::logview::{LogLevel, LogSource};

/// Map a `LogLevel` to a terminal color.
fn level_color(level: &LogLevel) -> Color {
    match level {
        LogLevel::Error => Color::Red,
        LogLevel::Warn => Color::Yellow,
        LogLevel::Info => Color::Green,
        LogLevel::Debug => Color::Blue,
        LogLevel::Trace => Color::DarkGray,
        LogLevel::Unknown => Color::White,
    }
}

/// Short name for display in the status bar.
fn level_label(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "ERR",
        LogLevel::Warn => "WARN",
        LogLevel::Info => "INFO",
        LogLevel::Debug => "DBG",
        LogLevel::Trace => "TRACE",
        LogLevel::Unknown => "ALL",
    }
}

/// Source label shown in the panel title.
fn source_label(source: &LogSource) -> String {
    match source {
        LogSource::File(p) => p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string(),
        LogSource::Journalctl => "journalctl".to_string(),
        LogSource::AppDb => "app_logs (db)".to_string(),
        LogSource::Command(a) => a.first().cloned().unwrap_or_else(|| "cmd".to_string()),
    }
}

pub fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let lv = &app.log_viewer;

    // ── Layout: content area + 1-line footer ──────────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let content_area = chunks[0];
    let footer_area = chunks[1];

    // ── Title ─────────────────────────────────────────────────────────────────
    let follow_str = if lv.follow { "follow:on" } else { "follow:off" };
    let filter_str = match &lv.level_filter {
        Some(lvl) => format!("  level:{}", level_label(lvl)),
        None => String::new(),
    };
    let text_filter = if lv.text_filter.is_empty() {
        String::new()
    } else {
        format!("  /\"{}\"", lv.text_filter)
    };
    let source = source_label(&lv.source);
    let title = format!(" Log — {source}  [{follow_str}]{filter_str}{text_filter} ");

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(content_area);
    f.render_widget(block, content_area);

    // ── Collect visible lines ─────────────────────────────────────────────────
    let visible_indices = lv.apply_filters();
    let view_height = inner.height as usize;

    // Clamp scroll so we never go past the end.
    let scroll = lv.scroll.min(visible_indices.len().saturating_sub(1));

    let visible_slice = if visible_indices.is_empty() {
        vec![]
    } else {
        let end = (scroll + view_height).min(visible_indices.len());
        visible_indices[scroll..end].to_vec()
    };

    // ── Build list items ──────────────────────────────────────────────────────
    let items: Vec<ListItem> = visible_slice
        .iter()
        .map(|&idx| {
            let log_line = &lv.lines[idx];
            let raw = &log_line.raw;
            let color = level_color(&log_line.level);

            let mut spans: Vec<Span> = Vec::new();

            // Timestamp span (DarkGray)
            if let Some(ref ts_range) = log_line.ts_range {
                let ts_end = ts_range.end.min(raw.len());
                if ts_end > 0 {
                    spans.push(Span::styled(
                        &raw[..ts_end],
                        Style::default().fg(Color::DarkGray),
                    ));
                    let rest = &raw[ts_end..];
                    if !rest.is_empty() {
                        spans.push(Span::styled(rest, Style::default().fg(color)));
                    }
                } else {
                    spans.push(Span::styled(raw.as_str(), Style::default().fg(color)));
                }
            } else {
                spans.push(Span::styled(raw.as_str(), Style::default().fg(color)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    if items.is_empty() {
        let placeholder = if lv.lines.is_empty() {
            Paragraph::new(" (no lines yet — waiting for data…)")
                .style(Style::default().fg(Color::DarkGray))
        } else {
            Paragraph::new(" (no lines match the current filter)")
                .style(Style::default().fg(Color::DarkGray))
        };
        f.render_widget(placeholder, inner);
    } else {
        let list = List::new(items);
        f.render_widget(list, inner);
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    let line_count = visible_indices.len();
    let pos_str = if line_count == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", scroll + 1, line_count)
    };

    let footer_spans = vec![
        Span::styled(" [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "f",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("]ollow  [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "e",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled("]rr  [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "w",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("]arn  [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "i",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("]nfo  [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "/",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("]filter  [", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("]close", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("  {pos_str}"), Style::default().fg(Color::DarkGray)),
    ];

    let footer = Paragraph::new(Line::from(footer_spans)).style(Style::default().bg(Color::Reset));
    f.render_widget(footer, footer_area);
}
