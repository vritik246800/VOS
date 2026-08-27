#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Menu,
    FileManager,
    Editor,
    Terminal,
    ProcessViewer,
    Git,
    Config,
    AudioPlayer,
    VideoPlayer,
    ImageViewer,
    PdfViewer,
    Favorites,
    Help,
    ThemeSwitcher,
    // Phase 2 modules
    LogViewer,
    ServiceManager,
    NetworkPanel,
    DiskManager,
    Calculator,
    // Phase 4 modules
    PackageManager,
    SshManager,
    SshConnectForm,
    SftpPanel,
    DockerPanel,
    CronEditor,
    ManViewer,
    // Phase 6 apps
    Notes,
    Weather,
    Calendar,
    CommandPalette,
    Command(String),
    Dialog(DialogKind),
    Quitting,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DialogKind {
    Confirm {
        message: String,
        action: ConfirmAction,
    },
    Input {
        prompt: String,
        value: String,
    },
    Alert(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteFile(String),
    OverwriteFile(String),
    QuitUnsaved,
    KillProcess(u32),
    /// Persist the just-submitted SSH connect-form fields to
    /// `~/.ssh/config` — append a new `Host` block (`existing = false`) or
    /// rewrite an existing one (`existing = true`).
    SaveSshHost {
        alias: String,
        hostname: String,
        user: String,
        port: u16,
        existing: bool,
    },
    /// Remove a `Host` block from `~/.ssh/config`.
    DeleteSshHost(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelLayout {
    Single,
    HSplit(u16),
    VSplit(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusedPanel {
    Main,
    Side,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SidePanelMode {
    None,
    Git,
    Terminal,
    SystemMonitor,
    AudioPlayer,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub level: NotificationLevel,
    pub message: String,
    pub ttl: u8,
}

impl Notification {
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Info,
            message: msg.into(),
            ttl: 60,
        }
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Success,
            message: msg.into(),
            ttl: 60,
        }
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Warning,
            message: msg.into(),
            ttl: 90,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            level: NotificationLevel::Error,
            message: msg.into(),
            ttl: 120,
        }
    }
}

pub struct AppState {
    pub mode: AppMode,
    pub prev_mode: AppMode,
    pub layout: PanelLayout,
    pub focused: FocusedPanel,
    pub side_mode: SidePanelMode,
    pub side_pct: u16,
    pub side_pct_target: u16,
    pub side_focused: bool,
    pub split_pct: u16,
    pub current_dir: std::path::PathBuf,
    pub status_msg: Option<String>,
    pub notifications: Vec<Notification>,
    pub notification_history: Vec<Notification>,
    /// Git panel vertical slide animation (0 = hidden, 100 = full screen).
    pub git_pct: u16,
    pub git_pct_target: u16,
    /// Music notch panel height in rows (0 = hidden, 10 = fully open).
    pub music_pct: u16,
    pub music_pct_target: u16,
    /// Whether the music notch panel has keyboard focus.
    pub music_panel_focused: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Menu,
            prev_mode: AppMode::Menu,
            layout: PanelLayout::Single,
            focused: FocusedPanel::Main,
            side_mode: SidePanelMode::None,
            side_pct: 0,
            side_pct_target: 0,
            side_focused: false,
            split_pct: 50,
            git_pct: 0,
            git_pct_target: 0,
            music_pct: 0,
            music_pct_target: 0,
            music_panel_focused: false,
            current_dir: std::env::current_dir().unwrap_or_default(),
            status_msg: None,
            notifications: Vec::new(),
            notification_history: Vec::new(),
        }
    }

    pub fn push_notification(&mut self, n: Notification) {
        self.notification_history.push(n.clone());
        self.notifications.push(n);
    }

    pub fn tick_notifications(&mut self) {
        self.notifications.retain_mut(|n| {
            if n.ttl > 0 {
                n.ttl -= 1;
                true
            } else {
                false
            }
        });
    }

    pub fn set_mode(&mut self, mode: AppMode) {
        self.prev_mode = self.mode.clone();
        self.mode = mode;
    }

    pub fn restore_mode(&mut self) {
        let prev = self.prev_mode.clone();
        self.mode = prev;
    }

    pub fn tick_side_panel(&mut self) {
        if self.side_pct < self.side_pct_target {
            self.side_pct = (self.side_pct + 4).min(self.side_pct_target);
        } else if self.side_pct > self.side_pct_target {
            self.side_pct = self.side_pct.saturating_sub(4).max(self.side_pct_target);
            if self.side_pct == 0 {
                self.side_mode = SidePanelMode::None;
                self.side_focused = false;
            }
        }
    }

    pub fn tick_git_panel(&mut self) {
        if self.git_pct < self.git_pct_target {
            self.git_pct = (self.git_pct + 8).min(self.git_pct_target);
        } else if self.git_pct > self.git_pct_target {
            self.git_pct = self.git_pct.saturating_sub(8).max(self.git_pct_target);
            if self.git_pct == 0 && self.mode == AppMode::Git {
                let safe = match &self.prev_mode {
                    AppMode::Git => AppMode::Menu,
                    m => m.clone(),
                };
                self.mode = safe;
            }
        }
    }

    pub fn tick_music_panel(&mut self) {
        if self.music_pct < self.music_pct_target {
            self.music_pct = (self.music_pct + 2).min(self.music_pct_target);
        } else if self.music_pct > self.music_pct_target {
            self.music_pct = self.music_pct.saturating_sub(2).max(self.music_pct_target);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
