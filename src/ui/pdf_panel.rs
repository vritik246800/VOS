use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_pdf_panel(f: &mut Frame, area: Rect, lines: &[String], scroll: usize, title: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title))
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let visible_h = inner.height as usize;
    let start = scroll.min(lines.len());
    let end = (start + visible_h).min(lines.len());

    let visible: Vec<Line<'static>> = lines[start..end]
        .iter()
        .map(|l| Line::from(l.clone()))
        .collect();

    f.render_widget(
        Paragraph::new(visible).style(Style::default().fg(Color::White)),
        inner,
    );

    let info = if lines.is_empty() {
        "empty".to_string()
    } else {
        format!(
            "{}/{}",
            scroll.saturating_add(1).min(lines.len()),
            lines.len()
        )
    };

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ ", Style::default().fg(Color::Yellow)),
        Span::raw("scroll  "),
        Span::styled("PgUp/PgDn ", Style::default().fg(Color::Yellow)),
        Span::raw("page  "),
        Span::styled("Home/End ", Style::default().fg(Color::Yellow)),
        Span::raw("start/end  "),
        Span::styled(info, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("Esc ", Style::default().fg(Color::Yellow)),
        Span::raw("close"),
    ]))
    .style(Style::default().bg(Color::DarkGray));
    f.render_widget(hint, chunks[1]);
}
