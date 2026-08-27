use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum WindowLayout {
    Monocle,
    HSplit(u16),
    VSplit(u16),
    Float { x: u16, y: u16, w: u16, h: u16 },
}

impl WindowLayout {
    pub fn split_areas(&self, area: Rect) -> (Rect, Option<Rect>) {
        match self {
            WindowLayout::Monocle | WindowLayout::Float { .. } => (area, None),
            WindowLayout::HSplit(pct) => {
                let pct = (*pct).clamp(20, 80);
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(pct),
                        Constraint::Percentage(100 - pct),
                    ])
                    .split(area);
                (chunks[0], Some(chunks[1]))
            }
            WindowLayout::VSplit(pct) => {
                let pct = (*pct).clamp(20, 80);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(pct),
                        Constraint::Percentage(100 - pct),
                    ])
                    .split(area);
                (chunks[0], Some(chunks[1]))
            }
        }
    }
}

pub fn base_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

pub fn with_side_panel(main: Rect, side_pct: u16) -> (Rect, Rect) {
    let side_pct = side_pct.clamp(0, 60);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(100 - side_pct),
            Constraint::Percentage(side_pct),
        ])
        .split(main);
    (chunks[0], chunks[1])
}
