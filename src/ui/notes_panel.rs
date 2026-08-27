//! Notes panel (Phase 6.1): list of notes on the left, live markdown preview
//! on the right (reusing `md_panel::parse_markdown`).

use crate::app::App;
use crate::ui::icons::ICON_FILE_GENERIC;
use crate::ui::md_panel::parse_markdown;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn render_notes(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(chunks[0]);

    render_list(f, app, cols[0]);
    render_preview(f, app, cols[1]);

    // Footer hints
    let hints = crate::ui::help_panel::keymap_for(&crate::core::state::AppMode::Notes);
    crate::ui::help_panel::render_key_hints(f, chunks[1], &hints);
}

fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    let count = app.notes.notes.len();
    let items: Vec<ListItem> = if count == 0 {
        vec![ListItem::new(Line::from(Span::styled(
            " No notes — press 'n' to create one",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.notes
            .notes
            .iter()
            .map(|n| {
                let mut spans = vec![
                    Span::styled(
                        format!("{ICON_FILE_GENERIC} "),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(n.title.clone(), Style::default().fg(Color::White)),
                ];
                if !n.tags.is_empty() {
                    spans.push(Span::styled(
                        format!("  #{}", n.tags.join(" #")),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    let title = format!(" Notes ({count}) ");
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.notes.list_state);
}

fn render_preview(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.notes.selected() {
        Some(n) => format!(" {} ", n.title),
        None => " Preview ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::DarkGray));

    if app.notes.preview.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            " Nothing to preview.",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        f.render_widget(msg, area);
        return;
    }

    let rendered = parse_markdown(&app.notes.preview_content());
    let para = Paragraph::new(rendered)
        .scroll((app.notes.preview_scroll as u16, 0))
        .block(block);
    f.render_widget(para, area);
}
