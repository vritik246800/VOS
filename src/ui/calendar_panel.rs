//! Calendar panel (Phase 6.5): a large month grid that shows each day's events
//! inline (calcure-style) on the left, plus a per-day task pane with an inline
//! editor on the right. Reads `app.calendar`; purely a renderer — all state
//! changes happen in `handle_input`.

use crate::app::App;
use crate::modules::calendar::{CalFocus, Task, month_name};
use crate::modules::weather::weather_symbol;
use chrono::Datelike;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

const WEEKDAYS: [&str; 7] = [
    "MONDAY",
    "TUESDAY",
    "WEDNESDAY",
    "THURSDAY",
    "FRIDAY",
    "SATURDAY",
    "SUNDAY",
];

/// A small palette so consecutive events in a cell are easy to tell apart.
const EVENT_COLORS: [Color; 5] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
];

pub fn render_calendar(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(76), Constraint::Percentage(24)])
        .split(chunks[0]);

    render_grid(f, app, cols[0]);
    render_journal(f, app, cols[1]);

    // Context-aware footer hints.
    let hints: &[(&str, &str)] = if app.calendar.focus == CalFocus::Tasks {
        &[
            ("type", "task title"),
            ("Enter", "add"),
            ("↑/↓", "select"),
            ("Enter∅", "toggle done"),
            ("Del", "remove"),
            ("Tab/Esc", "save → grid"),
        ]
    } else {
        &[
            ("←/→", "month"),
            ("h/l", "±day"),
            ("↑/↓", "±week"),
            ("t", "today"),
            ("Tab/a", "add task"),
            ("Esc", "menu"),
        ]
    };
    crate::ui::help_panel::render_key_hints(f, chunks[1], hints);
}

// ── Month grid ──────────────────────────────────────────────────────────────

fn render_grid(f: &mut Frame, app: &App, area: Rect) {
    let cal = &app.calendar;
    let grid_focused = cal.focus == CalFocus::Grid;

    // Title: month + year, with the current weather appended when available.
    let mut title = format!(
        " {} {} ",
        month_name(cal.selected.month()),
        cal.selected.year()
    );
    if let Some(d) = app.weather.primary_data() {
        title = format!(
            " {} {}   {} {:.0}°C ",
            month_name(cal.selected.month()),
            cal.selected.year(),
            weather_symbol(d.weather_code),
            d.temp
        );
    }

    let border_color = if grid_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 || inner.width < 7 {
        return;
    }

    // Weekday header row + the week rows below it.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let col_constraints = [Constraint::Ratio(1, 7); 7];

    // Header.
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(rows[0]);
    for (i, cell) in header_cols.iter().enumerate() {
        let style = if i >= 5 {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        let name = truncate(WEEKDAYS[i], cell.width as usize);
        f.render_widget(Paragraph::new(Span::styled(name, style)), *cell);
    }

    // Week rows.
    let weeks = cal.weeks();
    let n = weeks.len().max(1) as u32;
    let week_constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n)).collect();
    let week_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(week_constraints)
        .split(rows[1]);

    for (wi, week) in weeks.iter().enumerate() {
        let Some(row_area) = week_rows.get(wi) else {
            break;
        };
        let day_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);
        for (ci, cell) in week.iter().enumerate() {
            if let Some(date) = cell {
                render_day_cell(f, day_cols[ci], date, ci, app, grid_focused);
            }
        }
    }
}

fn render_day_cell(
    f: &mut Frame,
    area: Rect,
    date: &chrono::NaiveDate,
    col_idx: usize,
    app: &App,
    grid_focused: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let cal = &app.calendar;
    let is_today = *date == cal.today;
    let is_selected = *date == cal.selected;
    let weekend = col_idx >= 5;

    // Day-number style.
    let num_style = if is_selected {
        let s = Style::default().bg(Color::Yellow).fg(Color::Black);
        if grid_focused {
            s.add_modifier(Modifier::BOLD)
        } else {
            s
        }
    } else if is_today {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else if weekend {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::White)
    };

    let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);
    lines.push(Line::from(Span::styled(
        format!("{:>2}", date.day()),
        num_style,
    )));

    // Inline events for this day, capped to the cell height.
    let tasks: &[Task] = cal.tasks_on(date);
    let cap = (area.height as usize).saturating_sub(1);
    if cap > 0 && !tasks.is_empty() {
        let (shown, overflow) = if tasks.len() > cap {
            (cap.saturating_sub(1), true)
        } else {
            (tasks.len(), false)
        };
        for (i, t) in tasks.iter().take(shown).enumerate() {
            let text = truncate(&format!("•{}", t.text), area.width as usize);
            let style = if t.done {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT | Modifier::DIM)
            } else {
                Style::default().fg(EVENT_COLORS[i % EVENT_COLORS.len()])
            };
            lines.push(Line::from(Span::styled(text, style)));
        }
        if overflow {
            let more = tasks.len() - shown;
            lines.push(Line::from(Span::styled(
                truncate(&format!("+{more} more"), area.width as usize),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    // Subtle cell tint for the selected day while the grid is focused.
    let mut para = Paragraph::new(lines);
    if is_selected && grid_focused {
        para = para.style(Style::default().bg(Color::Rgb(38, 38, 54)));
    }
    f.render_widget(para, area);
}

// ── Per-day task pane (with inline editor) ───────────────────────────────────

fn render_journal(f: &mut Frame, app: &mut App, area: Rect) {
    let tasks_focused = app.calendar.focus == CalFocus::Tasks;
    let editing = app.calendar.editing;
    let border_color = if tasks_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let title = format!(" Tasks — {} ", app.calendar.selected_date_str());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Reserve a bottom line for the inline input while editing.
    let (list_area, input_area) = if editing {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner);
        (parts[0], Some(parts[1]))
    } else {
        (inner, None)
    };

    if app.calendar.tasks.is_empty() {
        let msg = if editing {
            " Type a title below, Enter to add"
        } else {
            " No tasks — Tab or 'a' to add"
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(Color::DarkGray),
            ))),
            list_area,
        );
    } else {
        let items: Vec<ListItem> = app
            .calendar
            .tasks
            .iter()
            .map(|t| {
                let (checkbox, text_style) = if t.done {
                    (
                        "[✓] ".to_string(),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
                    )
                } else {
                    ("[ ] ".to_string(), Style::default().fg(Color::White))
                };
                let box_color = if t.done {
                    Color::Green
                } else {
                    Color::DarkGray
                };
                ListItem::new(Line::from(vec![
                    Span::styled(checkbox, Style::default().fg(box_color)),
                    Span::styled(t.text.clone(), text_style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, list_area, &mut app.calendar.task_state);
    }

    // Inline editor line.
    if let Some(ia) = input_area {
        let line = Line::from(vec![
            Span::styled("› ", Style::default().fg(Color::Cyan)),
            Span::styled(
                app.calendar.input.clone(),
                Style::default().fg(Color::White),
            ),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]);
        f.render_widget(Paragraph::new(line), ia);
    }
}

/// Truncate to `max` display columns (rough — counts chars), adding an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
