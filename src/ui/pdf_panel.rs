use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::modules::pdf::{PdfDocument, PdfMode};

#[derive(Clone, Copy)]
pub struct PdfPanelAreas {
    pub body: Rect,
    pub previous: Rect,
    pub next: Rect,
    pub mode: Rect,
}

pub fn render_pdf_panel(f: &mut Frame, area: Rect, document: &PdfDocument) -> PdfPanelAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let zoom = (document.view.zoom * 100.0).round() as u16;
    let title = format!(
        " {} — Page {}/{} — {}% ",
        document.title,
        document.page + 1,
        document.pages(),
        zoom
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    if document.mode == PdfMode::Text {
        let visible_height = inner.height as usize;
        let start = document.scroll.min(document.current_lines().len());
        let end = (start + visible_height).min(document.current_lines().len());
        let lines = document.current_lines()[start..end]
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).style(Style::default().fg(Color::White)),
            inner,
        );
    }

    let mode_label = if document.is_visual() {
        "Text"
    } else {
        "Visual"
    };
    let mode_button = format!(" {mode_label} ");
    let labels = [" < Prev ", " Next > ", mode_button.as_str()];
    let mut spans = Vec::new();
    let mut rectangles = Vec::new();
    let mut x = chunks[1].x;
    for label in labels {
        let width = label.chars().count() as u16;
        rectangles.push(Rect::new(x, chunks[1].y, width, 1));
        spans.push(Span::styled(
            label.to_owned(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        spans.push(Span::raw(" "));
        x = x.saturating_add(width + 1);
    }
    spans.push(Span::styled(
        if document.is_visual() {
            " Wheel=page  Drag=pan  +/-=zoom  0=fit  F=full  Esc=close"
        } else {
            " Wheel=scroll  PgUp/PgDn=page  Home/End=first/last  Esc=close"
        },
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), chunks[1]);

    PdfPanelAreas {
        body: inner,
        previous: rectangles[0],
        next: rectangles[1],
        mode: rectangles[2],
    }
}
