use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

// ── Logo lines ────────────────────────────────────────────────────────────────

const LOGO: &[&str] = &[
    "       \u{2554}\u{2557}                   \u{2554}\u{2557}",
    "      \u{2554}\u{255d}\u{255a}\u{2557}                 \u{2554}\u{255d}\u{255a}\u{2557}",
    "      \u{255a}\u{2557}\u{2554}\u{255d}                 \u{255a}\u{2557}\u{2554}\u{255d}",
    "",
    "    \u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}   \u{25b2}",
    "    \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}  \u{2551}",
    "    \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}  \u{2551}",
    "    \u{255a}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}  \u{2551}",
    "     \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}  \u{2551}",
    "      \u{255a}\u{2550}\u{2550}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}\u{2550}\u{2550}\u{255d}",
];

const LOGO_W: u16 = 31;

// ── Pixel-matrix decode timing ─────────────────────────────────────────────────
//
// Each glyph cell resolves at its own tick (cascading left→right, top→bottom
// with per-cell jitter), flickering through random "matrix" glyphs before
// locking into the real character with a brief bright flash.

const ROW_STAGGER: u16 = 2;
const COL_STAGGER: u16 = 1;
const JITTER_RANGE: u16 = 10;
const LOCK_HOLD: u16 = 3;

pub const REVEAL_END: u16 =
    9 * ROW_STAGGER + (LOGO_W - 1) * COL_STAGGER + (JITTER_RANGE - 1) + LOCK_HOLD;

const LILAC: Color = Color::Rgb(180, 100, 255);
const MATRIX_GREEN: Color = Color::Rgb(70, 230, 130);
const MATRIX_GREEN_DIM: Color = Color::Rgb(20, 90, 55);

const MATRIX_CHARS: &[char] = &[
    '0', '1', '#', '%', '@', '*', '+', '=', '-', ':', '.', '~', '\u{2591}', '\u{2592}', '\u{2593}',
    '\u{2588}', '\u{256c}', '\u{253c}',
];

// ── Deterministic pseudo-random helpers (no external crate needed) ────────────

fn xorshift32(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

fn cell_hash(row: u16, col: u16, salt: u32) -> u32 {
    let seed = (row as u32)
        .wrapping_mul(73_856_093)
        .wrapping_add((col as u32).wrapping_mul(19_349_663))
        .wrapping_add(salt.wrapping_mul(83_492_791))
        .wrapping_add(0x9E37_79B9)
        .max(1);
    xorshift32(seed)
}

fn lock_tick_for(row: u16, col: u16) -> u16 {
    let jitter = (cell_hash(row, col, 0xABCD) % JITTER_RANGE as u32) as u16;
    row * ROW_STAGGER + col * COL_STAGGER + jitter
}

fn matrix_noise_char(row: u16, col: u16, tick: u16) -> char {
    let h = cell_hash(row, col, (tick / 2) as u32);
    MATRIX_CHARS[h as usize % MATRIX_CHARS.len()]
}

// ── Final (settled) brand styling per glyph cell ──────────────────────────────

fn final_style(row: u16, col: u16, phase2: u16) -> Style {
    match row {
        0..=2 => {
            let v_wave = (phase2 / 5) % 3;
            let is_bright = v_wave == row % 3;
            if is_bright {
                Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(LILAC)
            }
        }
        4..=8 if col < 21 => {
            let vos_pulse = (phase2 / 12) % 2 == 0;
            let m = if vos_pulse {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            Style::default().fg(Color::Cyan).add_modifier(m)
        }
        4..=8 => {
            let tip_bright = (phase2 / 8) % 2 == 0;
            if row == 4 && tip_bright {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            }
        }
        9 => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::White),
    }
}

// ── Render logo inside an arbitrary area (centered) ───────────────────────────

/// Renders the animated VD logo centred inside `area` as a pixel-matrix
/// decode: glyph cells flicker through random characters and resolve into
/// the real logo in a cascading, Matrix-style reveal. Call every tick;
/// animation freezes naturally once all cells have settled.
pub fn render_logo(f: &mut Frame, area: Rect, tick: u16) {
    let phase2 = tick.saturating_sub(REVEAL_END);

    let mut lines: Vec<Line> = Vec::with_capacity(LOGO.len());

    for (row, &text) in LOGO.iter().enumerate() {
        if text.is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        let row = row as u16;

        let spans: Vec<Span> = text
            .chars()
            .enumerate()
            .map(|(col, ch)| {
                if ch == ' ' {
                    return Span::raw(" ");
                }
                let col = col as u16;
                let lock_tick = lock_tick_for(row, col);

                if tick >= lock_tick + LOCK_HOLD {
                    Span::styled(ch.to_string(), final_style(row, col, phase2))
                } else if tick >= lock_tick {
                    Span::styled(
                        ch.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    let noise = matrix_noise_char(row, col, tick);
                    let near = lock_tick - tick < 6;
                    let style = if near {
                        Style::default()
                            .fg(MATRIX_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(MATRIX_GREEN_DIM)
                    };
                    Span::styled(noise.to_string(), style)
                }
            })
            .collect();

        lines.push(Line::from(spans));
    }

    let logo_h = lines.len() as u16;

    let x = area.x + area.width.saturating_sub(LOGO_W) / 2;
    let y = area.y + area.height.saturating_sub(logo_h) / 2;

    let render_w = LOGO_W.min(area.width.saturating_sub(x.saturating_sub(area.x)));
    let render_h = logo_h.min(area.height.saturating_sub(y.saturating_sub(area.y)));

    if render_w == 0 || render_h == 0 {
        return;
    }

    f.render_widget(Paragraph::new(lines), Rect::new(x, y, render_w, render_h));
}
