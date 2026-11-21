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
pub mod outputs;
pub mod rng;
pub mod scene_file;
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
pub const APP_NAME: &str = "Nitor";
pub const GITHUB_REPO: &str = "Navascar/Nitor";
