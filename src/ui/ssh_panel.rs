//! SSH Manager panel renderer.
//!
//! Displays the list of hosts parsed from `~/.ssh/config` (optionally
//! grouped under collapsible `# group: <name>` headers), a strip of this
//! run's session tabs, and two popup overlays — connection history and
//! active tunnels — drawn on top of the panel (same pattern as the Weather
//! city-search overlay: still `AppMode::SshManager`, no separate mode).
//!
//! The actual SSH connection (suspending the TUI and exec-ing ssh) is
//! handled by the input event handler — this file is purely read-only
//! rendering.

use crate::modules::ssh::{SshConnField, SshConnForm, SshManager};
use crate::ui::icons::{ICON_ERROR, ICON_HIGHLIGHT, ICON_SUCCESS, ICON_UNKNOWN, MD_LOCK};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState},
};

/// Render the full SSH Manager panel into `area`.
///
/// `connecting` is `Some(alias)` for exactly one frame — the one drawn right
/// before the TUI suspends to run a real `ssh` session (see
/// `App::ssh_pending_connect` / `App::ssh_connect_now`) — and paints a small
/// "Connecting…" popup so the suspend never looks like the app simply froze.
pub fn render_ssh_panel(f: &mut Frame, mgr: &SshManager, area: Rect, connecting: Option<&str>) {
    let tab_strip_h: u16 = if mgr.tabs.is_empty() { 0 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),           // header block
            Constraint::Length(tab_strip_h), // session tab strip
            Constraint::Min(0),              // host table
            Constraint::Length(1),           // status / hint bar
        ])
        .split(area);

    render_header(f, mgr, chunks[0]);

    if tab_strip_h > 0 {
        render_tab_strip(f, mgr, chunks[1]);
    }

    if mgr.hosts.is_empty() {
        render_empty(f, chunks[2]);
    } else {
        render_table(f, mgr, chunks[2]);
    }

    render_statusbar(f, mgr, chunks[3]);

    if mgr.history_open {
        render_history_overlay(f, mgr, area);
    }
    if mgr.tunnels_open {
        render_tunnels_overlay(f, mgr, area);
    }

    // Loading popups — non-interactive, auto-clear themselves once the
    // background work finishes (or, for `connecting`, once the suspended
    // session returns). Drawn last so they sit on top of everything else.
    if let Some(alias) = connecting {
        render_mini_overlay(f, area, &format!("Connecting to {alias}…"), Color::Cyan);
    } else if mgr.testing {
        render_mini_overlay(f, area, &mgr.status_msg, Color::Yellow);
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let count = mgr.hosts.len();
    let title_text = if count == 1 {
        " SSH Manager — 1 host ".to_string()
    } else {
        format!(" SSH Manager — {} hosts ", count)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_text)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(block, area);
}

// ── Session tab strip ────────────────────────────────────────────────────────

fn render_tab_strip(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    for (i, tab) in mgr.tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" │ "));
        }
        let selected = i == mgr.tabs_selected;
        let status = match tab.exit_code {
            Some(0) => Span::styled(ICON_SUCCESS, Style::default().fg(Color::Green)),
            Some(_) => Span::styled(ICON_ERROR, Style::default().fg(Color::Red)),
            None => Span::styled(ICON_UNKNOWN, Style::default().fg(Color::DarkGray)),
        };
        let dur = tab
            .duration_secs
            .map(|d| format!("{d}s"))
            .unwrap_or_default();
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(status);
        spans.push(Span::styled(format!(" {} {dur}", tab.alias), style));
    }
    let para = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    f.render_widget(para, area);
}

// ── Empty state ───────────────────────────────────────────────────────────────

fn render_empty(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pad = inner.height.saturating_sub(3) / 2;
    let msg_area = Rect {
        y: inner.y + pad,
        height: inner.height.saturating_sub(pad),
        ..inner
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No hosts in ~/.ssh/config",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Add Host entries to ~/.ssh/config and press F5 to reload.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(para, msg_area);
}

// ── Host table (grouped, collapsible) ────────────────────────────────────────

/// One renderable row: either a collapsible group header or a host (carrying
/// its index into `mgr.hosts` so we can highlight the selected one).
enum DisplayRow {
    GroupHeader {
        name: String,
        count: usize,
        collapsed: bool,
    },
    Host(usize),
}

/// Build the ordered list of display rows: hosts grouped by `group` in
/// first-seen order, ungrouped hosts interleaved at their original position.
/// A collapsed group only shows its header — except the currently selected
/// host stays visible even inside a collapsed group, so the user never loses
/// their cursor and can still press `g` to re-expand.
fn build_display_rows(mgr: &SshManager) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();

    for (i, host) in mgr.hosts.iter().enumerate() {
        match &host.group {
            None => rows.push(DisplayRow::Host(i)),
            Some(group) => {
                if !seen_groups.contains(group) {
                    seen_groups.push(group.clone());
                    let count = mgr
                        .hosts
                        .iter()
                        .filter(|h| h.group.as_deref() == Some(group.as_str()))
                        .count();
                    rows.push(DisplayRow::GroupHeader {
                        name: group.clone(),
                        count,
                        collapsed: mgr.collapsed_groups.contains(group),
                    });
                }
                let collapsed = mgr.collapsed_groups.contains(group);
                if !collapsed || i == mgr.selected {
                    rows.push(DisplayRow::Host(i));
                }
            }
        }
    }
    rows
}

fn render_table(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let header_cells = ["Alias", "Host:Port", "User", "Identity", "Status"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells)
        .style(Style::default().bg(Color::DarkGray))
        .height(1);

    let display_rows = build_display_rows(mgr);
    let mut selected_row = 0usize;
    let rows: Vec<Row> = display_rows
        .iter()
        .enumerate()
        .map(|(row_idx, dr)| match dr {
            DisplayRow::GroupHeader {
                name,
                count,
                collapsed,
            } => {
                let icon = if *collapsed { "▸" } else { "▾" };
                Row::new(vec![Cell::from(Span::styled(
                    format!("{icon} {name} ({count})"),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ))])
                .height(1)
            }
            DisplayRow::Host(i) => {
                let host = &mgr.hosts[*i];
                let selected = *i == mgr.selected;
                if selected {
                    selected_row = row_idx;
                }

                let host_port = if host.port == 22 {
                    host.hostname.clone()
                } else {
                    format!("{}:{}", host.hostname, host.port)
                };
                let identity = host
                    .identity_file
                    .as_deref()
                    .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                    .unwrap_or_else(|| MD_LOCK.to_string());
                let (status_text, status_color) = match host.reachable {
                    Some(true) => (ICON_SUCCESS, Color::Green),
                    Some(false) => (ICON_ERROR, Color::Red),
                    None => (ICON_UNKNOWN, Color::DarkGray),
                };
                let row_style = if selected {
                    Style::default().bg(Color::Cyan).fg(Color::Black)
                } else {
                    Style::default()
                };
                // Indent grouped hosts so the header hierarchy reads clearly.
                let alias = if host.group.is_some() {
                    format!("  {}", clip(&host.alias, 18))
                } else {
                    clip(&host.alias, 20)
                };

                Row::new(vec![
                    Cell::from(alias),
                    Cell::from(clip(&host_port, 28)),
                    Cell::from(clip(&host.user, 14)),
                    Cell::from(clip(&identity, 18)),
                    Cell::from(Span::styled(
                        status_text,
                        Style::default()
                            .fg(if selected { Color::Black } else { status_color })
                            .add_modifier(Modifier::BOLD),
                    )),
                ])
                .style(row_style)
                .height(1)
            }
        })
        .collect();

    let widths = [
        Constraint::Length(22),
        Constraint::Length(30),
        Constraint::Length(16),
        Constraint::Length(20),
        Constraint::Length(8),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Hosts ")
                .border_style(Style::default().fg(Color::Blue)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(format!("{} ", ICON_HIGHLIGHT));

    let mut table_state = TableState::default();
    table_state.select(if mgr.hosts.is_empty() {
        None
    } else {
        Some(selected_row)
    });

    f.render_stateful_widget(table, area, &mut table_state);
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_statusbar(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let left_spans: Vec<Span> = if mgr.testing {
        vec![Span::styled(
            format!(" {} ", mgr.status_msg),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]
    } else if !mgr.status_msg.is_empty() {
        vec![Span::styled(
            format!(" {} ", mgr.status_msg),
            Style::default().fg(Color::Cyan),
        )]
    } else {
        vec![]
    };

    let hints = vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("connect  "),
        Span::styled(
            "[t]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("test  "),
        Span::styled(
            "[h]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("history  "),
        Span::styled(
            "[g/n/a]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("groups  "),
        Span::styled(
            "[s]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("sftp  "),
        Span::styled(
            "[T]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("tunnels  "),
        Span::styled(
            "[Tab/e/d]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("new/edit/del  "),
        Span::styled(
            "[F5]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("reload  "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("back"),
    ];

    if !left_spans.is_empty() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let msg_para =
            Paragraph::new(Line::from(left_spans)).style(Style::default().bg(Color::DarkGray));
        f.render_widget(msg_para, cols[0]);

        let hint_para = Paragraph::new(Line::from(hints))
            .style(Style::default().bg(Color::DarkGray))
            .alignment(Alignment::Right);
        f.render_widget(hint_para, cols[1]);
    } else {
        let hint_para = Paragraph::new(Line::from(hints))
            .style(Style::default().bg(Color::DarkGray))
            .alignment(Alignment::Right);
        f.render_widget(hint_para, area);
    }
}

// ── History overlay ───────────────────────────────────────────────────────────

fn render_history_overlay(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let w = (area.width * 2 / 3)
        .max(40)
        .min(area.width.saturating_sub(4));
    let h = ((mgr.history.len() as u16) + 2)
        .clamp(6, 20)
        .min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height / 6;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let items: Vec<ListItem> = if mgr.history.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  (no recorded sessions)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        mgr.history
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let status = match entry.exit_code {
                    Some(0) => Span::styled(ICON_SUCCESS, Style::default().fg(Color::Green)),
                    Some(_) => Span::styled(ICON_ERROR, Style::default().fg(Color::Red)),
                    None => Span::styled(ICON_UNKNOWN, Style::default().fg(Color::DarkGray)),
                };
                let style = if i == mgr.history_selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    status,
                    Span::styled(
                        format!(" {:<18}", entry.alias),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {}  ({}s)", entry.connected_at, entry.duration_secs),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
                .style(style)
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" History — Enter: reconnect  Esc: close ")
            .border_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    f.render_widget(list, popup);
}

// ── Tunnels overlay ───────────────────────────────────────────────────────────

fn render_tunnels_overlay(f: &mut Frame, mgr: &SshManager, area: Rect) {
    let w = (area.width * 2 / 3)
        .max(40)
        .min(area.width.saturating_sub(4));
    let h = ((mgr.tunnels.len() as u16) + 2)
        .clamp(6, 16)
        .min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height / 6;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let items: Vec<ListItem> = if mgr.tunnels.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  (no active tunnels — 'c' to create one)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        mgr.tunnels
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let style = if i == mgr.tunnels_selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} ", ICON_SUCCESS),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(
                        format!(
                            "localhost:{} → {}:{} via {}",
                            t.local_port, t.remote_host, t.remote_port, t.alias
                        ),
                        Style::default(),
                    ),
                ]))
                .style(style)
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tunnels — c: create  d: kill  Esc/T: close ")
            .border_style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    f.render_widget(list, popup);
}

// ── Add/Edit Connection form ────────────────────────────────────────────────

/// Render the Add/Edit Connection form: a 4-field horizontal bar
/// (Host/Username/Password/Port), the focused field highlighted in cyan.
pub fn render_ssh_connect_form(f: &mut Frame, area: Rect, form: &SshConnForm, connecting: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // field bar
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // hints
        ])
        .split(area);

    let title = if let Some(alias) = &form.editing_alias {
        format!(" Edit Connection — {alias} ")
    } else {
        " Add Connection ".to_string()
    };
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        chunks[0],
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(chunks[1]);

    render_form_field(
        f,
        cols[0],
        "Host",
        &form.host,
        form.focus == SshConnField::Host,
        false,
    );
    render_form_field(
        f,
        cols[1],
        "Username",
        &form.username,
        form.focus == SshConnField::Username,
        false,
    );
    render_form_field(
        f,
        cols[2],
        "Password",
        &form.password,
        form.focus == SshConnField::Password,
        true,
    );
    render_form_field(
        f,
        cols[3],
        "Port",
        &form.port,
        form.focus == SshConnField::Port,
        false,
    );

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" next field  "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(if form.editing_alias.is_some() {
            " save  "
        } else {
            " connect  "
        }),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" cancel"),
    ]))
    .style(Style::default().bg(Color::Black));
    f.render_widget(hint, chunks[3]);

    if connecting {
        let label = form.host.trim();
        render_mini_overlay(f, area, &format!("Connecting to {label}…"), Color::Cyan);
    }
}

fn render_form_field(
    f: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
    mask: bool,
) {
    let color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let display: String = if mask {
        "*".repeat(value.chars().count())
    } else {
        value.to_string()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {label} "))
        .border_style(Style::default().fg(color).add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }));
    let para = Paragraph::new(Line::from(Span::styled(
        display,
        Style::default().fg(Color::White),
    )))
    .block(block);
    f.render_widget(para, area);
}

// ── Loading overlay ───────────────────────────────────────────────────────────

/// Small centered, non-interactive popup used for transient "in progress"
/// states (connecting, testing). Unlike the history/tunnels overlays this
/// captures no input — it just disappears once the caller's state changes.
fn render_mini_overlay(f: &mut Frame, area: Rect, text: &str, color: Color) {
    // `max_w` is floored at 24 so `clamp` never sees min > max on a narrow
    // terminal (which would panic) — the popup just clips instead.
    let max_w = area.width.saturating_sub(4).max(24);
    let w = ((text.chars().count() as u16) + 6).clamp(24, max_w);
    let h = 3u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let para = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    f.render_widget(para, popup);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Clip a string to at most `max` characters, appending `…` if truncated.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
