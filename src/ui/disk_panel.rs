use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::fs::explorer::FileExplorer;
use crate::modules::disk::DiskEntry;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const BAR_WIDTH: u64 = 20;

pub fn render_disk(f: &mut Frame, app: &mut App, area: Rect) {
    let dm = &mut app.disk_manager;

    // ── Layout: header (3 lines) + list (rest) + footer (1 line) ────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Header ───────────────────────────────────────────────────────────────
    let spinner = if dm.scanning {
        let frame = SPINNER_FRAMES[dm.spinner_tick % SPINNER_FRAMES.len()];
        format!(" {} Scanning… {} entries", frame, dm.scan_count)
    } else {
        format!(
            " Total: {}  ({} items)",
            FileExplorer::format_size(dm.total),
            dm.entries.len()
        )
    };

    let path_str = dm.current.to_string_lossy().to_string();
    let header_text = vec![
        Line::from(vec![
            Span::styled(" Path: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                path_str,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![Span::styled(
            spinner,
            Style::default().fg(Color::Yellow),
        )]),
    ];

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Disk Manager (ncdu) ")
            .border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    f.render_widget(header, chunks[0]);

    // ── Entry list ───────────────────────────────────────────────────────────
    let entries = dm.entries.clone();
    let total = dm.total.max(1);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| build_list_item(entry, total))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, chunks[1], &mut dm.list_state);

    // ── Footer (key hints) ────────────────────────────────────────────────────
    let footer = Paragraph::new(Line::from(vec![
        hint("↑/↓", "select"),
        Span::raw("  "),
        hint("Enter", "enter dir"),
        Span::raw("  "),
        hint("Bksp/←", "up"),
        Span::raw("  "),
        hint("d", "delete"),
        Span::raw("  "),
        hint("F5", "re-scan"),
        Span::raw("  "),
        hint("Esc", "close"),
    ]))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);
}

/// Build a list item with a proportional bar, percentage, and human size.
fn build_list_item(entry: &DiskEntry, total: u64) -> ListItem<'static> {
    let pct = (entry.size * 100).checked_div(total).unwrap_or(0);
    let bar_filled = (pct * BAR_WIDTH / 100) as usize;
    let bar: String = "█".repeat(bar_filled) + &" ".repeat(BAR_WIDTH as usize - bar_filled);

    let size_str = FileExplorer::format_size(entry.size);
    let type_icon = if entry.is_dir { "▶ " } else { "  " };

    let line = Line::from(vec![
        Span::styled(type_icon.to_string(), Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("{:<40}", entry.name),
            Style::default().fg(if entry.is_dir {
                Color::Cyan
            } else {
                Color::White
            }),
        ),
        Span::styled(format!("[{bar}]"), Style::default().fg(Color::Green)),
        Span::styled(format!(" {:>3}%", pct), Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("  {:>10}", size_str),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    ListItem::new(line)
}

fn hint(key: &str, label: &str) -> Span<'static> {
    Span::styled(
        format!("[{key}]{label}"),
        Style::default().fg(Color::DarkGray),
    )
}
