#![allow(dead_code)]
use crate::db::sqlite::Database;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub current_dir: String,
    pub open_tabs: Vec<String>,
    pub theme: String,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            current_dir: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            open_tabs: Vec::new(),
            theme: "dark".to_string(),
        }
    }
}

pub struct SessionManager;

impl SessionManager {
    pub fn save(db: &Database, name: &str, data: &SessionData) -> Result<()> {
        let json = toml::to_string(data).map_err(|e| anyhow::anyhow!("session serialise: {e}"))?;
        db.save_session(name, &json)?;
        Ok(())
    }

    pub fn load(db: &Database, name: &str) -> Result<Option<SessionData>> {
        if let Some(json) = db.load_session(name)? {
            let data =
                toml::from_str(&json).map_err(|e| anyhow::anyhow!("session deserialise: {e}"))?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
}
