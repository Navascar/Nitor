//! Теки програми: `%LOCALAPPDATA%\Nitor` (програма й дані), `config.json`, `nitor.log`.

use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NITOR_HOME") {
        return PathBuf::from(d);
    }
    #[cfg(windows)]
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(d).join("Nitor");
    }
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Nitor")
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn log_path() -> PathBuf {
    app_dir().join("nitor.log")
}
