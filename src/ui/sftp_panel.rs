//! SFTP transfer panel renderer.
//!
//! Two-column layout: local pane (left, reuses `FileExplorer`) and remote
//! pane (right, `SftpPanel::remote_entries`). Purely read-only rendering —
//! the only mutation allowed is `ListState`/scroll bookkeeping needed to
//! draw the selection, per the project-wide "render functions never mutate
//! application state" rule (see `CLAUDE.md`).

use crate::modules::sftp::{SftpFocus, SftpPanel};
use crate::ui::icons::{ICON_DIR, ICON_FILE_GENERIC};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

/// Render the SFTP panel into `area`.
pub fn render_sftp_panel(f: &mut Frame, area: Rect, panel: &mut SftpPanel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    render_local_pane(f, cols[0], panel);
    render_remote_pane(f, cols[1], panel);
    render_status_bar(f, rows[1], panel);
}

// ── Local pane ───────────────────────────────────────────────────────────────

fn render_local_pane(f: &mut Frame, area: Rect, panel: &SftpPanel) {
    let focused = panel.focus == SftpFocus::Local;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = format!(" Local: {} ", panel.local.current_dir.display());

    let items: Vec<ListItem> = panel
        .local
        .entries
        .iter()
        .map(|e| {
            let name_fg = if e.is_dir { Color::Cyan } else { Color::White };
            let icon = if e.is_dir {
                ICON_DIR
            } else {
                ICON_FILE_GENERIC
            };
            let size_str = if e.is_dir {
                "       ".to_string()
            } else {
                format!("{:>7}", format_size(e.size))
            };
            let line = Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(name_fg)),
                Span::styled(format!("{:<30}", e.name), Style::default().fg(name_fg)),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = panel.local.list_state.clone();
    f.render_stateful_widget(list, area, &mut state);
}

// ── Remote pane ──────────────────────────────────────────────────────────────

fn render_remote_pane(f: &mut Frame, area: Rect, panel: &SftpPanel) {
    let focused = panel.focus == SftpFocus::Remote;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = format!(" {}: {} ", panel.alias, panel.remote_path);

    let items: Vec<ListItem> = panel
        .remote_entries
        .iter()
        .map(|e| {
            let name_fg = if e.is_dir { Color::Cyan } else { Color::White };
            let icon = if e.is_dir {
                ICON_DIR
            } else {
                ICON_FILE_GENERIC
            };
            let size_str = if e.is_dir {
                "       ".to_string()
            } else {
                format!("{:>7}", format_size(e.size))
            };
            let line = Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(name_fg)),
                Span::styled(format!("{:<30}", e.name), Style::default().fg(name_fg)),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !panel.remote_entries.is_empty() {
        state.select(Some(
            panel.remote_selected.min(panel.remote_entries.len() - 1),
        ));
    }
    f.render_stateful_widget(list, area, &mut state);
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn render_status_bar(f: &mut Frame, area: Rect, panel: &SftpPanel) {
    let mut left_spans: Vec<Span> = Vec::new();

    if panel.loading {
        left_spans.push(Span::styled(
            " loading… ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if !panel.status_msg.is_empty() {
        left_spans.push(Span::styled(
            format!(" {} ", panel.status_msg),
            Style::default().fg(Color::Cyan),
        ));
    }

    let hints = Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" focus  "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" open  "),
        Span::styled(
            "[g]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" get  "),
        Span::styled(
            "[p]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" put  "),
        Span::styled(
            "[Bksp]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" up  "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" back"),
    ]);

    if left_spans.is_empty() {
        let para = Paragraph::new(hints)
            .style(Style::default().bg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Right);
        f.render_widget(para, area);
    } else {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let msg_para =
            Paragraph::new(Line::from(left_spans)).style(Style::default().bg(Color::DarkGray));
        f.render_widget(msg_para, cols[0]);

        let hint_para = Paragraph::new(hints)
            .style(Style::default().bg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Right);
        f.render_widget(hint_para, cols[1]);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
