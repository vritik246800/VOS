use crate::ui::icons::{
    ICON_HIGHLIGHT, ICON_MODE_FILES, ICON_MODE_HELP, ICON_MODE_PROCESS, ICON_MODE_QUIT,
    ICON_MODE_TERMINAL,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub const MENU_LEN: usize = 5;

/// Returns the menu list area so callers can cache it for mouse hit-testing.
pub fn render_menu(f: &mut Frame, area: Rect, state: &mut ListState, logo_tick: u16) -> Rect {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // Left half: animated logo centred vertically and horizontally
    crate::ui::splash::render_logo(f, halves[0], logo_tick);

    // Right half: menu list
    let entries = [
        (
            format!("{}  File Manager", ICON_MODE_FILES),
            "Browse and manage files · Enter opens/edits · t side terminal",
        ),
        (
            format!("{}  Terminal", ICON_MODE_TERMINAL),
            "Async integrated shell",
        ),
        (
            format!("{}  Monitor", ICON_MODE_PROCESS),
            "CPU, RAM, Disk, Network, processes · kill",
        ),
        (
            format!("{}  Help", ICON_MODE_HELP),
            "Keyboard shortcuts and documentation",
        ),
        (format!("{}  Quit", ICON_MODE_QUIT), "Exit VOS"),
    ];

    let items: Vec<ListItem> = entries
        .iter()
        .map(|(label, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {label:<32}"),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let hl_sym = format!("{} ", ICON_HIGHLIGHT);
    let menu = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" VOS — Visual OS Shell ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(hl_sym.as_str());

    f.render_stateful_widget(menu, halves[1], state);
    halves[1]
}

pub fn render_tab_bar(f: &mut Frame, area: Rect, tabs: &[String], active: usize) {
    let mut spans = vec![Span::styled(
        " VOS ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    for (i, name) in tabs.iter().enumerate() {
        if i == active {
            spans.push(Span::styled(
                format!(" [{name}] "),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!("  {name}  "),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    f.render_widget(bar, area);
}
