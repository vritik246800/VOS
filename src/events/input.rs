use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use image::GenericImageView;

use crate::app::App;
use crate::audio::player::AudioPlayer;
use crate::core::command::CommandParser;
use crate::core::keybinds::Action;
use crate::core::state::{AppMode, ConfirmAction, DialogKind, Notification, SidePanelMode};
use crate::modules::process::SortBy;
use crate::plugins::git::GitPlugin;
use crate::ui::command_palette::PaletteAction;
use ratatui::widgets::ListState;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff"];

pub fn handle_input(app: &mut App, event: Event) -> Result<bool> {
    match event {
        Event::Key(key) => handle_key(app, key),
        Event::Mouse(mouse) => {
            handle_mouse(app, mouse);
            Ok(false)
        }
        Event::Resize(_, _) => Ok(false),
        _ => Ok(false),
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    // Global overrides (always active)
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if matches!(app.state.mode, AppMode::Terminal) {
                return Ok(false);
            }
        }
        _ => {}
    }

    match &app.state.mode.clone() {
        AppMode::Command(input) => return handle_command_mode(app, key, input.clone()),
        AppMode::CommandPalette => return handle_palette(app, key),
        AppMode::Dialog(_) => return handle_dialog(app, key),
        _ => {}
    }

    // Mini music notch panel focus: capture music keys when panel is open and focused
    if app.state.music_panel_focused && app.state.music_pct_target > 0 {
        return handle_mini_music_panel(app, key);
    }

    // Side panel focus routing
    if app.state.side_focused {
        return handle_side_panel(app, key);
    }

    // Global Ctrl+G: toggle git panel from any mode
    if key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.toggle_side_git();
        return Ok(false);
    }

    // Global Ctrl+A: open/toggle the calculator bottom sheet (from any non-typing mode)
    if key.code == KeyCode::Char('a')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !matches!(app.state.mode, AppMode::Editor | AppMode::Terminal)
    {
        if matches!(app.state.mode, AppMode::Calculator) {
            // Already open — close with animation
            app.calculator.anim_target = 0;
            app.state.restore_mode();
        } else {
            // If currently in a transient overlay (Help, CommandPalette, ThemeSwitcher),
            // close it first so the calculator returns to the real content mode on Esc.
            if matches!(
                app.state.mode,
                AppMode::Help | AppMode::CommandPalette | AppMode::ThemeSwitcher
            ) {
                app.state.restore_mode();
            }
            app.state.set_mode(AppMode::Calculator);
            app.calculator.anim_pct = 0;
            app.calculator.anim_target = 100;
        }
        return Ok(false);
    }

    // 'm' toggles music mini-panel from any non-typing mode when audio is active.
    // NOTE: Ctrl+M = Enter (CR) in terminals, so we use bare 'm' instead.
    if key.code == KeyCode::Char('m')
        && key.modifiers.is_empty()
        && app.audio_player.is_some()
        && !matches!(app.state.mode, AppMode::Editor | AppMode::Terminal)
        && !(matches!(app.state.mode, AppMode::Calendar) && app.calendar.editing)
        && !(matches!(app.state.mode, AppMode::Weather) && app.weather.search.active)
    {
        if matches!(app.state.mode, AppMode::AudioPlayer) {
            // Minimize full-screen player to notch mini panel
            app.state.set_mode(AppMode::FileManager);
            app.state.music_pct_target = 10;
            app.state.music_panel_focused = true;
        } else {
            app.toggle_music_panel();
        }
        return Ok(false);
    }

    // Global actions via keybind engine
    if let Some(action) = app.keybinds.resolve(&key).cloned() {
        match action {
            Action::CommandPalette => {
                app.open_command_palette();
                return Ok(false);
            }
            Action::Help => {
                if !matches!(app.state.mode, AppMode::Calculator) {
                    app.state.set_mode(AppMode::Help);
                }
                return Ok(false);
            }
            Action::GlobalMenu => {
                app.state.set_mode(AppMode::Menu);
                return Ok(false);
            }
            Action::Quit => {
                app.state.set_mode(AppMode::Quitting);
                return Ok(false);
            }
            Action::NewTab => {
                app.new_tab();
                return Ok(false);
            }
            Action::CloseTab => {
                app.close_tab();
                return Ok(false);
            }
            Action::SwitchTab(n) => {
                app.switch_tab(n);
                return Ok(false);
            }
            Action::SwitchWindow(n) => {
                /* window manager — future */
                let _ = n;
                return Ok(false);
            }
            _ => {}
        }
    }

    match app.state.mode.clone() {
        AppMode::Menu => handle_menu(app, key)?,
        AppMode::FileManager => handle_file_manager(app, key)?,
        AppMode::Editor => handle_editor(app, key)?,
        AppMode::Terminal => handle_terminal(app, key)?,
        AppMode::ProcessViewer => handle_process(app, key)?,
        AppMode::Git => handle_git(app, key)?,
        AppMode::Config => handle_config(app, key),
        AppMode::AudioPlayer => handle_audio(app, key)?,
        AppMode::VideoPlayer => handle_video(app, key)?,
        AppMode::ImageViewer => handle_image(app, key),
        AppMode::PdfViewer => handle_pdf(app, key),
        AppMode::Help => handle_help(app, key),
        // Phase 2 modules
        AppMode::LogViewer => handle_logviewer(app, key),
        AppMode::ServiceManager => handle_services(app, key),
        AppMode::NetworkPanel => handle_network(app, key),
        AppMode::DiskManager => handle_disk(app, key),
        AppMode::Calculator => handle_calculator(app, key),
        // Phase 3 overlays
        AppMode::Favorites => handle_favorites(app, key),
        AppMode::ThemeSwitcher => handle_theme_switcher(app, key),
        // Phase 4 modules
        AppMode::PackageManager => handle_packages(app, key),
        AppMode::SshManager => handle_ssh(app, key),
        AppMode::SshConnectForm => handle_ssh_connect_form(app, key),
        AppMode::SftpPanel => handle_sftp(app, key),
        AppMode::DockerPanel => handle_docker(app, key),
        AppMode::CronEditor => handle_cron(app, key),
        AppMode::ManViewer => handle_man(app, key),
        // Phase 6 apps
        AppMode::Notes => handle_notes(app, key),
        AppMode::Weather => handle_weather(app, key),
        AppMode::Calendar => handle_calendar(app, key),
        AppMode::Quitting => {}
        _ => {}
    }
    Ok(false)
}

// ── Command mode ──────────────────────────────────────────────────────────────

fn handle_command_mode(app: &mut App, key: KeyEvent, input: String) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            let cmd = CommandParser::parse(&input);
            app.state.mode = AppMode::FileManager;
            app.execute_command(cmd)?;
        }
        KeyCode::Esc => {
            app.state.restore_mode();
        }
        KeyCode::Char(c) => {
            let mut new_input = input;
            new_input.push(c);
            app.state.mode = AppMode::Command(new_input);
        }
        KeyCode::Backspace => {
            let mut new_input = input;
            new_input.pop();
            if new_input.is_empty() {
                app.state.restore_mode();
            } else {
                app.state.mode = AppMode::Command(new_input);
            }
        }
        _ => {}
    }
    Ok(false)
}

// ── Command palette ───────────────────────────────────────────────────────────

fn handle_palette(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc => {
            app.state.restore_mode();
        }
        KeyCode::Up => app.command_palette.move_up(),
        KeyCode::Down => app.command_palette.move_down(),
        KeyCode::Enter => {
            if let Some(action) = app.command_palette.selected_action().cloned() {
                app.state.restore_mode();
                execute_palette_action(app, action)?;
            }
        }
        KeyCode::Backspace => app.command_palette.pop_char(),
        KeyCode::Char(c) => app.command_palette.push_char(c),
        _ => {}
    }
    Ok(false)
}

fn execute_palette_action(app: &mut App, action: PaletteAction) -> Result<()> {
    match action {
        PaletteAction::OpenModule(m) => {
            let mode = match m.as_str() {
                "FileManager" => AppMode::FileManager,
                "Editor" => AppMode::Editor,
                "Terminal" => AppMode::Terminal,
                "ProcessViewer" | "SystemMonitor" => AppMode::ProcessViewer,
                "Git" => AppMode::Git,
                "Config" => AppMode::Config,
                "AudioPlayer" => AppMode::AudioPlayer,
                "VideoPlayer" => AppMode::VideoPlayer,
                "Help" => AppMode::Help,
                "Favorites" => {
                    let favs = app.db.get_favorites().unwrap_or_default();
                    app.favorites_list = favs.into_iter().map(std::path::PathBuf::from).collect();
                    if !app.favorites_list.is_empty() {
                        app.favorites_state.select(Some(0));
                    }
                    AppMode::Favorites
                }
                "ThemeSwitcher" => {
                    app.theme_switcher_original = app.settings.theme.clone();
                    let themes = crate::ui::theme::Theme::all();
                    app.theme_switcher_idx = themes
                        .iter()
                        .position(|&t| t == app.settings.theme.as_str())
                        .unwrap_or(0);
                    AppMode::ThemeSwitcher
                }
                // Phase 2
                "LogViewer" => AppMode::LogViewer,
                "ServiceManager" => AppMode::ServiceManager,
                "NetworkPanel" => AppMode::NetworkPanel,
                "DiskManager" => {
                    let root = app.state.current_dir.clone();
                    app.disk_manager.start_scan(root);
                    AppMode::DiskManager
                }
                "Calculator" => AppMode::Calculator,
                // Phase 4
                "PackageManager" => {
                    app.package_manager.load_installed();
                    AppMode::PackageManager
                }
                "SshManager" => {
                    app.enter_ssh();
                    return Ok(());
                }
                "DockerPanel" => {
                    let _ = app.docker_panel.refresh();
                    AppMode::DockerPanel
                }
                "CronEditor" => AppMode::CronEditor,
                "ManViewer" => AppMode::ManViewer,
                // Phase 6
                "Notes" => {
                    app.enter_notes();
                    return Ok(());
                }
                "Weather" => {
                    app.enter_weather();
                    return Ok(());
                }
                "Calendar" => {
                    app.enter_calendar();
                    return Ok(());
                }
                _ => AppMode::Menu,
            };
            app.state.set_mode(mode);
        }
        PaletteAction::RunCommand(cmd) => {
            let c = CommandParser::parse(&cmd);
            app.execute_command(c)?;
        }
        PaletteAction::SetTheme(t) => {
            app.settings.theme = t.clone();
            let _ = app
                .settings
                .save(std::path::Path::new("config/config.toml"));
            app.state
                .push_notification(Notification::success(format!("Tema: {t}")));
        }
        PaletteAction::OpenFile(p) => {
            app.open_file(std::path::PathBuf::from(p))?;
        }
    }
    Ok(())
}

// ── Dialog ────────────────────────────────────────────────────────────────────

fn handle_dialog(app: &mut App, key: KeyEvent) -> Result<bool> {
    let mode = app.state.mode.clone();
    if let AppMode::Dialog(ref kind) = mode {
        match kind {
            DialogKind::Confirm { action, .. } => {
                let action = action.clone();
                match key.code {
                    KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                        app.state.restore_mode();
                        dispatch_confirm(app, action)?;
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        app.state.restore_mode();
                    }
                    _ => {}
                }
            }
            DialogKind::Input { value, .. } => {
                let value = value.clone();
                match key.code {
                    KeyCode::Enter => {
                        let val = value.clone();
                        app.state.restore_mode();
                        if app.rename_src.is_some() {
                            let _ = app.execute_rename(&val);
                        } else if app.notes_new {
                            app.notes_new = false;
                            app.create_note(&val);
                        } else if app.ssh_new_group {
                            app.ssh_new_group = false;
                            app.ssh_create_group(&val);
                        } else if let Some(idx) = app.ssh_assign_group_host.take() {
                            app.ssh_assign_group(idx, &val);
                        } else if app.ssh_tunnel_form {
                            app.ssh_tunnel_form = false;
                            app.ssh_create_tunnel_from_form(&val);
                        }
                    }
                    KeyCode::Esc => {
                        app.rename_src = None;
                        app.notes_new = false;
                        app.ssh_new_group = false;
                        app.ssh_assign_group_host = None;
                        app.ssh_tunnel_form = false;
                        app.state.restore_mode();
                    }
                    KeyCode::Char(c) => {
                        if let AppMode::Dialog(DialogKind::Input { ref mut value, .. }) =
                            app.state.mode
                        {
                            value.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        if let AppMode::Dialog(DialogKind::Input { ref mut value, .. }) =
                            app.state.mode
                        {
                            value.pop();
                        }
                    }
                    _ => {}
                }
                let _ = value;
            }
            DialogKind::Alert(_) => {
                app.state.restore_mode();
            }
        }
    }
    Ok(false)
}

fn dispatch_confirm(app: &mut App, action: ConfirmAction) -> Result<()> {
    match action {
        ConfirmAction::KillProcess(pid) => {
            let _ = pid;
            if app.process_viewer.kill_selected() {
                app.state
                    .push_notification(Notification::success("Process terminated"));
            }
        }
        ConfirmAction::DeleteFile(path) => {
            if let Err(e) = std::fs::remove_file(&path) {
                app.state
                    .push_notification(Notification::error(format!("Error: {e}")));
            } else {
                let _ = app.explorer.load_entries();
                app.state
                    .push_notification(Notification::success(format!("Deleted: {path}")));
            }
        }
        ConfirmAction::QuitUnsaved => {
            app.state.set_mode(AppMode::Quitting);
        }
        ConfirmAction::OverwriteFile(_) => {}
        ConfirmAction::SaveSshHost {
            alias,
            hostname,
            user,
            port,
            existing,
        } => {
            app.ssh_save_host(&alias, &hostname, &user, port, existing);
        }
        ConfirmAction::DeleteSshHost(alias) => {
            app.ssh_delete_host(&alias);
        }
    }
    Ok(())
}

// ── Music notch panel ─────────────────────────────────────────────────────────

fn handle_mini_music_panel(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char(' ') => {
            if let Some(ap) = &mut app.audio_player {
                if ap.is_playing {
                    ap.pause()
                } else {
                    ap.resume()
                }
            }
        }
        KeyCode::Left => {
            if let Some(ap) = &mut app.audio_player {
                let _ = ap.prev_track();
            }
        }
        KeyCode::Right => {
            if let Some(ap) = &mut app.audio_player {
                let _ = ap.next_track();
            }
        }
        KeyCode::Char('q') => {
            // Stop music completely and close panel
            app.audio_player = None;
            app.state.music_pct_target = 0;
            app.state.music_panel_focused = false;
            app.state
                .push_notification(crate::core::state::Notification::info("Music stopped"));
        }
        KeyCode::Char('m') | KeyCode::Esc => {
            // Close panel, keep playing
            app.state.music_pct_target = 0;
            app.state.music_panel_focused = false;
        }
        _ => {} // other keys fall through; return false so normal handling continues
    }
    Ok(false)
}

// ── Side panel ────────────────────────────────────────────────────────────────

fn handle_side_panel(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Tab => {
            app.state.side_focused = false;
        }
        KeyCode::Esc => {
            app.state.side_focused = false;
            app.state.side_pct_target = 0;
        }
        _ => match app.state.side_mode {
            SidePanelMode::Git => {
                if let Some(gp) = &mut app.git_plugin {
                    match key.code {
                        KeyCode::Up => gp.move_up(),
                        KeyCode::Down => gp.move_down(),
                        KeyCode::Char('r') => gp.refresh(),
                        _ => {}
                    }
                }
            }
            SidePanelMode::Terminal => {
                handle_terminal_inner(app, key)?;
            }
            SidePanelMode::AudioPlayer => {
                if let Some(ap) = &mut app.audio_player {
                    match key.code {
                        KeyCode::Char(' ') => {
                            if ap.is_playing {
                                ap.pause()
                            } else {
                                ap.resume();
                            }
                        }
                        KeyCode::Left => ap.seek_backward(10),
                        KeyCode::Right => ap.seek_forward(10),
                        KeyCode::Char('n') => {
                            let _ = ap.next_track();
                        }
                        KeyCode::Char('p') => {
                            let _ = ap.prev_track();
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => ap.volume_up(),
                        KeyCode::Char('-') => ap.volume_down(),
                        KeyCode::Char('m') => {
                            // Close music mini panel, return focus to main content
                            app.state.music_pct_target = 0;
                            app.state.music_panel_focused = false;
                            app.state.side_focused = false;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        },
    }
    Ok(false)
}

// ── Menu ──────────────────────────────────────────────────────────────────────

fn handle_menu(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => {
            if let Some(i) = app.menu_state.selected() {
                if i > 0 {
                    app.menu_state.select(Some(i - 1));
                }
            }
        }
        KeyCode::Down => {
            if let Some(i) = app.menu_state.selected() {
                let max = crate::ui::menu::MENU_LEN - 1;
                if i < max {
                    app.menu_state.select(Some(i + 1));
                }
            }
        }
        KeyCode::Enter => {
            if let Some(i) = app.menu_state.selected() {
                let mode = match i {
                    0 => AppMode::FileManager,
                    1 => AppMode::Terminal,
                    2 => AppMode::ProcessViewer,
                    3 => AppMode::Help,
                    4 => AppMode::Quitting,
                    _ => return Ok(()),
                };
                app.state.set_mode(mode);
            }
        }
        KeyCode::Char(':') => {
            app.state.set_mode(AppMode::Command(String::new()));
        }
        _ => {}
    }
    Ok(())
}

// ── File Manager ──────────────────────────────────────────────────────────────

fn handle_file_manager(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => app.explorer.move_up(),
        KeyCode::Down => app.explorer.move_down(),
        KeyCode::PageUp => app.explorer.page_up(10),
        KeyCode::PageDown => app.explorer.page_down(10),
        KeyCode::Backspace => {
            let _ = app.explorer.go_parent();
        }
        KeyCode::Enter => {
            if let Some(entry) = app.explorer.selected_entry().cloned() {
                if entry.is_dir {
                    let path = entry.path.clone();
                    let has_git = path.join(".git").exists();
                    let _ = app.explorer.enter_dir(path.clone());
                    app.state.current_dir = path.clone();
                    if has_git {
                        let mut gp = GitPlugin::new(path.clone());
                        gp.open_cwd_repo(&path);
                        app.git_plugin = Some(gp);
                    }
                } else {
                    let path = entry.path.clone();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    match ext.as_str() {
                        "mp3" | "wav" | "flac" | "ogg" | "m4a" => {
                            app.audio_player = AudioPlayer::new(path).ok();
                            if let Some(ap) = &mut app.audio_player {
                                let _ = ap.play();
                            }
                            app.state.set_mode(AppMode::AudioPlayer);
                        }
                        "mp4" | "mkv" | "avi" | "mov" | "webm" => {
                            let cell_px = if app.use_kitty {
                                app.kitty_cell_px
                            } else {
                                (1, 2)
                            };
                            app.video_player =
                                crate::video::player::VideoPlayer::new(path, 80, 40, cell_px).ok();
                            app.state.set_mode(AppMode::VideoPlayer);
                        }
                        "pdf" => {
                            app.open_pdf(path);
                        }
                        ext if IMAGE_EXTENSIONS.contains(&ext) => {
                            let title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("image")
                                .to_string();
                            match std::fs::read(&path) {
                                Ok(bytes) => match image::load_from_memory(&bytes) {
                                    Ok(img) => {
                                        let max_w = 1200u32;
                                        let max_h = 900u32;
                                        let (orig_w, orig_h) = img.dimensions();
                                        let img = if orig_w > max_w || orig_h > max_h {
                                            img.resize(
                                                max_w,
                                                max_h,
                                                image::imageops::FilterType::Triangle,
                                            )
                                        } else {
                                            img
                                        };
                                        let (w, h) = img.dimensions();
                                        app.image_rgb = Some(img.to_rgb8().into_raw());
                                        app.image_dims = (w, h);
                                        app.image_title = title;
                                        app.state.set_mode(AppMode::ImageViewer);
                                    }
                                    Err(e) => {
                                        app.state.push_notification(Notification::error(format!(
                                            "Cannot decode image: {e}"
                                        )));
                                    }
                                },
                                Err(e) => {
                                    app.state.push_notification(Notification::error(format!(
                                        "Cannot read image: {e}"
                                    )));
                                }
                            }
                        }
                        _ => {
                            app.open_file(path)?;
                        }
                    }
                }
            }
        }
        KeyCode::Delete => {
            if let Some(entry) = app.explorer.selected_entry().cloned() {
                let path = entry.path.to_string_lossy().to_string();
                app.confirm_dialog(ConfirmAction::DeleteFile(path));
            }
        }
        KeyCode::F(2) | KeyCode::Char('r') => {
            if let Some(entry) = app.explorer.selected_entry().cloned() {
                app.rename_src = Some(entry.path.clone());
                app.state.set_mode(AppMode::Dialog(DialogKind::Input {
                    prompt: "Renomear:".to_string(),
                    value: entry.name.clone(),
                }));
            }
        }
        KeyCode::Char('h') => {
            let _ = app.explorer.toggle_hidden();
        }
        KeyCode::Char('c') => app.clipboard_copy(),
        KeyCode::Char('x') => app.clipboard_cut(),
        KeyCode::Char('p') => app.paste_clipboard()?,
        KeyCode::Char('t') => app.toggle_side_terminal(),
        KeyCode::Char('g') if key.modifiers.is_empty() => {
            if let Some(entry) = app.explorer.selected_entry().cloned() {
                if entry.is_dir && entry.path.join(".git").exists() {
                    let path = entry.path.clone();
                    let mut gp = GitPlugin::new(path.clone());
                    gp.open_cwd_repo(&path);
                    app.git_plugin = Some(gp);
                    app.state.side_mode = SidePanelMode::Git;
                    app.state.side_pct_target = 40;
                    app.state.side_focused = false;
                }
            }
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_side_git();
        }
        KeyCode::Tab => {
            if app.state.side_mode != SidePanelMode::None && app.state.side_pct > 0 {
                app.state.side_focused = true;
            }
        }
        KeyCode::Char(':') => {
            app.state.set_mode(AppMode::Command(String::new()));
        }
        KeyCode::F(5) => {
            let _ = app.explorer.load_entries();
        }
        KeyCode::Esc => app.state.set_mode(AppMode::Menu),
        _ => {}
    }
    Ok(())
}

// ── Editor ────────────────────────────────────────────────────────────────────

fn handle_editor(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('s') => {
                if let Some(buf) = app.active_buffer_mut() {
                    buf.save_file()?;
                    app.state.push_notification(Notification::success("Saved"));
                }
                return Ok(());
            }
            KeyCode::Char('z') => {
                if let Some(b) = app.active_buffer_mut() {
                    b.undo();
                }
                return Ok(());
            }
            KeyCode::Char('y') => {
                if let Some(b) = app.active_buffer_mut() {
                    b.redo();
                }
                return Ok(());
            }
            // Toggle markdown view/edit (only for .md/.markdown buffers).
            // The two modes use different coordinate spaces — the view scrolls
            // by *rendered* lines, the editor by *source* lines — so map the
            // position proportionally to roughly stay where the user was.
            KeyCode::Char('e') => {
                if let Some(b) = app.active_buffer_mut() {
                    if b.is_markdown() {
                        let source_total = b.lines.len().max(1);
                        let rendered_total =
                            crate::ui::md_panel::parse_markdown(&b.lines.join("\n"))
                                .len()
                                .max(1);
                        if b.md_view {
                            // view → edit: rendered scroll offset → source line
                            let row =
                                (b.md_scroll * source_total / rendered_total).min(source_total - 1);
                            b.cursor = (row, 0);
                            b.md_view = false;
                        } else {
                            // edit → view: source cursor row → rendered offset
                            b.md_scroll = (b.cursor.0 * rendered_total / source_total)
                                .min(rendered_total - 1);
                            b.md_view = true;
                        }
                    }
                }
                return Ok(());
            }
            _ => {}
        }
    }

    // In markdown *view* mode the buffer is read-only: keys scroll the rendered
    // preview instead of moving the (hidden) edit cursor.
    let in_md_view = app
        .active_buffer_mut()
        .map(|b| b.is_markdown() && b.md_view)
        .unwrap_or(false);
    if in_md_view {
        match key.code {
            KeyCode::Up => {
                if let Some(b) = app.active_buffer_mut() {
                    b.md_scroll = b.md_scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(b) = app.active_buffer_mut() {
                    let total = crate::ui::md_panel::parse_markdown(&b.lines.join("\n")).len();
                    b.md_scroll = (b.md_scroll + 1).min(total.saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let Some(b) = app.active_buffer_mut() {
                    b.md_scroll = b.md_scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(b) = app.active_buffer_mut() {
                    let total = crate::ui::md_panel::parse_markdown(&b.lines.join("\n")).len();
                    b.md_scroll = (b.md_scroll + 10).min(total.saturating_sub(1));
                }
            }
            KeyCode::Home => {
                if let Some(b) = app.active_buffer_mut() {
                    b.md_scroll = 0;
                }
            }
            KeyCode::Esc => app.state.set_mode(AppMode::FileManager),
            _ => {}
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Up => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_down();
            }
        }
        KeyCode::Left => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_left();
            }
        }
        KeyCode::Right => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_right();
            }
        }
        KeyCode::Home => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_home();
            }
        }
        KeyCode::End => {
            if let Some(b) = app.active_buffer_mut() {
                b.move_end();
            }
        }
        KeyCode::PageUp => {
            if let Some(b) = app.active_buffer_mut() {
                b.page_up(20);
            }
        }
        KeyCode::PageDown => {
            if let Some(b) = app.active_buffer_mut() {
                b.page_down(20);
            }
        }
        KeyCode::Enter => {
            if let Some(b) = app.active_buffer_mut() {
                b.insert_newline();
            }
        }
        KeyCode::Backspace => {
            if let Some(b) = app.active_buffer_mut() {
                b.delete_char_before();
            }
        }
        KeyCode::Char(c) => {
            if let Some(b) = app.active_buffer_mut() {
                b.insert_char(c);
            }
        }
        KeyCode::Esc => app.state.set_mode(AppMode::FileManager),
        _ => {}
    }
    Ok(())
}

// ── Terminal ──────────────────────────────────────────────────────────────────

fn handle_terminal(app: &mut App, key: KeyEvent) -> Result<()> {
    handle_terminal_inner(app, key)
}

fn handle_terminal_inner(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter => app.terminal.execute(),
        KeyCode::Up => app.terminal.history_up(),
        KeyCode::Down => app.terminal.history_down(),
        KeyCode::Backspace => {
            app.terminal.input.pop();
        }
        KeyCode::Char(c) => app.terminal.input.push(c),
        KeyCode::Esc => {
            app.state.set_mode(AppMode::FileManager);
        }
        _ => {}
    }
    Ok(())
}

// ── Process Viewer ────────────────────────────────────────────────────────────

fn handle_process(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => app.process_viewer.move_up(),
        KeyCode::Down => app.process_viewer.move_down(),
        KeyCode::F(5) => app.process_viewer.refresh(),
        KeyCode::Char('d') => app.monitor_dashboard = !app.monitor_dashboard,
        KeyCode::Char('k') => {
            if let Some(p) = app.process_viewer.selected_process() {
                let pid = p.pid;
                app.confirm_dialog(ConfirmAction::KillProcess(pid));
            }
        }
        KeyCode::Char('1') => app.process_viewer.set_sort(SortBy::Cpu),
        KeyCode::Char('2') => app.process_viewer.set_sort(SortBy::Memory),
        KeyCode::Char('3') => app.process_viewer.set_sort(SortBy::Pid),
        KeyCode::Char('4') => app.process_viewer.set_sort(SortBy::Name),
        KeyCode::Char('/') => {
            app.process_viewer.filter.clear();
        }
        KeyCode::Backspace => app.process_viewer.filter_pop(),
        KeyCode::Char(c) if key.modifiers.is_empty() => app.process_viewer.filter_push(c),
        // Esc backs out one layer at a time: process list → dashboard → menu.
        KeyCode::Esc if !app.monitor_dashboard => app.monitor_dashboard = true,
        KeyCode::Esc => app.state.set_mode(AppMode::Menu),
        _ => {}
    }
    Ok(())
}

// ── Git ───────────────────────────────────────────────────────────────────────

fn handle_git(app: &mut App, key: KeyEvent) -> Result<()> {
    use crate::plugins::git::{GitCard, git_branch, git_branches, git_checkout};

    if app.state.git_pct_target == 0 {
        return Ok(());
    }

    let path = app.state.current_dir.clone();

    if let Some(gp) = &mut app.git_plugin {
        if gp.push_output.is_some() && !gp.push_running {
            gp.push_output = None;
            return Ok(());
        }
    }

    let dropdown_open = app
        .git_plugin
        .as_ref()
        .map_or(false, |g| g.branch_dropdown_open);
    if dropdown_open {
        if let Some(gp) = &mut app.git_plugin {
            let local_branches: Vec<String> = gp
                .branches
                .iter()
                .filter(|b| {
                    let t = b.trim();
                    !t.starts_with("remotes/") && !t.contains("->")
                })
                .map(|b| {
                    let t = b.trim();
                    if t.starts_with('*') {
                        t[1..].trim().to_string()
                    } else {
                        t.to_string()
                    }
                })
                .collect();
            let n = local_branches.len();
            match key.code {
                KeyCode::Up => {
                    if let Some(i) = gp.branch_dropdown_state.selected() {
                        if i > 0 {
                            gp.branch_dropdown_state.select(Some(i - 1));
                        }
                    }
                }
                KeyCode::Down => {
                    if let Some(i) = gp.branch_dropdown_state.selected() {
                        if i + 1 < n {
                            gp.branch_dropdown_state.select(Some(i + 1));
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(idx) = gp.branch_dropdown_state.selected() {
                        if let Some(name) = local_branches.get(idx).cloned() {
                            let _ = git_checkout(&path, &name);
                            gp.branch = git_branch(&path).unwrap_or(name);
                            gp.refresh_panel_data();
                        }
                    }
                    gp.branch_dropdown_open = false;
                }
                KeyCode::Esc => {
                    gp.branch_dropdown_open = false;
                }
                _ => {}
            }
        }
        return Ok(());
    }

    let committing = app
        .git_plugin
        .as_ref()
        .map_or(false, |g| g.commit_msg.is_some());
    if committing {
        match key.code {
            KeyCode::Enter => {
                if let Some(gp) = &mut app.git_plugin {
                    if let Some(msg) = gp.commit_msg.take() {
                        if !msg.is_empty() {
                            let result = crate::plugins::git::git_commit(&path, &msg);
                            let notif = match result {
                                Ok(out) => Notification::success(out),
                                Err(e) => Notification::error(format!("{e}")),
                            };
                            app.state.push_notification(notif);
                            if let Some(gp) = &mut app.git_plugin {
                                gp.refresh_panel_data();
                            }
                        }
                    }
                }
            }
            KeyCode::Esc => {
                if let Some(gp) = &mut app.git_plugin {
                    gp.commit_msg = None;
                }
            }
            KeyCode::Char(c) => {
                if let Some(gp) = &mut app.git_plugin {
                    if let Some(ref mut msg) = gp.commit_msg {
                        msg.push(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(gp) = &mut app.git_plugin {
                    if let Some(ref mut msg) = gp.commit_msg {
                        msg.pop();
                    }
                }
            }
            _ => {}
        }
        return Ok(());
    }

    if let Some(gp) = &mut app.git_plugin {
        match key.code {
            KeyCode::Tab => {
                gp.active_card = gp.active_card.next();
            }
            KeyCode::BackTab => {
                gp.active_card = gp.active_card.prev();
            }
            KeyCode::Up => match &gp.active_card {
                GitCard::Log => {
                    if gp.log_scroll > 0 {
                        gp.log_scroll -= 1;
                    }
                }
                GitCard::Status => {
                    if let Some(i) = gp.list_state.selected() {
                        if i > 0 {
                            gp.list_state.select(Some(i - 1));
                        }
                    }
                }
                GitCard::Shortcuts => {
                    if gp.shortcuts_scroll > 0 {
                        gp.shortcuts_scroll -= 1;
                    }
                }
                GitCard::BranchGraph => {
                    if gp.branch_graph_scroll > 0 {
                        gp.branch_graph_scroll -= 1;
                    }
                }
                _ => {}
            },
            KeyCode::Down => match &gp.active_card {
                GitCard::Log => {
                    let max = gp.log_lines.len().saturating_sub(1);
                    if gp.log_scroll < max {
                        gp.log_scroll += 1;
                    }
                }
                GitCard::Status => {
                    if let Some(i) = gp.list_state.selected() {
                        if i + 1 < gp.all_entries.len() {
                            gp.list_state.select(Some(i + 1));
                        }
                    }
                }
                GitCard::Shortcuts => {
                    gp.shortcuts_scroll = (gp.shortcuts_scroll + 1).min(6);
                }
                GitCard::BranchGraph => {
                    gp.branch_graph_scroll += 1;
                }
                _ => {}
            },
            KeyCode::Enter => {
                if gp.active_card == GitCard::BranchHeader {
                    if gp.branches.is_empty() {
                        gp.branches = git_branches(&path).unwrap_or_default();
                    }
                    let local_branches: Vec<String> = gp
                        .branches
                        .iter()
                        .filter(|b| {
                            let t = b.trim();
                            !t.starts_with("remotes/") && !t.contains("->")
                        })
                        .map(|b| {
                            let t = b.trim();
                            if t.starts_with('*') {
                                t[1..].trim().to_string()
                            } else {
                                t.to_string()
                            }
                        })
                        .collect();
                    let cur = gp.branch.clone();
                    let idx = local_branches.iter().position(|b| b == &cur).unwrap_or(0);
                    gp.branch_dropdown_state = ListState::default();
                    if !local_branches.is_empty() {
                        gp.branch_dropdown_state.select(Some(idx));
                    }
                    gp.branch_dropdown_open = true;
                }
            }
            KeyCode::Char('r') => {
                gp.refresh_panel_data();
            }
            KeyCode::Char('a') => {
                let _ = crate::plugins::git::git_add_all(&path);
                gp.entries = crate::plugins::git::git_status(&path).unwrap_or_default();
                gp.all_entries = crate::plugins::git::git_all_files(&path).unwrap_or_default();
                app.state
                    .push_notification(Notification::success("git add . — tudo em stage"));
            }
            KeyCode::Char('c') => {
                gp.commit_msg = Some(String::new());
                gp.active_card = GitCard::Status;
            }
            KeyCode::Char('p') => {
                let branch = gp.branch.clone();
                gp.start_push(&path, &branch);
            }
            KeyCode::Char('P') => {
                let result = crate::plugins::git::git_pull(&path);
                let notif = match result {
                    Ok(out) => Notification::info(out),
                    Err(e) => Notification::error(format!("{e}")),
                };
                app.state.push_notification(notif);
                if let Some(gp) = &mut app.git_plugin {
                    gp.refresh_panel_data();
                }
            }
            KeyCode::Char('i') => {
                let result = crate::plugins::git::create_gitignore(&path);
                let notif = match result {
                    Ok(out) => Notification::success(out),
                    Err(e) => Notification::error(format!("{e}")),
                };
                app.state.push_notification(notif);
            }
            KeyCode::Esc => {
                app.state.git_pct_target = 0;
            }
            _ => {}
        }
    } else {
        if key.code == KeyCode::Esc {
            app.state.git_pct_target = 0;
        }
    }
    Ok(())
}

// ── Config ────────────────────────────────────────────────────────────────────

fn handle_config(app: &mut App, key: KeyEvent) {
    const MAX: usize = crate::ui::config_panel::CONFIG_FIELDS;
    match key.code {
        KeyCode::Up => {
            if app.config_selected > 0 {
                app.config_selected -= 1;
            }
        }
        KeyCode::Down => {
            if app.config_selected + 1 < MAX {
                app.config_selected += 1;
            }
        }
        KeyCode::Left => {
            let sel = app.config_selected;
            app.config_cycle(sel, false);
        }
        KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
            let sel = app.config_selected;
            app.config_cycle(sel, true);
        }
        KeyCode::Esc => {
            let _ = app
                .settings
                .save(std::path::Path::new("config/config.toml"));
            app.state
                .push_notification(crate::core::state::Notification::success("Settings saved"));
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── Audio ─────────────────────────────────────────────────────────────────────

fn expand_home(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(format!("{}/{}", home, &path[2..]))
    } else {
        std::path::PathBuf::from(path)
    }
}

fn handle_audio(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Global playback controls (always active)
        KeyCode::Char(' ') => {
            if let Some(ap) = &mut app.audio_player {
                if ap.is_playing {
                    ap.pause()
                } else {
                    ap.resume();
                }
            }
        }
        KeyCode::Char('n') => {
            if let Some(ap) = &mut app.audio_player {
                let _ = ap.next_track();
            }
        }
        KeyCode::Char('p') => {
            if let Some(ap) = &mut app.audio_player {
                let _ = ap.prev_track();
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            if let Some(ap) = &mut app.audio_player {
                ap.volume_up();
            }
        }
        KeyCode::Char('-') => {
            if let Some(ap) = &mut app.audio_player {
                ap.volume_down();
            }
        }
        KeyCode::Left => {
            if let Some(ap) = &mut app.audio_player {
                ap.seek_backward(10);
            }
        }
        KeyCode::Right => {
            if let Some(ap) = &mut app.audio_player {
                ap.seek_forward(10);
            }
        }

        // Focus toggle
        KeyCode::Tab => {
            use crate::modules::music_library::LibraryFocus;
            app.music_library.focus = match app.music_library.focus {
                LibraryFocus::Library => LibraryFocus::Queue,
                LibraryFocus::Queue => LibraryFocus::Library,
            };
        }

        // Navigation
        KeyCode::Up => {
            use crate::modules::music_library::LibraryFocus;
            let queue_len = app
                .audio_player
                .as_ref()
                .map(|ap| ap.playlist.len())
                .unwrap_or(0);
            match app.music_library.focus {
                LibraryFocus::Library => app.music_library.nav_up(),
                LibraryFocus::Queue => app.music_library.queue_up(queue_len),
            }
        }
        KeyCode::Down => {
            use crate::modules::music_library::LibraryFocus;
            let queue_len = app
                .audio_player
                .as_ref()
                .map(|ap| ap.playlist.len())
                .unwrap_or(0);
            match app.music_library.focus {
                LibraryFocus::Library => app.music_library.nav_down(),
                LibraryFocus::Queue => app.music_library.queue_down(queue_len),
            }
        }

        // Enter / drill-down / play
        KeyCode::Enter => {
            use crate::modules::music_library::{LibraryFocus, LibraryView};
            match app.music_library.focus {
                LibraryFocus::Library => {
                    if app.music_library.view == LibraryView::Tracks {
                        // Play selected track: set queue to all tracks in album
                        if let Some(track) = app.music_library.selected_track().cloned() {
                            let idx = app.music_library.track_state.selected().unwrap_or(0);
                            let all_paths: Vec<_> = app
                                .music_library
                                .tracks
                                .iter()
                                .map(|t| t.path.clone())
                                .collect();
                            if let Ok(mut player) = AudioPlayer::from_playlist(all_paths) {
                                player.playlist_idx = idx;
                                player.load_current_track_meta_pub();
                                let _ = player.play();
                                app.state.push_notification(
                                    crate::core::state::Notification::info(format!(
                                        "Playing: {}",
                                        track.title
                                    )),
                                );
                                app.audio_player = Some(player);
                            }
                        }
                    } else {
                        app.music_library.enter();
                    }
                }
                LibraryFocus::Queue => {
                    // Jump to selected track in queue
                    if let Some(idx) = app.music_library.queue_state.selected() {
                        if let Some(ap) = &mut app.audio_player {
                            ap.stop();
                            ap.playlist_idx = idx;
                            ap.load_current_track_meta_pub();
                            let _ = ap.play();
                        }
                    }
                }
            }
        }

        // Back / drill-up
        KeyCode::Backspace => {
            use crate::modules::music_library::LibraryFocus;
            if app.music_library.focus == LibraryFocus::Library {
                app.music_library.go_back();
            }
        }

        // Add selected track to queue
        KeyCode::Char('a') => {
            use crate::modules::music_library::{LibraryFocus, LibraryView};
            if app.music_library.focus == LibraryFocus::Library
                && app.music_library.view == LibraryView::Tracks
            {
                if let Some(track) = app.music_library.selected_track().cloned() {
                    let path = track.path.clone();
                    let title = track.title.clone();
                    if let Some(ap) = &mut app.audio_player {
                        ap.add_to_queue(path);
                        app.state
                            .push_notification(crate::core::state::Notification::info(format!(
                                "Added: {}",
                                title
                            )));
                    } else {
                        // No player yet — create one
                        if let Ok(mut player) = AudioPlayer::new(path) {
                            player.load_current_track_meta_pub();
                            let _ = player.play();
                            app.audio_player = Some(player);
                        }
                    }
                }
            }
        }

        // Remove from queue
        KeyCode::Char('d') => {
            use crate::modules::music_library::LibraryFocus;
            if app.music_library.focus == LibraryFocus::Queue {
                if let Some(idx) = app.music_library.queue_state.selected() {
                    if let Some(ap) = &mut app.audio_player {
                        ap.remove_from_queue(idx);
                        let new_len = ap.playlist.len();
                        if new_len == 0 {
                            app.music_library.queue_state.select(None);
                        } else {
                            app.music_library
                                .queue_state
                                .select(Some(idx.min(new_len - 1)));
                        }
                    }
                }
            }
        }

        // Rescan library
        KeyCode::F(5) => {
            let music_dir = expand_home(&app.settings.music_dir.clone());
            app.music_library.scan_directory(&music_dir);
            app.state
                .push_notification(crate::core::state::Notification::info(format!(
                    "Library: {} tracks",
                    app.music_library.scan_count
                )));
        }

        // Back to FileManager (music keeps playing)
        KeyCode::Esc => {
            app.state.set_mode(AppMode::FileManager);
            if app
                .audio_player
                .as_ref()
                .map(|ap| ap.is_playing)
                .unwrap_or(false)
            {
                app.state
                    .push_notification(crate::core::state::Notification::info(
                        "♪ A tocar em fundo  [m]",
                    ));
            }
        }

        _ => {}
    }
    Ok(())
}

// ── Video ─────────────────────────────────────────────────────────────────────

fn handle_video(app: &mut App, key: KeyEvent) -> Result<()> {
    if let Some(vp) = &mut app.video_player {
        match key.code {
            KeyCode::Char(' ') => {
                if vp.is_playing {
                    vp.pause()
                } else {
                    vp.play();
                }
            }
            KeyCode::Left => vp.seek_backward(5.0),
            KeyCode::Right => vp.seek_forward(5.0),
            KeyCode::Esc | KeyCode::Char('q') => {
                vp.stop();
                let _ = crate::kitty::clear_image(2);
                app.video_player = None;
                app.pending_kitty = None;
                app.kitty_last_area = None;
                app.state.restore_mode();
            }
            _ => {}
        }
    } else {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            let _ = crate::kitty::clear_image(2);
            app.state.restore_mode();
        }
    }
    Ok(())
}

// ── Image Viewer ──────────────────────────────────────────────────────────────

fn handle_image(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            let _ = crate::kitty::clear_image(1);
            app.image_rgb = None;
            app.image_dims = (0, 0);
            app.image_title = String::new();
            app.pending_kitty = None;
            app.kitty_last_area = None;
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── PDF Viewer ────────────────────────────────────────────────────────────────

fn handle_pdf(app: &mut App, key: KeyEvent) {
    let total = app.pdf_pages.len();
    match key.code {
        KeyCode::Up => {
            app.pdf_scroll = app.pdf_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.pdf_scroll + 1 < total {
                app.pdf_scroll += 1;
            }
        }
        KeyCode::PageUp => {
            app.pdf_scroll = app.pdf_scroll.saturating_sub(20);
        }
        KeyCode::PageDown => {
            app.pdf_scroll = (app.pdf_scroll + 20).min(total.saturating_sub(1));
        }
        KeyCode::Home => {
            app.pdf_scroll = 0;
        }
        KeyCode::End => {
            app.pdf_scroll = total.saturating_sub(1);
        }
        KeyCode::Esc => {
            app.pdf_pages.clear();
            app.pdf_title.clear();
            app.pdf_scroll = 0;
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── Help ──────────────────────────────────────────────────────────────────────

fn handle_help(app: &mut App, key: KeyEvent) {
    use crate::ui::help_panel::{section_count, tab_count, topic_count};
    match key.code {
        KeyCode::Esc => app.state.restore_mode(),
        // ↑/↓ move through the grouped topic list on the left
        KeyCode::Up => {
            if app.help_topic > 0 {
                app.help_topic -= 1;
                app.help_tab = 0;
                app.help_scroll = 0;
            }
        }
        KeyCode::Down => {
            if app.help_topic + 1 < topic_count() {
                app.help_topic += 1;
                app.help_tab = 0;
                app.help_scroll = 0;
            }
        }
        // ←/→/Tab switch the tabs of the selected topic
        KeyCode::Left | KeyCode::BackTab => {
            if app.help_tab > 0 {
                app.help_tab -= 1;
                app.help_scroll = 0;
            }
        }
        KeyCode::Right | KeyCode::Tab => {
            if app.help_tab + 1 < tab_count(app.help_topic) {
                app.help_tab += 1;
                app.help_scroll = 0;
            }
        }
        // PgUp/PgDn scroll the sub-panels of the active tab
        KeyCode::PageUp => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        KeyCode::PageDown => {
            if app.help_scroll + 1 < section_count(app.help_topic, app.help_tab) {
                app.help_scroll += 1;
            }
        }
        KeyCode::Home => {
            app.help_scroll = 0;
        }
        _ => {}
    }
}

// ── Mouse ─────────────────────────────────────────────────────────────────────

fn handle_mouse(app: &mut App, event: MouseEvent) {
    if !app.settings.mouse_enabled {
        return;
    }

    let col = event.column;
    let row = event.row;

    match event.kind {
        // ── Scroll wheel: 3 steps per notch ──────────────────────────────────
        MouseEventKind::ScrollDown => match app.state.mode {
            AppMode::FileManager => {
                for _ in 0..3 {
                    app.explorer.move_down();
                }
            }
            AppMode::ProcessViewer => {
                for _ in 0..3 {
                    app.process_viewer.move_down();
                }
            }
            _ => {}
        },
        MouseEventKind::ScrollUp => match app.state.mode {
            AppMode::FileManager => {
                for _ in 0..3 {
                    app.explorer.move_up();
                }
            }
            AppMode::ProcessViewer => {
                for _ in 0..3 {
                    app.process_viewer.move_up();
                }
            }
            _ => {}
        },

        // ── Left click: select item; double-click = Enter ─────────────────────
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            // Detect double-click (threshold: 400 ms)
            let is_double = app
                .last_click
                .map(|t| t.elapsed().as_millis() < 400)
                .unwrap_or(false);
            app.last_click = Some(std::time::Instant::now());

            match app.state.mode {
                AppMode::FileManager => {
                    let area = app.last_areas.file_list;
                    if col >= area.x
                        && col < area.x + area.width
                        && row >= area.y
                        && row < area.y + area.height
                    {
                        // The List widget draws items starting at y+1 (inside border).
                        let inner_y = area.y + 1;
                        if row >= inner_y {
                            let click_row = (row - inner_y) as usize;
                            let offset = app.explorer.list_state.offset();
                            let idx = offset + click_row;
                            if idx < app.explorer.entries.len() {
                                app.explorer.list_state.select(Some(idx));
                                if is_double {
                                    // Simulate Enter key press on the selected item
                                    let fake_enter =
                                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                                    let _ = handle_file_manager(app, fake_enter);
                                }
                            }
                        }
                    }
                }
                AppMode::ProcessViewer => {
                    let area = app.last_areas.process_list;
                    if col >= area.x
                        && col < area.x + area.width
                        && row >= area.y
                        && row < area.y + area.height
                    {
                        // render_list draws a 1-row header then the list inside a border
                        let inner_y = area.y + 2; // border(1) + header(1)
                        if row >= inner_y {
                            let click_row = (row - inner_y) as usize;
                            let offset = app.process_viewer.list_state.offset();
                            let idx = offset + click_row;
                            if idx < app.process_viewer.filtered.len() {
                                app.process_viewer.list_state.select(Some(idx));
                            }
                        }
                    }
                }
                AppMode::Menu => {
                    let area = app.last_areas.menu_list;
                    if col >= area.x
                        && col < area.x + area.width
                        && row >= area.y
                        && row < area.y + area.height
                    {
                        let inner_y = area.y + 1;
                        if row >= inner_y {
                            let click_row = (row - inner_y) as usize;
                            let offset = app.menu_state.offset();
                            let idx = offset + click_row;
                            if idx < crate::ui::menu::MENU_LEN {
                                app.menu_state.select(Some(idx));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

// ── Phase 2: Log Viewer ────────────────────────────────────────────────────────

fn handle_logviewer(app: &mut App, key: KeyEvent) {
    use crate::modules::logview::LogLevel;
    match key.code {
        KeyCode::Up => {
            if app.log_viewer.scroll > 0 {
                app.log_viewer.scroll -= 1;
                app.log_viewer.follow = false;
            }
        }
        KeyCode::Down => {
            app.log_viewer.scroll =
                (app.log_viewer.scroll + 1).min(app.log_viewer.lines.len().saturating_sub(1));
            app.log_viewer.follow = false;
        }
        KeyCode::PageUp => {
            app.log_viewer.scroll = app.log_viewer.scroll.saturating_sub(20);
            app.log_viewer.follow = false;
        }
        KeyCode::PageDown => {
            app.log_viewer.scroll =
                (app.log_viewer.scroll + 20).min(app.log_viewer.lines.len().saturating_sub(1));
            app.log_viewer.follow = false;
        }
        KeyCode::End => {
            app.log_viewer.scroll = app.log_viewer.lines.len().saturating_sub(1);
            app.log_viewer.follow = true;
        }
        KeyCode::Char('f') => {
            app.log_viewer.follow = !app.log_viewer.follow;
        }
        KeyCode::Char('e') => {
            app.log_viewer.level_filter = Some(LogLevel::Error);
        }
        KeyCode::Char('w') => {
            app.log_viewer.level_filter = Some(LogLevel::Warn);
        }
        KeyCode::Char('i') => {
            app.log_viewer.level_filter = Some(LogLevel::Info);
        }
        KeyCode::Char('0') => {
            app.log_viewer.level_filter = None;
        }
        KeyCode::Esc => {
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── Phase 2: Service Manager ──────────────────────────────────────────────────

fn handle_services(app: &mut App, key: KeyEvent) {
    use crate::modules::services::ServiceManager;
    match key.code {
        KeyCode::Up => app.service_manager.move_up(),
        KeyCode::Down => app.service_manager.move_down(),
        KeyCode::F(5) => app.service_manager.refresh(),
        KeyCode::Char('s') => {
            if let Some(u) = app.service_manager.selected_unit() {
                let name = u.name.clone();
                let result = ServiceManager::run_action("start", &name);
                app.state.push_notification(match result {
                    Ok(()) => Notification::success(format!("Started: {name}")),
                    Err(e) => Notification::error(e),
                });
            }
        }
        KeyCode::Char('e') => {
            if let Some(u) = app.service_manager.selected_unit() {
                let name = u.name.clone();
                let result = ServiceManager::run_action("enable", &name);
                app.state.push_notification(match result {
                    Ok(()) => Notification::success(format!("Enabled: {name}")),
                    Err(e) => Notification::error(e),
                });
            }
        }
        KeyCode::Char('t') => {
            if let Some(u) = app.service_manager.selected_unit() {
                let name = u.name.clone();
                let result = ServiceManager::run_action("stop", &name);
                app.state.push_notification(match result {
                    Ok(()) => Notification::success(format!("Stopped: {name}")),
                    Err(e) => Notification::error(e),
                });
            }
        }
        KeyCode::Char('r') => {
            if let Some(u) = app.service_manager.selected_unit() {
                let name = u.name.clone();
                let result = ServiceManager::run_action("restart", &name);
                app.state.push_notification(match result {
                    Ok(()) => Notification::success(format!("Restarted: {name}")),
                    Err(e) => Notification::error(e),
                });
            }
        }
        KeyCode::Char(c) if key.modifiers.is_empty() => app.service_manager.filter_push(c),
        KeyCode::Backspace => {
            app.service_manager.filter_pop();
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 2: Network Panel ────────────────────────────────────────────────────

fn handle_network(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.network_panel.select_up(),
        KeyCode::Down => app.network_panel.select_down(),
        KeyCode::F(5) => app.network_panel.tick(),
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 2: Disk Manager ─────────────────────────────────────────────────────

fn handle_disk(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.disk_manager.move_up(),
        KeyCode::Down => app.disk_manager.move_down(),
        KeyCode::Enter => {
            if let Some(idx) = app.disk_manager.list_state.selected() {
                app.disk_manager.enter(idx);
            }
        }
        KeyCode::Backspace | KeyCode::Left => app.disk_manager.up(),
        KeyCode::F(5) => {
            let root = app.disk_manager.current.clone();
            app.disk_manager.start_scan(root);
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = app.disk_manager.selected_entry().cloned() {
                let path = entry.path.to_string_lossy().to_string();
                app.confirm_dialog(ConfirmAction::DeleteFile(path));
            }
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 2: Calculator ───────────────────────────────────────────────────────

fn handle_calculator(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            // Trigger close animation, then restore mode immediately.
            // The panel stays visible until anim_pct reaches 0 via tick_anim().
            app.calculator.anim_target = 0;
            app.state.restore_mode();
        }
        KeyCode::Enter => {
            app.calculator.commit();
        }
        KeyCode::Backspace => {
            app.calculator.backspace();
        }
        KeyCode::Char(c) => {
            app.calculator.push_char(c);
        }
        _ => {}
    }
}

// ── Phase 3: Favorites popup ──────────────────────────────────────────────────

fn handle_favorites(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            if let Some(i) = app.favorites_state.selected() {
                if i > 0 {
                    app.favorites_state.select(Some(i - 1));
                }
            }
        }
        KeyCode::Down => {
            let max = app.favorites_list.len().saturating_sub(1);
            if let Some(i) = app.favorites_state.selected() {
                if i < max {
                    app.favorites_state.select(Some(i + 1));
                }
            } else if !app.favorites_list.is_empty() {
                app.favorites_state.select(Some(0));
            }
        }
        KeyCode::Enter => {
            if let Some(idx) = app.favorites_state.selected() {
                if let Some(path) = app.favorites_list.get(idx).cloned() {
                    app.state.restore_mode();
                    if path.is_dir() {
                        let _ = app.explorer.enter_dir(path.clone());
                        app.state.current_dir = path;
                        app.state.set_mode(AppMode::FileManager);
                    } else if path.exists() {
                        let _ = app.open_file(path);
                    } else {
                        let path_str = path.to_string_lossy();
                        let _ = app.db.remove_favorite(&path_str);
                        app.favorites_list.retain(|p| p != &path);
                        app.state.push_notification(Notification::warning(format!(
                            "Path no longer exists, removed: {}",
                            path.display()
                        )));
                    }
                }
            }
        }
        KeyCode::Esc => {
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── Phase 3: Theme Switcher popup ─────────────────────────────────────────────

fn handle_theme_switcher(app: &mut App, key: KeyEvent) {
    let themes = crate::ui::theme::Theme::all();
    match key.code {
        KeyCode::Up => {
            if app.theme_switcher_idx > 0 {
                app.theme_switcher_idx -= 1;
                app.settings.theme = themes[app.theme_switcher_idx].to_string();
            }
        }
        KeyCode::Down => {
            if app.theme_switcher_idx + 1 < themes.len() {
                app.theme_switcher_idx += 1;
                app.settings.theme = themes[app.theme_switcher_idx].to_string();
            }
        }
        KeyCode::Enter => {
            let _ = app
                .settings
                .save(std::path::Path::new("config/config.toml"));
            app.state.push_notification(Notification::success(format!(
                "Tema aplicado: {}",
                app.settings.theme
            )));
            app.state.restore_mode();
        }
        KeyCode::Esc => {
            app.settings.theme = app.theme_switcher_original.clone();
            app.state.restore_mode();
        }
        _ => {}
    }
}

// ── Phase 4: Package Manager ──────────────────────────────────────────────────

fn handle_packages(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.package_manager.move_up(),
        KeyCode::Down => app.package_manager.move_down(),
        KeyCode::Tab => app.package_manager.next_tab(),
        KeyCode::F(5) => app.package_manager.load_installed(),
        KeyCode::Char(c) if key.modifiers.is_empty() && !c.is_control() => {
            app.package_manager.search_query.push(c);
            app.package_manager.do_search();
        }
        KeyCode::Backspace => {
            app.package_manager.search_query.pop();
            if app.package_manager.search_query.is_empty() {
                app.package_manager.search_results.clear();
            } else {
                app.package_manager.do_search();
            }
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 4: SSH Manager ──────────────────────────────────────────────────────

fn handle_ssh(app: &mut App, key: KeyEvent) {
    // The history and tunnels overlays capture input while open — both
    // render as popups on top of `render_ssh_panel` (same pattern as the
    // Weather city-search overlay), so they get first refusal here.
    if app.ssh_manager.history_open {
        handle_ssh_history(app, key);
        return;
    }
    if app.ssh_manager.tunnels_open {
        handle_ssh_tunnels(app, key);
        return;
    }

    match key.code {
        KeyCode::Up => app.ssh_manager.move_up(),
        KeyCode::Down => app.ssh_manager.move_down(),
        KeyCode::Char('t') => {
            let idx = app.ssh_manager.selected;
            app.ssh_manager.test_connectivity(idx);
        }
        KeyCode::F(5) => app.enter_ssh(),
        KeyCode::Enter => {
            // Defer the actual (blocking) connect to right after the next
            // draw — see `App::ssh_connect_now` and the hook in `main.rs` —
            // so the "Connecting…" popup gets a chance to paint first.
            if !app.ssh_manager.hosts.is_empty() {
                app.ssh_pending_connect = Some(app.ssh_manager.selected);
            }
        }
        // History popup.
        KeyCode::Char('h') => {
            app.ssh_refresh_history();
            app.ssh_manager.history_open = true;
        }
        // Groups: collapse/expand, create, assign selected host.
        KeyCode::Char('g') => {
            if let Some(group) = app
                .ssh_manager
                .hosts
                .get(app.ssh_manager.selected)
                .and_then(|h| h.group.clone())
            {
                app.ssh_manager.toggle_group_collapse(&group);
            }
        }
        KeyCode::Char('n') => {
            app.ssh_new_group = true;
            app.state.set_mode(AppMode::Dialog(DialogKind::Input {
                prompt: "New group name:".to_string(),
                value: String::new(),
            }));
        }
        KeyCode::Char('a') => {
            if !app.ssh_manager.hosts.is_empty() {
                app.ssh_assign_group_host = Some(app.ssh_manager.selected);
                app.state.set_mode(AppMode::Dialog(DialogKind::Input {
                    prompt: "Assign host to group:".to_string(),
                    value: String::new(),
                }));
            }
        }
        // SFTP for the selected host.
        KeyCode::Char('s') => {
            if !app.ssh_manager.hosts.is_empty() {
                let idx = app.ssh_manager.selected;
                app.open_sftp(idx);
            }
        }
        // Add a new ad-hoc connection (Host/Username/Password/Port form).
        KeyCode::Tab => app.ssh_open_add_form(),
        // Edit the selected host's entry in the same form.
        KeyCode::Char('e') => {
            if !app.ssh_manager.hosts.is_empty() {
                let idx = app.ssh_manager.selected;
                app.ssh_open_edit_form(idx);
            }
        }
        // Remove the selected host from ~/.ssh/config (asks to confirm).
        KeyCode::Char('d') => {
            if let Some(host) = app.ssh_manager.hosts.get(app.ssh_manager.selected) {
                let alias = host.alias.clone();
                app.confirm_dialog(ConfirmAction::DeleteSshHost(alias));
            }
        }
        // Tunnels sub-panel (Shift+t — plain 't' is connectivity test).
        KeyCode::Char('T') => {
            app.ssh_manager.tunnels_open = true;
        }
        // Session-tab strip (this run's history).
        KeyCode::Char('[') => {
            if app.ssh_manager.tabs_selected > 0 {
                app.ssh_manager.tabs_selected -= 1;
            }
        }
        KeyCode::Char(']') => {
            if app.ssh_manager.tabs_selected + 1 < app.ssh_manager.tabs.len() {
                app.ssh_manager.tabs_selected += 1;
            }
        }
        KeyCode::Char('x') => {
            if !app.ssh_manager.tabs.is_empty() {
                let i = app.ssh_manager.tabs_selected;
                app.ssh_manager.tabs.remove(i);
                app.ssh_manager.tabs_selected = app
                    .ssh_manager
                    .tabs_selected
                    .min(app.ssh_manager.tabs.len().saturating_sub(1));
            }
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Add/Edit Connection form ──────────────────────────────────────────────────

fn handle_ssh_connect_form(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Tab => {
            if let Some(form) = &mut app.ssh_manager.conn_form {
                form.focus = form.focus.next();
            }
        }
        KeyCode::BackTab => {
            if let Some(form) = &mut app.ssh_manager.conn_form {
                form.focus = form.focus.prev();
            }
        }
        KeyCode::Char(c) => {
            if let Some(form) = &mut app.ssh_manager.conn_form {
                form.focused_field_mut().push(c);
            }
        }
        KeyCode::Backspace => {
            if let Some(form) = &mut app.ssh_manager.conn_form {
                form.focused_field_mut().pop();
            }
        }
        KeyCode::Enter => {
            let Some(form) = &app.ssh_manager.conn_form else {
                return;
            };
            if form.editing_alias.is_some() {
                // Editing: no connection attempt, straight to the save
                // confirm dialog — instant, safe to call directly.
                app.ssh_submit_form();
            } else if !form.host.trim().is_empty() {
                // Adding: defer the (blocking) connect to right after the
                // next draw — see `app.ssh_pending_adhoc_connect` and the
                // hook in `main.rs` — so the "Connecting…" popup paints
                // first, same pattern as the host-list Enter.
                app.ssh_pending_adhoc_connect = true;
            }
        }
        KeyCode::Esc => app.ssh_close_form(),
        _ => {}
    }
}

/// Input routing while the SSH connection-history popup is open.
fn handle_ssh_history(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            if app.ssh_manager.history_selected > 0 {
                app.ssh_manager.history_selected -= 1;
            }
        }
        KeyCode::Down => {
            if app.ssh_manager.history_selected + 1 < app.ssh_manager.history.len() {
                app.ssh_manager.history_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(entry) = app
                .ssh_manager
                .history
                .get(app.ssh_manager.history_selected)
                .cloned()
            {
                app.ssh_manager.history_open = false;
                match app
                    .ssh_manager
                    .hosts
                    .iter()
                    .position(|h| h.alias == entry.alias)
                {
                    Some(idx) => app.ssh_pending_connect = Some(idx),
                    None => app.state.push_notification(Notification::warning(format!(
                        "{}: no longer in ~/.ssh/config",
                        entry.alias
                    ))),
                }
            }
        }
        KeyCode::Esc => app.ssh_manager.history_open = false,
        _ => {}
    }
}

/// Input routing while the tunnels popup is open.
fn handle_ssh_tunnels(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            if app.ssh_manager.tunnels_selected > 0 {
                app.ssh_manager.tunnels_selected -= 1;
            }
        }
        KeyCode::Down => {
            if app.ssh_manager.tunnels_selected + 1 < app.ssh_manager.tunnels.len() {
                app.ssh_manager.tunnels_selected += 1;
            }
        }
        KeyCode::Char('c') => {
            if !app.ssh_manager.hosts.is_empty() {
                app.ssh_tunnel_form = true;
                app.state.set_mode(AppMode::Dialog(DialogKind::Input {
                    prompt: "local_port:remote_host:remote_port".to_string(),
                    value: String::new(),
                }));
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            let idx = app.ssh_manager.tunnels_selected;
            app.ssh_manager.kill_tunnel(idx);
        }
        KeyCode::Char('T') | KeyCode::Esc => app.ssh_manager.tunnels_open = false,
        _ => {}
    }
}

// ── Phase 6.8: SFTP Panel ─────────────────────────────────────────────────────

fn handle_sftp(app: &mut App, key: KeyEvent) {
    let Some(panel) = app.sftp_panel.as_mut() else {
        app.state.restore_mode();
        return;
    };
    match key.code {
        KeyCode::Tab => panel.toggle_focus(),
        KeyCode::Up => panel.move_up(),
        KeyCode::Down => panel.move_down(),
        KeyCode::Enter => {
            let _ = panel.enter_selected();
        }
        KeyCode::Backspace => panel.go_parent(),
        KeyCode::Char('g') => panel.download_selected(),
        KeyCode::Char('p') => panel.upload_selected(),
        KeyCode::F(5) => panel.refresh_remote(),
        KeyCode::Esc => app.close_sftp(),
        _ => {}
    }
}

// ── Phase 4: Docker Panel ─────────────────────────────────────────────────────

fn handle_docker(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.docker_panel.move_up(),
        KeyCode::Down => app.docker_panel.move_down(),
        KeyCode::Tab => app.docker_panel.next_tab(),
        KeyCode::F(5) => {
            let _ = app.docker_panel.refresh();
        }
        KeyCode::Char('s') => {
            if let Some(c) = app
                .docker_panel
                .containers
                .get(app.docker_panel.selected)
                .cloned()
            {
                let r = app.docker_panel.run_action("start", &c.id);
                app.state.push_notification(match r {
                    Ok(m) => Notification::success(m),
                    Err(e) => Notification::error(format!("{e}")),
                });
            }
        }
        KeyCode::Char('S') => {
            if let Some(c) = app
                .docker_panel
                .containers
                .get(app.docker_panel.selected)
                .cloned()
            {
                let r = app.docker_panel.run_action("stop", &c.id);
                app.state.push_notification(match r {
                    Ok(m) => Notification::success(m),
                    Err(e) => Notification::error(format!("{e}")),
                });
            }
        }
        KeyCode::Char('r') => {
            if let Some(c) = app
                .docker_panel
                .containers
                .get(app.docker_panel.selected)
                .cloned()
            {
                let r = app.docker_panel.run_action("restart", &c.id);
                app.state.push_notification(match r {
                    Ok(m) => Notification::success(m),
                    Err(e) => Notification::error(format!("{e}")),
                });
            }
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 4: Cron Editor ──────────────────────────────────────────────────────

fn handle_cron(app: &mut App, key: KeyEvent) {
    if app.cron_editor.editing.is_some() {
        match key.code {
            KeyCode::Tab => {
                if app.cron_editor.edit_cursor + 1 < 6 {
                    app.cron_editor.edit_cursor += 1;
                }
            }
            KeyCode::BackTab => {
                if app.cron_editor.edit_cursor > 0 {
                    app.cron_editor.edit_cursor -= 1;
                }
            }
            KeyCode::Enter => {
                app.cron_editor.save_edit();
            }
            KeyCode::Esc => {
                app.cron_editor.cancel_edit();
            }
            KeyCode::Char(c) => {
                let cursor = app.cron_editor.edit_cursor;
                app.cron_editor.edit_fields[cursor].push(c);
            }
            KeyCode::Backspace => {
                let cursor = app.cron_editor.edit_cursor;
                app.cron_editor.edit_fields[cursor].pop();
            }
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Up => app.cron_editor.move_up(),
        KeyCode::Down => app.cron_editor.move_down(),
        KeyCode::Char('a') => app.cron_editor.add_new_job(),
        KeyCode::Char('d') | KeyCode::Delete => app.cron_editor.delete_selected(),
        KeyCode::Char('e') | KeyCode::Enter => {
            let sel = app.cron_editor.selected;
            app.cron_editor.begin_edit(sel);
        }
        KeyCode::Char('w') => match app.cron_editor.write_crontab() {
            Ok(()) => app
                .state
                .push_notification(Notification::success("Crontab saved")),
            Err(e) => app
                .state
                .push_notification(Notification::error(format!("{e}"))),
        },
        KeyCode::F(5) => {
            app.cron_editor = crate::modules::cron::CronEditor::new();
        }
        KeyCode::Esc => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 4: Man Page Viewer ──────────────────────────────────────────────────

fn handle_man(app: &mut App, key: KeyEvent) {
    let height = 40u16;
    match key.code {
        KeyCode::Up => app.man_viewer.scroll_up(1),
        KeyCode::Down => app.man_viewer.scroll_down(1),
        KeyCode::PageUp => app.man_viewer.scroll_page_up(height),
        KeyCode::PageDown => app.man_viewer.scroll_page_down(height),
        KeyCode::Home => app.man_viewer.scroll_to_top(),
        KeyCode::End => app.man_viewer.scroll_to_bottom(),
        KeyCode::Char('n') => app.man_viewer.next_match(),
        KeyCode::Char('N') => app.man_viewer.prev_match(),
        KeyCode::Char('/') => {
            app.man_viewer.search_query.clear();
        }
        KeyCode::Char(c) if !app.man_viewer.search_query.is_empty() => {
            app.man_viewer.search_query.push(c);
            let q = app.man_viewer.search_query.clone();
            app.man_viewer.search(&q);
        }
        KeyCode::Esc | KeyCode::Char('q') => app.state.restore_mode(),
        _ => {}
    }
}

// ── Phase 6: Notes ────────────────────────────────────────────────────────────

fn handle_notes(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.notes.move_up(),
        KeyCode::Down => app.notes.move_down(),
        KeyCode::PageUp => {
            for _ in 0..10 {
                app.notes.scroll_up();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                app.notes.scroll_down();
            }
        }
        // Open the selected note in the editor.
        KeyCode::Enter => {
            if let Some(note) = app.notes.selected().cloned() {
                let _ = app.open_file(note.path);
            }
        }
        // New note: prompt for a title.
        KeyCode::Char('n') => {
            app.notes_new = true;
            app.state.set_mode(AppMode::Dialog(DialogKind::Input {
                prompt: "New note title:".to_string(),
                value: String::new(),
            }));
        }
        // Open the first embedded image in the ImageViewer.
        KeyCode::Char('i') => match app.notes.first_image() {
            Some(path) => open_image_file(app, path),
            None => app
                .state
                .push_notification(Notification::warning("No image in this note")),
        },
        KeyCode::F(5) => {
            app.notes.scan();
            app.state.push_notification(Notification::info(format!(
                "Notes: {} indexed",
                app.notes.notes.len()
            )));
        }
        KeyCode::Esc => app.state.set_mode(AppMode::Menu),
        _ => {}
    }
}

/// Decode an image file and open it in the ImageViewer popup (reused by Notes).
fn open_image_file(app: &mut App, path: std::path::PathBuf) {
    if !path.exists() {
        app.state.push_notification(Notification::warning(format!(
            "Image not found: {}",
            path.display()
        )));
        return;
    }
    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    match std::fs::read(&path) {
        Ok(bytes) => match image::load_from_memory(&bytes) {
            Ok(img) => {
                let (orig_w, orig_h) = img.dimensions();
                let img = if orig_w > 1200 || orig_h > 900 {
                    img.resize(1200, 900, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                let (w, h) = img.dimensions();
                app.image_rgb = Some(img.to_rgb8().into_raw());
                app.image_dims = (w, h);
                app.image_title = title;
                app.state.set_mode(AppMode::ImageViewer);
            }
            Err(e) => app
                .state
                .push_notification(Notification::error(format!("Cannot decode image: {e}"))),
        },
        Err(e) => app
            .state
            .push_notification(Notification::error(format!("Cannot read image: {e}"))),
    }
}

// ── Phase 6.5: Weather ─────────────────────────────────────────────────────────

fn handle_weather(app: &mut App, key: KeyEvent) {
    // Add-city search overlay (drawn on top, stays in Weather mode): type to
    // filter live candidates, ↑/↓ to pick, Enter to add, Esc to cancel.
    if app.weather.search.active {
        match key.code {
            KeyCode::Char(c) => app.weather.search_push(c),
            KeyCode::Backspace => app.weather.search_pop(),
            KeyCode::Up => app.weather.search_up(),
            KeyCode::Down => app.weather.search_down(),
            KeyCode::Enter => app.weather_add_selected(),
            KeyCode::Esc => app.weather.search_close(),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up => app.weather.select_up(),
        KeyCode::Down => app.weather.select_down(),
        KeyCode::Char('a') => app.weather_begin_add(),
        KeyCode::Char('d') | KeyCode::Delete => app.weather_delete_selected(),
        KeyCode::Char('u') | KeyCode::Char('c') | KeyCode::Char('f') => app.weather_toggle_unit(),
        KeyCode::F(5) => app.weather.refresh_selected(),
        KeyCode::Char('r') => app.weather.force_refresh_selected(),
        KeyCode::Esc => app.state.set_mode(AppMode::Menu),
        _ => {}
    }
}

// ── Phase 6.5: Calendar ────────────────────────────────────────────────────────

fn handle_calendar(app: &mut App, key: KeyEvent) {
    use crate::modules::calendar::CalFocus;

    // ── Tasks pane: inline editor for the selected day ──────────────────────
    // Typing builds a new task title for the selected day; Enter commits it (and
    // keeps editing so several tasks can be added in a row). Tab/Esc commit any
    // pending text and hand focus back to the grid ("save on leave").
    if app.calendar.focus == CalFocus::Tasks {
        match key.code {
            KeyCode::Char(c) => app.calendar.input.push(c),
            KeyCode::Backspace => {
                app.calendar.input.pop();
            }
            KeyCode::Enter => {
                let text = app.calendar.input.trim().to_string();
                if !text.is_empty() {
                    app.cal_add_task(&text);
                    app.calendar.input.clear();
                } else {
                    // Empty buffer + Enter toggles the highlighted task done.
                    app.cal_toggle_selected();
                }
            }
            KeyCode::Delete => app.cal_delete_selected(),
            KeyCode::Up => app.calendar.task_up(),
            KeyCode::Down => app.calendar.task_down(),
            KeyCode::Tab | KeyCode::Esc => {
                let text = app.calendar.input.trim().to_string();
                if !text.is_empty() {
                    app.cal_add_task(&text);
                }
                app.calendar.input.clear();
                app.calendar.editing = false;
                app.calendar.focus = CalFocus::Grid;
            }
            _ => {}
        }
        return;
    }

    // ── Grid focus: navigate days/months; Tab or 'a' opens the inline editor ──
    match key.code {
        KeyCode::Left => {
            app.calendar.prev_month();
            app.cal_reload_tasks();
        }
        KeyCode::Right => {
            app.calendar.next_month();
            app.cal_reload_tasks();
        }
        KeyCode::Char('h') => {
            app.calendar.move_day(-1);
            app.cal_reload_tasks();
        }
        KeyCode::Char('l') => {
            app.calendar.move_day(1);
            app.cal_reload_tasks();
        }
        KeyCode::Up => {
            app.calendar.move_week(-1);
            app.cal_reload_tasks();
        }
        KeyCode::Down => {
            app.calendar.move_week(1);
            app.cal_reload_tasks();
        }
        KeyCode::Char('t') => {
            app.calendar.goto_today();
            app.cal_reload_tasks();
        }
        KeyCode::Tab | KeyCode::Char('a') => {
            app.calendar.focus = CalFocus::Tasks;
            app.calendar.editing = true;
            app.calendar.input.clear();
        }
        KeyCode::Esc => app.state.set_mode(AppMode::Menu),
        _ => {}
    }
}
