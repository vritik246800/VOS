use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_image_panel(f: &mut Frame, area: Rect, title: &str, status: &str) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} — {status} "))
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let hints = Paragraph::new(Line::from(vec![
        Span::styled(" Wheel ", Style::default().fg(Color::Yellow)),
        Span::raw("zoom  "),
        Span::styled(" Drag ", Style::default().fg(Color::Yellow)),
        Span::raw("pan  "),
        Span::styled(" Space ", Style::default().fg(Color::Yellow)),
        Span::raw("GIF  "),
        Span::styled(" 0 ", Style::default().fg(Color::Yellow)),
        Span::raw("fit  "),
        Span::styled(" F ", Style::default().fg(Color::Yellow)),
        Span::raw("full  "),
        Span::styled(" Esc ", Style::default().fg(Color::Yellow)),
        Span::raw("close"),
    ]));
    f.render_widget(hints, chunks[1]);
    inner
}
