#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

mod app;
mod audio;
mod config;
mod core;
mod db;
mod editor;
mod events;
mod fs;
mod kitty;
mod modules;
mod plugins;
mod session;
mod terminal;
mod ui;
mod video;
mod wm;

use anyhow::Result;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, EventStream, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use tokio::time::{Duration, interval};

use app::App;
use core::state::AppMode;
use events::input::handle_input;

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Enable the enhanced keyboard protocol so the terminal reports modifier
    // combos like Ctrl+1..9/0 (which legacy terminals can't encode). Only
    // pushed when the terminal supports it; popped on exit.
    let kbd_enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if kbd_enhanced {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    app.kbd_enhanced = kbd_enhanced;
    let result = run_app(&mut terminal, &mut app).await;

    if kbd_enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut event_stream = EventStream::new();
    let mut tick_timer = interval(Duration::from_millis(16));

    loop {
        if app.force_full_redraw {
            terminal.clear()?;
            app.force_full_redraw = false;
        }
        terminal.draw(|f| render(f, app))?;

        // Inject Kitty graphics frame AFTER terminal.draw() so the event loop
        // is not blocked by image encoding during the draw phase.
        if let Some(frame) = app.pending_kitty.take() {
            let _ = kitty::inject_frame(&frame);
        }

        // Run the (blocking) SSH connect only AFTER the frame showing the
        // "Connecting…" popup has actually been drawn — see
        // `App::ssh_connect_now` and `app.ssh_pending_connect`.
        if let Some(idx) = app.ssh_pending_connect.take() {
            app.ssh_connect_now(idx);
        }
        if app.ssh_pending_adhoc_connect {
            app.ssh_pending_adhoc_connect = false;
            app.ssh_connect_adhoc();
        }

        if matches!(app.state.mode, AppMode::Quitting) {
            return Ok(());
        }

        tokio::select! {
            Some(Ok(event)) = event_stream.next() => {
                let quit = handle_input(app, event)?;
                if quit { break; }
            }
            _ = tick_timer.tick() => {
                if app.needs_tick() {
                    app.tick();
                }
            }
        }
    }
    Ok(())
}

fn render(f: &mut ratatui::Frame, app: &mut App) {
    use wm::layout::{base_layout, with_side_panel};

    let (tab_area, content_area, status_area) = base_layout(f.area());

    // Keep the active tab's title in sync with its live activity (the inactive
    // tabs already cached their title when the user switched away from them).
    if app.tabs.get(app.active_tab).is_some() {
        let title = app::mode_title(
            &app.state.mode,
            &app.explorer,
            app.tabs[app.active_tab].editor.as_ref(),
        );
        app.tabs[app.active_tab].title = title;
    }
    let tab_titles: Vec<String> = app.tabs.iter().map(|t| t.title.clone()).collect();
    ui::menu::render_tab_bar(f, tab_area, &tab_titles, app.active_tab);

    let cwd = app.state.current_dir.to_string_lossy().to_string();
    let msg = app.state.status_msg.clone().unwrap_or_default();

    // Music info for the status bar footer (hidden when AudioPlayer is active — it shows its own UI)
    let music_info: Option<(String, bool)> = app
        .audio_player
        .as_ref()
        .filter(|_| !matches!(app.state.mode, AppMode::AudioPlayer))
        .map(|ap| (ap.track_name.clone(), ap.is_playing));
    let music_ref = music_info
        .as_ref()
        .map(|(name, playing)| (name.as_str(), *playing));
    let cpu_u64: Vec<u64> = app
        .system_monitor
        .cpu_history
        .iter()
        .map(|&v| v as u64)
        .collect();
    ui::status_bar::render_status(
        f,
        status_area,
        &app.state.mode,
        &msg,
        &cwd,
        music_ref,
        &cpu_u64,
    );

    // ── Right side panel (Terminal / Git) ────────────────────────────────────
    let (content_after_side, side_area) = if app.state.side_pct > 0 {
        let (m, s) = with_side_panel(content_area, app.state.side_pct);
        (m, Some(s))
    } else {
        (content_area, None)
    };

    // Music notch panel is now a floating overlay — no split needed
    let main_area = content_after_side;

    render_main(f, app, main_area);

    if let Some(sa) = side_area {
        app.last_areas.side_panel = sa;
        render_side(f, app, sa);
    }

    // ── Music notch panel: floating overlay, centered at top ─────────────────
    if app.state.music_pct > 0 {
        if let Some(ap) = &app.audio_player {
            use ratatui::layout::Rect;
            let panel_width = (content_after_side.width * 30 / 100).max(22).min(52);
            let panel_height = app.state.music_pct.min(content_after_side.height);
            // Center horizontally
            let panel_x =
                content_after_side.x + (content_after_side.width.saturating_sub(panel_width)) / 2;
            let panel_y = content_after_side.y;
            let panel_rect = Rect::new(panel_x, panel_y, panel_width, panel_height);
            f.render_widget(ratatui::widgets::Clear, panel_rect);
            ui::music_panel::render_music_panel(f, panel_rect, ap, app.state.music_panel_focused);
        }
    }

    // ── Calculator bottom-sheet overlay (drawn even during close animation) ─────
    if app.calculator.is_visible() && !matches!(app.state.mode, AppMode::Calculator) {
        // Mode has been restored but the panel is still animating closed — keep rendering it.
        ui::calc_panel::render_calc(f, app, main_area);
    }

    if matches!(app.state.mode, AppMode::CommandPalette) {
        app.command_palette.render(f, f.area());
    }

    if let AppMode::Dialog(ref kind) = app.state.mode.clone() {
        render_dialog(f, kind, f.area());
    }

    ui::notifications::render_notifications(f, f.area(), &app.state.notifications);
}

fn popup_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = (area.width * 88 / 100).max(20).min(area.width);
    let h = (area.height * 88 / 100).max(8).min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn render_main(f: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    use ratatui::widgets::Clear;

    // For popup modes, render the underlying panel first, then overlay the popup.
    let is_popup = matches!(
        app.state.mode,
        AppMode::ImageViewer | AppMode::VideoPlayer | AppMode::PdfViewer
    );
    let base_mode = if is_popup {
        let prev = app.state.prev_mode.clone();
        // Guard: avoid recursion if somehow prev is also a popup mode
        match prev {
            AppMode::ImageViewer | AppMode::VideoPlayer | AppMode::PdfViewer => {
                AppMode::FileManager
            }
            m => m,
        }
    } else {
        app.state.mode.clone()
    };

    // ── Base content ────────────────────────────────────────────────────────────
    match &base_mode {
        AppMode::Menu => {
            app.last_areas.menu_list =
                ui::menu::render_menu(f, area, &mut app.menu_state, app.splash_tick);
        }
        AppMode::FileManager | AppMode::ImageViewer | AppMode::VideoPlayer | AppMode::PdfViewer => {
            let focused = !app.state.side_focused;
            let cut = app
                .clipboard
                .as_ref()
                .filter(|c| c.op == crate::fs::explorer::ClipboardOp::Cut)
                .map(|c| c.path.as_path());
            app.last_areas.file_list =
                ui::file_panel::render_file_panel(f, area, &app.explorer, focused, cut);
        }
        AppMode::Editor => {
            let focused = !app.state.side_focused;
            if let Some(buf) = app
                .tabs
                .get_mut(app.active_tab)
                .and_then(|t| t.editor.as_mut())
            {
                ui::editor_panel::render_editor_panel(f, area, buf, focused);
            } else {
                app.last_areas.file_list =
                    ui::file_panel::render_file_panel(f, area, &app.explorer, focused, None);
            }
        }
        AppMode::Terminal => {
            ui::terminal_panel::render_terminal(f, area, &app.terminal, !app.state.side_focused);
        }
        AppMode::ProcessViewer => {
            if app.monitor_dashboard {
                ui::sysmon_panel::render_sysmon(f, area, &app.system_monitor);
                // No process rows on screen — kill the stale mouse hit box.
                app.last_areas.process_list = ratatui::layout::Rect::default();
            } else {
                app.last_areas.process_list = ui::process_panel::render_monitor(
                    f,
                    area,
                    &mut app.process_viewer,
                    &app.system_monitor,
                );
            }
        }
        AppMode::Git => {
            let panel_h = (area.height as u32 * app.state.git_pct as u32 / 100) as u16;
            if panel_h > 0 {
                let panel_rect = ratatui::layout::Rect::new(
                    area.x,
                    area.y,
                    area.width,
                    panel_h.min(area.height),
                );
                if let Some(gp) = &mut app.git_plugin {
                    gp.render_panel(f, panel_rect);
                }
            }
        }
        AppMode::Config => {
            ui::config_panel::render_config_panel(f, area, &app.settings, app.config_selected);
        }
        AppMode::AudioPlayer => {
            ui::audio_panel::render_audio_panel(f, area, app);
        }
        AppMode::Help => {
            ui::help_panel::render_help_panel(
                f,
                area,
                app.help_topic,
                app.help_tab,
                app.help_scroll,
            );
        }
        AppMode::LogViewer => {
            ui::log_panel::render_logs(f, app, area);
        }
        AppMode::ServiceManager => {
            ui::service_panel::render_services(f, app, area);
        }
        AppMode::NetworkPanel => {
            ui::network_panel::render_network(f, app, area);
        }
        AppMode::DiskManager => {
            ui::disk_panel::render_disk(f, app, area);
        }
        AppMode::Calculator => {
            // Calculator renders as a popup over the current mode
            let cut = app
                .clipboard
                .as_ref()
                .filter(|c| c.op == crate::fs::explorer::ClipboardOp::Cut)
                .map(|c| c.path.as_path());
            ui::file_panel::render_file_panel(f, area, &app.explorer, !app.state.side_focused, cut);
            ui::calc_panel::render_calc(f, app, area);
        }
        AppMode::Favorites => {
            // Render the underlying FileManager first, then the popup overlay
            let cut = app
                .clipboard
                .as_ref()
                .filter(|c| c.op == crate::fs::explorer::ClipboardOp::Cut)
                .map(|c| c.path.as_path());
            ui::file_panel::render_file_panel(f, area, &app.explorer, !app.state.side_focused, cut);
            ui::favorites_panel::render_favorites(f, app, area);
        }
        AppMode::ThemeSwitcher => {
            // Render underlying mode first, then the switcher popup on top
            let cut = app
                .clipboard
                .as_ref()
                .filter(|c| c.op == crate::fs::explorer::ClipboardOp::Cut)
                .map(|c| c.path.as_path());
            ui::file_panel::render_file_panel(f, area, &app.explorer, !app.state.side_focused, cut);
            ui::theme_switcher::render_theme_switcher(f, app, area);
        }
        AppMode::PackageManager => {
            ui::packages_panel::render_packages_panel(f, app, area);
        }
        AppMode::SshManager => {
            let connecting = app
                .ssh_pending_connect
                .and_then(|i| app.ssh_manager.hosts.get(i))
                .map(|h| h.alias.clone());
            ui::ssh_panel::render_ssh_panel(f, &app.ssh_manager, area, connecting.as_deref());
        }
        AppMode::SshConnectForm => {
            if let Some(form) = &app.ssh_manager.conn_form {
                ui::ssh_panel::render_ssh_connect_form(
                    f,
                    area,
                    form,
                    app.ssh_pending_adhoc_connect,
                );
            }
        }
        AppMode::SftpPanel => {
            if let Some(panel) = &mut app.sftp_panel {
                ui::sftp_panel::render_sftp_panel(f, area, panel);
            }
        }
        AppMode::DockerPanel => {
            ui::docker_panel::render_docker_panel(f, app, area);
        }
        AppMode::CronEditor => {
            ui::cron_panel::render_cron_panel(f, &app.cron_editor, area);
        }
        AppMode::ManViewer => {
            ui::man_panel::render_man_panel(f, &app.man_viewer, area);
        }
        AppMode::Notes => {
            ui::notes_panel::render_notes(f, app, area);
        }
        AppMode::Weather => {
            ui::weather_panel::render_weather(f, app, area);
        }
        AppMode::Calendar => {
            ui::calendar_panel::render_calendar(f, app, area);
        }
        AppMode::Command(_) | AppMode::CommandPalette | AppMode::Dialog(_) | AppMode::Quitting => {
            let cut = app
                .clipboard
                .as_ref()
                .filter(|c| c.op == crate::fs::explorer::ClipboardOp::Cut)
                .map(|c| c.path.as_path());
            app.last_areas.file_list = ui::file_panel::render_file_panel(
                f,
                area,
                &app.explorer,
                !app.state.side_focused,
                cut,
            );
        }
    }

    // ── Popup overlays ──────────────────────────────────────────────────────────
    if is_popup {
        let popup = popup_rect(area);
        // Skip Clear in Kitty+video mode: writing to cells erases the Kitty
        // image layer, causing flicker. The image covers the inner area anyway.
        let skip_clear = app.use_kitty && matches!(app.state.mode, AppMode::VideoPlayer);
        if !skip_clear {
            f.render_widget(Clear, popup);
        }

        match app.state.mode.clone() {
            AppMode::ImageViewer => {
                let title = app.image_title.clone();
                let inner = ui::image_panel::render_image_panel(f, popup, &title);
                if app.use_kitty {
                    // Kitty protocol: only retransmit when area changes (static image)
                    if app.kitty_last_area != Some(inner) {
                        app.kitty_last_area = Some(inner);
                        if let Some(rgb) = app.image_rgb.clone() {
                            let dims = app.image_dims;
                            app.pending_kitty = Some(kitty::KittyFrame {
                                image_id: 1,
                                area: inner,
                                rgb_data: rgb,
                                px_width: dims.0,
                                px_height: dims.1,
                            });
                        }
                    }
                } else if let Some(rgb) = &app.image_rgb {
                    // 24-bit half-block fallback
                    let dims = app.image_dims;
                    let lines = kitty::rgb_to_halfblock_lines(
                        rgb,
                        dims.0,
                        dims.1,
                        inner.width,
                        inner.height,
                    );
                    f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
                }
            }
            AppMode::VideoPlayer => {
                if let Some(vp) = &mut app.video_player {
                    let frame_area = ui::video_panel::render_video_panel(f, popup, vp);
                    // Keep cell_px in sync (terminal may have been resized).
                    if app.use_kitty {
                        vp.cell_px = app.kitty_cell_px;
                    }
                    if let Some(ka) = frame_area {
                        if let Some(rgb) = vp.current_raw_frame.clone() {
                            if app.use_kitty {
                                // Pixel dims = char dims × cell pixel size.
                                let px_w = vp.width as u32 * vp.cell_px.0 as u32;
                                let px_h = vp.height as u32 * vp.cell_px.1 as u32;
                                app.pending_kitty = Some(kitty::KittyFrame {
                                    image_id: 2,
                                    area: ka,
                                    rgb_data: rgb,
                                    px_width: px_w,
                                    px_height: px_h,
                                });
                            } else {
                                // 24-bit half-block fallback
                                let px_w = vp.width as u32;
                                let px_h = vp.height as u32 * 2;
                                let lines = kitty::rgb_to_halfblock_lines(
                                    &rgb, px_w, px_h, ka.width, ka.height,
                                );
                                f.render_widget(ratatui::widgets::Paragraph::new(lines), ka);
                            }
                        }
                    }
                }
            }
            AppMode::PdfViewer => {
                let lines = app.pdf_pages.clone();
                let scroll = app.pdf_scroll;
                let title = app.pdf_title.clone();
                ui::pdf_panel::render_pdf_panel(f, popup, &lines, scroll, &title);
            }
            _ => {}
        }
    }
}

fn render_side(f: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    use core::state::SidePanelMode;
    use ratatui::{
        style::{Color, Style},
        widgets::{Block, Borders},
    };

    let focused = app.state.side_focused;
    let border_color = if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    match app.state.side_mode {
        SidePanelMode::Git => {
            if let Some(gp) = &mut app.git_plugin {
                use plugins::Plugin;
                gp.render(f, inner);
            }
        }
        SidePanelMode::Terminal => {
            ui::terminal_panel::render_terminal(f, inner, &app.terminal, focused);
        }
        _ => {}
    }
}

fn render_dialog(
    f: &mut ratatui::Frame,
    kind: &core::state::DialogKind,
    area: ratatui::layout::Rect,
) {
    use core::state::DialogKind;
    use ratatui::{
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Clear, Paragraph},
    };

    let (title, body, hint) = match kind {
        DialogKind::Confirm { message, .. } => (
            " Confirm ",
            message.as_str(),
            " Enter/Y — Yes     Esc/N — No ",
        ),
        DialogKind::Input { prompt, .. } => (
            " Input ",
            prompt.as_str(),
            " Enter — Confirm     Esc — Cancel ",
        ),
        DialogKind::Alert(msg) => (" Warning ", msg.as_str(), " Any key to close "),
    };

    let w = 50u16.min(area.width.saturating_sub(4));
    let h = 7u16;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {body}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(hint, Style::default().fg(Color::Yellow))),
    ];
    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    f.render_widget(widget, popup);
}
