//! Theme Switcher popup overlay (3.2)

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::theme::Theme;

/// Render the theme switcher as a centered popup overlay.
pub fn render_theme_switcher(f: &mut Frame, app: &mut App, area: Rect) {
    // Popup geometry: 60% width, 16 rows tall
    let w = ((area.width as u32 * 60 / 100) as u16)
        .max(50)
        .min(area.width.saturating_sub(4));
    let h = 16u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    // Outer border
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Theme Switcher — ↑↓ preview  Enter apply  Esc cancel ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Split inner into left (list) and right (preview)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(inner);

    let all = Theme::all();
    let selected_idx = app.theme_switcher_idx;

    // ── Left column: numbered theme list ────────────────────────────────────
    let items: Vec<ListItem> = all
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_sel = i == selected_idx;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![Span::styled(
                format!("  {}. {}", i + 1, name),
                style,
            )]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, cols[0]);

    // ── Right column: theme preview ──────────────────────────────────────────
    let preview_theme = Theme::by_name(all[selected_idx]);
    render_theme_preview(f, cols[1], &preview_theme);
}

fn render_theme_preview(f: &mut Frame, area: Rect, theme: &Theme) {
    if area.width < 4 || area.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // padding
            Constraint::Length(1), // theme name
            Constraint::Length(1), // padding
            Constraint::Length(1), // fake status bar
            Constraint::Length(1), // padding
            Constraint::Min(0),    // palette squares
        ])
        .split(area);

    // Theme name (accent color, bold)
    let name_line = Paragraph::new(Line::from(vec![Span::styled(
        format!("  {}", theme.name.to_uppercase()),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]));
    f.render_widget(name_line, chunks[1]);

    // Fake mini status bar using theme colors
    let bar_line = Paragraph::new(Line::from(vec![
        Span::styled(
            " MODE ",
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  sample text  ",
            Style::default().fg(theme.fg).bg(theme.bg),
        ),
        Span::styled(" ok ", Style::default().fg(theme.ok).bg(theme.bg)),
        Span::styled(" err ", Style::default().fg(theme.error).bg(theme.bg)),
    ]));
    f.render_widget(bar_line, chunks[3]);

    // 8 color swatches with labels
    let palette = [
        ("bg", theme.bg),
        ("fg", theme.fg),
        ("acc", theme.accent),
        ("sel", theme.selection),
        ("err", theme.error),
        ("wrn", theme.warn),
        ("ok", theme.ok),
        ("dim", theme.dim),
    ];

    let swatch_area = chunks[5];
    if swatch_area.height == 0 {
        return;
    }

    // Render all swatches on one row (or two rows if width is narrow)
    let items_per_row = ((swatch_area.width as usize) / 6).max(1).min(palette.len());
    let rows = (palette.len() + items_per_row - 1) / items_per_row;

    for row in 0..rows {
        if swatch_area.y + row as u16 >= swatch_area.y + swatch_area.height {
            break;
        }
        let row_area = Rect::new(
            swatch_area.x,
            swatch_area.y + row as u16,
            swatch_area.width,
            1,
        );
        let start = row * items_per_row;
        let end = (start + items_per_row).min(palette.len());
        let spans: Vec<Span> = palette[start..end]
            .iter()
            .flat_map(|(label, color)| {
                vec![
                    Span::styled("  ", Style::default().bg(*color)),
                    Span::styled(format!("{label} "), Style::default().fg(Color::DarkGray)),
                ]
            })
            .collect();
        f.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }
}
