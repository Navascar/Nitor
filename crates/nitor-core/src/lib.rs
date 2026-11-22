//! Рушій Nitor: стан шоу, прилади й профілі, ефекти, сцени з кросфейдом, рендер DMX,
//! бібліотека моделей, збереження налаштувань і DMX-виходи.
//!
//! Без інтерфейсу: програма (`nitor`) лише надсилає [`Command`] у [`Engine`] і читає його стан.

pub mod color;
pub mod config;
pub mod effects;
pub mod engine;
pub mod error;
pub mod fixtures;
pub mod library;
pub mod midi;
pub mod outputs;
pub mod rng;
pub mod scene_file;
pub mod tempo;
pub mod userdata;

pub use color::Rgb;
pub use effects::{Effect, EffectKind};
pub use engine::{Command, Engine};
pub use error::{Error, Result};
pub use fixtures::{Fixture, Model};

/// Кількість каналів DMX-всесвіту.
pub const DMX_CHANNELS: usize = 512;
/// Версія програми (з Cargo.toml workspace).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "Navascar/Nitor";

/// Тека програми й даних: `NITOR_HOME`, у Windows `%LOCALAPPDATA%\Nitor`, інакше `~/.local/share/Nitor`.
/// Одна на програму й інсталятор — інакше вони шукали б одне одного в різних місцях.
pub fn app_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(d) = var("NITOR_HOME") {
        return d;
    }
    if let Some(d) = var("LOCALAPPDATA").filter(|_| cfg!(windows)) {
        return d.join("Nitor");
    }
    let data = var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")));
    data.unwrap_or_else(|| PathBuf::from(".")).join("Nitor")
}
