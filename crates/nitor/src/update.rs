//! Стан оновлення (перевірка й встановлення — етап 4).

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    Installing,
    Restarting,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateInfo {
    pub status: Status,
    /// Перевірка завершилась (будь-як): вікно запуску чекає саме на це.
    pub checked: bool,
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub url: Option<String>,
    pub page: String,
    pub error: String,
    pub progress: String,
    /// Чому автооновлення недоступне.
    pub blocked: Option<String>,
    pub offline: bool,
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<UpdateInfo>>,
}

impl Updater {
    pub fn new() -> Updater {
        let info = UpdateInfo {
            current: nitor_core::VERSION.into(),
            page: format!("https://github.com/{}/releases", nitor_core::GITHUB_REPO),
            ..Default::default()
        };
        Updater { state: Arc::new(Mutex::new(info)) }
    }

    pub fn info(&self) -> UpdateInfo {
        self.state.lock().unwrap().clone()
    }

    pub fn check(&self) {}

    pub fn apply(&self) {}
}
