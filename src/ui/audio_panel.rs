use crate::app::App;
use crate::modules::music_library::{LibraryFocus, LibraryView};
use crate::ui::icons::{ICON_MUSIC_NOTE, ICON_PAUSE, ICON_PLAY, ICON_VOLUME};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

pub fn render_audio_panel(f: &mut Frame, area: Rect, app: &App) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} Music Player ", ICON_MUSIC_NOTE))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // Vertical split: main content | progress (2 lines) | volume (1 line) | hints (1 line)
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // main content
            Constraint::Length(2), // progress gauge + time label
            Constraint::Length(1), // volume bar
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Main content: left column (35%) | right column (65%)
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(v_chunks[0]);

    // Left column: Library (top 55%) | Queue (bottom 45%)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(h_chunks[0]);

    render_library_pane(f, left_chunks[0], app);
    render_queue_pane(f, left_chunks[1], app);
    render_center_pane(f, h_chunks[1], app);
    render_progress_area(f, v_chunks[1], app);
    render_volume_bar(f, v_chunks[2], app);
    render_audio_hints(f, v_chunks[3]);
}

// ─── Library pane (top-left) ────────────────────────────────────────────────

fn render_library_pane(f: &mut Frame, area: Rect, app: &App) {
    let lib = &app.music_library;
    let focused = lib.focus == LibraryFocus::Library;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = match &lib.view {
        LibraryView::Artists => " Library — Artists ".to_string(),
        LibraryView::Albums => {
            let a = lib.selected_artist.as_deref().unwrap_or("?");
            format!(" {} > Albums ", a)
        }
        LibraryView::Tracks => {
            let alb = lib.selected_album.as_deref().unwrap_or("?");
            format!(" {} > Tracks ", alb)
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if lib.scanning {
        let msg = Paragraph::new(format!("Scanning... {} tracks", lib.scan_count))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(msg, inner);
        return;
    }

    match &lib.view {
        LibraryView::Artists => {
            if lib.artists.is_empty() {
                let msg = Paragraph::new(Line::from(vec![
                    Span::styled("No tracks. Press ", Style::default().fg(Color::DarkGray)),
                    Span::styled("F5", Style::default().fg(Color::Yellow)),
                    Span::styled(" to scan library", Style::default().fg(Color::DarkGray)),
                ]));
                f.render_widget(msg, inner);
            } else {
                let items: Vec<ListItem> = lib
                    .artists
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let selected = lib.artist_state.selected() == Some(i);
                        let style = if selected && focused {
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else if selected {
                            Style::default().fg(Color::Cyan)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        ListItem::new(format!("  {}", a)).style(style)
                    })
                    .collect();
                let mut state = lib.artist_state.clone();
                f.render_stateful_widget(List::new(items), inner, &mut state);
            }
        }
        LibraryView::Albums => {
            let items: Vec<ListItem> = lib
                .albums
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let selected = lib.album_state.selected() == Some(i);
                    let style = if selected && focused {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else if selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(format!("  {}", a)).style(style)
                })
                .collect();
            let mut state = lib.album_state.clone();
            f.render_stateful_widget(List::new(items), inner, &mut state);
        }
        LibraryView::Tracks => {
            let items: Vec<ListItem> = lib
                .tracks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let selected = lib.track_state.selected() == Some(i);
                    let style = if selected && focused {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else if selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let dur = t.fmt_duration();
                    ListItem::new(format!("  {:02}. {} [{}]", i + 1, t.title, dur)).style(style)
                })
                .collect();
            let mut state = lib.track_state.clone();
            f.render_stateful_widget(List::new(items), inner, &mut state);
        }
    }
}

// ─── Queue pane (bottom-left) ───────────────────────────────────────────────

fn render_queue_pane(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.music_library.focus == LibraryFocus::Queue;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Queue ")
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(ap) = &app.audio_player else {
        let msg = Paragraph::new("No player active").style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner);
        return;
    };

    if ap.playlist.is_empty() {
        let msg = Paragraph::new("Queue empty").style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = ap
        .playlist
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = p.file_stem().and_then(|n| n.to_str()).unwrap_or("?");
            let is_current = i == ap.playlist_idx;
            let selected = app.music_library.queue_state.selected() == Some(i);

            let prefix = if is_current {
                format!("{} ", ICON_PLAY)
            } else {
                "  ".to_string()
            };
            let style = if is_current && selected && focused {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if selected && focused {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(format!("{}{:02}. {}", prefix, i + 1, name)).style(style)
        })
        .collect();

    let mut state = app.music_library.queue_state.clone();
    f.render_stateful_widget(List::new(items), inner, &mut state);
}

// ─── Center pane (right column): cover art + track info + large EQ ──────────

fn render_center_pane(f: &mut Frame, area: Rect, app: &App) {
    let Some(ap) = &app.audio_player else {
        let icon = Paragraph::new(format!("\n\n  {} No music playing", ICON_MUSIC_NOTE))
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(icon, area);
        return;
    };

    // Vertical split within the right column:
    // cover area (~40%) | title | artist | album | large EQ bars (remainder)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // cover art or music note placeholder
            Constraint::Length(1),      // track title (bold)
            Constraint::Length(1),      // artist
            Constraint::Length(1),      // album
            Constraint::Min(0),         // large EQ bars
        ])
        .split(area);

    // Cover art (halfblock) or placeholder
    let (cover_w, cover_h) = ap.cover_dims;
    if let Some(rgb) = &ap.cover_art {
        if cover_w > 0 && cover_h > 0 {
            let lines = crate::kitty::rgb_to_halfblock_lines(
                rgb,
                cover_w,
                cover_h,
                chunks[0].width,
                chunks[0].height,
            );
            f.render_widget(Paragraph::new(lines), chunks[0]);
        } else {
            render_music_placeholder(f, chunks[0], &ap.track_name);
        }
    } else {
        render_music_placeholder(f, chunks[0], &ap.track_name);
    }

    // Track title
    let play_icon = if ap.is_playing { ICON_PLAY } else { ICON_PAUSE };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", play_icon),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                ap.track_name.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        chunks[1],
    );

    // Artist
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(
                "   {}",
                if ap.artist.is_empty() {
                    "Unknown Artist"
                } else {
                    ap.artist.as_str()
                }
            ),
            Style::default().fg(Color::Cyan),
        )),
        chunks[2],
    );

    // Album
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("   {}", ap.album.as_str()),
            Style::default().fg(Color::DarkGray),
        )),
        chunks[3],
    );

    // Large EQ visualization
    render_big_eq(f, chunks[4], &ap.eq_bars, ap.is_playing);
}

fn render_music_placeholder(f: &mut Frame, area: Rect, track_name: &str) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}  ", ICON_MUSIC_NOTE),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", track_name),
            Style::default().fg(Color::White),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::NONE)),
        area,
    );
}

// ─── Full-width progress bar (2 lines: gauge + time label) ──────────────────

fn render_progress_area(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let Some(ap) = &app.audio_player else {
        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(Color::DarkGray).bg(Color::DarkGray))
                .ratio(0.0),
            chunks[0],
        );
        f.render_widget(
            Paragraph::new("  0:00 / 0:00").style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
        return;
    };

    let ratio = (ap.progress_ratio() as f64).clamp(0.0, 1.0);
    let progress = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(Color::Magenta)
                .bg(Color::Rgb(40, 20, 40)),
        )
        .ratio(ratio);
    f.render_widget(progress, chunks[0]);

    let pos = fmt_secs(ap.position_secs);
    let dur = fmt_secs(ap.duration_secs);
    let play_sym = if ap.is_playing { ICON_PLAY } else { ICON_PAUSE };
    f.render_widget(
        Paragraph::new(format!("  {} {} / {}", play_sym, pos, dur))
            .style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

// ─── Full-width volume bar ───────────────────────────────────────────────────

fn render_volume_bar(f: &mut Frame, area: Rect, app: &App) {
    let Some(ap) = &app.audio_player else {
        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(Color::DarkGray).bg(Color::DarkGray))
                .ratio(0.0)
                .label(format!("{} --", ICON_VOLUME)),
            area,
        );
        return;
    };

    let vol_pct = (ap.volume * 100.0).round() as u16;
    let vol = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Rgb(10, 30, 10)))
        .ratio(ap.volume as f64)
        .label(format!("{} {}%", ICON_VOLUME, vol_pct));
    f.render_widget(vol, area);
}

// ─── Large EQ visualization (fills available height) ────────────────────────

fn render_big_eq(f: &mut Frame, area: Rect, bars: &[f32; 8], playing: bool) {
    if area.height == 0 || area.width < 8 {
        return;
    }

    let bar_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let (color_a, color_b) = if playing {
        (Color::Cyan, Color::Magenta)
    } else {
        (Color::Rgb(40, 40, 40), Color::Rgb(40, 40, 40))
    };

    let bar_width = ((area.width as usize) / (bars.len() * 2)).max(1);

    for row in 0..area.height {
        let threshold = 1.0 - (row as f32 / area.height as f32);
        let mut spans: Vec<Span> = Vec::new();

        for (i, &h) in bars.iter().enumerate() {
            let color = if i % 2 == 0 { color_a } else { color_b };
            let ch = if h >= threshold {
                let level = (h * (bar_chars.len() as f32 - 1.0)) as usize;
                bar_chars[level.min(bar_chars.len() - 1)]
            } else {
                ' '
            };
            let bar_str: String = std::iter::repeat(ch).take(bar_width).collect();
            spans.push(Span::styled(bar_str, Style::default().fg(color)));
            spans.push(Span::raw(" "));
        }

        let rect = Rect::new(area.x, area.y + row, area.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}

// ─── Hints bar ──────────────────────────────────────────────────────────────

fn render_audio_hints(f: &mut Frame, area: Rect) {
    let hints = Line::from(vec![
        Span::styled(" Tab", Style::default().fg(Color::Yellow)),
        Span::raw("=focus  "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw("=nav  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw("=play  "),
        Span::styled("a", Style::default().fg(Color::Yellow)),
        Span::raw("=add  "),
        Span::styled("d", Style::default().fg(Color::Yellow)),
        Span::raw("=del  "),
        Span::styled("Spc", Style::default().fg(Color::Yellow)),
        Span::raw("=⏯  "),
        Span::styled("←→", Style::default().fg(Color::Yellow)),
        Span::raw("=seek  "),
        Span::styled("m", Style::default().fg(Color::Yellow)),
        Span::raw("=mini  "),
        Span::styled("F5", Style::default().fg(Color::Yellow)),
        Span::raw("=scan  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw("=back"),
    ]);
    f.render_widget(
        Paragraph::new(hints).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn fmt_secs(secs: f64) -> String {
    let total = secs as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
