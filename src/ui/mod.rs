/// Cut a label to `max` characters. `&s[..n]` panics on a multi-byte boundary —
/// volume labels, interface and process names are not all ASCII.
pub fn trunc(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

pub mod audio_panel;
pub mod calc_panel;
pub mod calendar_panel;
pub mod command_palette;
pub mod config_panel;
pub mod cron_panel;
pub mod disk_panel;
pub mod docker_panel;
pub mod editor_panel;
pub mod favorites_panel;
pub mod file_panel;
pub mod help_panel;
pub mod icons;
pub mod image_panel;
pub mod layout;
pub mod log_panel;
pub mod man_panel;
pub mod md_panel;
pub mod menu;
pub mod music_panel;
pub mod network_panel;
pub mod notes_panel;
pub mod notifications;
pub mod packages_panel;
pub mod pdf_panel;
pub mod process_panel;
pub mod service_panel;
pub mod sftp_panel;
pub mod splash;
pub mod ssh_panel;
pub mod status_bar;
pub mod sysmon_panel;
pub mod tabs;
pub mod terminal_panel;
pub mod theme;
pub mod theme_switcher;
pub mod video_panel;
pub mod weather_panel;
