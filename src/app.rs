use anyhow::Result;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::path::PathBuf;

/// Cached render geometry — written during draw, read by mouse handler.
#[derive(Default, Clone, Copy)]
pub struct AppAreas {
    pub file_list: Rect,
    pub process_list: Rect,
    pub menu_list: Rect,
    pub side_panel: Rect,
}

use crate::audio::player::AudioPlayer;
use crate::config::settings::Settings;
use crate::core::command::{Command, GitCommand};
use crate::core::event_bus::EventBus;
use crate::core::keybinds::KeybindEngine;
use crate::core::state::{
    AppMode, AppState, ConfirmAction, DialogKind, Notification, PanelLayout, SidePanelMode,
};
use crate::db::sqlite::Database;
use crate::editor::buffer::Buffer;
use crate::fs::explorer::{ClipboardEntry, ClipboardOp, FileExplorer, copy_dir_recursive};
use crate::kitty::KittyFrame;
use crate::modules::calc::Calculator;
use crate::modules::calendar::CalendarPanel;
use crate::modules::cron::CronEditor;
use crate::modules::disk::DiskManager;
use crate::modules::docker::DockerPanel;
use crate::modules::logview::{LogSource, LogViewer};
use crate::modules::manpage::ManViewer;
use crate::modules::music_library::MusicLibrary;
use crate::modules::network::NetworkPanel;
use crate::modules::notes::NotesManager;
use crate::modules::packages::PackageManager;
use crate::modules::process::ProcessViewer;
use crate::modules::services::ServiceManager;
use crate::modules::ssh::SshManager;
use crate::modules::sysmon::SystemMonitor;
use crate::modules::weather::WeatherPanel;
use crate::plugins::PluginRegistry;
use crate::plugins::git::GitPlugin;
use crate::terminal::TerminalPane;
use crate::ui::command_palette::CommandPalette;
use crate::video::player::VideoPlayer;

/// A tab is a self-contained activity: it remembers its own `mode` (the module
/// it shows), its own file-manager state (`explorer` — so several tabs can sit
/// at different paths), and its own editor `buffer`. The *active* tab's explorer
/// lives in `App::explorer` while it is focused and is swapped back into the tab
/// on `switch_tab` (see `save_active_tab` / `load_active_tab`).
pub struct Tab {
    pub title: String,
    pub mode: AppMode,
    pub explorer: FileExplorer,
    pub editor: Option<Buffer>,
}

/// Modes that represent a persistent tab activity (worth remembering when the
/// user switches away). Transient overlays (palette, dialogs, viewers, help)
/// are excluded so they never overwrite a tab's real activity.
pub fn mode_is_tab_activity(mode: &AppMode) -> bool {
    !matches!(
        mode,
        AppMode::CommandPalette
            | AppMode::Command(_)
            | AppMode::Dialog(_)
            | AppMode::Quitting
            | AppMode::Help
            | AppMode::ImageViewer
            | AppMode::VideoPlayer
            | AppMode::PdfViewer
            | AppMode::SshConnectForm
    )
}

/// Human-readable title for a tab given its current activity.
pub fn mode_title(mode: &AppMode, explorer: &FileExplorer, editor: Option<&Buffer>) -> String {
    match mode {
        AppMode::Menu => "Menu".into(),
        AppMode::FileManager => explorer
            .current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| explorer.current_dir.to_string_lossy().to_string()),
        AppMode::Editor => editor
            .and_then(|b| b.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Editor".into()),
        AppMode::Terminal => "Terminal".into(),
        AppMode::ProcessViewer => "Monitor".into(),
        AppMode::Git => "Git".into(),
        AppMode::Config => "Config".into(),
        AppMode::AudioPlayer => "Audio".into(),
        AppMode::VideoPlayer => "Video".into(),
        AppMode::ImageViewer => "Image".into(),
        AppMode::PdfViewer => "PDF".into(),
        AppMode::Favorites => "Favorites".into(),
        AppMode::Help => "Help".into(),
        AppMode::ThemeSwitcher => "Theme".into(),
        AppMode::LogViewer => "Logs".into(),
        AppMode::ServiceManager => "Services".into(),
        AppMode::NetworkPanel => "Network".into(),
        AppMode::DiskManager => "Disks".into(),
        AppMode::Calculator => "Calc".into(),
        AppMode::PackageManager => "Packages".into(),
        AppMode::SshManager => "SSH".into(),
        AppMode::SshConnectForm => "SSH Connect".into(),
        AppMode::SftpPanel => "SFTP".into(),
        AppMode::DockerPanel => "Docker".into(),
        AppMode::CronEditor => "Cron".into(),
        AppMode::ManViewer => "Man".into(),
        AppMode::Notes => "Notes".into(),
        AppMode::Weather => "Weather".into(),
        AppMode::Calendar => "Calendar".into(),
        AppMode::CommandPalette => "Palette".into(),
        AppMode::Command(_) => "Command".into(),
        AppMode::Dialog(_) => "Dialog".into(),
        AppMode::Quitting => "Quit".into(),
    }
}

/// Expand a leading `~/` in a path string to the user's home directory.
pub fn expand_home_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub struct App {
    pub state: AppState,
    pub settings: Settings,
    pub db: Database,
    pub event_bus: EventBus,
    pub keybinds: KeybindEngine,

    // File manager
    pub explorer: FileExplorer,

    // Tabs
    pub tabs: Vec<Tab>,
    pub active_tab: usize,

    // Terminal
    pub terminal: TerminalPane,

    // Split / layout
    pub split_pct: u16,

    // Modules — Phase 2
    pub process_viewer: ProcessViewer,
    pub system_monitor: SystemMonitor,
    /// Monitor opens on the system dashboard; `d` swaps to the process list.
    pub monitor_dashboard: bool,
    pub log_viewer: LogViewer,
    pub service_manager: ServiceManager,
    pub network_panel: NetworkPanel,
    pub disk_manager: DiskManager,
    pub calculator: Calculator,
    // Modules — Phase 4
    pub package_manager: PackageManager,
    pub ssh_manager: SshManager,
    /// `Some` while the SFTP panel (`AppMode::SftpPanel`) is open for a host;
    /// `None` otherwise so no background sftp thread/session lingers.
    pub sftp_panel: Option<crate::modules::sftp::SftpPanel>,
    pub docker_panel: DockerPanel,
    pub cron_editor: CronEditor,
    pub man_viewer: ManViewer,
    // Modules — Phase 6
    pub notes: NotesManager,
    /// True while a "new note" title dialog is open (routes dialog Enter to
    /// `create_note`).
    pub notes_new: bool,
    /// True while the "new SSH group" name dialog is open.
    pub ssh_new_group: bool,
    /// `Some(host idx)` while the "assign host to group" dialog is open.
    pub ssh_assign_group_host: Option<usize>,
    /// True while the "create tunnel" form dialog is open.
    pub ssh_tunnel_form: bool,
    /// `Some(host idx)` between pressing Enter/reconnect and the next draw —
    /// lets `render_ssh_panel` paint a "Connecting…" popup *before* the TUI
    /// suspends to run the real `ssh` process (consumed in `main.rs` right
    /// after that draw call).
    pub ssh_pending_connect: Option<usize>,
    /// Same idea as `ssh_pending_connect`, for the ad-hoc Add-Connection
    /// form (`ssh_manager.conn_form`) instead of a saved host by index.
    pub ssh_pending_adhoc_connect: bool,
    pub weather: WeatherPanel,
    pub calendar: CalendarPanel,

    // Git
    pub git_plugin: Option<GitPlugin>,

    // Music library
    pub music_library: MusicLibrary,

    // Media
    pub audio_player: Option<AudioPlayer>,
    pub video_player: Option<VideoPlayer>,
    /// Pre-decoded RGB24 pixels (avoids re-decoding every frame)
    pub image_rgb: Option<Vec<u8>>,
    /// Original image dimensions (width, height) after resize cap
    pub image_dims: (u32, u32),
    pub image_title: String,
    /// Set during draw(), consumed after draw() for Kitty injection
    pub pending_kitty: Option<KittyFrame>,
    /// Last area used for Kitty image render — skip retransmit when unchanged
    pub kitty_last_area: Option<ratatui::layout::Rect>,

    // UI state
    pub command_palette: CommandPalette,
    pub menu_state: ListState,

    // Clipboard (copy/cut/paste)
    pub clipboard: Option<ClipboardEntry>,

    // Rename
    pub rename_src: Option<PathBuf>,

    // Plugins
    pub plugins: PluginRegistry,

    // Terminal detection
    pub use_kitty: bool,
    /// Pixel dimensions of one terminal cell: (cell_width_px, cell_height_px).
    /// Used to decode video / images at the display's native pixel resolution
    /// when using the Kitty graphics protocol.
    pub kitty_cell_px: (u16, u16),

    // Config panel navigation
    pub config_selected: usize,

    // PDF viewer
    pub pdf_pages: Vec<String>,
    pub pdf_scroll: usize,
    pub pdf_title: String,

    // Help panel
    pub help_topic: usize,
    pub help_tab: usize,
    pub help_scroll: usize,

    // Splash
    pub splash_tick: u16,

    // Mouse support (3.4): cached panel geometry + double-click detection.
    pub last_areas: AppAreas,
    pub last_click: Option<std::time::Instant>,

    // Favorites popup (3.6)
    pub favorites_list: Vec<PathBuf>,
    pub favorites_state: ratatui::widgets::ListState,

    // Theme switcher (3.2)
    pub theme_switcher_idx: usize,
    /// Index of the theme that was active when the switcher opened (for Esc restore)
    pub theme_switcher_original: String,

    /// Set after suspending the TUI for an external interactive process (SSH,
    /// future external editor). `main.rs` calls `terminal.clear()` and resets
    /// this before the next draw — ratatui's diffed buffer doesn't otherwise
    /// know the real screen was overwritten by the child process.
    pub force_full_redraw: bool,
    /// Mirrors `main.rs`'s `kbd_enhanced` (whether the terminal supports — and
    /// `main` pushed — the keyboard-disambiguation escape-code protocol).
    /// Passed to `run_external_interactive` so it can pop/re-push the flag
    /// around a suspended session — see that function's doc comment.
    pub kbd_enhanced: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let settings =
            Settings::load(std::path::Path::new("config/config.toml")).unwrap_or_default();
        let db = Database::open(std::path::Path::new("data/app.db"))?;
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
        let cwd = home;
        let explorer = FileExplorer::new(cwd.clone())?;

        let mut menu_state = ListState::default();
        menu_state.select(Some(0));

        let mut git_plugin = None;
        if explorer.has_git() {
            let mut gp = GitPlugin::new(cwd.clone());
            gp.open_cwd_repo(&cwd);
            git_plugin = Some(gp);
        }

        let mut state = AppState::new();
        state.current_dir = cwd.clone();

        let use_kitty = crate::kitty::is_kitty_terminal();
        let kitty_cell_px = if use_kitty {
            crate::kitty::cell_pixel_size()
        } else {
            (1, 2)
        };
        state.push_notification(if use_kitty {
            Notification::info(format!(
                "Kitty terminal detected — Kitty protocol active (cell {}×{}px)",
                kitty_cell_px.0, kitty_cell_px.1
            ))
        } else {
            Notification::info("24-bit terminal — half-block rendering active")
        });

        // Notes module: build at the configured directory and index it up-front
        // (without creating the folder) so the palette can search notes from the
        // start.
        let mut notes_manager = NotesManager::new(expand_home_path(&settings.notes_dir));
        if notes_manager.dir.exists() {
            notes_manager.scan();
            for n in &notes_manager.notes {
                let _ = db.upsert_note(
                    &n.path.to_string_lossy(),
                    &n.title,
                    &n.tags.join(","),
                    n.modified,
                );
            }
        }

        Ok(Self {
            state,
            settings,
            db,
            event_bus: EventBus::new(),
            keybinds: KeybindEngine::default_gui(),
            explorer,
            tabs: vec![Tab {
                title: "Menu".into(),
                mode: AppMode::Menu,
                explorer: FileExplorer::new(cwd.clone())?,
                editor: None,
            }],
            active_tab: 0,
            terminal: TerminalPane::new(),
            split_pct: 50,
            process_viewer: ProcessViewer::new(),
            system_monitor: SystemMonitor::new(),
            monitor_dashboard: true,
            log_viewer: LogViewer::new(LogSource::AppDb),
            service_manager: ServiceManager::new(),
            network_panel: NetworkPanel::new(),
            disk_manager: DiskManager::new(),
            calculator: Calculator::new(),
            package_manager: PackageManager::new(),
            ssh_manager: SshManager::new(),
            sftp_panel: None,
            docker_panel: DockerPanel::new(),
            cron_editor: CronEditor::new(),
            man_viewer: ManViewer::new(),
            notes: notes_manager,
            notes_new: false,
            ssh_new_group: false,
            ssh_assign_group_host: None,
            ssh_tunnel_form: false,
            ssh_pending_connect: None,
            ssh_pending_adhoc_connect: false,
            weather: WeatherPanel::new(),
            calendar: CalendarPanel::new(),
            git_plugin,
            music_library: MusicLibrary::new(),
            audio_player: None,
            video_player: None,
            image_rgb: None,
            image_dims: (0, 0),
            image_title: String::new(),
            pending_kitty: None,
            kitty_last_area: None,
            command_palette: CommandPalette::new(),
            menu_state,
            clipboard: None,
            rename_src: None,
            plugins: PluginRegistry::new(),
            use_kitty,
            kitty_cell_px,
            config_selected: 0,
            pdf_pages: Vec::new(),
            pdf_scroll: 0,
            pdf_title: String::new(),
            help_topic: 0,
            help_tab: 0,
            help_scroll: 0,
            splash_tick: 0,
            last_areas: AppAreas::default(),
            last_click: None,
            favorites_list: Vec::new(),
            favorites_state: ratatui::widgets::ListState::default(),
            theme_switcher_idx: 0,
            theme_switcher_original: String::new(),
            force_full_redraw: false,
            kbd_enhanced: false,
        })
    }

    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.tabs
            .get_mut(self.active_tab)
            .and_then(|t| t.editor.as_mut())
    }

    pub const MAX_TABS: usize = 10;

    /// Persist the live state (current activity mode + file-manager explorer)
    /// back into the active tab before leaving it.
    fn save_active_tab(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            if mode_is_tab_activity(&self.state.mode) {
                tab.mode = self.state.mode.clone();
            }
            std::mem::swap(&mut self.explorer, &mut tab.explorer);
            tab.title = mode_title(&tab.mode, &tab.explorer, tab.editor.as_ref());
        }
    }

    /// Restore the active tab's activity into the live app state (swap its
    /// explorer in, set the mode, sync current_dir).
    fn load_active_tab(&mut self) {
        // Swap the tab's explorer into the live `self.explorer` slot.
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            std::mem::swap(&mut self.explorer, &mut tab.explorer);
        }
        let mode = self.tabs[self.active_tab].mode.clone();
        self.state.set_mode(mode);
        self.state.current_dir = self.explorer.current_dir.clone();
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<()> {
        let buf = Buffer::load_file(&path)?;
        let _ = self.db.insert_recent_file(&path.to_string_lossy());
        if self.tabs.len() >= Self::MAX_TABS {
            self.state.push_notification(Notification::warning(format!(
                "Tab limit reached ({})",
                Self::MAX_TABS
            )));
            return Ok(());
        }
        // Open the file in a fresh tab so it becomes its own activity.
        self.save_active_tab();
        let explorer = FileExplorer::new(self.state.current_dir.clone())
            .unwrap_or_else(|_| self.explorer_placeholder());
        self.tabs.push(Tab {
            title: "Editor".into(),
            mode: AppMode::Editor,
            explorer,
            editor: Some(buf),
        });
        self.active_tab = self.tabs.len() - 1;
        self.load_active_tab();
        self.event_bus
            .publish(crate::core::event_bus::BusEvent::FileOpened(
                path.to_string_lossy().to_string(),
            ));
        Ok(())
    }

    /// Fallback explorer at the current directory (used when a fresh one fails
    /// to load — e.g. permission errors on the target path).
    fn explorer_placeholder(&self) -> FileExplorer {
        FileExplorer::new(std::env::current_dir().unwrap_or_default()).unwrap_or_else(|_| {
            FileExplorer {
                current_dir: PathBuf::from("/"),
                entries: Vec::new(),
                list_state: ratatui::widgets::ListState::default(),
                show_hidden: false,
            }
        })
    }

    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        // The closing tab's live explorer (in self.explorer) is discarded.
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.load_active_tab();
    }

    pub fn new_tab(&mut self) {
        if self.tabs.len() >= Self::MAX_TABS {
            self.state.push_notification(Notification::warning(format!(
                "Tab limit reached ({})",
                Self::MAX_TABS
            )));
            return;
        }
        // A new tab always starts on the main menu so the user can pick its activity.
        self.save_active_tab();
        let explorer = FileExplorer::new(self.state.current_dir.clone())
            .unwrap_or_else(|_| self.explorer_placeholder());
        self.tabs.push(Tab {
            title: "Menu".into(),
            mode: AppMode::Menu,
            explorer,
            editor: None,
        });
        self.active_tab = self.tabs.len() - 1;
        self.load_active_tab();
    }

    pub fn switch_tab(&mut self, idx: u8) {
        // Ctrl+1..9/0 map to tab positions 1..10; the mapping is purely
        // positional, so closing/reordering tabs re-arranges it automatically.
        let i = (idx as usize).saturating_sub(1);
        if i >= self.tabs.len() || i == self.active_tab {
            return;
        }
        self.save_active_tab();
        self.active_tab = i;
        self.load_active_tab();
    }

    pub fn needs_tick(&self) -> bool {
        self.audio_player.as_ref().map_or(false, |_| true)
            || self
                .video_player
                .as_ref()
                .map_or(false, |v| v.is_playing || v.is_loading)
            || self.state.side_pct != self.state.side_pct_target
            || self.state.git_pct != self.state.git_pct_target
            || self.state.music_pct != self.state.music_pct_target
            || self.calculator.anim_pct != self.calculator.anim_target
            || !self.state.notifications.is_empty()
            || self.terminal.running
            || self.weather.is_loading()
            || matches!(self.state.mode, AppMode::ProcessViewer | AppMode::Weather)
            || (matches!(self.state.mode, AppMode::Menu)
                && self.splash_tick < crate::ui::splash::REVEAL_END)
            || self.sftp_panel.as_ref().is_some_and(|p| p.is_loading())
            || !self.ssh_manager.tunnels.is_empty()
            || self.ssh_manager.testing
    }

    pub fn tick(&mut self) {
        self.terminal.tick();
        self.state.tick_side_panel();
        self.state.tick_git_panel();
        self.state.tick_music_panel();
        self.calculator.tick_anim();
        self.state.tick_notifications();
        self.ssh_manager.tick_tunnels();
        if let Some(result) = self.ssh_manager.tick_test() {
            // Only interrupt with a dialog if the user is still looking at
            // the SSH panel — otherwise just leave the status for when they
            // return (host.reachable / status_msg are already updated).
            if !result.reachable && matches!(self.state.mode, AppMode::SshManager) {
                let reason = if result.detail.is_empty() {
                    "connection refused or timed out".to_string()
                } else {
                    result.detail
                };
                self.state
                    .set_mode(AppMode::Dialog(DialogKind::Alert(format!(
                        "Failed to test {}: {reason}",
                        result.alias
                    ))));
            }
        }
        if let Some(p) = &mut self.sftp_panel {
            p.tick();
        }

        if matches!(self.state.mode, AppMode::Menu)
            && self.splash_tick < crate::ui::splash::REVEAL_END
        {
            self.splash_tick += 1;
        }

        if let Some(ap) = &mut self.audio_player {
            ap.tick();
        }
        if let Some(vp) = &mut self.video_player {
            let _ = vp.tick();
        }
        if matches!(self.state.mode, AppMode::ProcessViewer) {
            self.system_monitor.tick();
        }
        // Drain in-flight weather fetches and the live city search (cheap no-ops
        // when idle).
        self.weather.tick();
        self.weather.tick_search();
        // Advance the weather art animation only while the panel is open.
        if matches!(self.state.mode, AppMode::Weather) {
            self.weather.anim_tick = self.weather.anim_tick.wrapping_add(1);
        }
    }

    pub fn execute_command(&mut self, cmd: Command) -> Result<()> {
        let _ = self.db.insert_command(&format!("{cmd:?}"));
        match cmd {
            Command::Quit => {
                self.state.set_mode(AppMode::Quitting);
            }
            Command::Save => {
                if let Some(buf) = self.active_buffer_mut() {
                    buf.save_file()?;
                    self.state
                        .push_notification(Notification::success("File saved"));
                }
            }
            Command::OpenFile(path) => {
                self.open_file(PathBuf::from(path))?;
            }
            Command::SplitH => {
                self.state.layout = PanelLayout::HSplit(self.split_pct);
            }
            Command::SplitV => {
                self.state.layout = PanelLayout::VSplit(self.split_pct);
            }
            Command::SetTheme(t) => {
                self.settings.theme = t.clone();
                let _ = self
                    .settings
                    .save(std::path::Path::new("config/config.toml"));
                self.state
                    .push_notification(Notification::success(format!("Theme: {t}")));
                self.event_bus
                    .publish(crate::core::event_bus::BusEvent::ThemeChanged(t));
            }
            Command::ShowHelp => self.state.set_mode(AppMode::Help),
            Command::ShowTerminal => {
                if self.state.side_mode == SidePanelMode::Terminal {
                    self.state.side_pct_target = 0;
                } else {
                    self.state.side_mode = SidePanelMode::Terminal;
                    self.state.side_pct_target = 40;
                }
            }
            Command::ShowProcessViewer => self.state.set_mode(AppMode::ProcessViewer),
            Command::ShowNotes => self.enter_notes(),
            Command::ShowWeather => self.enter_weather(),
            Command::ShowCalendar => self.enter_calendar(),
            Command::ShowSsh => self.enter_ssh(),
            Command::Git(gcmd) => {
                if let Some(gp) = &mut self.git_plugin {
                    let path = gp.repo_path.clone();
                    let msg = match gcmd {
                        GitCommand::Status => {
                            gp.refresh();
                            "Git status refreshed".to_string()
                        }
                        GitCommand::Add(f) => {
                            if f.is_empty() || f == "." {
                                crate::plugins::git::git_add_all(&path).unwrap_or_default()
                            } else {
                                crate::plugins::git::git_add(&path, &f).unwrap_or_default()
                            }
                        }
                        GitCommand::Commit(m) => {
                            crate::plugins::git::git_commit(&path, &m).unwrap_or_default()
                        }
                        GitCommand::Pull => {
                            crate::plugins::git::git_pull(&path).unwrap_or_default()
                        }
                        GitCommand::Push => {
                            crate::plugins::git::git_push(&path).unwrap_or_default()
                        }
                    };
                    self.state.push_notification(Notification::info(msg));
                }
            }
            Command::PluginList => {
                let names = self.plugins.names().join(", ");
                self.state
                    .push_notification(Notification::info(format!("Plugins: {names}")));
            }
            Command::Unknown(s) if !s.is_empty() => {
                self.state
                    .push_notification(Notification::error(format!("Unknown command: {s}")));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn open_command_palette(&mut self) {
        // Feed the current notes into the palette as dynamic `nota:` items so
        // they are fuzzy-searchable alongside the static commands.
        let note_items: Vec<(String, String)> = self
            .notes
            .notes
            .iter()
            .map(|n| (n.title.clone(), n.path.to_string_lossy().to_string()))
            .collect();
        self.command_palette.set_note_items(note_items);
        self.command_palette.reset();
        self.state.set_mode(AppMode::CommandPalette);
    }

    pub fn confirm_dialog(&mut self, action: ConfirmAction) {
        let message = match &action {
            ConfirmAction::DeleteFile(p) => format!("Delete '{p}'?"),
            ConfirmAction::OverwriteFile(p) => format!("Overwrite '{p}'?"),
            ConfirmAction::QuitUnsaved => "Quit without saving?".to_string(),
            ConfirmAction::KillProcess(pid) => format!("Kill process {pid}?"),
            ConfirmAction::SaveSshHost {
                alias, existing, ..
            } => {
                if *existing {
                    format!("Save changes to '{alias}' in ~/.ssh/config?")
                } else {
                    format!("Save '{alias}' to ~/.ssh/config?")
                }
            }
            ConfirmAction::DeleteSshHost(alias) => {
                format!("Remove '{alias}' from ~/.ssh/config?")
            }
        };
        self.state
            .set_mode(AppMode::Dialog(DialogKind::Confirm { message, action }));
    }

    pub fn toggle_side_git(&mut self) {
        if matches!(self.state.mode, AppMode::Git) {
            self.state.git_pct_target = 0;
        } else {
            if self.git_plugin.is_none() {
                let cwd = self.state.current_dir.clone();
                let mut gp = GitPlugin::new(cwd.clone());
                gp.open_cwd_repo(&cwd);
                self.git_plugin = Some(gp);
            }
            if let Some(gp) = &mut self.git_plugin {
                gp.load_panel_data();
            }
            self.state.set_mode(AppMode::Git);
            self.state.git_pct = 0;
            self.state.git_pct_target = 100;
        }
    }

    pub fn clipboard_copy(&mut self) {
        if let Some(entry) = self.explorer.selected_entry().cloned() {
            self.state
                .push_notification(Notification::info(format!("Copied: {}", entry.name)));
            self.clipboard = Some(ClipboardEntry {
                path: entry.path,
                op: ClipboardOp::Copy,
            });
        }
    }

    pub fn clipboard_cut(&mut self) {
        if let Some(entry) = self.explorer.selected_entry().cloned() {
            self.state
                .push_notification(Notification::info(format!("Cut: {}", entry.name)));
            self.clipboard = Some(ClipboardEntry {
                path: entry.path,
                op: ClipboardOp::Cut,
            });
        }
    }

    pub fn paste_clipboard(&mut self) -> Result<()> {
        let Some(clip) = self.clipboard.clone() else {
            return Ok(());
        };
        let src = &clip.path;
        let filename = match src.file_name() {
            Some(n) => n,
            None => {
                self.state
                    .push_notification(Notification::error("Invalid filename"));
                return Ok(());
            }
        };
        let dst = self.explorer.current_dir.join(filename);
        if dst == *src {
            self.state
                .push_notification(Notification::warning("Source and destination are the same"));
            return Ok(());
        }
        let name = filename.to_string_lossy().to_string();
        match clip.op {
            ClipboardOp::Copy => {
                if src.is_dir() {
                    copy_dir_recursive(src, &dst)?;
                } else {
                    std::fs::copy(src, &dst)?;
                }
                self.state
                    .push_notification(Notification::success(format!("Copied → {name}")));
            }
            ClipboardOp::Cut => {
                if let Err(_) = std::fs::rename(src, &dst) {
                    if src.is_dir() {
                        copy_dir_recursive(src, &dst)?;
                        std::fs::remove_dir_all(src)?;
                    } else {
                        std::fs::copy(src, &dst)?;
                        std::fs::remove_file(src)?;
                    }
                }
                self.clipboard = None;
                self.state
                    .push_notification(Notification::success(format!("Moved → {name}")));
            }
        }
        let _ = self.explorer.load_entries();
        Ok(())
    }

    pub fn execute_rename(&mut self, new_name: &str) -> Result<()> {
        let Some(src) = self.rename_src.take() else {
            return Ok(());
        };
        let dst = src
            .parent()
            .unwrap_or(std::path::Path::new("/"))
            .join(new_name);
        if dst == src {
            return Ok(());
        }
        std::fs::rename(&src, &dst)?;
        let _ = self.explorer.load_entries();
        self.state
            .push_notification(Notification::success(format!("Renamed → {new_name}")));
        Ok(())
    }

    pub fn config_cycle(&mut self, field: usize, forward: bool) {
        match field {
            0 => {
                self.settings.theme = if self.settings.theme == "dark" {
                    "light".into()
                } else {
                    "dark".into()
                };
            }
            1 => {
                self.settings.show_hidden = !self.settings.show_hidden;
            }
            2 => {
                self.settings.mouse_enabled = !self.settings.mouse_enabled;
            }
            3 => {
                self.settings.autosave = !self.settings.autosave;
            }
            4 => {
                self.settings.tab_width = match (self.settings.tab_width, forward) {
                    (2, true) => 4,
                    (4, true) => 8,
                    (_, true) => 2,
                    (2, false) => 8,
                    (4, false) => 2,
                    _ => 4,
                };
            }
            _ => {}
        }
    }

    pub fn open_pdf(&mut self, path: std::path::PathBuf) {
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("PDF")
            .to_string();

        let text = std::process::Command::new("pdftotext")
            .arg(&path)
            .arg("-")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_else(|| {
                format!(
                    "Error: pdftotext not found.\nInstall: brew install poppler\n\nFile: {}",
                    path.display()
                )
            });

        self.pdf_pages = text.lines().map(|l| l.to_string()).collect();
        self.pdf_scroll = 0;
        self.pdf_title = title;
        self.state.set_mode(AppMode::PdfViewer);
    }

    pub fn toggle_side_terminal(&mut self) {
        if self.state.side_mode == SidePanelMode::Terminal {
            self.state.side_pct_target = 0;
        } else {
            self.state.side_mode = SidePanelMode::Terminal;
            self.state.side_pct_target = 40;
            self.state.side_focused = false;
        }
    }

    /// Open the Notes module: (re)scan the configured directory, refresh the
    /// DB index and switch to `AppMode::Notes`.
    pub fn enter_notes(&mut self) {
        self.notes.dir = expand_home_path(&self.settings.notes_dir);
        self.notes.scan();
        self.sync_notes_index();
        self.state.set_mode(AppMode::Notes);
    }

    /// Create a new note from a title, index it and open it in the editor.
    pub fn create_note(&mut self, title: &str) {
        match self.notes.create_note(title) {
            Ok(path) => {
                self.sync_notes_index();
                self.state.push_notification(Notification::success(format!(
                    "Note created: {}",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                )));
                let _ = self.open_file(path);
            }
            Err(e) => {
                self.state
                    .push_notification(Notification::error(format!("Could not create note: {e}")));
            }
        }
    }

    /// Persist the current in-memory notes index to the DB.
    fn sync_notes_index(&self) {
        let _ = self.db.clear_notes_index();
        for n in &self.notes.notes {
            let _ = self.db.upsert_note(
                &n.path.to_string_lossy(),
                &n.title,
                &n.tags.join(","),
                n.modified,
            );
        }
    }

    /// Open the Weather panel: load the saved cities (seeding the default
    /// location on first run) and refresh them all (cache-aware).
    pub fn enter_weather(&mut self) {
        let mut rows = self.db.get_weather_cities().unwrap_or_default();
        if rows.is_empty() {
            // First run: seed with the configured default location.
            if let Ok(id) = self.db.add_weather_city(
                &self.settings.location_name,
                self.settings.latitude,
                self.settings.longitude,
            ) {
                rows.push((
                    id,
                    self.settings.location_name.clone(),
                    self.settings.latitude,
                    self.settings.longitude,
                ));
            }
        }
        self.weather.unit = if self.settings.fahrenheit {
            crate::modules::weather::TempUnit::Fahrenheit
        } else {
            crate::modules::weather::TempUnit::Celsius
        };
        self.weather.set_cities(rows);
        self.weather.refresh_all();
        self.state.set_mode(AppMode::Weather);
    }

    /// Toggle the weather temperature unit (°C ↔ °F) and persist the choice.
    pub fn weather_toggle_unit(&mut self) {
        self.weather.toggle_unit();
        self.settings.fahrenheit =
            self.weather.unit == crate::modules::weather::TempUnit::Fahrenheit;
        let _ = self
            .settings
            .save(std::path::Path::new("config/config.toml"));
        self.state.push_notification(Notification::info(format!(
            "Unit: {}",
            self.weather.unit.label()
        )));
    }

    /// Open the in-panel "add city" search overlay (stays in Weather mode).
    pub fn weather_begin_add(&mut self) {
        self.weather.search_open();
    }

    /// Add the city currently highlighted in the search overlay: persist it,
    /// start fetching its weather, and close the overlay.
    pub fn weather_add_selected(&mut self) {
        let Some(cand) = self.weather.selected_candidate().cloned() else {
            return;
        };
        if self.weather.has_city(cand.lat, cand.lon) {
            self.state.push_notification(Notification::warning(format!(
                "Already in the list: {}",
                cand.label
            )));
        } else if let Ok(id) = self.db.add_weather_city(&cand.label, cand.lat, cand.lon) {
            self.weather
                .add_city(id, cand.label.clone(), cand.lat, cand.lon);
            self.state
                .push_notification(Notification::success(format!("Added: {}", cand.label)));
        }
        self.weather.search_close();
    }

    /// Delete the selected city from the list and the database.
    pub fn weather_delete_selected(&mut self) {
        if let Some(id) = self.weather.selected_city_id() {
            let _ = self.db.delete_weather_city(id);
            self.weather.remove_selected();
            self.state
                .push_notification(Notification::info("City removed"));
        }
    }

    /// Open the Calendar: refresh "today", reload tasks for the selected day
    /// and the month's task markers, then switch mode.
    pub fn enter_calendar(&mut self) {
        self.calendar.today = chrono::Local::now().date_naive();
        self.cal_reload_tasks();
        self.state.set_mode(AppMode::Calendar);
    }

    /// Reload all tasks for the visible month into `calendar.month_tasks`
    /// (keyed by day) so each grid cell can show its events inline. The selected
    /// day's slice and the task-day markers are derived from that map.
    pub fn cal_reload_tasks(&mut self) {
        use crate::modules::calendar::Task;
        use std::collections::HashMap;

        let (first, last) = self.calendar.visible_month_range();
        let rows = self
            .db
            .get_tasks_between(
                &first.format("%Y-%m-%d").to_string(),
                &last.format("%Y-%m-%d").to_string(),
            )
            .unwrap_or_default();

        let mut map: HashMap<chrono::NaiveDate, Vec<Task>> = HashMap::new();
        for (id, date, text, done) in rows {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
                map.entry(d).or_default().push(Task {
                    id,
                    date,
                    text,
                    done,
                });
            }
        }
        self.calendar.load(map);
    }

    /// Add a task to the selected day, persist it and reload.
    pub fn cal_add_task(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let date = self.calendar.selected_date_str();
        match self.db.add_task(&date, text) {
            Ok(_) => {
                self.cal_reload_tasks();
                self.state
                    .push_notification(Notification::success("Task added"));
            }
            Err(e) => {
                self.state
                    .push_notification(Notification::error(format!("Could not add task: {e}")));
            }
        }
    }

    /// Toggle the done flag on the selected task, persist and reload.
    pub fn cal_toggle_selected(&mut self) {
        if let Some(task) = self.calendar.selected_task() {
            let id = task.id;
            let _ = self.db.toggle_task(id);
            self.cal_reload_tasks();
        }
    }

    /// Delete the selected task, persist and reload.
    pub fn cal_delete_selected(&mut self) {
        if let Some(task) = self.calendar.selected_task() {
            let id = task.id;
            let _ = self.db.delete_task(id);
            self.cal_reload_tasks();
            self.state
                .push_notification(Notification::info("Task deleted"));
        }
    }

    /// Toggle the notch-style floating music mini-panel ('m' key).
    pub fn toggle_music_panel(&mut self) {
        if self.state.music_pct_target > 0 {
            self.state.music_pct_target = 0;
            self.state.music_panel_focused = false;
        } else {
            self.state.music_pct_target = 10;
            self.state.music_panel_focused = true;
        }
    }

    /// Open the SSH Manager: reload hosts from `~/.ssh/config`, merge any
    /// DB-assigned groups (these override comment-derived `# group:`
    /// labels), and load the connection history.
    pub fn enter_ssh(&mut self) {
        self.ssh_manager.reload();
        if let Ok(assignments) = self.db.get_host_groups() {
            for (alias, group) in assignments {
                if let Some(h) = self.ssh_manager.hosts.iter_mut().find(|h| h.alias == alias) {
                    h.group = Some(group);
                }
            }
        }
        self.ssh_refresh_history();
        self.state.set_mode(AppMode::SshManager);
    }

    /// Reload the connection-history list from SQLite into the SSH Manager.
    pub fn ssh_refresh_history(&mut self) {
        let history = self
            .db
            .get_ssh_history(50)
            .unwrap_or_default()
            .into_iter()
            .map(|(alias, connected_at, duration_secs, exit_code)| {
                crate::modules::ssh::SshHistoryEntry {
                    alias,
                    connected_at,
                    duration_secs,
                    exit_code,
                }
            })
            .collect();
        self.ssh_manager.set_history(history);
    }

    /// Open a real interactive SSH session to host `idx`, suspending the TUI
    /// for the duration of the call. Records the session in SQLite history
    /// and the in-run tab strip, then forces a full redraw on return.
    ///
    /// Called from `main.rs` right after the frame showing the "Connecting…"
    /// popup (`app.ssh_pending_connect`) is drawn — never called directly
    /// from input handling, so the popup always gets a chance to paint
    /// before the TUI suspends.
    pub fn ssh_connect_now(&mut self, idx: usize) {
        let Some(host) = self.ssh_manager.hosts.get(idx).cloned() else {
            return;
        };
        self.ssh_run_session(&host.alias, vec![host.alias.clone()]);
    }

    /// Connect using the ad-hoc Add-Connection form (`ssh_manager.conn_form`,
    /// `editing_alias: None`): build `ssh` args directly from the typed
    /// Host/Username/Port (the form never reached `~/.ssh/config`, so there's
    /// no alias to pass). If the attempt wasn't a hard connection-level
    /// failure, follow up by asking whether to save it — see
    /// `ssh_run_session`'s `denied` return value.
    pub fn ssh_connect_adhoc(&mut self) {
        let Some(form) = self.ssh_manager.conn_form.take() else {
            return;
        };
        let host = form.host.trim().to_string();
        if host.is_empty() {
            self.state
                .push_notification(Notification::warning("Host can't be empty"));
            self.ssh_manager.conn_form = Some(form);
            return;
        }
        // Leave the form mode now (back to the host list) so the suspend,
        // any failure `Alert`, and the save `Confirm` dialog all chain off
        // `AppMode::SshManager` instead of the now-emptied connect form.
        self.state.restore_mode();

        let port = form.port_or_default();
        let mut args = vec!["-p".to_string(), port.to_string()];
        let target = if form.username.trim().is_empty() {
            host.clone()
        } else {
            format!("{}@{}", form.username.trim(), host)
        };
        args.push(target);

        let denied = self.ssh_run_session(&host, args);
        if !denied {
            self.confirm_dialog(ConfirmAction::SaveSshHost {
                alias: host.clone(),
                hostname: host,
                user: form.username.trim().to_string(),
                port,
                existing: false,
            });
        }
    }

    /// Shared by `ssh_connect_now`/`ssh_connect_adhoc`: suspends the TUI,
    /// runs `ssh <ssh_args>`, restores it, records the session in history,
    /// and notifies the user. `alias` is only used for history/notification
    /// labels. Returns `true` when `ssh` reported a connection-level failure
    /// (exit 255, or it couldn't even be spawned) as opposed to a normal
    /// session whose *remote* shell/command happened to exit non-zero — in
    /// that case it also shows a dialog with the failure reason, since the
    /// real `ssh` error text was printed straight to the terminal during the
    /// (now-overwritten) suspended session and this is the only place left
    /// to tell the user what happened.
    fn ssh_run_session(&mut self, alias: &str, ssh_args: Vec<String>) -> bool {
        let started_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let start = std::time::Instant::now();
        let result = crate::terminal::run_external_interactive("ssh", &ssh_args, self.kbd_enhanced);
        let duration_secs = start.elapsed().as_secs() as i64;
        self.force_full_redraw = true;

        let exit_code = result.as_ref().ok().and_then(|s| s.code());
        let _ = self.db.record_ssh_session(alias, duration_secs, exit_code);
        self.ssh_manager
            .record_session_inmem(alias, started_at, duration_secs, exit_code);

        self.state.push_notification(match &result {
            Ok(status) if status.success() => {
                Notification::success(format!("{alias}: session ended"))
            }
            Ok(status) => {
                Notification::warning(format!("{alias}: exited with code {:?}", status.code()))
            }
            Err(e) => Notification::error(format!("{alias}: failed to connect — {e}")),
        });

        let denied = matches!(exit_code, Some(255)) || result.is_err();
        if denied {
            let detail = match &result {
                Ok(_) => "connection refused (check host, port, key or credentials)".to_string(),
                Err(e) => e.to_string(),
            };
            self.state
                .set_mode(AppMode::Dialog(DialogKind::Alert(format!(
                    "Failed to connect to {alias}: {detail}"
                ))));
        }
        denied
    }

    /// Create (or reuse) an SSH group by name — used by the "new group" dialog.
    pub fn ssh_create_group(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        match self.db.upsert_group(name) {
            Ok(_) => self
                .state
                .push_notification(Notification::success(format!("Group '{name}' created"))),
            Err(e) => self
                .state
                .push_notification(Notification::error(format!("Error: {e}"))),
        }
    }

    /// Assign host `idx` to a (possibly new) group — used by the "assign
    /// host to group" dialog.
    pub fn ssh_assign_group(&mut self, idx: usize, group_name: &str) {
        let group_name = group_name.trim();
        if group_name.is_empty() {
            return;
        }
        let Some(alias) = self.ssh_manager.hosts.get(idx).map(|h| h.alias.clone()) else {
            return;
        };
        let result = self
            .db
            .upsert_group(group_name)
            .and_then(|gid| self.db.assign_host_group(&alias, gid));
        match result {
            Ok(()) => {
                if let Some(h) = self.ssh_manager.hosts.get_mut(idx) {
                    h.group = Some(group_name.to_string());
                }
                self.state
                    .push_notification(Notification::success(format!("{alias} → {group_name}")));
            }
            Err(e) => self
                .state
                .push_notification(Notification::error(format!("Error: {e}"))),
        }
    }

    /// Parse `local:remote_host:remote_port` and open a tunnel for the
    /// currently selected host — used by the "create tunnel" dialog.
    pub fn ssh_create_tunnel_from_form(&mut self, input: &str) {
        let parts: Vec<&str> = input.trim().splitn(3, ':').collect();
        let [local_s, remote_host, remote_port_s] = parts[..] else {
            self.state.push_notification(Notification::error(
                "Format: local_port:remote_host:remote_port",
            ));
            return;
        };
        let (Ok(local_port), Ok(remote_port)) =
            (local_s.parse::<u16>(), remote_port_s.parse::<u16>())
        else {
            self.state
                .push_notification(Notification::error("Invalid ports"));
            return;
        };
        let Some(alias) = self
            .ssh_manager
            .hosts
            .get(self.ssh_manager.selected)
            .map(|h| h.alias.clone())
        else {
            return;
        };
        match self.ssh_manager.create_tunnel(
            local_port,
            remote_host.to_string(),
            remote_port,
            alias.clone(),
        ) {
            Ok(()) => self.state.push_notification(Notification::success(format!(
                "Tunnel {local_port} → {remote_host}:{remote_port} via {alias}"
            ))),
            Err(e) => self
                .state
                .push_notification(Notification::error(format!("Error creating tunnel: {e}"))),
        }
    }

    /// Open the SFTP panel for host `idx`, starting the local pane at the
    /// current FileManager directory.
    pub fn open_sftp(&mut self, idx: usize) {
        let Some(host) = self.ssh_manager.hosts.get(idx).cloned() else {
            return;
        };
        let start_dir = self.explorer.current_dir.clone();
        match crate::modules::sftp::SftpPanel::new(host.alias.clone(), start_dir) {
            Ok(mut panel) => {
                panel.refresh_remote();
                self.sftp_panel = Some(panel);
                self.state.set_mode(AppMode::SftpPanel);
            }
            Err(e) => self
                .state
                .push_notification(Notification::error(format!("SFTP: {e}"))),
        }
    }

    /// Close the SFTP panel, dropping the background channel/threads.
    pub fn close_sftp(&mut self) {
        self.sftp_panel = None;
        self.state.restore_mode();
    }

    /// Open a blank Add-Connection form (`Tab` from the SSH host list).
    pub fn ssh_open_add_form(&mut self) {
        self.ssh_manager.conn_form = Some(crate::modules::ssh::SshConnForm::new_blank());
        self.state.set_mode(AppMode::SshConnectForm);
    }

    /// Open the same form pre-filled for editing host `idx` (`e` in the
    /// host list) — submitting it saves directly, no connection attempt.
    pub fn ssh_open_edit_form(&mut self, idx: usize) {
        let Some(host) = self.ssh_manager.hosts.get(idx) else {
            return;
        };
        self.ssh_manager.conn_form = Some(crate::modules::ssh::SshConnForm::from_host(host));
        self.state.set_mode(AppMode::SshConnectForm);
    }

    /// Close the connect/edit form without saving or connecting.
    pub fn ssh_close_form(&mut self) {
        self.ssh_manager.conn_form = None;
        self.state.restore_mode();
    }

    /// Submit the open form: editing an existing host asks to save the
    /// changes directly; adding a new one attempts the connection first
    /// (`ssh_connect_adhoc` asks to save afterward on its own).
    pub fn ssh_submit_form(&mut self) {
        let Some(form) = &self.ssh_manager.conn_form else {
            return;
        };
        if let Some(alias) = form.editing_alias.clone() {
            let hostname = form.host.trim().to_string();
            let user = form.username.trim().to_string();
            let port = form.port_or_default();
            self.ssh_manager.conn_form = None;
            self.state.restore_mode();
            self.confirm_dialog(ConfirmAction::SaveSshHost {
                alias,
                hostname,
                user,
                port,
                existing: true,
            });
        } else {
            self.ssh_connect_adhoc();
        }
    }

    /// Persist a `SaveSshHost` confirm action: append or update the `Host`
    /// block in `~/.ssh/config` (backed up first), then reload the list so
    /// the change is visible immediately.
    pub fn ssh_save_host(
        &mut self,
        alias: &str,
        hostname: &str,
        user: &str,
        port: u16,
        existing: bool,
    ) {
        let current = crate::modules::ssh::read_ssh_config();
        let updated = if existing {
            crate::modules::ssh::update_host_block(&current, alias, hostname, user, port)
        } else {
            crate::modules::ssh::append_host_block(&current, alias, hostname, user, port)
        };
        match crate::modules::ssh::write_ssh_config(&updated) {
            Ok(()) => {
                self.enter_ssh();
                self.state
                    .push_notification(Notification::success(format!("Saved '{alias}'")));
            }
            Err(e) => self.state.push_notification(Notification::error(format!(
                "Could not save ~/.ssh/config: {e}"
            ))),
        }
    }

    /// Persist a `DeleteSshHost` confirm action: remove the `Host` block
    /// from `~/.ssh/config` (backed up first), then reload the list.
    pub fn ssh_delete_host(&mut self, alias: &str) {
        let current = crate::modules::ssh::read_ssh_config();
        let updated = crate::modules::ssh::remove_host_block(&current, alias);
        match crate::modules::ssh::write_ssh_config(&updated) {
            Ok(()) => {
                self.enter_ssh();
                self.state
                    .push_notification(Notification::info(format!("Removed '{alias}'")));
            }
            Err(e) => self.state.push_notification(Notification::error(format!(
                "Could not update ~/.ssh/config: {e}"
            ))),
        }
    }
}
