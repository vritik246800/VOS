use crate::terminal::TerminalPane;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn render_terminal(f: &mut Frame, area: Rect, panel: &TerminalPane, focused: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let visible = chunks[0].height.saturating_sub(2) as usize;
    let total = panel.output.len();
    let start = total.saturating_sub(visible);

    let items: Vec<ListItem> = panel.output[start..]
        .iter()
        .map(|line| {
            let style = if line.starts_with("$ ") {
                Style::default().fg(Color::Yellow)
            } else if line.starts_with("✗") {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(line.as_str()).style(style)
        })
        .collect();

    let border_color = if panel.running {
        Color::Yellow
    } else if focused {
        Color::Green
    } else {
        Color::DarkGray
    };
    let cwd_str = panel.cwd.display().to_string();
    let title = if panel.running {
        format!(" Terminal [running...] — {} ", cwd_str)
    } else {
        format!(" Terminal — {} ", cwd_str)
    };

    let output_widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.as_str())
            .border_style(Style::default().fg(border_color)),
    );
    f.render_widget(output_widget, chunks[0]);

    let prompt = format!("$ {}_", panel.input);
    let input_widget = Paragraph::new(prompt)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(input_widget, chunks[1]);
}
