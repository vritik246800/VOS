use crate::app::App;
use crate::modules::packages::PkgTab;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
};

const TAB_TITLES: &[&str] = &["Installed", "Search", "Updates"];

/// Column widths for the package list header + rows.
const COL_NAME: usize = 28;
const COL_VER: usize = 16;
// description fills the rest

pub fn render_packages_panel(f: &mut Frame, app: &App, area: Rect) {
    let pm = &app.package_manager;

    // ── Overall layout ────────────────────────────────────────────────────────
    // tab bar (3) + optional search input (1) + header (1) + list (rest) + status (1)
    let search_line_height: u16 = if pm.tab == PkgTab::Search { 1 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                  // tab bar
            Constraint::Length(search_line_height), // search input (0 or 1)
            Constraint::Length(1),                  // column header
            Constraint::Min(0),                     // package list
            Constraint::Length(1),                  // status line
        ])
        .split(area);

    render_tabs(f, chunks[0], pm.tab.clone());

    if pm.tab == PkgTab::Search {
        render_search_input(f, chunks[1], &pm.search_query);
    }

    render_header(f, chunks[2]);
    render_list(f, chunks[3], app);
    render_status(f, chunks[4], app);
}

// ── Tab bar ───────────────────────────────────────────────────────────────────

fn render_tabs(f: &mut Frame, area: Rect, active: PkgTab) {
    let selected = match active {
        PkgTab::Installed => 0,
        PkgTab::Search => 1,
        PkgTab::Updates => 2,
    };
    let titles: Vec<Line> = TAB_TITLES
        .iter()
        .map(|t| Line::from(Span::styled(*t, Style::default().fg(Color::Cyan))))
        .collect();
    let tabs = Tabs::new(titles)
        .select(selected)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Package Manager ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");
    f.render_widget(tabs, area);
}

// ── Search input line ─────────────────────────────────────────────────────────

fn render_search_input(f: &mut Frame, area: Rect, query: &str) {
    let text = format!(" Search: {query}_");
    let p = Paragraph::new(text).style(Style::default().fg(Color::White).bg(Color::DarkGray));
    f.render_widget(p, area);
}

// ── Column header row ─────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, area: Rect) {
    let header = Line::from(vec![
        Span::styled(
            format!(" {:<width$}", "Name", width = COL_NAME),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<width$}", "Version", width = COL_VER),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " Description",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

// ── Package list ──────────────────────────────────────────────────────────────

fn render_list(f: &mut Frame, area: Rect, app: &App) {
    let pm = &app.package_manager;
    let pkgs = pm.visible_packages();

    if pkgs.is_empty() {
        let msg = if pm.loading {
            " Loading…"
        } else {
            match pm.tab {
                PkgTab::Installed => " No packages loaded — press F5 to refresh",
                PkgTab::Search => " No results — type a query and press Enter",
                PkgTab::Updates => " No updates found — press F5 to check",
            }
        };
        let p = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }

    let desc_width = (area.width as usize).saturating_sub(COL_NAME + 1 + COL_VER + 1 + 3); // 3 = leading space + margin

    let items: Vec<ListItem> = pkgs
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            let is_selected = i == pm.selected;

            // Choose colours based on context
            let (name_color, ver_color) = match pm.tab {
                PkgTab::Updates => (Color::Yellow, Color::Yellow),
                _ if pkg.installed => (Color::Green, Color::White),
                _ => (Color::White, Color::DarkGray),
            };

            let name_str = truncate(&pkg.name, COL_NAME);
            let ver_str = truncate(&pkg.version, COL_VER);
            let desc_str = truncate(&pkg.description, desc_width.max(4));

            let mut style = Style::default();
            if is_selected {
                style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
            }

            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<width$}", name_str, width = COL_NAME),
                    Style::default().fg(name_color),
                ),
                Span::styled(
                    format!(" {:<width$}", ver_str, width = COL_VER),
                    Style::default().fg(ver_color),
                ),
                Span::styled(
                    format!(" {}", desc_str),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line).style(style)
        })
        .collect();

    let count = pkgs.len();
    let title = match pm.tab {
        PkgTab::Installed => format!(" Installed ({count}) "),
        PkgTab::Search => format!(" Search results ({count}) "),
        PkgTab::Updates => format!(" Updates available ({count}) "),
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    f.render_widget(list, area);
}

// ── Status line ───────────────────────────────────────────────────────────────

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let pm = &app.package_manager;
    let backend = pm.backend_name();
    let msg = &pm.status_msg;

    let spans = vec![
        Span::styled(format!(" [{backend}] "), Style::default().fg(Color::Cyan)),
        Span::styled(msg.clone(), Style::default().fg(Color::White)),
        Span::raw("  "),
        key_hint("/", "search"),
        Span::raw("  "),
        key_hint("Tab", "tabs"),
        Span::raw("  "),
        key_hint("i", "install"),
        Span::raw("  "),
        key_hint("r", "remove"),
        Span::raw("  "),
        key_hint("F5", "refresh"),
        Span::raw("  "),
        key_hint("Esc", "back"),
    ];

    let p = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::DarkGray));
    f.render_widget(p, area);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn key_hint(key: &'static str, action: &'static str) -> Span<'static> {
    Span::styled(
        format!("[{key}]{action}"),
        Style::default().fg(Color::Yellow),
    )
}

/// Truncate a string to at most `max` characters (byte-safe, grapheme-approximate).
fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
