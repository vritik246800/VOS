//! Calculator panel — animated bottom-sheet.
//!
//! Slides up from the bottom using `app.calculator.anim_pct`
//! (0 = hidden, 100 = fully open). Covers ~30% of the terminal height.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;

pub fn render_calc(f: &mut Frame, app: &App, area: Rect) {
    let calc = &app.calculator;

    if calc.anim_pct == 0 {
        return;
    }

    let max_h = ((area.height as u32 * 35 / 100) as u16)
        .max(8)
        .min(area.height);
    let visible_h = ((max_h as u32 * calc.anim_pct as u32) / 100) as u16;
    if visible_h < 2 {
        return;
    }

    let panel_y = area.y + area.height.saturating_sub(visible_h);
    let panel_rect = Rect::new(area.x, panel_y, area.width, visible_h);

    f.render_widget(Clear, panel_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Calculator ")
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(panel_rect);
    f.render_widget(block, panel_rect);

    if inner.height == 0 || inner.width < 4 {
        return;
    }

    let expr_h: u16 = 1;
    let result_h: u16 = if inner.height >= 2 { 1 } else { 0 };
    let history_h: u16 = inner.height.saturating_sub(expr_h + result_h);

    let mut constraints = vec![Constraint::Length(expr_h)];
    if result_h > 0 {
        constraints.push(Constraint::Length(result_h));
    }
    if history_h > 0 {
        constraints.push(Constraint::Length(history_h));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Expression — right-aligned, bold white
    let expr_display = if calc.input.is_empty() {
        "…".to_string()
    } else {
        format!("{}_", calc.input)
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            expr_display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Right),
        chunks[0],
    );

    // Result — right-aligned, grey
    if result_h > 0 && chunks.len() >= 2 {
        let (result_text, result_color) = match &calc.result {
            Ok(v) => (
                format!("= {}", format_result(*v)),
                Color::Rgb(160, 160, 160),
            ),
            Err(msg) if msg.is_empty() => (String::new(), Color::DarkGray),
            Err(_) => ("Error".to_string(), Color::Rgb(200, 80, 80)),
        };
        f.render_widget(
            Paragraph::new(Span::styled(result_text, Style::default().fg(result_color)))
                .alignment(Alignment::Right),
            chunks[1],
        );
    }

    // History — most recent first
    if history_h > 0 && chunks.len() >= 3 {
        let items: Vec<ListItem> = calc
            .history
            .iter()
            .rev()
            .take(history_h as usize)
            .map(|(expr, val)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{expr} = "), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format_result(*val),
                        Style::default().fg(Color::Rgb(120, 160, 120)),
                    ),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), chunks[2]);
    }
}

fn format_result(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.3}", v);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}
