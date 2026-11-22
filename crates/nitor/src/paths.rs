//! Теки програми: `%LOCALAPPDATA%\Nitor` (програма й дані), `config.json`, `nitor.log`.

use std::path::PathBuf;

pub use nitor_core::app_dir;

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn log_path() -> PathBuf {
    app_dir().join("nitor.log")
}
