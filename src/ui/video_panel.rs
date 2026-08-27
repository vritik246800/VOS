use crate::ui::icons::{ICON_MODE_VIDEO, ICON_PAUSE, ICON_PLAY, ICON_WARNING, MD_LOADING};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

use crate::video::player::VideoPlayer;

/// Render the video panel.
///
/// Returns `Some(inner_rect)` when a Kitty frame should be injected into the
/// video area, or `None` when showing an error / loading placeholder.
pub fn render_video_panel(f: &mut Frame, area: Rect, player: &mut VideoPlayer) -> Option<Rect> {
    // Reserve rows: Min(1) for video, 1 for gauge, 1 for status, 2 for controls
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(area);

    // Keep the player's render dimensions in sync with the available area.
    let video_width = chunks[0].width.saturating_sub(2); // minus borders
    let video_height = chunks[0].height.saturating_sub(2); // minus borders
    player.resize_if_needed(video_width, video_height);

    // ── Video frame ──────────────────────────────────────────────────────────
    let border_style = Style::default().fg(Color::Cyan);
    let video_title = format!(" {} {} ", ICON_MODE_VIDEO, player.title);
    let frame_block = Block::default()
        .borders(Borders::ALL)
        .title(video_title.as_str())
        .border_style(border_style);

    let kitty_rect: Option<Rect>;

    if let Some(ref err) = player.error.clone() {
        let paragraph = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{} Error rendering video:", ICON_WARNING),
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                err.clone(),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Make sure ffmpeg is installed:",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  brew install ffmpeg",
                Style::default().fg(Color::Green),
            )),
        ])
        .block(frame_block);
        f.render_widget(paragraph, chunks[0]);
        kitty_rect = None;
    } else if player.current_raw_frame.is_none() {
        let hint = if player.is_loading {
            format!("Buffering... {}%", player.buffer_fill_pct())
        } else {
            "Press Space to play".to_string()
        };
        let paragraph = Paragraph::new(hint)
            .block(frame_block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, chunks[0]);
        kitty_rect = None;
    } else {
        // Render the border only; leave the inner cells untouched.
        // In Kitty mode: writing to cells erases the Kitty image, so we must
        // not touch the inner area — the image covers it anyway.
        // In half-block mode: render_main overwrites inner with rgb_to_halfblock_lines.
        let inner = frame_block.inner(chunks[0]);
        f.render_widget(frame_block, chunks[0]);
        kitty_rect = Some(inner);
    }

    // ── Progress gauge ───────────────────────────────────────────────────────
    let progress = player.progress_ratio() as f64;
    let pos = player.position_secs;
    let dur = player.duration_secs;
    let time_str = format!(
        "{:02}:{:02} / {:02}:{:02}",
        pos as u64 / 60,
        pos as u64 % 60,
        dur as u64 / 60,
        dur as u64 % 60,
    );
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(progress.clamp(0.0, 1.0))
        .label(time_str);
    f.render_widget(gauge, chunks[1]);

    // ── Playback status ──────────────────────────────────────────────────────
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
    let status = Paragraph::new(status_text).style(Style::default().fg(status_color));
    f.render_widget(status, chunks[2]);

    // ── Controls hint ────────────────────────────────────────────────────────
    let controls = Paragraph::new("Space=play/pause  ←/→=5s  Esc/q=close")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(controls, chunks[3]);

    kitty_rect
}

// ──────────────────────────────────────────────────────────────────────────────
// ANSI → ratatui span parser (kept for potential fallback use)
// ──────────────────────────────────────────────────────────────────────────────

/// Parse a string that may contain `\x1b[38;2;R;G;Bm` (fg) and `\x1b[48;2;R;G;Bm` (bg)
/// sequences into a ratatui [`Line`] with styled [`Span`]s.
#[allow(dead_code)]
fn parse_ansi_line(s: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut fg = Color::White;
    let mut bg = Color::Black;
    let mut buf = String::new();

    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Detect ESC + '['
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            // Flush pending characters as a span before handling the escape
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(fg).bg(bg)));
                buf.clear();
            }

            // Find the terminating alphabetic byte
            let seq_start = i + 2;
            let seq_end = bytes[seq_start..]
                .iter()
                .position(|b| b.is_ascii_alphabetic())
                .map(|p| seq_start + p)
                .unwrap_or(len.saturating_sub(1));

            let seq = &s[seq_start..seq_end];

            if seq == "0" {
                // Reset
                fg = Color::White;
                bg = Color::Black;
            } else if let Some(rest) = seq.strip_prefix("38;2;") {
                if let Some(rgb) = parse_rgb(rest) {
                    fg = rgb;
                }
            } else if let Some(rest) = seq.strip_prefix("48;2;") {
                if let Some(rgb) = parse_rgb(rest) {
                    bg = rgb;
                }
            }

            // Advance past the terminating letter
            i = seq_end + 1;
        } else {
            // Regular UTF-8 character – advance by char boundary
            let c = s[i..].chars().next().unwrap_or('\u{FFFD}');
            buf.push(c);
            i += c.len_utf8();
        }
    }

    // Flush any remaining characters
    if !buf.is_empty() {
        spans.push(Span::styled(buf, Style::default().fg(fg).bg(bg)));
    }

    Line::from(spans)
}

/// Parse "R;G;B" into a [`Color::Rgb`].
#[allow(dead_code)]
fn parse_rgb(s: &str) -> Option<Color> {
    let mut parts = s.splitn(3, ';');
    let r: u8 = parts.next()?.parse().ok()?;
    let g: u8 = parts.next()?.parse().ok()?;
    // The third segment may have a trailing ';' from a combined sequence – take
    // only the leading digits.
    let b_str = parts.next()?;
    let b_end = b_str
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(b_str.len());
    let b: u8 = b_str[..b_end].parse().ok()?;
    Some(Color::Rgb(r, g, b))
}
