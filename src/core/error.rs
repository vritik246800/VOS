#![allow(dead_code)]
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VosError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("No active buffer")]
    NoActiveBuffer,

    #[error("No active tab")]
    NoActiveTab,
}
