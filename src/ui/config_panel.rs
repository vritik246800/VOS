use crate::config::settings::Settings;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

#[allow(dead_code)]
pub const CONFIG_FIELDS: usize = 5;

pub fn render_config_panel(f: &mut Frame, area: Rect, settings: &Settings, selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let rows: Vec<(&str, String, Vec<&str>)> = vec![
        ("theme", settings.theme.clone(), vec!["dark", "light"]),
        (
            "show hidden",
            fmt_bool(settings.show_hidden),
            vec!["false", "true"],
        ),
        (
            "mouse enabled",
            fmt_bool(settings.mouse_enabled),
            vec!["false", "true"],
        ),
        (
            "autosave",
            fmt_bool(settings.autosave),
            vec!["false", "true"],
        ),
        (
            "tab width",
            settings.tab_width.to_string(),
            vec!["2", "4", "8"],
        ),
    ];

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value, options))| {
            let is_selected = i == selected;

            // Find prev/next values for spinner display
            let cur_idx = options.iter().position(|o| o == value).unwrap_or(0);
            let prev = options[(cur_idx + options.len() - 1) % options.len()];
            let next = options[(cur_idx + 1) % options.len()];

            let label_span = Span::styled(
                format!("  {label:<20}"),
                Style::default().fg(if is_selected {
                    Color::Black
                } else {
                    Color::White
                }),
            );

            let spinner = if is_selected {
                Line::from(vec![
                    label_span,
                    Span::styled(
                        format!(" ◀ {prev} "),
                        Style::default().fg(Color::DarkGray).bg(Color::Yellow),
                    ),
                    Span::styled(
                        format!(" {} ", value),
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {next} ▶"),
                        Style::default().fg(Color::DarkGray).bg(Color::Yellow),
                    ),
                ])
            } else {
                Line::from(vec![
                    label_span,
                    Span::styled(format!("   {value}"), Style::default().fg(Color::Cyan)),
                ])
            };

            let style = if is_selected {
                Style::default().bg(Color::Yellow)
            } else {
                Style::default()
            };

            ListItem::new(spinner).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Settings  ↑↓ select  ←/→ change  Esc save and exit ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(list, chunks[0]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("←/→", Style::default().fg(Color::Yellow)),
        Span::raw(" or "),
        Span::styled("Enter/Space", Style::default().fg(Color::Yellow)),
        Span::raw(" change  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" save and exit"),
    ]))
    .style(Style::default().bg(Color::DarkGray));
    f.render_widget(hint, chunks[1]);
}

fn fmt_bool(b: bool) -> String {
    if b {
        "true".to_string()
    } else {
        "false".to_string()
    }
}
