use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the image panel border and reserve the inner area for Kitty rendering.
///
/// Returns the inner [`Rect`] so the caller can set `app.pending_kitty`.
/// The inner area is filled with black spaces so ratatui does not paint over
/// the Kitty image that will be injected after `terminal.draw()`.
pub fn render_image_panel(f: &mut Frame, area: Rect, title: &str) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title))
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Fill inner area with black spaces to reserve it for Kitty.
    let blank_lines: Vec<Line> = (0..inner.height)
        .map(|_| {
            Line::from(vec![Span::styled(
                " ".repeat(inner.width as usize),
                Style::default().bg(Color::Black),
            )])
        })
        .collect();
    f.render_widget(Paragraph::new(blank_lines), inner);

    inner
}
