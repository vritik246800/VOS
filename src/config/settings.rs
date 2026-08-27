use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme: String,
    pub show_hidden: bool,
    pub mouse_enabled: bool,
    pub autosave: bool,
    pub tab_width: u8,
    pub notification_timeout: u8,
    pub terminal_scrollback: usize,
    pub music_dir: String,
    #[serde(default = "default_notes_dir")]
    pub notes_dir: String,
    // Weather location (Phase 6.5)
    #[serde(default = "default_latitude")]
    pub latitude: f64,
    #[serde(default = "default_longitude")]
    pub longitude: f64,
    #[serde(default = "default_location_name")]
    pub location_name: String,
    /// Show weather temperatures in Fahrenheit instead of Celsius.
    #[serde(default)]
    pub fahrenheit: bool,
}

fn default_notes_dir() -> String {
    "~/Notes".to_string()
}

fn default_latitude() -> f64 {
    38.7223
}

fn default_longitude() -> f64 {
    -9.1393
}

fn default_location_name() -> String {
    "Lisbon".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_hidden: false,
            mouse_enabled: true,
            autosave: false,
            tab_width: 4,
            notification_timeout: 60,
            terminal_scrollback: 2000,
            music_dir: "~/Music".to_string(),
            notes_dir: default_notes_dir(),
            latitude: default_latitude(),
            longitude: default_longitude(),
            location_name: default_location_name(),
            fahrenheit: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content).unwrap_or_default())
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
