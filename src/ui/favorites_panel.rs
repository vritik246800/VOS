//! Favorites popup overlay (3.6) — styled after the Command Palette.

use crate::ui::icons::{ICON_DIR, ICON_FILE_GENERIC, ICON_HIGHLIGHT, ICON_STAR};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
};

use crate::app::App;

/// Render the favorites list as a centered popup overlay.
pub fn render_favorites(f: &mut Frame, app: &mut App, area: Rect) {
    let w = (area.width * 2 / 3)
        .max(40)
        .min(area.width.saturating_sub(4));
    let h = ((app.favorites_list.len() as u16) + 2)
        .clamp(6, 20)
        .min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height / 6;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let items: Vec<ListItem> = if app.favorites_list.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  (no favorites — 'b' in File Manager adds one)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.favorites_list
            .iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.to_string_lossy().to_string());
                let is_dir = p.is_dir();
                let type_icon = if is_dir { ICON_DIR } else { ICON_FILE_GENERIC };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} {} ", ICON_STAR, type_icon),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("{name:<24}"),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}", p.display()),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect()
    };

    let hl_sym = format!("{} ", ICON_HIGHLIGHT);
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Favorites — Enter: open  Esc: close ")
                .border_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(hl_sym.as_str());

    f.render_stateful_widget(list, popup, &mut app.favorites_state);
}
