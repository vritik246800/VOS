use crate::app::Tab;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

#[allow(dead_code)]
pub fn render_tabs(f: &mut Frame, area: Rect, tabs: &[Tab], active: usize) {
    let titles: Vec<Line> = tabs
        .iter()
        .map(|t| Line::from(Span::raw(format!(" {} ", t.title))))
        .collect();

    let widget = Tabs::new(titles)
        .select(active)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw("│"));

    f.render_widget(widget, area);
}
