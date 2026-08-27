use crate::audio::player::AudioPlayer;
use crate::ui::icons::{ICON_NEXT, ICON_PAUSE, ICON_PLAY, ICON_PREV};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};

/// Render the "Dynamic Island" music notch panel.
///
/// The panel is open at the top (no top border) so it looks embedded in the
/// terminal top edge. Two "ear" characters (╯ … ╰) are drawn just outside the
/// panel's top-left and top-right corners, giving the outward-curve effect
/// shown in the Dynamic Island design.
pub fn render_music_panel(f: &mut Frame, area: Rect, player: &AudioPlayer, focused: bool) {
    let border_color = if focused {
        Color::Cyan
    } else if player.is_playing {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    // ── "Ear" outward curves at the top corners ───────────────────────────────
    // ╯ left-ear  (draws as if the screen material curves outward into the notch)
    // ╰ right-ear
    if area.height > 0 {
        if area.x > 0 {
            let ear = Rect::new(area.x - 1, area.y, 1, 1);
            f.render_widget(
                Paragraph::new(Span::styled(
                    "\u{256e}", // ╮  curves down-left (top-right corner of outside space)
                    Style::default().fg(border_color),
                )),
                ear,
            );
        }
        let right_x = area.x + area.width;
        if right_x < f.area().width {
            let ear = Rect::new(right_x, area.y, 1, 1);
            f.render_widget(
                Paragraph::new(Span::styled(
                    "\u{256d}", // ╭  curves down-right (top-left corner of outside space)
                    Style::default().fg(border_color),
                )),
                ear,
            );
        }
    }

    // ── Panel block — open top (no top border) ───────────────────────────────
    let play_icon = if player.is_playing {
        ICON_PLAY
    } else {
        ICON_PAUSE
    };

    // Use LEFT | RIGHT | BOTTOM only so the top blends with the terminal edge.
    // The title is shown on the left border side-border at row 0 via a custom
    // first-row render instead (a Block title would require a top border).
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width < 4 {
        return;
    }

    // ── Adaptive vertical layout ──────────────────────────────────────────────
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut remaining = inner.height;

    // Row 0: play icon + scrolling track name (always shown)
    constraints.push(Constraint::Length(1));
    remaining = remaining.saturating_sub(1);

    // Row 1: artist name
    push_row(&mut constraints, &mut remaining);
    // Row 2: EQ bars
    push_row(&mut constraints, &mut remaining);
    // Row 3: progress gauge
    push_row(&mut constraints, &mut remaining);
    // Row 4: control hints
    push_row(&mut constraints, &mut remaining);
    // Fill
    constraints.push(Constraint::Min(0));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // ── Row 0: icon + scrolling track name ───────────────────────────────────
    let icon_str = format!("{play_icon} ");
    let icon_width = 2usize;
    let max_name = inner.width.saturating_sub(icon_width as u16 + 1) as usize;
    let name = player.track_name.as_str();
    let display_name = marquee(name, max_name, player.eq_tick);

    let title_line = Line::from(vec![
        Span::styled(
            icon_str,
            Style::default()
                .fg(if player.is_playing {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display_name,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(title_line), chunks[0]);

    // ── Row 1: artist ─────────────────────────────────────────────────────────
    if chunks[1].height > 0 {
        let artist = if player.artist.is_empty() {
            "\u{266b} Unknown Artist"
        } else {
            &player.artist
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                artist,
                Style::default().fg(Color::Rgb(160, 160, 200)),
            )),
            chunks[1],
        );
    }

    // ── Row 2: EQ bars ───────────────────────────────────────────────────────
    if chunks[2].height > 0 {
        render_eq_bars(f, chunks[2], &player.eq_bars, player.is_playing);
    }

    // ── Row 3: progress gauge ─────────────────────────────────────────────────
    if chunks[3].height > 0 {
        let ratio = (player.progress_ratio() as f64).clamp(0.0, 1.0);
        let pos = fmt_secs(player.position_secs);
        let dur = fmt_secs(player.duration_secs);
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(border_color).bg(Color::Rgb(30, 15, 30)))
            .ratio(ratio)
            .label(format!("{pos}/{dur}"));
        f.render_widget(gauge, chunks[3]);
    }

    // ── Row 4: key hints ──────────────────────────────────────────────────────
    if chunks[4].height > 0 {
        let hint = Line::from(vec![
            Span::styled(format!("{}", ICON_PREV), Style::default().fg(Color::Yellow)),
            Span::raw("\u{2190} "),
            Span::styled("Spc", Style::default().fg(Color::Yellow)),
            Span::raw("=\u{23ef} "),
            Span::styled(format!("{}", ICON_NEXT), Style::default().fg(Color::Yellow)),
            Span::raw("\u{2192} "),
            Span::styled(
                "q",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("=\u{2715}", Style::default().fg(Color::Red)),
        ]);
        f.render_widget(Paragraph::new(hint), chunks[4]);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn push_row(constraints: &mut Vec<Constraint>, remaining: &mut u16) {
    if *remaining >= 1 {
        constraints.push(Constraint::Length(1));
        *remaining -= 1;
    } else {
        constraints.push(Constraint::Length(0));
    }
}

fn marquee(text: &str, max_chars: usize, tick: u64) -> String {
    if text.len() <= max_chars || max_chars == 0 {
        return text.to_string();
    }
    let padded = format!("{text}   ");
    let chars: Vec<char> = padded.chars().collect();
    let offset = (tick as usize / 6) % chars.len();
    chars.iter().cycle().skip(offset).take(max_chars).collect()
}

fn render_eq_bars(f: &mut Frame, area: Rect, bars: &[f32; 8], playing: bool) {
    let bar_chars = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let (ca, cb) = if playing {
        (Color::Cyan, Color::Magenta)
    } else {
        (Color::Rgb(45, 45, 45), Color::Rgb(45, 45, 45))
    };

    let mut spans = Vec::new();
    for (i, &h) in bars.iter().enumerate() {
        let level = (h * (bar_chars.len() as f32 - 1.0)).round() as usize;
        let color = if i % 2 == 0 { ca } else { cb };
        spans.push(Span::styled(
            bar_chars[level.min(bar_chars.len() - 1)].to_string(),
            Style::default().fg(color),
        ));
        if i + 1 < bars.len() {
            spans.push(Span::raw(" "));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn fmt_secs(secs: f64) -> String {
    let t = secs as u64;
    format!("{}:{:02}", t / 60, t % 60)
}
