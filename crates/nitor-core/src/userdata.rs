//! Вибіркове видалення даних користувача з config.json (для вікна видалення програми).
//!
//! Частини: сцени; прилади (разом із профілями їхніх моделей і нещодавніми моделями); власні
//! профілі (крім тих, якими користуються прилади, що лишаються); статичні кольори; налаштування
//! (DMX-інтерфейс, яскравість, гарячі клавіші). Якщо нічого не лишилося, файл видаляється.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value};

use crate::Result;
use crate::config::write_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    Scenes,
    Fixtures,
    Profiles,
    Colors,
    Settings,
}

impl Part {
    pub const ALL: [Part; 5] = [Part::Scenes, Part::Fixtures, Part::Profiles, Part::Colors, Part::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Part::Scenes => "Сцени",
            Part::Fixtures => "Прилади",
            Part::Profiles => "Власні профілі",
            Part::Colors => "Статичні кольори",
            Part::Settings => "Налаштування",
        }
    }
}

/// Копія налаштувань без вибраних частин.
pub fn strip(cfg: &Map<String, Value>, parts: &BTreeSet<Part>) -> Map<String, Value> {
    let mut cfg = cfg.clone();
    let mut profiles = cfg.get("profiles").and_then(Value::as_object).cloned().unwrap_or_default();
    let device: BTreeSet<String> = cfg
        .get("device_profiles")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if parts.contains(&Part::Scenes) {
        cfg.remove("scenes");
    }
    if parts.contains(&Part::Fixtures) {
        for k in ["fixtures", "recent_models", "device_profiles"] {
            cfg.remove(k);
        }
        profiles.retain(|k, _| !device.contains(k));
    }
    if parts.contains(&Part::Profiles) {
        // профілі, на яких працюють прилади, що лишаються, не видаляємо — інакше ці прилади згаснуть
        let used: BTreeSet<String> = cfg
            .get("fixtures")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|f| f.get("profile")?.as_str().map(String::from)).collect())
            .unwrap_or_default();
        profiles.retain(|k, _| device.contains(k) || used.contains(k));
    }
    if parts.contains(&Part::Colors) {
        cfg.remove("static_colors");
    }
    if parts.contains(&Part::Settings) {
        for k in ["output", "master", "hotkeys", "prefs"] {
            cfg.remove(k);
        }
    }
    if cfg.contains_key("profiles") || !profiles.is_empty() {
        cfg.insert("profiles".into(), Value::Object(profiles));
    }
    cfg
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn empty(cfg: &Map<String, Value>) -> bool {
    !cfg.iter().any(|(k, v)| !matches!(k.as_str(), "device_profiles" | "version") && truthy(v))
}

/// Видаляє частини з файлу налаштувань. `true` — файл видалено повністю.
pub fn apply(path: &Path, parts: &BTreeSet<Part>) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    if Part::ALL.iter().all(|p| parts.contains(p)) {
        std::fs::remove_file(path)?;
        return Ok(true);
    }
    let cfg: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let cfg = strip(cfg.as_object().ok_or_else(|| crate::err!("налаштування — не об'єкт JSON"))?, parts);
    if empty(&cfg) {
        std::fs::remove_file(path)?;
        return Ok(true);
    }
    write_json(path, &Value::Object(cfg))?;
    Ok(false)
}
