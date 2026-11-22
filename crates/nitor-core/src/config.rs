//! Налаштування `config.json` (формат версії 2): вихід, майстер, профілі, прилади, сцени,
//! статичні кольори, гарячі клавіші. Запис атомарний (через `config.tmp`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::engine::{Engine, Prefs, Scene, StaticColor, check_hotkeys, default_static_colors};
use crate::fixtures::{Fixture, Profiles, default_profiles, has_presets, preset_count};
use crate::library::{RECENT_LIMIT, Recent};
use crate::outputs::OutputConfig;
use crate::{Result, bail};

pub const CONFIG_VERSION: u32 = 2;

fn d255() -> u8 {
    255
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub version: u32,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default = "d255")]
    pub master: u8,
    /// Власні профілі та профілі моделей, якими користуються прилади.
    #[serde(default)]
    pub profiles: Profiles,
    #[serde(default)]
    pub device_profiles: BTreeSet<String>,
    #[serde(default)]
    pub recent_models: Vec<Recent>,
    #[serde(default)]
    pub fixtures: Vec<Fixture>,
    #[serde(default)]
    pub scenes: IndexMap<String, Scene>,
    /// Стандартні кольори й ті, що зберіг користувач (стандартні під час читання беруться з програми).
    #[serde(default = "default_static_colors")]
    pub static_colors: Vec<StaticColor>,
    #[serde(default)]
    pub prefs: Prefs,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub hotkeys: IndexMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub groups: IndexMap<String, Vec<String>>,
    #[serde(default)]
    pub audio: crate::tempo::AudioSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi: Vec<crate::midi::MidiBinding>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub operator: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

impl ConfigFile {
    pub fn parse(text: &str) -> Result<ConfigFile> {
        let cfg: ConfigFile = serde_json::from_str(text)?;
        if cfg.version != CONFIG_VERSION {
            bail!("непідтримувана версія налаштувань {}", cfg.version);
        }
        Ok(cfg)
    }
}

/// Мітка часу `РРРРММДД-ГГХХСС` (UTC) для назви відкладеного файлу.
fn stamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

impl Engine {
    /// Рушій з налаштуваннями з файлу. Зіпсований файл відкладається вбік
    /// (`config.broken-*.json`), а програма стартує з чистими налаштуваннями.
    pub fn load(path: impl Into<PathBuf>) -> Engine {
        let path = path.into();
        let mut e = Engine::new();
        e.config_path = Some(path.clone());
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return e, // перший запуск
            Err(err) => {
                log::error!("Не вдалося прочитати {}: {err}", path.display());
                String::new()
            }
        };
        match ConfigFile::parse(&text) {
            Ok(cfg) => e.apply_config(cfg),
            Err(err) => {
                let broken = path.with_file_name(format!("config.broken-{}.json", stamp()));
                let name = broken.file_name().unwrap_or_default().to_string_lossy().to_string();
                if let Err(re) = std::fs::rename(&path, &broken) {
                    // файл не прочитався й не перейменувався (наприклад, його тримає інша програма):
                    // не зберігаємо нічого, інакше порожні налаштування затруть справжні
                    log::error!("Не вдалося прочитати чи відкласти файл налаштувань ({err}; {re})");
                    e.config_path = None;
                    e.load_warning = Some(format!(
                        "Не вдалося відкрити файл налаштувань {}. Програма працює з чистими налаштуваннями, \
                         і зміни не збережуться. Закрийте Nitor і відкрийте знову.",
                        path.display()
                    ));
                    return e;
                }
                log::error!("Файл налаштувань пошкоджено ({err}), його збережено як {name}");
                e.load_warning = Some(format!(
                    "Файл налаштувань був пошкоджений, тож програма почала з чистих налаштувань. \
                     Старий файл збережено як {name} у теці програми."
                ));
            }
        }
        e
    }

    pub fn apply_config(&mut self, cfg: ConfigFile) {
        let s = &mut self.show;
        s.profiles = default_profiles().clone();
        s.profiles.extend(cfg.profiles);
        s.device_profiles = cfg.device_profiles.into_iter().filter(|p| s.profiles.contains_key(p)).collect();
        self.recent_models = cfg.recent_models.into_iter().take(RECENT_LIMIT).collect();
        self.show.fixtures = cfg.fixtures;
        let names: Vec<String> = self.show.fixtures.iter().map(|f| f.profile.clone()).collect();
        for n in names {
            self.ensure_profile(&n);
        }
        let profiles = self.show.profiles.clone();
        for f in &mut self.show.fixtures {
            f.model = f.model.take().and_then(crate::Model::valid);
            let presets_ok = profiles.get(&f.profile).is_some_and(|c| has_presets(c));
            if f.preset.is_some_and(|p| !(1..=preset_count()).contains(&p) || !presets_ok) {
                f.preset = None;
            }
        }
        self.show.scenes = cfg.scenes;
        // палітра — завжди стандартний набір (новіші версії можуть його розширити) і після нього
        // кольори, які зберіг користувач; кольори старого стандартного набору вже є серед стандартних
        let mut colors = default_static_colors();
        for c in cfg.static_colors {
            if colors.len() < crate::engine::STATIC_COLORS_MAX && !colors.iter().any(|d| d.hex == c.hex && d.w == c.w) {
                colors.push(c);
            }
        }
        self.show.static_colors = colors;
        self.prefs = cfg.prefs.clamped();
        self.output_cfg = cfg.output;
        self.show.master = cfg.master;
        self.hotkeys = check_hotkeys(cfg.hotkeys).unwrap_or_default();
        self.show.groups = self.check_groups(cfg.groups).unwrap_or_default();
        self.operator = cfg.operator;
        self.audio = cfg.audio;
        self.midi = crate::midi::check(cfg.midi).unwrap_or_default();
        self.pin = cfg.pin.filter(|p| p.contains(':'));
    }

    /// Налаштування для запису: лише власні профілі та профілі моделей, якими користуються прилади.
    pub fn to_config(&self) -> ConfigFile {
        let used: BTreeSet<&str> = self.show.fixtures.iter().map(|f| f.profile.as_str()).collect();
        let defaults = default_profiles();
        let custom: Profiles = self
            .show
            .profiles
            .iter()
            .filter(|(k, v)| defaults.get(*k) != Some(*v) && (used.contains(k.as_str()) || !self.is_device(k)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        ConfigFile {
            version: CONFIG_VERSION,
            output: self.output_cfg.clone(),
            master: self.show.master,
            device_profiles: custom.keys().filter(|k| self.is_device(k)).cloned().collect(),
            profiles: custom,
            recent_models: self.recent_models.clone(),
            fixtures: self.show.fixtures.clone(),
            scenes: self.show.scenes.clone(),
            static_colors: self.show.static_colors.clone(),
            prefs: self.prefs.clone(),
            hotkeys: self.hotkeys.clone(),
            groups: self.show.groups.clone(),
            operator: self.operator,
            audio: self.audio,
            midi: self.midi.clone(),
            pin: self.pin.clone(),
        }
    }

    /// Зберігає налаштування у `config_path` (якщо задано).
    pub fn save(&mut self, now: f64) -> Result<()> {
        if let Some(path) = self.config_path.clone() {
            write_json(&path, &serde_json::to_value(self.to_config())?)?;
        }
        self.mark_saved(now);
        Ok(())
    }
}

/// Атомарний запис JSON: спершу `*.tmp`, потім перейменування.
pub fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    let tmp = path.with_extension("tmp");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::File::create(&tmp)?;
    std::io::Write::write_all(&mut f, serde_json::to_string_pretty(value)?.as_bytes())?;
    // на диск до перейменування: інакше після вимкнення живлення файл може лишитися порожнім
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_format() {
        let s = stamp();
        assert_eq!(s.len(), 15);
        assert_eq!(&s[8..9], "-");
        assert!(s.starts_with("20"));
    }
}
