use crate::core::state::{Notification, NotificationLevel};
use crate::ui::icons::{ICON_ERROR, ICON_INFO, ICON_SUCCESS, ICON_WARNING};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_notifications(f: &mut Frame, area: Rect, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }
    let visible: Vec<&Notification> = notifications.iter().rev().take(3).collect();
    let h = visible.len() as u16 + 2;
    let w = 50u16.min(area.width.saturating_sub(2));
    let x = area.width.saturating_sub(w + 1);
    let y = 1u16;
    let popup = Rect::new(x, y, w, h);

    let lines: Vec<Line> = visible
        .iter()
        .map(|n| {
            let (icon, color) = match n.level {
                NotificationLevel::Info => (ICON_INFO, Color::Cyan),
                NotificationLevel::Success => (ICON_SUCCESS, Color::Green),
                NotificationLevel::Warning => (ICON_WARNING, Color::Yellow),
                NotificationLevel::Error => (ICON_ERROR, Color::Red),
            };
            Line::from(vec![
                Span::styled(
                    format!(" {icon} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(n.message.clone()),
            ])
        })
        .collect();

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(widget, popup);
}
