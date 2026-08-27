/// Centralized Nerd Font icon constants used across all VOS panels.
/// Source crates: nerd-font-symbols (md/seti sets) + nerdicons_rs (cod set).
/// Never use raw \u{xxxx} in panel files — always import from here.
// ── Material Design (md) ────────────────────────────────────────────────────
pub use nerd_font_symbols::md::{
    // Status & Alerts
    MD_ALERT,
    MD_ALERT_CIRCLE,
    MD_ARROW_DOWN,
    MD_ARROW_UP,
    MD_BOOK_OPEN,
    MD_BOOKMARK,
    MD_CALCULATOR,
    MD_CALENDAR,
    MD_CHECK,
    MD_CHECK_CIRCLE,
    // System
    MD_CHIP,
    MD_CLOCK,
    MD_CLOSE,
    MD_COG,
    MD_COGS,
    MD_CONSOLE,
    // Misc
    MD_DOCKER,
    MD_DOWNLOAD_NETWORK,
    MD_ETHERNET,
    MD_EXPANSION_CARD,
    MD_EYE,
    MD_EYE_OFF,
    MD_FILE,
    // Files & Editor
    MD_FOLDER,
    // Git
    MD_GIT,
    MD_HARDDISK,
    MD_HEADPHONES,
    MD_HELP,
    MD_HELP_CIRCLE,
    MD_HISTORY,
    // Navigation & UI
    MD_HOME,
    MD_INFORMATION,
    MD_KEY,
    MD_LAN,
    MD_LOADING,
    MD_LOCK,
    MD_MAGNIFY,
    MD_MEMORY,
    MD_MENU,
    MD_MONITOR,
    MD_MUSIC,
    MD_NETWORK,
    MD_NOTE_TEXT,
    MD_PACKAGE,
    MD_PALETTE,
    MD_PAUSE,
    MD_PENCIL,
    // Player controls
    MD_PLAY,
    MD_POWER,
    MD_REFRESH,
    MD_RELOAD,
    MD_ROBOT,
    MD_SERVER,
    MD_SKIP_NEXT,
    MD_SKIP_PREVIOUS,
    MD_SSH,
    MD_STAR,
    MD_STOP,
    MD_SWAP_HORIZONTAL,
    MD_SYNC,
    MD_TELEVISION,
    MD_USB,
    MD_VOLUME_HIGH,
    MD_VOLUME_MEDIUM,
    // Weather
    MD_WEATHER_PARTLY_CLOUDY,
    // Network
    MD_WIFI,
};

// ── Seti-UI + Custom (file-type icons) ─────────────────────────────────────
pub use nerd_font_symbols::seti::{
    // Folder variants
    CUSTOM_FOLDER,
    CUSTOM_FOLDER_CONFIG,
    CUSTOM_FOLDER_GIT,
    CUSTOM_FOLDER_GITHUB,
    CUSTOM_FOLDER_NPM,
    // Media file types
    SETI_AUDIO,
    SETI_DEFAULT,
    SETI_DOCKER,
    // Status icons
    SETI_GIT,
    SETI_IMAGE,
    SETI_PDF,
    SETI_VIDEO,
};

// ── Codicons (nerdicons_rs cod set) ─────────────────────────────────────────
pub use nerdicons_rs::cod::{RSFILE, RSFOLDER, RSFOLDER_OPENED, RSMUSIC};

// ── Semantic aliases — use these in panel files ─────────────────────────────

/// Active / connected / success indicator
pub const ICON_ACTIVE: &str = MD_CHECK_CIRCLE;
/// Inactive / disconnected indicator
pub const ICON_INACTIVE: &str = MD_CLOSE;
/// Unknown / pending indicator
pub const ICON_UNKNOWN: &str = MD_HELP_CIRCLE;

/// Notification levels
pub const ICON_INFO: &str = MD_INFORMATION;
pub const ICON_SUCCESS: &str = MD_CHECK;
pub const ICON_WARNING: &str = MD_ALERT;
pub const ICON_ERROR: &str = MD_ALERT_CIRCLE;

/// Playback
pub const ICON_PLAY: &str = MD_PLAY;
pub const ICON_PAUSE: &str = MD_PAUSE;
pub const ICON_NEXT: &str = MD_SKIP_NEXT;
pub const ICON_PREV: &str = MD_SKIP_PREVIOUS;
pub const ICON_MUSIC_NOTE: &str = MD_MUSIC;
pub const ICON_VOLUME: &str = MD_VOLUME_HIGH;

/// System metrics
pub const ICON_CPU: &str = MD_CHIP;
pub const ICON_GPU: &str = MD_EXPANSION_CARD;
pub const ICON_RAM: &str = MD_MEMORY;
pub const ICON_DISK: &str = MD_HARDDISK;
pub const ICON_SWAP: &str = MD_SWAP_HORIZONTAL;
pub const ICON_USB: &str = MD_USB;
pub const ICON_LAN: &str = MD_LAN;
pub const ICON_NETWORK_RX: &str = MD_ARROW_DOWN;
pub const ICON_NETWORK_TX: &str = MD_ARROW_UP;
pub const ICON_NETWORK: &str = MD_NETWORK;
pub const ICON_UPTIME: &str = MD_CLOCK;

/// Modes (status bar)
pub const ICON_MODE_MENU: &str = MD_HOME;
pub const ICON_MODE_FILES: &str = MD_FOLDER;
pub const ICON_MODE_EDITOR: &str = MD_PENCIL;
pub const ICON_MODE_TERMINAL: &str = MD_CONSOLE;
pub const ICON_MODE_PROCESS: &str = MD_CHIP;
pub const ICON_MODE_GIT: &str = MD_GIT;
pub const ICON_MODE_CONFIG: &str = MD_COG;
pub const ICON_MODE_AUDIO: &str = MD_HEADPHONES;
pub const ICON_MODE_VIDEO: &str = MD_TELEVISION;
pub const ICON_MODE_IMAGE: &str = SETI_IMAGE;
pub const ICON_MODE_PDF: &str = SETI_PDF;
pub const ICON_MODE_FAVORITES: &str = MD_STAR;
pub const ICON_MODE_HELP: &str = MD_HELP_CIRCLE;
pub const ICON_MODE_THEMES: &str = MD_PALETTE;
pub const ICON_MODE_LOGS: &str = MD_HISTORY;
pub const ICON_MODE_SERVICES: &str = MD_SERVER;
pub const ICON_MODE_NETWORK: &str = MD_WIFI;
pub const ICON_MODE_DISK: &str = MD_HARDDISK;
pub const ICON_MODE_CALC: &str = MD_CALCULATOR;
pub const ICON_MODE_PACKAGES: &str = MD_PACKAGE;
pub const ICON_MODE_SSH: &str = MD_SSH;
pub const ICON_MODE_SFTP: &str = MD_DOWNLOAD_NETWORK;
pub const ICON_MODE_DOCKER: &str = MD_DOCKER;
pub const ICON_MODE_CRON: &str = MD_CALENDAR;
pub const ICON_MODE_MAN: &str = MD_BOOK_OPEN;
pub const ICON_MODE_NOTES: &str = MD_NOTE_TEXT;
pub const ICON_MODE_WEATHER: &str = MD_WEATHER_PARTLY_CLOUDY;
pub const ICON_MODE_CALENDAR: &str = MD_CALENDAR;
pub const ICON_MODE_PALETTE: &str = MD_MAGNIFY;
pub const ICON_MODE_CMD: &str = MD_CONSOLE;
pub const ICON_MODE_DIALOG: &str = MD_ALERT;
pub const ICON_MODE_QUIT: &str = MD_POWER;

/// Favorites
pub const ICON_STAR: &str = MD_STAR;
pub const ICON_DIR: &str = RSFOLDER;
pub const ICON_FILE_GENERIC: &str = RSFILE;

/// List highlight arrow
pub const ICON_HIGHLIGHT: &str = MD_PLAY;
