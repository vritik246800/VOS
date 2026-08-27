#![allow(dead_code)]
use crate::core::state::PanelLayout;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

pub struct AppAreas {
    pub title: Rect,
    pub main: Rect,
    pub status: Rect,
}

pub fn base_layout(f: &Frame) -> AppAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    AppAreas {
        title: chunks[0],
        main: chunks[1],
        status: chunks[2],
    }
}

pub fn split_main(area: Rect, layout: &PanelLayout) -> (Rect, Option<Rect>) {
    match layout {
        PanelLayout::Single => (area, None),
        PanelLayout::HSplit(pct) => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(*pct),
                    Constraint::Percentage(100 - pct),
                ])
                .split(area);
            (chunks[0], Some(chunks[1]))
        }
        PanelLayout::VSplit(pct) => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(*pct),
                    Constraint::Percentage(100 - pct),
                ])
                .split(area);
            (chunks[0], Some(chunks[1]))
        }
    }
}
