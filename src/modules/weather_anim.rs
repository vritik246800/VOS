//! Animated ASCII weather art (Phase 6.5+): one looping animation per WMO
//! weather-condition group, mirroring the static `weather_art(code)` in
//! [`weather`](crate::modules::weather).
//!
//! This is **pure data + a tiny selector function** — no ratatui, no `App`, no
//! networking. The caller advances a monotonic `anim_tick` counter (one bump per
//! UI tick) and we pick the visible frame internally (per-condition speed +
//! frame count), so the renderer never has to know how fast each animation runs.
//!
//! Frames use the SAME WMO code grouping as `weather.rs`:
//! `0` clear/sun · `1..=2` partly cloudy · `3` overcast · `45|48` fog ·
//! `51..=67` rain/drizzle · `71..=77` snow · `80..=82` showers ·
//! `95..=99` thunderstorm · `_` unknown.
//!
//! Plain unicode glyphs (`☀`, `≈`, `*`, …) are used directly in the literals,
//! exactly like the existing `weather.rs` art (this is a module file, not a
//! panel — no `\u{...}` escapes).

/// Number of rows in every art frame (fixed so the layout never jumps).
pub const ART_HEIGHT: usize = 4;

/// Tick divisor: how many `anim_tick` bumps elapse per visible frame change.
/// At the project's 16 ms tick, `30` ≈ one frame every ~480 ms (~2 fps) — a
/// slow, gentle cadence rather than a frantic flicker.
const SPEED: u64 = 30;

// ── Per-condition frame banks ───────────────────────────────────────────────
//
// Every frame is exactly `ART_HEIGHT` rows; rows within a frame are padded to a
// common width so the art does not jitter as it cycles.

// Layout grid: every row in every bank is exactly 13 columns wide so the art
// never jitters horizontally as it cycles. Cloud puff parts used across banks:
//   top  ".--."        (4 cols)
//   mid  ".-(    )."    (9 cols)
//   body "(___.__)__)" (11 cols)

/// Clear / sun: a sun core whose rays pulse between `\ | /` and `· - ·`.
const CLEAR: &[[&str; ART_HEIGHT]] = &[
    [
        "    \\ | /    ",
        "  --  ☀  --  ",
        "    / | \\    ",
        "             ",
    ],
    [
        "    · · ·    ",
        "  -.  ☀  .-  ",
        "    · · ·    ",
        "             ",
    ],
    [
        "    / | \\    ",
        "  --  ☀  --  ",
        "    \\ | /    ",
        "             ",
    ],
    [
        "    · · ·    ",
        "  '.  ☀  .'  ",
        "    · · ·    ",
        "             ",
    ],
];

/// Partly cloudy: small sun, cloud drifting one column right each frame.
const PARTLY: &[[&str; ART_HEIGHT]] = &[
    [
        "\\ / ☀        ",
        "     .--.    ",
        "   .-(    ). ",
        "  (___.__)__)",
    ],
    [
        "\\ / ☀        ",
        "    .--.     ",
        "  .-(    ).  ",
        " (___.__)__) ",
    ],
    [
        "\\ / ☀        ",
        "     .--.    ",
        "   .-(    ). ",
        "  (___.__)__)",
    ],
    [
        "\\ / ☀        ",
        "   .--.      ",
        " .-(    ).   ",
        "(___.__)__)  ",
    ],
];

/// Overcast: a solid cloud drifting left/right by one column.
const OVERCAST: &[[&str; ART_HEIGHT]] = &[
    [
        "  .--.       ",
        "  .-(    ).  ",
        " (___.__)__) ",
        "             ",
    ],
    [
        "   .--.      ",
        "   .-(    ). ",
        "  (___.__)__)",
        "             ",
    ],
    [
        "  .--.       ",
        "  .-(    ).  ",
        " (___.__)__) ",
        "             ",
    ],
    [
        " .--.        ",
        " .-(    ).   ",
        "(___.__)__)  ",
        "             ",
    ],
];

/// Fog: horizontal bands shifting sideways each frame.
const FOG: &[[&str; ART_HEIGHT]] = &[
    [
        " ≈ ≈ ≈ ≈ ≈ ≈ ",
        " ~ ~ ~ ~ ~ ~ ",
        " - - - - - - ",
        " ≈ ≈ ≈ ≈ ≈ ≈ ",
    ],
    [
        "≈ ≈ ≈ ≈ ≈ ≈  ",
        "~ ~ ~ ~ ~ ~  ",
        "- - - - - -  ",
        "≈ ≈ ≈ ≈ ≈ ≈  ",
    ],
    [
        " ≈ ≈ ≈ ≈ ≈ ≈ ",
        " ~ ~ ~ ~ ~ ~ ",
        " - - - - - - ",
        " ≈ ≈ ≈ ≈ ≈ ≈ ",
    ],
    [
        "  ≈ ≈ ≈ ≈ ≈ ≈",
        "  ~ ~ ~ ~ ~ ~",
        "  - - - - - -",
        "  ≈ ≈ ≈ ≈ ≈ ≈",
    ],
];

/// Rain / drizzle: cloud on top, drops marching down the lower rows.
const RAIN: &[[&str; ART_HEIGHT]] = &[
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   ' ' ' '   ",
        "   . . . .   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   . . . .   ",
        "   ' ' ' '   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   | | | |   ",
        "   ' ' ' '   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   ' ' ' '   ",
        "   | | | |   ",
    ],
];

/// Snow: cloud on top, flakes drifting and falling between frames.
const SNOW: &[[&str; ART_HEIGHT]] = &[
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   *  .  *   ",
        "   .  *  .   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   .  *  .   ",
        "   *  .  *   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "    *  .  *  ",
        "   .  *  .   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   .  *  .   ",
        "    *  .  *  ",
    ],
];

/// Showers: heavier, faster rain — slashes shifting sideways.
const SHOWERS: &[[&str; ART_HEIGHT]] = &[
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   / / / /   ",
        "  / / / /    ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "  / / / /    ",
        "   / / / /   ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "   / / / /   ",
        "  / / / /    ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "  / / / /    ",
        "   / / / /   ",
    ],
];

/// Thunderstorm: cloud with a lightning bolt that flashes on and off.
const THUNDER: &[[&str; ART_HEIGHT]] = &[
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "     /_      ",
        "      /      ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "             ",
        "             ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "     Z       ",
        "    /        ",
    ],
    [
        "  .-(    ).  ",
        " (___.__)__) ",
        "             ",
        "             ",
    ],
];

/// Unknown: a blinking question mark.
const UNKNOWN: &[[&str; ART_HEIGHT]] = &[
    [
        "      ?      ",
        "     ???     ",
        "      ?      ",
        "             ",
    ],
    [
        "             ",
        "      ?      ",
        "             ",
        "             ",
    ],
];

/// All frames for a WMO weather `code`, using the same grouping as `weather.rs`.
fn frames_for(code: u32) -> &'static [[&'static str; ART_HEIGHT]] {
    match code {
        0 => CLEAR,
        1..=2 => PARTLY,
        3 => OVERCAST,
        45 | 48 => FOG,
        51..=67 => RAIN,
        71..=77 => SNOW,
        80..=82 => SHOWERS,
        95..=99 => THUNDER,
        _ => UNKNOWN,
    }
}

/// ASCII-art rows for a WMO weather `code` at animation time `anim_tick`
/// (a monotonically increasing counter advanced once per UI tick). The function
/// picks the visible frame internally (its own speed + per-condition frame
/// count), so the caller just passes the raw counter.
pub fn weather_frame(code: u32, anim_tick: u64) -> [&'static str; ART_HEIGHT] {
    let bank = frames_for(code);
    let f = (anim_tick / SPEED) as usize;
    bank[f % bank.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One representative code per WMO group (matching `weather.rs`).
    const REPRESENTATIVES: &[u32] = &[0, 2, 3, 48, 61, 75, 81, 96, 9999];

    #[test]
    fn every_group_has_art_height_rows() {
        for &code in REPRESENTATIVES {
            for &tick in &[0u64, 6, 12, 18, 60] {
                let frame = weather_frame(code, tick);
                assert_eq!(
                    frame.len(),
                    ART_HEIGHT,
                    "code {code} tick {tick} wrong row count"
                );
                // Rows may be all spaces, but the slot must exist (never absent).
                assert_eq!(frame.len(), ART_HEIGHT);
            }
        }
    }

    #[test]
    fn animation_actually_cycles() {
        for &code in &[0u32, 61] {
            // Sample across a couple of SPEED cycles.
            let mut seen: Vec<[&str; ART_HEIGHT]> = Vec::new();
            for tick in 0..(SPEED * 8) {
                let frame = weather_frame(code, tick);
                if !seen.contains(&frame) {
                    seen.push(frame);
                }
            }
            assert!(
                seen.len() >= 2,
                "code {code} did not animate (only {} distinct frame(s))",
                seen.len()
            );
        }
    }

    #[test]
    fn rows_within_a_frame_have_equal_width() {
        for &code in REPRESENTATIVES {
            let frame = weather_frame(code, 0);
            let width = frame[0].chars().count();
            for (i, row) in frame.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    width,
                    "code {code} row {i} width mismatch ({:?})",
                    row
                );
            }
        }
    }
}
