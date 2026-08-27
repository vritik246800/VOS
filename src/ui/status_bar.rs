use crate::core::state::AppMode;
use crate::ui::icons::{
    ICON_MODE_AUDIO, ICON_MODE_CALC, ICON_MODE_CALENDAR, ICON_MODE_CMD, ICON_MODE_CONFIG,
    ICON_MODE_CRON, ICON_MODE_DIALOG, ICON_MODE_DISK, ICON_MODE_DOCKER, ICON_MODE_EDITOR,
    ICON_MODE_FAVORITES, ICON_MODE_FILES, ICON_MODE_GIT, ICON_MODE_HELP, ICON_MODE_IMAGE,
    ICON_MODE_LOGS, ICON_MODE_MAN, ICON_MODE_MENU, ICON_MODE_NETWORK, ICON_MODE_NOTES,
    ICON_MODE_PACKAGES, ICON_MODE_PALETTE, ICON_MODE_PDF, ICON_MODE_PROCESS, ICON_MODE_QUIT,
    ICON_MODE_SERVICES, ICON_MODE_SFTP, ICON_MODE_SSH, ICON_MODE_TERMINAL, ICON_MODE_THEMES,
    ICON_MODE_VIDEO, ICON_MODE_WEATHER, ICON_MUSIC_NOTE, ICON_PAUSE, ICON_PLAY,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Sparkline},
};

/// `music` — optional `(track_name, is_playing)` to show background playback info.
/// `cpu_history` — recent CPU usage samples (0-100) for the mini sparkline shown
///   outside ProcessViewer mode.
pub fn render_status(
    f: &mut Frame,
    area: Rect,
    mode: &AppMode,
    msg: &str,
    cwd: &str,
    music: Option<(&str, bool)>,
    cpu_history: &[u64],
) {
    // When outside ProcessViewer, reserve the rightmost 12 columns for a small
    // CPU sparkline so the user always has a live pulse in the footer.
    let show_spark = !matches!(mode, AppMode::ProcessViewer) && area.width > 20;
    let spark_width: u16 = if show_spark { 12 } else { 0 };

    let (text_area, spark_area) = if show_spark {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(spark_width)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let mode_label = mode_str(mode);
    let hints = hint_spans(mode, msg);

    let mut spans = vec![
        Span::styled(
            format!(" {} ", mode_label),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    spans.extend(hints);

    // Music background playback indicator
    if let Some((track, playing)) = music {
        let sym = if playing { ICON_MUSIC_NOTE } else { ICON_PAUSE };
        let short_track = if track.len() > 20 {
            &track[..20]
        } else {
            track
        };
        spans.push(Span::styled(
            format!("  {sym} {short_track}"),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Right-align CWD
    let cwd_short = if cwd.len() > 40 {
        &cwd[cwd.len() - 40..]
    } else {
        cwd
    };
    spans.push(Span::styled(
        format!("  {cwd_short} "),
        Style::default().fg(Color::DarkGray),
    ));

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    f.render_widget(bar, text_area);

    // Render the mini CPU sparkline in the reserved right slice
    if let Some(sa) = spark_area {
        let last_pct = cpu_history.last().copied().unwrap_or(0);
        let spark_color = if last_pct > 80 {
            Color::Red
        } else if last_pct > 50 {
            Color::Yellow
        } else {
            Color::Cyan
        };
        let spark = Sparkline::default()
            .data(cpu_history)
            .style(Style::default().fg(spark_color).bg(Color::Black))
            .max(100);
        f.render_widget(spark, sa);
    }
}

fn mode_str(mode: &AppMode) -> String {
    match mode {
        AppMode::Menu => format!("{} MENU", ICON_MODE_MENU),
        AppMode::FileManager => format!("{} FILES", ICON_MODE_FILES),
        AppMode::Editor => format!("{} EDITOR", ICON_MODE_EDITOR),
        AppMode::Terminal => format!("{} TERMINAL", ICON_MODE_TERMINAL),
        AppMode::ProcessViewer => format!("{} PROCESSES", ICON_MODE_PROCESS),
        AppMode::Git => format!("{} GIT", ICON_MODE_GIT),
        AppMode::Config => format!("{} CONFIG", ICON_MODE_CONFIG),
        AppMode::AudioPlayer => format!("{} AUDIO", ICON_MODE_AUDIO),
        AppMode::VideoPlayer => format!("{} VIDEO", ICON_MODE_VIDEO),
        AppMode::ImageViewer => format!("{} IMAGE", ICON_MODE_IMAGE),
        AppMode::PdfViewer => format!("{} PDF", ICON_MODE_PDF),
        AppMode::Favorites => format!("{} FAVORITES", ICON_MODE_FAVORITES),
        AppMode::Help => format!("{} HELP", ICON_MODE_HELP),
        AppMode::ThemeSwitcher => format!("{} THEMES", ICON_MODE_THEMES),
        AppMode::LogViewer => format!("{} LOGS", ICON_MODE_LOGS),
        AppMode::ServiceManager => format!("{} SERVICES", ICON_MODE_SERVICES),
        AppMode::NetworkPanel => format!("{} NETWORK", ICON_MODE_NETWORK),
        AppMode::DiskManager => format!("{} DISK", ICON_MODE_DISK),
        AppMode::Calculator => format!("{} CALC", ICON_MODE_CALC),
        AppMode::PackageManager => format!("{} PACKAGES", ICON_MODE_PACKAGES),
        AppMode::SshManager => format!("{} SSH", ICON_MODE_SSH),
        AppMode::SshConnectForm => format!("{} SSH CONNECT", ICON_MODE_SSH),
        AppMode::SftpPanel => format!("{} SFTP", ICON_MODE_SFTP),
        AppMode::DockerPanel => format!("{} DOCKER", ICON_MODE_DOCKER),
        AppMode::CronEditor => format!("{} CRON", ICON_MODE_CRON),
        AppMode::ManViewer => format!("{} MAN", ICON_MODE_MAN),
        AppMode::Notes => format!("{} NOTES", ICON_MODE_NOTES),
        AppMode::Weather => format!("{} WEATHER", ICON_MODE_WEATHER),
        AppMode::Calendar => format!("{} CALENDAR", ICON_MODE_CALENDAR),
        AppMode::CommandPalette => format!("{} PALETTE", ICON_MODE_PALETTE),
        AppMode::Command(_) => format!("{} CMD", ICON_MODE_CMD),
        AppMode::Dialog(_) => format!("{} DIALOG", ICON_MODE_DIALOG),
        AppMode::Quitting => format!("{} QUITTING", ICON_MODE_QUIT),
    }
}

fn hint_spans<'a>(mode: &'a AppMode, msg: &'a str) -> Vec<Span<'a>> {
    let key = |s: &'static str| {
        Span::styled(
            s,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };
    let txt = |s: &'static str| Span::raw(s);

    let mut spans = match mode {
        AppMode::Menu => vec![
            key("↑↓"),
            txt(" Navigate  "),
            key("Enter"),
            txt(" Open  "),
            key("Ctrl+P"),
            txt(" Palette  "),
            key("F10"),
            txt(" Quit"),
        ],
        AppMode::FileManager => vec![
            key("↑↓"),
            txt(" Navigate  "),
            key("Enter"),
            txt(" Open/Edit  "),
            key("c"),
            txt(" Copy  "),
            key("x"),
            txt(" Cut  "),
            key("p"),
            txt(" Paste  "),
            key("Del"),
            txt(" Delete  "),
            key("t"),
            txt(" Terminal  "),
            key("h"),
            txt(" Hidden  "),
            key("F2"),
            txt(" Rename  "),
        ],
        AppMode::Editor => vec![
            key("Ctrl+S"),
            txt(" Save  "),
            key("Ctrl+Z"),
            txt(" Undo  "),
            key("Ctrl+Y"),
            txt(" Redo  "),
            key("Ctrl+F"),
            txt(" Search"),
        ],
        AppMode::Terminal => vec![
            key("Enter"),
            txt(" Run  "),
            key("↑↓"),
            txt(" History  "),
            key("Ctrl+P"),
            txt(" Palette"),
        ],
        AppMode::ProcessViewer => vec![
            key("d"),
            txt(" Dashboard ⇄ Processes  "),
            key("↑↓"),
            txt(" Navigate  "),
            key("k"),
            txt(" Kill  "),
            key("F5"),
            txt(" Refresh  "),
            key("1-4"),
            txt(" Sort"),
        ],
        AppMode::Git => vec![
            key("↑↓"),
            txt(" Navigate  "),
            key("r"),
            txt(" Refresh  "),
            key("a"),
            txt(" Add  "),
            key("c"),
            txt(" Commit  "),
            key("p"),
            txt(" Push"),
        ],
        AppMode::AudioPlayer => vec![
            key("Spc"),
            txt(" Play/Pause  "),
            key("←→"),
            txt(" ±10s  "),
            key("N/P"),
            txt(" Track  "),
            key("+/-"),
            txt(" Volume  "),
            key("Esc"),
            txt(" Background  "),
            key("m"),
            txt(" Panel"),
        ],
        AppMode::Calculator => vec![
            key("0-9 + - * / ( )"),
            txt("  Expression  "),
            key("Esc"),
            txt(" Close"),
        ],
        AppMode::Command(input) => vec![
            Span::styled(
                ":",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(input.clone(), Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::Yellow)),
        ],
        _ => vec![
            key("Ctrl+P"),
            txt(" Palette  "),
            key("F1"),
            txt(" Help  "),
            key("Esc"),
            txt(" Back"),
        ],
    };

    if !msg.is_empty() {
        spans.push(Span::styled(
            format!("  {msg}"),
            Style::default().fg(Color::Green),
        ));
    }
    spans
}
