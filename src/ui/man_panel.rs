//! Man Page Viewer UI panel.
//!
//! Render functions are read-only — zero state mutations here.
//! All state is owned by `ManViewer` in `modules/manpage.rs`.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::modules::manpage::{ManStyle, ManViewer};

// ── Public render entry point ─────────────────────────────────────────────────

/// Render the man page viewer into `area`.
///
/// Layout:
/// ```
/// ┌─ man <page_name> ────────────────────────────────────────────┐
/// │  <scrollable text>                                            │
/// └───────────────────────────────────────────────────────────────┘
/// [/]search  [n/N]next/prev  [PgUp/Dn]scroll  [q/Esc]back  N/Total
/// ```
pub fn render_man_panel(f: &mut Frame, viewer: &ManViewer, area: Rect) {
    // ── Layout: content block + 1-line status bar ──────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let content_area = chunks[0];
    let status_area = chunks[1];

    // ── Title / border ─────────────────────────────────────────────────────────
    let title = if viewer.loading {
        " man — loading… ".to_string()
    } else if let Some(ref err) = viewer.error {
        format!(
            " man — error: {} ",
            err.chars().take(40).collect::<String>()
        )
    } else if viewer.page_name.is_empty() {
        " man ".to_string()
    } else {
        format!(" man {} ", viewer.page_name)
    };

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

    // ── Body ───────────────────────────────────────────────────────────────────
    if viewer.loading {
        let spinner =
            Paragraph::new(" Loading manual page…").style(Style::default().fg(Color::DarkGray));
        f.render_widget(spinner, inner);
    } else if let Some(ref err) = viewer.error {
        let msg = Paragraph::new(format!(" Error: {err}")).style(Style::default().fg(Color::Red));
        f.render_widget(msg, inner);
    } else {
        render_body(f, viewer, inner);
    }

    // ── Status bar ─────────────────────────────────────────────────────────────
    render_status(f, viewer, status_area);
}

// ── Body renderer ─────────────────────────────────────────────────────────────

fn render_body(f: &mut Frame, viewer: &ManViewer, area: Rect) {
    let visible_h = area.height as usize;
    let total = viewer.lines.len();
    let start = viewer.scroll.min(total);
    let end = (start + visible_h).min(total);

    let q_lower = viewer.search_query.to_lowercase();

    let visible: Vec<Line<'static>> = viewer.lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, man_line)| {
            let line_idx = start + offset;
            let is_match = viewer.search_matches.contains(&line_idx);
            let is_current = viewer.search_matches.get(viewer.search_idx) == Some(&line_idx);

            let mut spans: Vec<Span<'static>> = Vec::new();

            for ms in &man_line.spans {
                let base_style = man_style_to_ratatui(&ms.style);

                if is_match && !q_lower.is_empty() {
                    // Highlight occurrences of the search query within the span text.
                    let text_lower = ms.text.to_lowercase();
                    let mut rest = ms.text.as_str();
                    let mut rest_lower = text_lower.as_str();

                    while let Some(pos) = rest_lower.find(&q_lower) {
                        // Text before match
                        if pos > 0 {
                            let before = rest[..pos].to_string();
                            let s = if is_current {
                                base_style.bg(Color::DarkGray)
                            } else {
                                base_style.bg(Color::DarkGray)
                            };
                            spans.push(Span::styled(before, s));
                        }
                        // The matched portion
                        let match_end = pos + q_lower.len();
                        let matched = rest[pos..match_end].to_string();
                        let highlight = if is_current {
                            base_style
                                .bg(Color::Yellow)
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            base_style.bg(Color::Yellow).fg(Color::Black)
                        };
                        spans.push(Span::styled(matched, highlight));

                        rest = &rest[match_end..];
                        rest_lower = &rest_lower[match_end..];
                    }
                    // Remainder after last match
                    if !rest.is_empty() {
                        let s = if is_match {
                            base_style.bg(Color::DarkGray)
                        } else {
                            base_style
                        };
                        spans.push(Span::styled(rest.to_string(), s));
                    }
                } else if is_match {
                    // Line is a match but no query string to highlight within
                    spans.push(Span::styled(
                        ms.text.clone(),
                        base_style.bg(Color::DarkGray),
                    ));
                } else {
                    spans.push(Span::styled(ms.text.clone(), base_style));
                }
            }

            Line::from(spans)
        })
        .collect();

    f.render_widget(Paragraph::new(visible), area);
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_status(f: &mut Frame, viewer: &ManViewer, area: Rect) {
    let total = viewer.lines.len();
    let pos = if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", viewer.scroll.saturating_add(1).min(total), total)
    };

    let match_info = if !viewer.search_matches.is_empty() {
        format!(
            "  match {}/{}",
            viewer.search_idx + 1,
            viewer.search_matches.len()
        )
    } else if !viewer.search_query.is_empty() {
        "  no matches".to_string()
    } else {
        String::new()
    };

    let key = |k: &'static str| {
        Span::styled(
            k,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };
    let sep = || Span::styled("  ", Style::default());
    let txt = |t: &'static str| Span::styled(t, Style::default().fg(Color::DarkGray));

    let spans = vec![
        key(" [/]"),
        txt("search"),
        sep(),
        key("[n/N]"),
        txt("next/prev match"),
        sep(),
        key("[PgUp/Dn]"),
        txt("scroll"),
        sep(),
        key("[q/Esc]"),
        txt("back"),
        Span::styled(match_info, Style::default().fg(Color::Cyan)),
        Span::styled(format!("  {pos}"), Style::default().fg(Color::DarkGray)),
    ];

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Reset)),
        area,
    );
}

// ── Style mapping ─────────────────────────────────────────────────────────────

fn man_style_to_ratatui(style: &ManStyle) -> Style {
    match style {
        ManStyle::Bold => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        ManStyle::Underline => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::UNDERLINED),
        ManStyle::Normal => Style::default().fg(Color::Gray),
        ManStyle::Dim => Style::default().fg(Color::DarkGray),
    }
}
