use crate::modules::cron::{CronEditor, CronJob, CronLine};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Render the Cron Editor panel.
///
/// `editor` is borrowed from `app.cron_editor` at the call site.
pub fn render_cron_panel(f: &mut Frame, editor: &CronEditor, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // list / edit form
            Constraint::Length(2), // hints + status
        ])
        .split(area);

    render_header(f, editor, chunks[0]);

    if editor.editing.is_some() {
        render_edit_form(f, editor, chunks[1]);
    } else {
        render_list(f, editor, chunks[1]);
    }

    render_footer(f, editor, chunks[2]);
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(f: &mut Frame, editor: &CronEditor, area: Rect) {
    let dirty_indicator = if editor.dirty { " [*]" } else { "" };
    let job_count = editor
        .lines
        .iter()
        .filter(|l| matches!(l, CronLine::Job(_)))
        .count();
    let title = format!(
        " Cron Editor{}  — {} job(s)  (crontab -l) ",
        dirty_indicator, job_count
    );

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Cron Editor",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            dirty_indicator,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  — {} job(s)", job_count),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    f.render_widget(header, area);
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

fn render_list(f: &mut Frame, editor: &CronEditor, area: Rect) {
    let header_cells = [
        Cell::from("Schedule").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Description").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Command").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let header = Row::new(header_cells)
        .height(1)
        .bottom_margin(0)
        .style(Style::default().bg(Color::DarkGray));

    let rows: Vec<Row> = editor
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let is_selected = i == editor.selected;
            let base_style = if is_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            match line {
                CronLine::Job(job) => {
                    let schedule = format_schedule(job);
                    Row::new(vec![
                        Cell::from(schedule)
                            .style(base_style.patch(Style::default().fg(Color::Cyan))),
                        Cell::from(job.description.as_str())
                            .style(base_style.patch(Style::default().fg(Color::Green))),
                        Cell::from(truncate(&job.command, 50))
                            .style(base_style.patch(Style::default().fg(Color::White))),
                    ])
                    .height(1)
                }

                CronLine::Comment(text) => Row::new(vec![
                    Cell::from(truncate(text, 70)).style(
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM),
                    ),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .height(1),

                CronLine::Empty => {
                    Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")]).height(1)
                }
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(30),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Jobs ")
            .border_style(Style::default().fg(Color::Blue)),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    f.render_widget(table, area);
}

// ---------------------------------------------------------------------------
// Edit form (centered popup)
// ---------------------------------------------------------------------------

fn render_edit_form(f: &mut Frame, editor: &CronEditor, area: Rect) {
    // Calculate a centered popup area
    let popup = centered_rect(70, 80, area);

    // Clear the background so the form floats over the list
    f.render_widget(Clear, popup);

    // Live schedule description from current field values
    let live_desc = CronEditor::describe_schedule(
        &editor.edit_fields[0],
        &editor.edit_fields[1],
        &editor.edit_fields[2],
        &editor.edit_fields[3],
        &editor.edit_fields[4],
    );

    // Vertical layout inside the popup
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // min
            Constraint::Length(3), // hour
            Constraint::Length(3), // day
            Constraint::Length(3), // month
            Constraint::Length(3), // weekday
            Constraint::Length(3), // command
            Constraint::Length(2), // live description
            Constraint::Min(1),    // hint
        ])
        .split(popup);

    // Outer block
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(" Edit Cron Job ")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(outer, popup);

    // Labels and field names
    const FIELD_LABELS: [(&str, &str); 6] = [
        ("Minute  ", "* or 0-59 or */n"),
        ("Hour    ", "* or 0-23 or */n"),
        ("Day     ", "* or 1-31"),
        ("Month   ", "* or 1-12"),
        ("Weekday ", "* or 0-7 (0=Sun)"),
        ("Command ", "full command path"),
    ];

    for (field_idx, (label, placeholder)) in FIELD_LABELS.iter().enumerate() {
        let is_focused = editor.edit_cursor == field_idx;
        let border_color = if is_focused {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let value = &editor.edit_fields[field_idx];

        let display = if value.is_empty() {
            Span::styled(
                format!("  {} ", placeholder),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            )
        } else {
            Span::styled(
                format!("  {} ", value),
                Style::default().fg(if field_idx < 5 {
                    Color::Cyan
                } else {
                    Color::White
                }),
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", label))
            .border_style(Style::default().fg(border_color));

        let para = Paragraph::new(Line::from(display)).block(block);
        f.render_widget(para, chunks[field_idx]);
    }

    // Live description
    let desc_line = Paragraph::new(Line::from(vec![
        Span::styled("  Schedule: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            &live_desc,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(desc_line, chunks[6]);

    // Hint
    let hint = Paragraph::new(Line::from(vec![
        hint_key("Tab"),
        hint_desc("next field  "),
        hint_key("Shift+Tab"),
        hint_desc("prev  "),
        hint_key("Enter"),
        hint_desc("save  "),
        hint_key("Esc"),
        hint_desc("cancel"),
    ]));
    f.render_widget(hint, chunks[7]);
}

// ---------------------------------------------------------------------------
// Footer: status message + key hints
// ---------------------------------------------------------------------------

fn render_footer(f: &mut Frame, editor: &CronEditor, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // Status message
    let status_color = if editor.status_msg.starts_with("Error") {
        Color::Red
    } else {
        Color::DarkGray
    };
    let status = Paragraph::new(Span::styled(
        format!("  {}", editor.status_msg),
        Style::default().fg(status_color),
    ));
    f.render_widget(status, chunks[0]);

    // Key hints (only shown when not editing)
    if editor.editing.is_none() {
        let hints = Line::from(vec![
            hint_key("a"),
            hint_desc("add  "),
            hint_key("d"),
            hint_desc("delete  "),
            hint_key("e"),
            hint_desc("edit  "),
            hint_key("w"),
            hint_desc("write  "),
            hint_key("F5"),
            hint_desc("reload  "),
            hint_key("Esc"),
            hint_desc("back"),
        ]);
        f.render_widget(Paragraph::new(hints), chunks[1]);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_schedule(job: &CronJob) -> String {
    if job.minute == "@reboot" {
        "@reboot".to_owned()
    } else {
        format!(
            "{} {} {} {} {}",
            job.minute, job.hour, job.day, job.month, job.weekday
        )
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

fn hint_key(key: &'static str) -> Span<'static> {
    Span::styled(
        format!("[{}]", key),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_desc(desc: &'static str) -> Span<'static> {
    Span::styled(
        desc,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    )
}

/// Returns a centered `Rect` of `percent_x` width and `percent_y` height.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
