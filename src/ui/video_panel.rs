use crate::ui::icons::{ICON_MODE_VIDEO, ICON_PAUSE, ICON_PLAY, ICON_WARNING, MD_LOADING};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::video::player::{VideoPlayer, format_time};

#[derive(Clone, Copy)]
pub struct VideoPanelAreas {
    pub frame: Rect,
    pub progress: Rect,
    pub controls: Rect,
}

pub fn render_video_panel(f: &mut Frame, area: Rect, player: &mut VideoPlayer) -> VideoPanelAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let video_width = chunks[0].width.saturating_sub(2);
    let video_height = chunks[0].height.saturating_sub(2);
    player.resize_if_needed(video_width, video_height);

    let frame_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} {} ", ICON_MODE_VIDEO, player.title))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = frame_block.inner(chunks[0]);

    if let Some(ref error) = player.error.clone() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("{} Error rendering video:", ICON_WARNING),
                    Style::default().fg(Color::Red),
                )),
                Line::from(Span::styled(
                    error.clone(),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("Install ffmpeg and ffprobe, then reopen the file."),
            ])
            .block(frame_block),
            chunks[0],
        );
    } else if player.current_raw_frame.is_none() {
        let hint = if player.is_loading {
            format!("Buffering... {}%", player.buffer_fill_pct())
        } else {
            "Press Space or click to play".to_owned()
        };
        f.render_widget(
            Paragraph::new(hint)
                .block(frame_block)
                .style(Style::default().fg(Color::DarkGray)),
            chunks[0],
        );
    } else {
        f.render_widget(frame_block, chunks[0]);
    }

    let (_, length) = player.progress_layout(chunks[1].width);
    let played = (f32::from(length) * player.progress_ratio()).round() as u16;
    let shown = format_time(player.display_position());
    let total = format_time(player.duration_secs);
    let progress = Line::from(vec![
        Span::styled(format!(" {shown} "), Style::default().fg(Color::DarkGray)),
        Span::styled(
            "━".repeat(usize::from(played.min(length))),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "─".repeat(usize::from(length.saturating_sub(played))),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!(" {total} "), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(progress), chunks[1]);

    let (status_text, status_color) = if player.is_loading {
        (
            format!("{} Buffer {}%", MD_LOADING, player.buffer_fill_pct()),
            Color::Cyan,
        )
    } else if player.is_playing {
        (format!("{} Playing", ICON_PLAY), Color::Green)
    } else {
        (format!("{} Paused", ICON_PAUSE), Color::Yellow)
    };
    let status = Paragraph::new(Line::from(vec![
        Span::styled(status_text, Style::default().fg(status_color)),
        Span::raw("  "),
        Span::styled(player.status(), Style::default().fg(Color::DarkGray)),
        Span::raw("  Wheel=volume  Drag bar=seek  F=full"),
    ]));
    f.render_widget(status, chunks[2]);

    let mut buttons = vec![Span::raw(" ")];
    for (_, label) in player.buttons() {
        buttons.push(Span::styled(
            format!(" {label} "),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(buttons)), chunks[3]);

    VideoPanelAreas {
        frame: inner,
        progress: chunks[1],
        controls: chunks[3],
    }
}
