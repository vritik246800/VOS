//! Weather panel: the saved-cities list on the left, and a dashboard for the
//! selected city on the right — a current-conditions card, a multi-day
//! forecast, a "Today's Highlight" grid (wind / UV / sun / humidity /
//! visibility / feels-like) and a weather-condition map.
//!
//! The layout, palette and animated charts follow `rust_apps/weather`; the
//! multi-city list, the DB-backed persistence and the °C/°F toggle are VOS's.
//! Below 76 columns the right-hand column (highlights + map) is dropped so the
//! now card and the forecast keep the full width.
//!
//! Add a city with `a` (geocoded by name), remove with `d`, refresh with `F5`
//! (or `r` to bypass the cache).
//!
//! Purely a renderer — all state changes happen in `handle_input`.

use std::f64::consts::TAU;

use crate::app::App;
use crate::modules::weather::{
    ForecastDay, SearchStatus, TempUnit, WeatherCity, WeatherData, WeatherStatus,
    weather_description, weather_glyph,
};
use crate::modules::weather_anim::{ART_HEIGHT, weather_frame};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
        canvas::{Canvas, Context, Line as CanvasLine, Points},
    },
};

// ── Palette (dark dashboard theme) ───────────────────────────────────────────

const PANEL: Color = Color::Rgb(18, 23, 30);
const CARD: Color = Color::Rgb(23, 29, 38);
const CARD_HI: Color = Color::Rgb(34, 43, 55);
const BORDER: Color = Color::Rgb(41, 51, 65);
const TEXT: Color = Color::Rgb(230, 236, 243);
const DIM: Color = Color::Rgb(125, 137, 152);
const ACCENT: Color = Color::Rgb(56, 189, 248);
const YELLOW: Color = Color::Rgb(250, 204, 21);
const GREEN: Color = Color::Rgb(74, 222, 128);
const TEAL: Color = Color::Rgb(45, 212, 191);
const BLUE: Color = Color::Rgb(59, 130, 246);
const ORANGE: Color = Color::Rgb(251, 146, 60);
const RED: Color = Color::Rgb(248, 113, 113);
const ROAD: Color = Color::Rgb(36, 45, 58);
const PILL: Color = Color::Rgb(30, 41, 59);

/// Highest UV index the gauge maps to a full arc.
const UV_MAX: f64 = 11.0;

/// Minimum width for the highlights + map column.
const WIDE_COLS: u16 = 76;

pub fn render_weather(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let cols = Layout::horizontal([Constraint::Length(26), Constraint::Min(0)])
        .spacing(1)
        .split(rows[0]);

    render_city_list(f, app, cols[0]);
    render_dashboard(f, app, cols[1]);

    // Footer hints.
    let hints: &[(&str, &str)] = &[
        ("↑/↓", "select"),
        ("a", "add city"),
        ("d", "remove"),
        ("u", "°C/°F"),
        ("F5", "refresh"),
        ("r", "force"),
        ("Esc", "menu"),
    ];
    crate::ui::help_panel::render_key_hints(f, rows[1], hints);

    // Add-city search overlay — drawn LAST so it sits on top of everything.
    if app.weather.search.active {
        render_search_overlay(f, app, area);
    }
}

/// A rounded card block in the dashboard palette.
fn card() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(CARD))
}

// ── City list (left) ─────────────────────────────────────────────────────────

fn render_city_list(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.weather;
    let block = card().style(Style::default().bg(PANEL)).title(Span::styled(
        format!(" Cities ({}) ", w.cities.len()),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    ));

    if w.cities.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            " No cities — press 'a' to add",
            Style::default().fg(DIM),
        )))
        .block(block);
        f.render_widget(msg, area);
        return;
    }

    let unit = w.unit;
    let items: Vec<ListItem> = w.cities.iter().map(|c| city_list_item(c, unit)).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(CARD_HI).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(w.selected.min(w.cities.len().saturating_sub(1))));
    f.render_stateful_widget(list, area, &mut state);
}

/// A two-line list entry: temperature + name on top, condition (or status text)
/// dim below.
fn city_list_item(c: &WeatherCity, unit: TempUnit) -> ListItem<'static> {
    let bold_accent = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let bold_text = Style::default().fg(TEXT).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(DIM);

    let (temp_span, sub_text, sub_style): (Span<'static>, String, Style) = match &c.status {
        WeatherStatus::Loaded => {
            if let Some(d) = &c.data {
                (
                    Span::styled(
                        format!("{:>3.0}{}  ", unit.convert(d.temp), unit.label()),
                        bold_accent,
                    ),
                    weather_description(d.weather_code).to_string(),
                    dim,
                )
            } else {
                (
                    Span::styled(format!("  --{}  ", unit.label()), dim),
                    "no data".to_string(),
                    dim,
                )
            }
        }
        WeatherStatus::Loading => (
            Span::styled("   …    ".to_string(), Style::default().fg(YELLOW)),
            "loading…".to_string(),
            Style::default().fg(YELLOW),
        ),
        WeatherStatus::Idle => (
            Span::styled(format!("  --{}  ", unit.label()), dim),
            "press F5".to_string(),
            dim,
        ),
        WeatherStatus::Error(msg) => (
            Span::styled("   !    ".to_string(), Style::default().fg(RED)),
            msg.clone(),
            Style::default().fg(RED),
        ),
    };

    let line1 = Line::from(vec![temp_span, Span::styled(c.name.clone(), bold_text)]);
    let line2 = Line::from(Span::styled(format!("   {sub_text}"), sub_style));
    ListItem::new(vec![line1, line2])
}

// ── Dashboard (right) ────────────────────────────────────────────────────────

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.weather;

    let Some(city) = w.cities.get(w.selected) else {
        render_notice(f, area, "No city selected — press 'a' to add", DIM);
        return;
    };
    match &city.status {
        WeatherStatus::Loading => {
            render_notice(f, area, "Loading weather…", ACCENT);
            return;
        }
        WeatherStatus::Error(msg) => {
            render_notice(f, area, &format!("Error: {msg}"), RED);
            return;
        }
        _ => {}
    }
    let Some(data) = &city.data else {
        render_notice(f, area, "Press F5 to fetch weather", DIM);
        return;
    };
    render_dashboard_body(f, &city.name, data, w.unit, w.anim_tick, area);
}

/// The dashboard proper, decoupled from `App` so it can be rendered in tests.
fn render_dashboard_body(
    f: &mut Frame,
    name: &str,
    data: &WeatherData,
    unit: TempUnit,
    tick: u64,
    area: Rect,
) {
    if area.height < 12 || area.width < 26 {
        return;
    }

    // Both columns share the same vertical rhythm — section header, top panel,
    // section header, bottom panel — so panel edges line up across columns. The
    // now card has no header, so the row above it stays empty.
    let column = [
        Constraint::Length(1),
        Constraint::Percentage(48),
        Constraint::Length(1),
        Constraint::Percentage(52),
    ];

    let (left_area, right_area) = if area.width >= WIDE_COLS {
        let cols = Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)])
            .spacing(1)
            .split(area);
        (cols[0], Some(cols[1]))
    } else {
        (area, None)
    };

    let left = Layout::vertical(column).spacing(1).split(left_area);
    render_now_card(f, name, data, unit, tick, left[1]);
    section_header(f, left[2], "Forecast", Some("next days"));
    render_forecast(f, data, unit, tick, left[3]);

    let Some(right_area) = right_area else { return };
    let right = Layout::vertical(column).spacing(1).split(right_area);
    section_header(f, right[0], "Today's Highlight", None);
    render_highlights(f, data, unit, tick, right[1]);
    section_header(f, right[2], "Weather condition map", Some("24 hr"));
    render_map(f, name, data, unit, tick, right[3]);
}

fn section_header(f: &mut Frame, area: Rect, title: &str, right: Option<&str>) {
    let right_width = right.map(|r| r.chars().count() as u16).unwrap_or(0);
    let chunks =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).split(area);
    f.render_widget(
        Paragraph::new(Span::styled(
            title.to_string(),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        chunks[0],
    );
    if let Some(label) = right {
        f.render_widget(
            Paragraph::new(Span::styled(label.to_string(), Style::default().fg(DIM)))
                .alignment(Alignment::Right),
            chunks[1],
        );
    }
}

/// Current conditions: status line, animated art + temperature, location/time.
fn render_now_card(
    f: &mut Frame,
    name: &str,
    data: &WeatherData,
    unit: TempUnit,
    tick: u64,
    area: Rect,
) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 || inner.width < 14 {
        return;
    }

    // The card degrades from the top down as height runs out: the status line
    // goes first, then the ASCII art (leaving just the temperature + condition).
    let art_height = ART_HEIGHT as u16;
    let show_art = inner.width >= 30 && inner.height >= art_height + 4;
    let body_height = if show_art { art_height } else { 2 };
    let show_status = inner.height >= body_height + 4;

    let mut cursor = inner;
    if show_status {
        let row = take_row(&mut cursor, 1);
        let status = Layout::horizontal([Constraint::Min(0), Constraint::Length(3)]).split(row);
        f.render_widget(
            Paragraph::new(Span::styled("● LIVE", Style::default().fg(GREEN))),
            status[0],
        );
        f.render_widget(
            Paragraph::new("a +")
                .style(Style::default().fg(DIM))
                .alignment(Alignment::Right),
            status[1],
        );
    }

    let body = take_row(&mut cursor, body_height);
    let info_area = if show_art {
        let cols = Layout::horizontal([Constraint::Length(16), Constraint::Min(0)])
            .spacing(1)
            .split(body);
        let art: Vec<Line> = weather_frame(data.weather_code, tick)
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    (*l).to_string(),
                    Style::default().fg(art_color(data.weather_code)),
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(art).alignment(Alignment::Center), cols[0]);
        cols[1]
    } else {
        body
    };

    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    format!("{:.0}", unit.convert(data.temp)),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(unit.label(), Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("{} ", weather_glyph(data.weather_code)),
                    Style::default().fg(glyph_color(data.weather_code)),
                ),
                Span::styled(
                    weather_description(data.weather_code),
                    Style::default().fg(DIM),
                ),
            ]),
        ]),
        info_area,
    );

    if cursor.height >= 3 {
        let divider = take_row(&mut cursor, 1);
        f.render_widget(
            Paragraph::new("─".repeat(divider.width as usize)).style(Style::default().fg(BORDER)),
            divider,
        );
    }
    if cursor.height >= 2 {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("◎ ", Style::default().fg(ACCENT)),
                Span::styled(name.to_string(), Style::default().fg(DIM)),
            ])),
            take_row(&mut cursor, 1),
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("◷ ", Style::default().fg(ACCENT)),
                Span::styled(data.datetime.clone(), Style::default().fg(DIM)),
            ])),
            take_row(&mut cursor, 1),
        );
    }
}

/// Carves `height` rows off the top of `area`, shrinking it. The returned rect
/// is clamped to what is left, so it is never taller than `area` was.
fn take_row(area: &mut Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    let row = Rect { height, ..*area };
    area.y += height;
    area.height -= height;
    row
}

/// Colour for the big animated art of a WMO code group.
fn art_color(code: u32) -> Color {
    match code {
        0 => YELLOW,
        45 | 48 => DIM,
        51..=67 | 80..=82 => BLUE,
        71..=77 => TEXT,
        95..=99 => YELLOW,
        _ => ACCENT,
    }
}

/// Colour for the single-cell condition glyph.
fn glyph_color(code: u32) -> Color {
    match code {
        0 | 1 | 2 | 95..=99 => YELLOW,
        3 | 45 | 48 => DIM,
        _ => ACCENT,
    }
}

/// The next days as rows, plus a chance-of-rain card with an animated chart.
fn render_forecast(f: &mut Frame, data: &WeatherData, unit: TempUnit, tick: u64, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .spacing(1)
    .split(area);

    for (i, day) in data.forecast.iter().enumerate().take(4) {
        render_forecast_row(f, day, unit, chunks[i]);
    }

    // Chance-of-rain card.
    let block = card();
    let inner = block.inner(chunks[4]);
    f.render_widget(block, chunks[4]);
    if inner.height == 0 {
        return;
    }
    if inner.height < 3 || inner.width < 20 {
        compact_card_line(f, inner, "Chance of Rain", format!("{}%", data.rain_chance));
        return;
    }
    let cols = Layout::horizontal([Constraint::Length(16), Constraint::Min(0)]).split(inner);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Chance of Rain", Style::default().fg(DIM))),
            Line::from(""),
            Line::from(vec![
                Span::styled("☂ ", Style::default().fg(ACCENT)),
                Span::styled(
                    format!("{}%", data.rain_chance),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
            ]),
        ]),
        cols[0],
    );
    f.render_widget(pulse_chart(&to_series(&data.rain_24h), tick, 40), cols[1]);
}

fn render_forecast_row(f: &mut Frame, day: &ForecastDay, unit: TempUnit, area: Rect) {
    let base = Style::default().bg(CARD);
    f.render_widget(Block::default().style(base), area);

    let cols = Layout::horizontal([
        Constraint::Length(4),
        Constraint::Length(10),
        Constraint::Min(0),
        Constraint::Length(10),
    ])
    .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            weather_glyph(day.code),
            base.fg(glyph_color(day.code)),
        ))
        .alignment(Alignment::Center),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{:.0}°", unit.convert(day.high)),
                base.fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("/{:.0}°", unit.convert(day.low)), base.fg(DIM)),
        ])),
        cols[1],
    );
    f.render_widget(
        Paragraph::new(Span::styled(day.date.clone(), base.fg(DIM))),
        cols[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(day.weekday.clone(), base.fg(DIM))).alignment(Alignment::Right),
        cols[3],
    );
}

/// The 3×2 highlight grid: wind / UV / sun on top, humidity / visibility /
/// feels-like below.
fn render_highlights(f: &mut Frame, data: &WeatherData, unit: TempUnit, tick: u64, area: Rect) {
    let grid = Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)])
        .spacing(1)
        .split(area);
    let thirds = [
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ];
    let top = Layout::horizontal(thirds).spacing(1).split(grid[0]);
    let bottom = Layout::horizontal(thirds).spacing(1).split(grid[1]);

    render_wind_card(f, data, tick, top[0]);
    render_uv_card(f, data, tick, top[1]);
    render_sun_card(f, data, top[2]);

    mini_card(
        f,
        bottom[0],
        "Humidity",
        "≋",
        ACCENT,
        &data.humidity.to_string(),
        " %",
        match data.humidity {
            0..=29 => "The air is dry",
            30..=69 => "Comfortable humidity",
            _ => "The air feels humid",
        },
    );
    mini_card(
        f,
        bottom[1],
        "Visibility",
        "◉",
        BLUE,
        &format!("{:.0}", data.visibility_km),
        " km",
        if data.visibility_km >= 10.0 {
            "Clear line of sight"
        } else if data.visibility_km >= 2.0 {
            "Some haze in the air"
        } else {
            "Visibility is poor"
        },
    );
    let delta = data.feels_like - data.temp;
    mini_card(
        f,
        bottom[2],
        "Feels Like",
        "♨",
        ORANGE,
        &format!("{:.0}", unit.convert(data.feels_like)),
        unit.label(),
        if delta > 1.0 {
            "Feels warmer than it is"
        } else if delta < -1.0 {
            "Feels colder than it is"
        } else {
            "Matches the real temperature"
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn mini_card(
    f: &mut Frame,
    area: Rect,
    title: &str,
    icon: &str,
    icon_color: Color,
    value: &str,
    unit: &str,
    note: &str,
) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    if inner.height < 2 {
        compact_card_line(f, inner, title, format!("{value}{unit}"));
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let header = Layout::horizontal([Constraint::Min(0), Constraint::Length(3)]).split(rows[0]);
    f.render_widget(
        Paragraph::new(Span::styled(title.to_string(), Style::default().fg(DIM))),
        header[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            icon.to_string(),
            Style::default().fg(icon_color),
        ))
        .alignment(Alignment::Right),
        header[1],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                value.to_string(),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(unit.to_string(), Style::default().fg(DIM)),
        ])),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(Span::styled(note.to_string(), Style::default().fg(DIM)))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
}

fn render_wind_card(f: &mut Frame, data: &WeatherData, tick: u64, area: Rect) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    if inner.height < 2 {
        compact_card_line(f, inner, "Wind", format!("{:.1} km/h", data.wind_speed));
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled("Wind Status", Style::default().fg(DIM))),
        rows[0],
    );

    let value = Layout::horizontal([Constraint::Min(0), Constraint::Length(8)]).split(rows[1]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{:.1}", data.wind_speed),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" km/h", Style::default().fg(DIM)),
        ])),
        value[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            data.time_short.clone(),
            Style::default().fg(DIM),
        ))
        .alignment(Alignment::Right),
        value[1],
    );

    f.render_widget(pulse_chart(&to_series(&data.wind_24h), tick, 0), rows[2]);
}

fn render_uv_card(f: &mut Frame, data: &WeatherData, tick: u64, area: Rect) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    if inner.height < 2 {
        compact_card_line(f, inner, "UV", format!("{:.1}", data.uv_index));
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled("UV Index", Style::default().fg(DIM))),
        rows[0],
    );

    let frac = (data.uv_index / UV_MAX).clamp(0.0, 1.0);
    // The tip marker gently breathes.
    let beat = 1.0 + 0.3 * (tick as f64 / 3.0).sin();
    let gauge = Canvas::default()
        .background_color(CARD)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 60.0])
        .paint(move |ctx| {
            let (cx, cy, rx, ry) = (50.0, 12.0, 42.0, 42.0);
            let mut track = Vec::new();
            let mut active = Vec::new();
            let mut angle = 180.0_f64;
            while angle >= 0.0 {
                let rad = angle.to_radians();
                let point = (cx + rx * rad.cos(), cy + ry * rad.sin());
                if (180.0 - angle) / 180.0 <= frac {
                    active.push(point);
                } else {
                    track.push(point);
                }
                angle -= 1.5;
            }
            ctx.draw(&Points {
                coords: &track,
                color: BORDER,
            });
            ctx.draw(&Points {
                coords: &active,
                color: ACCENT,
            });

            // Value marker at the tip of the filled arc.
            let tip = (180.0 - 180.0 * frac).to_radians();
            let (tx, ty) = (cx + rx * tip.cos(), cy + ry * tip.sin());
            let dot: Vec<(f64, f64)> =
                [(0.0, 0.0), (1.4, 0.0), (-1.4, 0.0), (0.0, 1.8), (0.0, -1.8)]
                    .iter()
                    .map(|(dx, dy)| (tx + dx * beat, ty + dy * beat))
                    .collect();
            ctx.draw(&Points {
                coords: &dot,
                color: TEXT,
            });
        });
    f.render_widget(gauge, rows[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{:.1}", data.uv_index),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        rows[2],
    );
}

fn render_sun_card(f: &mut Frame, data: &WeatherData, area: Rect) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    if inner.height < 2 {
        compact_card_line(
            f,
            inner,
            "Sun",
            format!("{} / {}", data.sunrise, data.sunset),
        );
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled("Sunrise & Sunset", Style::default().fg(DIM))),
        rows[0],
    );

    let sun_frac = data.sun_frac;
    let arc = Canvas::default()
        .background_color(CARD)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 60.0])
        .paint(move |ctx| {
            let (cx, cy, rx, ry) = (50.0, 12.0, 42.0, 42.0);

            // Dotted sun path.
            let mut angle = 180.0_f64;
            let mut path = Vec::new();
            while angle >= 0.0 {
                let rad = angle.to_radians();
                path.push((cx + rx * rad.cos(), cy + ry * rad.sin()));
                angle -= 4.0;
            }
            ctx.draw(&Points {
                coords: &path,
                color: DIM,
            });

            // Horizon line.
            let mut horizon = Vec::new();
            let mut x = 4.0_f64;
            while x <= 96.0 {
                horizon.push((x, cy));
                x += 1.5;
            }
            ctx.draw(&Points {
                coords: &horizon,
                color: BORDER,
            });

            // The sun at its real position between sunrise and sunset.
            let sun_angle = (180.0 - 180.0 * sun_frac).to_radians();
            let (sx, sy) = (cx + rx * sun_angle.cos(), cy + ry * sun_angle.sin());
            let mut sun = Vec::new();
            let mut a = 0.0_f64;
            while a < TAU {
                sun.push((sx + 2.2 * a.cos(), sy + 2.8 * a.sin()));
                a += TAU / 10.0;
            }
            sun.push((sx, sy));
            ctx.draw(&Points {
                coords: &sun,
                color: YELLOW,
            });
        });
    f.render_widget(arc, rows[1]);

    let times =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑ ", Style::default().fg(YELLOW)),
            Span::styled(data.sunrise.clone(), Style::default().fg(TEXT)),
        ])),
        times[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↓ ", Style::default().fg(ORANGE)),
            Span::styled(data.sunset.clone(), Style::default().fg(TEXT)),
        ]))
        .alignment(Alignment::Right),
        times[1],
    );
}

/// Stylised (non-geographic) condition map: an abstract road grid with
/// breathing precipitation blobs and a marker carrying the real reading.
fn render_map(
    f: &mut Frame,
    name: &str,
    data: &WeatherData,
    unit: TempUnit,
    tick: u64,
    area: Rect,
) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 30 || inner.height < 8 {
        return;
    }

    let blob_pulse = 1.0 + 0.07 * (tick as f64 / 5.0).sin();
    let marker = Style::default()
        .fg(TEXT)
        .bg(PILL)
        .add_modifier(Modifier::BOLD);
    let label = format!(
        " {} {:.0}{} ",
        weather_glyph(data.weather_code),
        unit.convert(data.temp),
        unit.label()
    );
    let map = Canvas::default()
        .background_color(CARD)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 100.0])
        .paint(move |ctx| {
            const ROADS: &[&[(f64, f64)]] = &[
                &[
                    (0.0, 78.0),
                    (18.0, 70.0),
                    (40.0, 74.0),
                    (66.0, 64.0),
                    (100.0, 68.0),
                ],
                &[(8.0, 100.0), (14.0, 72.0), (10.0, 40.0), (22.0, 0.0)],
                &[(46.0, 100.0), (44.0, 60.0), (52.0, 30.0), (48.0, 0.0)],
                &[(0.0, 30.0), (26.0, 36.0), (58.0, 28.0), (100.0, 34.0)],
                &[
                    (72.0, 100.0),
                    (78.0, 74.0),
                    (74.0, 44.0),
                    (84.0, 12.0),
                    (100.0, 8.0),
                ],
                &[(20.0, 52.0), (50.0, 50.0), (82.0, 54.0)],
            ];
            for path in ROADS {
                for seg in path.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: seg[0].0,
                        y1: seg[0].1,
                        x2: seg[1].0,
                        y2: seg[1].1,
                        color: ROAD,
                    });
                }
            }

            // Outer rings first so the bright core paints over them.
            paint_blob(
                ctx,
                62.0,
                52.0,
                &[
                    (30.0, Color::Rgb(30, 64, 175)),
                    (24.0, BLUE),
                    (18.0, ACCENT),
                    (12.0, TEAL),
                    (6.0, Color::Rgb(153, 246, 228)),
                ],
                blob_pulse,
            );
            paint_blob(
                ctx,
                22.0,
                30.0,
                &[(14.0, Color::Rgb(30, 64, 175)), (9.0, BLUE), (5.0, ACCENT)],
                blob_pulse,
            );

            ctx.print(56.0, 50.0, Span::styled(label.clone(), marker));
        });
    f.render_widget(map, inner);

    // Precipitation legend (top-left overlay).
    let legend = Rect {
        x: inner.x + 1,
        y: inner.y + 1,
        width: 18.min(inner.width.saturating_sub(2)),
        height: 3.min(inner.height),
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Precipitation",
                Style::default().fg(DIM).bg(CARD),
            )),
            Line::from(vec![
                Span::styled("██", Style::default().fg(GREEN).bg(CARD)),
                Span::styled("██", Style::default().fg(Color::Rgb(163, 230, 53)).bg(CARD)),
                Span::styled("██", Style::default().fg(YELLOW).bg(CARD)),
                Span::styled("██", Style::default().fg(ORANGE).bg(CARD)),
            ]),
            Line::from(Span::styled(
                "Low        High",
                Style::default().fg(DIM).bg(CARD),
            )),
        ]),
        legend,
    );

    // Current-location overlay (bottom-left).
    let location = Rect {
        x: inner.x + 1,
        y: inner.y + inner.height.saturating_sub(2),
        width: 24.min(inner.width.saturating_sub(2)),
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◎ ", Style::default().fg(ACCENT).bg(CARD)),
            Span::styled(name.to_string(), Style::default().fg(TEXT).bg(CARD)),
        ])),
        location,
    );
}

/// Concentric point rings forming a filled precipitation blob.
fn paint_blob(ctx: &mut Context, cx: f64, cy: f64, rings: &[(f64, Color)], pulse: f64) {
    for (radius, color) in rings {
        let radius = radius * pulse;
        let n = ((TAU * radius) / 1.1).max(12.0) as usize;
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let a = i as f64 / n as f64 * TAU;
                (cx + radius * a.cos(), cy + radius * 0.62 * a.sin())
            })
            .collect();
        ctx.draw(&Points {
            coords: &pts,
            color: *color,
        });
    }
}

/// One-line fallback for a highlight card too short for a title + value row.
fn compact_card_line(f: &mut Frame, area: Rect, title: &str, value: String) {
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{title} "), Style::default().fg(DIM)),
            Span::styled(
                value,
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
        ])),
        area,
    );
}

/// Braille line chart with a bright pulse travelling along the series.
/// The pulse wraps around; `lag` offsets the phase between charts.
fn pulse_chart(
    series: &[(f64, f64)],
    tick: u64,
    lag: usize,
) -> Canvas<'_, impl Fn(&mut Context) + '_> {
    let pulse = if series.is_empty() {
        0
    } else {
        (tick as usize * 2 + lag) % series.len()
    };
    Canvas::default()
        .background_color(CARD)
        .marker(Marker::Braille)
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 100.0])
        .paint(move |ctx| {
            ctx.draw(&Points {
                coords: series,
                color: ACCENT,
            });
            if let Some(&(px, py)) = series.get(pulse) {
                let dot = [
                    (px, py),
                    (px + 1.0, py),
                    (px - 1.0, py),
                    (px, py + 1.4),
                    (px, py - 1.4),
                ];
                ctx.draw(&Points {
                    coords: &dot,
                    color: TEXT,
                });
            }
            if let Some(&(tx, ty)) = pulse.checked_sub(1).and_then(|i| series.get(i)) {
                ctx.draw(&Points {
                    coords: &[(tx, ty)],
                    color: DIM,
                });
            }
        })
}

/// Resamples an hourly series into smooth canvas coordinates (x 2..98,
/// y 15..85), normalised to its own min/max.
fn to_series(vals: &[f64]) -> Vec<(f64, f64)> {
    if vals.len() < 2 {
        return Vec::new();
    }
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for v in vals {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let span = (hi - lo).max(1.0);
    let n = 96;
    (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64;
            let pos = t * (vals.len() - 1) as f64;
            let k = pos.floor() as usize;
            let frac = pos - k as f64;
            let a = vals[k];
            let b = vals[(k + 1).min(vals.len() - 1)];
            let v = a + (b - a) * frac;
            (2.0 + t * 96.0, 15.0 + (v - lo) / span * 70.0)
        })
        .collect()
}

// ── Add-city search overlay ───────────────────────────────────────────────────

/// A centered popup (~60% × 50%) drawn over the whole panel after a `Clear`.
fn render_search_overlay(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(60, 50, area);
    if popup.width < 4 || popup.height < 4 {
        return;
    }
    f.render_widget(Clear, popup);

    let search = &app.weather.search;
    let block = card()
        .style(Style::default().bg(PANEL))
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Add City ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // input line
        Constraint::Length(1), // status hint
        Constraint::Min(0),    // candidates
        Constraint::Length(1), // bottom hint
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Search city: {}", search.query),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(ACCENT)),
        ])),
        rows[0],
    );

    let status_text = if search.query.chars().count() < 3 {
        "type ≥3 chars…".to_string()
    } else {
        match &search.status {
            SearchStatus::Searching => "searching…".to_string(),
            SearchStatus::Done if search.candidates.is_empty() => "no matches".to_string(),
            SearchStatus::Error(e) => format!("error: {e}"),
            _ => String::new(),
        }
    };
    if !status_text.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {status_text}"),
                Style::default().fg(DIM),
            ))),
            rows[1],
        );
    }

    if !search.candidates.is_empty() {
        let items: Vec<ListItem> = search
            .candidates
            .iter()
            .map(|c| {
                ListItem::new(Line::from(Span::styled(
                    c.label.clone(),
                    Style::default().fg(TEXT),
                )))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().bg(CARD_HI).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(
            search
                .selected
                .min(search.candidates.len().saturating_sub(1)),
        ));
        f.render_stateful_widget(list, rows[2], &mut state);
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [type] filter  [↑/↓] select  [Enter] add  [Esc] cancel",
            Style::default().fg(DIM),
        ))),
        rows[3],
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Compute a centered Rect that is `pct_x` % wide and `pct_y` % tall of `area`.
fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}

/// A single-line message centered inside a dashboard card.
fn render_notice(f: &mut Frame, area: Rect, text: &str, color: Color) {
    let block = card();
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mid = Layout::vertical([
        Constraint::Percentage(45),
        Constraint::Length(1),
        Constraint::Percentage(55),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(color),
        )))
        .alignment(Alignment::Center),
        mid[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn sample() -> WeatherData {
        let hourly: Vec<f64> = (0..24)
            .map(|h| 18.0 + (h as f64 / 3.0).sin() * 6.0)
            .collect();
        WeatherData {
            temp: 24.3,
            weather_code: 61,
            fetched_at: 0,
            feels_like: 26.1,
            humidity: 78,
            wind_speed: 12.4,
            datetime: "11 August, 2026  3:05 PM".into(),
            time_short: "3:05 PM".into(),
            forecast: (1..=4)
                .map(|i| ForecastDay {
                    code: 3,
                    high: 25.0 + i as f64,
                    low: 15.0,
                    date: format!("{} August, 2026", 11 + i),
                    weekday: "Wednesday".into(),
                })
                .collect(),
            rain_chance: 42,
            wind_24h: hourly.iter().map(|t| t * 0.7).collect(),
            rain_24h: (0..24).map(|h| (h * 4) as f64).collect(),
            uv_index: 5.5,
            sunrise: "6:42 AM".into(),
            sunset: "8:31 PM".into(),
            sun_frac: 0.6,
            visibility_km: 8.0,
            hourly,
        }
    }

    /// Renders the dashboard at a few sizes: catches layout panics (Rect
    /// overflow in the map overlays, zero-height cards) that would crash the app.
    #[test]
    fn dashboard_renders_at_several_sizes() {
        for (w, h) in [(140u16, 42u16), (100, 30), (80, 24), (40, 14), (28, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|f| {
                    render_dashboard_body(
                        f,
                        "Lisbon, PT",
                        &sample(),
                        TempUnit::Celsius,
                        7,
                        f.area(),
                    )
                })
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            println!("── {w}x{h} ──");
            for y in 0..buffer.area.height {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                println!("{row}");
            }
        }
    }

    #[test]
    fn series_stays_inside_canvas_bounds() {
        assert!(to_series(&[]).is_empty());
        assert!(to_series(&[5.0]).is_empty());

        // A flat series must not divide by zero and stays on the baseline.
        let flat = to_series(&[7.0; 24]);
        assert_eq!(flat.len(), 96);
        assert!(flat.iter().all(|(_, y)| (*y - 15.0).abs() < 1e-9));

        let pts = to_series(&[0.0, 10.0, 5.0, 20.0, 3.0]);
        assert_eq!(pts.len(), 96);
        for (x, y) in &pts {
            assert!((2.0..=98.0).contains(x), "x out of bounds: {x}");
            assert!((15.0..=85.0).contains(y), "y out of bounds: {y}");
        }
        // The interpolation starts at the first sample and ends at the last.
        assert!((pts[0].1 - 15.0).abs() < 1e-9);
        assert!((pts[95].1 - (15.0 + 3.0 / 20.0 * 70.0)).abs() < 1e-9);
    }
}
