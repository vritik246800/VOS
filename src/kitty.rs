// Kitty terminal graphics protocol helpers.
// Write AFTER terminal.draw() to overlay images on the TUI.

use anyhow::Result;
use std::io::Write;

/// Query the pixel dimensions of one terminal cell.
/// Returns `(cell_width_px, cell_height_px)`.
/// Queries via crossterm `window_size()`; falls back to (14, 28) — a
/// sensible default for macOS Retina + Kitty — when the terminal does not
/// report pixel geometry.
pub fn cell_pixel_size() -> (u16, u16) {
    if let Ok(ws) = crossterm::terminal::window_size() {
        if ws.columns > 0 && ws.rows > 0 && ws.width > 0 && ws.height > 0 {
            let cw = ws.width / ws.columns;
            let ch = ws.height / ws.rows;
            if cw > 0 && ch > 0 {
                return (cw, ch);
            }
        }
    }
    (14, 28)
}

/// Returns true when running inside Kitty terminal.
///
/// Checks (in order):
///   $FORCE_KITTY_GRAPHICS=1  — manual override (always force Kitty protocol)
///   $TERM=xterm-kitty        — standard Kitty TERM string
///   $KITTY_WINDOW_ID         — set by Kitty for every window
///   $KITTY_PID               — set by Kitty since ≥0.26
///   $KITTY_INSTALLATION_DIR  — always set by Kitty
///   $TERM_PROGRAM=kitty      — set by Kitty on macOS
pub fn is_kitty_terminal() -> bool {
    if std::env::var("FORCE_KITTY_GRAPHICS").ok().as_deref() == Some("1") {
        return true;
    }
    std::env::var("TERM").ok().as_deref() == Some("xterm-kitty")
        || std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("KITTY_PID").is_ok()
        || std::env::var("KITTY_INSTALLATION_DIR").is_ok()
        || std::env::var("TERM_PROGRAM").ok().as_deref() == Some("kitty")
}

/// Convert raw RGB24 pixel data to half-block (▀) ratatui Lines for 24-bit terminals.
/// Each terminal row covers 2 pixel rows: fg = top pixel colour, bg = bottom pixel colour.
pub fn rgb_to_halfblock_lines(
    rgb: &[u8],
    px_w: u32,
    px_h: u32,
    cols: u16,
    rows: u16,
) -> Vec<ratatui::text::Line<'static>> {
    use image::{ImageBuffer, Rgb, imageops};
    use ratatui::{
        style::{Color, Style},
        text::{Line, Span},
    };

    let Some(img) = ImageBuffer::<Rgb<u8>, _>::from_raw(px_w, px_h, rgb.to_vec()) else {
        return vec![];
    };
    let target_w = (cols as u32).max(1);
    let target_h = ((rows as u32) * 2).max(2);
    let scaled = imageops::resize(&img, target_w, target_h, imageops::FilterType::Triangle);
    let sw = scaled.width();
    let sh = scaled.height();

    (0..rows as u32)
        .map(|row| {
            let top_y = (row * 2).min(sh.saturating_sub(1));
            let bot_y = (row * 2 + 1).min(sh.saturating_sub(1));
            let spans: Vec<Span<'static>> = (0..cols as u32)
                .map(|x| {
                    let xi = x.min(sw.saturating_sub(1));
                    let tp = scaled.get_pixel(xi, top_y);
                    let bp = scaled.get_pixel(xi, bot_y);
                    Span::styled(
                        "▀".to_string(),
                        Style::default()
                            .fg(Color::Rgb(tp[0], tp[1], tp[2]))
                            .bg(Color::Rgb(bp[0], bp[1], bp[2])),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Image ID 1 = ImageViewer, 2 = VideoPlayer.
/// Using stable IDs allows Kitty to replace frames in-place (no flicker)
/// and allows targeted deletion when the viewer is closed.
pub struct KittyFrame {
    pub image_id: u32,
    pub area: ratatui::layout::Rect,
    pub rgb_data: Vec<u8>, // RGB24 (3 bytes per pixel)
    pub px_width: u32,
    pub px_height: u32,
}

/// Transmit a frame via the Kitty graphics protocol.
/// Using a stable `image_id` means Kitty replaces the previous frame
/// with the same ID in-place — no delete-before-retransmit, no flicker.
pub fn inject_frame(frame: &KittyFrame) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());

    // Position cursor at top-left of the image area (1-indexed).
    write!(out, "\x1b[{};{}H", frame.area.y + 1, frame.area.x + 1)?;

    let b64 = base64_encode(&frame.rgb_data);
    let bytes = b64.as_bytes();
    let cols = frame.area.width;
    let rows = frame.area.height;
    let id = frame.image_id;

    const CHUNK: usize = 4096;
    let total = (bytes.len() + CHUNK - 1) / CHUNK;

    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        let m = if i + 1 == total { 0u8 } else { 1u8 };
        if i == 0 {
            write!(
                out,
                "\x1b_Ga=T,i={id},f=24,s={w},v={h},q=2,c={cols},r={rows},m={m};{data}\x1b\\",
                id = id,
                w = frame.px_width,
                h = frame.px_height,
                cols = cols,
                rows = rows,
                m = m,
                data = chunk_str
            )?;
        } else {
            write!(
                out,
                "\x1b_Gi={id},m={m};{data}\x1b\\",
                id = id,
                m = m,
                data = chunk_str
            )?;
        }
    }

    out.flush()?;
    Ok(())
}

/// Delete a specific Kitty image by ID. Call when closing the viewer.
pub fn clear_image(image_id: u32) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    write!(out, "\x1b_Ga=d,i={},q=2\x1b\\", image_id)?;
    out.flush()?;
    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
