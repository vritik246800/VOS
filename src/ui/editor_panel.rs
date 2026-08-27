use crate::editor::buffer::Buffer;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_editor_panel(f: &mut Frame, area: Rect, buffer: &Buffer, focused: bool) {
    if buffer.is_markdown() && buffer.md_view {
        crate::ui::md_panel::render_md_panel(f, area, buffer, focused);
        return;
    }

    let border_color = if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let modified = if buffer.is_modified {
        " [modified]"
    } else {
        " [saved]"
    };
    let md_hint = if buffer.is_markdown() {
        "  Ctrl+E=view"
    } else {
        ""
    };
    let name = buffer
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");
    let title = format!(" Editor: {}{}{} ", name, modified, md_hint);

    let (cursor_row, cursor_col) = buffer.cursor;
    let scroll = cursor_row.saturating_sub((area.height as usize).saturating_sub(3));

    let lines: Vec<Line> = buffer
        .lines
        .iter()
        .enumerate()
        .skip(scroll)
        .take(area.height as usize)
        .map(|(i, line)| {
            let num = Span::styled(
                format!("{:>4} ", i + 1),
                Style::default().fg(Color::DarkGray),
            );
            let content = if i == cursor_row {
                let col = cursor_col.min(line.len());
                let before = &line[..col];
                let cur_char = if col < line.len() {
                    line.chars().nth(col).unwrap_or(' ').to_string()
                } else {
                    " ".to_string()
                };
                let after = if col < line.len() {
                    &line[col + 1..]
                } else {
                    ""
                };
                Line::from(vec![
                    num,
                    Span::raw(before.to_string()),
                    Span::styled(cur_char, Style::default().bg(Color::White).fg(Color::Black)),
                    Span::raw(after.to_string()),
                ])
            } else {
                Line::from(vec![num, Span::raw(line.clone())])
            };
            content
        })
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.as_str())
            .border_style(Style::default().fg(border_color)),
    );
    f.render_widget(para, area);
}
