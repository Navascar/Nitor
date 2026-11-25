//! Рушій: стан шоу, команди, скасування, сцени з кросфейдом і рендер кадрів.
//!
//! Час передається явно (`now` — секунди монотонного годинника), тож рушій детермінований
//! і перевіряється тестами без очікування.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::effects::{Effect, EffectKind, PALETTE_MAX, SPEED_MAX, SPEED_MIN, beat_pulse, effect_color};
use crate::fixtures::{
    Color, Fixture, Model, Profiles, Segs, default_profiles, find_overlaps, has_presets, model_profiles, preset_count,
    preset_level, programs, render_fixture, segment_count, validate_fixture, validate_profile,
};
use crate::library::{RECENT_LIMIT, Recent};
use crate::outputs::OutputConfig;
use crate::tempo::{AudioSource, Tempo};
use crate::{DMX_CHANNELS, Error, Result, Rgb, bail, err};

pub const FPS: f64 = 40.0;
pub const UNDO_LIMIT: usize = 100;
/// Серія однакових команд (рух повзунка) — один крок скасування.
pub const UNDO_MERGE_S: f64 = 1.0;
/// Найбільша частота мерехтіння «Спалаху», Гц.
pub const FLASH_RATE_MAX: f64 = 25.0;
/// Найбільше кольорів у палітрі (стандартні + збережені користувачем).
pub const STATIC_COLORS_MAX: usize = 48;
/// Ключі стану приладу, що зберігаються в сцені й плавно змінюються під час кросфейду.
pub const STATE_KEYS: [&str; 7] = ["r", "g", "b", "w", "a", "uv", "dimmer"];

/// Статичний колір. `hex` — для RGB-приладів; для приладів з білим каналом
/// використовується `hex_w` + `w` (чистіший білий від W-світлодіода).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticColor {
    pub name: String,
    pub hex: Rgb,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex_w: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<u8>,
}

pub fn default_static_colors() -> Vec<StaticColor> {
    let c = |name: &str, hex: &str| StaticColor {
        name: name.into(),
        hex: Rgb::from_hex(hex).unwrap(),
        hex_w: None,
        w: None,
    };
    let white = |name: &str, hex: &str, hex_w: &str| StaticColor {
        name: name.into(),
        hex: Rgb::from_hex(hex).unwrap(),
        hex_w: Some(Rgb::from_hex(hex_w).unwrap()),
        w: Some(255),
    };
    vec![
        c("Червоний", "#ff0000"),
        c("Помаранчевий", "#ff5000"),
        c("Жовтий", "#ffb400"),
        c("Зелений", "#00ff00"),
        c("Бірюзовий", "#00ffb4"),
        c("Блакитний", "#00b4ff"),
        c("Синій", "#0000ff"),
        c("Фіолетовий", "#8000ff"),
        c("Пурпуровий", "#ff00ff"),
        c("Рожевий", "#ff3c78"),
        white("Теплий білий", "#ffa050", "#783200"),
        white("Білий", "#ffffff", "#000000"),
        white("Холодний білий", "#d2e6ff", "#002878"),
        c("Малиновий", "#ff0040"),
        c("Бурштиновий", "#ff8000"),
        c("Лимонний", "#ffe600"),
        c("Салатовий", "#80ff00"),
        c("Смарагдовий", "#00ff60"),
        c("Аквамарин", "#40ffc0"),
        c("Небесний", "#60c0ff"),
        c("Індиго", "#4000ff"),
        c("Лавандовий", "#b080ff"),
        c("Ніжно-рожевий", "#ff80c0"),
        c("Персиковий", "#ff9060"),
    ]
}

fn d255() -> u8 {
    255
}
fn d128() -> u8 {
    128
}

/// Стан приладу в сцені.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    #[serde(default)]
    pub r: u8,
    #[serde(default)]
    pub g: u8,
    #[serde(default)]
    pub b: u8,
    #[serde(default)]
    pub w: u8,
    #[serde(default)]
    pub a: u8,
    #[serde(default)]
    pub uv: u8,
    #[serde(default = "d255")]
    pub dimmer: u8,
    #[serde(default)]
    pub segs: Option<Segs>,
    #[serde(default)]
    pub preset: Option<u8>,
    #[serde(default = "d128")]
    pub speed: u8,
}

impl SceneState {
    pub fn of(f: &Fixture) -> SceneState {
        SceneState {
            r: f.r,
            g: f.g,
            b: f.b,
            w: f.w,
            a: f.a,
            uv: f.uv,
            dimmer: f.dimmer,
            segs: f.segs.clone(),
            preset: f.preset,
            speed: f.speed,
        }
    }

    pub fn get(&self, key: &str) -> u8 {
        match key {
            "r" => self.r,
            "g" => self.g,
            "b" => self.b,
            "w" => self.w,
            "a" => self.a,
            "uv" => self.uv,
            "dimmer" => self.dimmer,
            _ => 0,
        }
    }

    /// Кольори n сегментів (для кросфейду).
    fn seg_rgbs(&self, n: usize) -> Vec<[u8; 3]> {
        let base = [self.r, self.g, self.b];
        (0..n).map(|k| self.segs.as_ref().and_then(|s| s.get(k).copied().flatten()).unwrap_or(base)).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub fixtures: IndexMap<String, SceneState>,
    #[serde(default)]
    pub effect: Option<Effect>,
}

/// Усе, що зберігається в кроці скасування.
#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub fixtures: Vec<Fixture>,
    pub master: u8,
    pub blackout: bool,
    pub effect: Effect,
    pub scenes: IndexMap<String, Scene>,
    pub static_colors: Vec<StaticColor>,
    pub profiles: Profiles,
    /// Профілі, що належать моделям приладів (обираються разом із приладом, не в меню).
    pub device_profiles: BTreeSet<String>,
    /// Групи приладів: назва → прилади. Група — збережене виділення.
    pub groups: IndexMap<String, Vec<String>>,
}

impl Default for Show {
    fn default() -> Self {
        Show {
            fixtures: vec![],
            master: 255,
            blackout: false,
            effect: Effect::default(),
            scenes: IndexMap::new(),
            static_colors: default_static_colors(),
            profiles: default_profiles().clone(),
            device_profiles: BTreeSet::new(),
            groups: IndexMap::new(),
        }
    }
}

/// Налаштування вигляду й «Спалаху». Зберігаються, не скасовуються.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    /// Праворуч спершу «Сцени», під ними «Ефекти».
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scenes_first: bool,
    /// Біля назви програми — ще й назва пристрою DMX-виходу.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub output_device: bool,
    /// Мерехтіння «Спалаху», Гц: 0 — постійне світло, більше — строб.
    #[serde(default)]
    pub flash_rate: f64,
    /// Не показувати підказки клавіш на кнопках.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_keys: bool,
    /// Прилади показуються планом сцени, а не сіткою.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    /// Не показувати знаки «?» з поясненнями.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_help: bool,
    /// Порядок стовпців головного вікна (`fixtures`, `color`, `side`); порожньо — стандартний.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// Ширина стовпців у пікселях; без запису — стандартна частка вікна.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub col_width: BTreeMap<String, f32>,
    /// Кнопки верхньої панелі без назви, лише значок (`clear`, `flash`, `blackout`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub short_buttons: Vec<String>,
    /// Не показувати DMX-монітор унизу вікна.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_monitor: bool,
    /// Масштаб інтерфейсу відносно масштабу Windows: 0 — як у Windows.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub zoom: f32,
    /// Плавність «Очистити», секунди: 0 — одразу.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub clear_fade: f64,
    /// Плавність Blackout (згасання й повернення світла), секунди: 0 — одразу.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub blackout_fade: f64,
    /// Крива переходу між сценами.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade_curve: FadeCurve,
    /// «Зменшити рух»: `None` — як у налаштуваннях системи.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduce_motion: Option<bool>,
}

/// Як світло рухається під час переходу до сцени.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    /// рівномірно
    #[default]
    Linear,
    /// спершу повільно, наприкінці швидко
    EaseIn,
    /// спершу швидко, наприкінці повільно
    EaseOut,
}

impl FadeCurve {
    /// Частка переходу 0..1 з урахуванням кривої.
    pub fn apply(self, k: f64) -> f64 {
        let k = k.clamp(0.0, 1.0);
        match self {
            FadeCurve::Linear => k,
            FadeCurve::EaseIn => k * k * k,
            FadeCurve::EaseOut => 1.0 - (1.0 - k).powi(3),
        }
    }
}

fn is_zero<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Найдовша плавність «Очистити» й Blackout, секунди.
pub const FADE_MAX: f64 = 10.0;

/// Масштаб інтерфейсу: від 50 % до 300 % від масштабу Windows.
pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.5..=3.0;

impl Prefs {
    pub fn clamped(self) -> Prefs {
        let rate = if self.flash_rate.is_finite() { self.flash_rate.clamp(0.0, FLASH_RATE_MAX) } else { 0.0 };
        let mut col_width = self.col_width;
        col_width.retain(|_, w| w.is_finite() && *w > 0.0);
        let zoom = if self.zoom.is_finite() && self.zoom != 0.0 {
            self.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end())
        } else {
            0.0
        };
        let fade = |v: f64| if v.is_finite() { v.clamp(0.0, FADE_MAX) } else { 0.0 };
        let (clear_fade, blackout_fade) = (fade(self.clear_fade), fade(self.blackout_fade));
        Prefs { flash_rate: rate, col_width, zoom, clear_fade, blackout_fade, ..self }
    }
}

/// Що показує «Спалах», поки його тримають.
#[derive(Debug, Clone, PartialEq)]
pub enum FlashSource {
    /// Колір на вибрані прилади (порожньо — на всі).
    Color {
        ids: Vec<String>,
        source: StaticSource,
    },
    Scene(String),
}

/// Значення для `SetColor`: лише задані поля змінюються.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorValues {
    pub r: Option<u8>,
    pub g: Option<u8>,
    pub b: Option<u8>,
    pub w: Option<u8>,
    pub a: Option<u8>,
    pub uv: Option<u8>,
    pub dimmer: Option<u8>,
    pub strobe: Option<u8>,
    /// Колір RGB одним значенням (перекриває r, g, b).
    pub hex: Option<Rgb>,
}

impl ColorValues {
    fn pairs(&self) -> Vec<(&'static str, u8)> {
        let mut out: Vec<(&'static str, u8)> = [
            ("r", self.r),
            ("g", self.g),
            ("b", self.b),
            ("w", self.w),
            ("a", self.a),
            ("uv", self.uv),
            ("dimmer", self.dimmer),
            ("strobe", self.strobe),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect();
        if let Some(h) = self.hex {
            for (k, v) in [("r", h.0), ("g", h.1), ("b", h.2)] {
                match out.iter_mut().find(|(key, _)| *key == k) {
                    Some(p) => p.1 = v,
                    None => out.push((k, v)),
                }
            }
        }
        out
    }

    fn keys(&self) -> String {
        let mut k: Vec<&str> = self.pairs().iter().map(|p| p.0).collect();
        if self.hex.is_some() {
            k.push("hex");
        }
        k.sort();
        k.join(",")
    }
}

/// Звідки береться профіль приладу.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileSpec {
    /// Назва профілю.
    pub profile: String,
    /// Канали профілю моделі з бібліотеки (разом з `model`).
    pub channels: Option<Vec<String>>,
    pub model: Option<Model>,
}

impl ProfileSpec {
    pub fn named(name: impl Into<String>) -> ProfileSpec {
        ProfileSpec { profile: name.into(), ..Default::default() }
    }
}

/// Звідки береться статичний колір.
#[derive(Debug, Clone, PartialEq)]
pub enum StaticSource {
    /// Номер збереженого кольору.
    Index(usize),
    Custom {
        hex: Rgb,
        hex_w: Option<Rgb>,
        w: u8,
    },
}

/// Зміни ефекту: лише задані поля.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectPatch {
    pub kind: Option<EffectKind>,
    pub ids: Option<Vec<String>>,
    pub speed: Option<f64>,
    pub spread: Option<f64>,
    pub colors: Option<Vec<Rgb>>,
    pub by_segment: Option<bool>,
    pub beat: Option<bool>,
    pub beat_pulse: Option<bool>,
    pub order: Option<crate::effects::Order>,
}

/// Команда від інтерфейсу. Вибір `ids`: `"id"` — увесь прилад, `"id#k"` — сегмент k.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Undo,
    Redo,
    SetColor {
        ids: Vec<String>,
        values: ColorValues,
    },
    /// Вбудована програма: `n` = `Some(Some(1..N))` — увімкнути, `Some(None)` — вимкнути,
    /// `None` — не змінювати (лише швидкість).
    Preset {
        ids: Vec<String>,
        n: Option<Option<u8>>,
        speed: Option<u8>,
    },
    Clear {
        ids: Vec<String>,
    },
    StaticColor {
        ids: Vec<String>,
        source: StaticSource,
    },
    /// Зберегти колір у палітру (після стандартних).
    AddStaticColor {
        name: String,
        hex: Rgb,
        w: u8,
    },
    /// Видалити збережений користувачем колір (стандартні не видаляються).
    DeleteStaticColor {
        index: usize,
    },
    SetMaster(u8),
    SetHotkeys(IndexMap<String, Vec<String>>),
    Blackout(Option<bool>),
    AddFixture {
        spec: ProfileSpec,
        name: String,
        address: u16,
        count: u32,
    },
    UpdateFixture {
        id: String,
        name: Option<String>,
        address: Option<u16>,
        profile: Option<ProfileSpec>,
    },
    DeleteFixture {
        id: String,
    },
    MoveFixture {
        id: String,
        delta: i32,
    },
    /// Місце приладу на плані сцени; `None` — прибрати з плану.
    PlaceFixture {
        id: String,
        pos: Option<[i16; 2]>,
    },
    SaveProfile {
        name: String,
        channels: Vec<String>,
        assign_to: Option<String>,
    },
    RawSet {
        address: u16,
        value: u8,
    },
    /// `None` — очистити всі.
    RawClear(Option<Vec<u16>>),
    DeleteProfile {
        name: String,
    },
    SetEffect(EffectPatch),
    SaveScene {
        name: String,
        with_effect: bool,
    },
    RecallScene {
        name: String,
        fade: f64,
    },
    DeleteScene {
        name: String,
    },
    /// Сцена з файлу `.nitor` іншого комп'ютера.
    ImportScene {
        data: serde_json::Value,
        name: Option<String>,
    },
    SetOutput(OutputConfig),
    /// Удар для «у такт музиці»: тап, звук чи OS2L (момент — годинник рушія).
    Beat(f64),
    SetAudio(AudioSource),
    SetMidi(Vec<crate::midi::MidiBinding>),
    SetSchedule(Vec<crate::schedule::Event>),
    SetPlace(Option<crate::schedule::Place>),
    /// Сервер OS2L для VirtualDJ: порт або `None` — вимкнено.
    SetOs2l(Option<u16>),
    /// Пульт з телефона: порт або `None` — вимкнено (працює лише із заданим PIN).
    SetRemote(Option<u16>),
    /// `Some` — «Спалах» натиснули, `None` — відпустили.
    Flash(Option<FlashSource>),
    SetGroups(IndexMap<String, Vec<String>>),
    /// Режим оператора: вмикається одразу, вимикається лише з правильним PIN (якщо його задано).
    Operator {
        on: bool,
        pin: String,
    },
    /// Новий PIN режиму оператора; порожній — без PIN.
    SetPin(String),
    SetPrefs(Prefs),
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::SetColor { .. } => "set_color",
            Command::Preset { .. } => "preset",
            Command::Clear { .. } => "clear",
            Command::StaticColor { .. } => "static_color",
            Command::AddStaticColor { .. } => "add_static_color",
            Command::DeleteStaticColor { .. } => "delete_static_color",
            Command::SetMaster(_) => "set_master",
            Command::SetHotkeys(_) => "set_hotkeys",
            Command::Blackout(_) => "blackout",
            Command::AddFixture { .. } => "add_fixture",
            Command::UpdateFixture { .. } => "update_fixture",
            Command::DeleteFixture { .. } => "delete_fixture",
            Command::MoveFixture { .. } => "move_fixture",
            Command::PlaceFixture { .. } => "place_fixture",
            Command::SaveProfile { .. } => "save_profile",
            Command::RawSet { .. } => "raw_set",
            Command::RawClear(_) => "raw_clear",
            Command::DeleteProfile { .. } => "delete_profile",
            Command::SetEffect(_) => "set_effect",
            Command::SaveScene { .. } => "save_scene",
            Command::RecallScene { .. } => "recall_scene",
            Command::DeleteScene { .. } => "delete_scene",
            Command::ImportScene { .. } => "import_scene",
            Command::SetOutput(_) => "set_output",
            Command::Flash(_) => "flash",
            Command::Beat(_) => "beat",
            Command::SetAudio(_) => "set_audio",
            Command::SetMidi(_) => "set_midi",
            Command::SetSchedule(_) => "set_schedule",
            Command::SetPlace(_) => "set_place",
            Command::SetOs2l(_) => "set_os2l",
            Command::SetRemote(_) => "set_remote",
            Command::SetGroups(_) => "set_groups",
            Command::Operator { .. } => "operator",
            Command::SetPin(_) => "set_pin",
            Command::SetPrefs(_) => "set_prefs",
        }
    }

    /// Команда не створює кроку скасування.
    pub fn no_undo(&self) -> bool {
        matches!(
            self,
            Command::Undo
                | Command::Redo
                | Command::RawSet { .. }
                | Command::RawClear(_)
                | Command::SetOutput(_)
                | Command::SetPrefs(_)
                | Command::SetHotkeys(_)
                | Command::Flash(_)
                | Command::Beat(_)
                | Command::SetAudio(_)
                | Command::SetMidi(_)
                | Command::SetSchedule(_)
                | Command::SetPlace(_)
                | Command::SetOs2l(_)
                | Command::SetRemote(_)
                | Command::Operator { .. }
                | Command::SetPin(_)
        )
    }

    /// Команди повзунків: серія з однаковим ключем — один крок скасування.
    fn mergeable(&self) -> bool {
        matches!(
            self,
            Command::SetColor { .. } | Command::SetMaster(_) | Command::SetEffect(_) | Command::Preset { .. }
        )
    }

    /// Ключ злиття: та сама команда з тими самими приладами й тим самим набором полів.
    fn merge_key(&self) -> String {
        let ids = |ids: &[String]| {
            let mut v = ids.to_vec();
            v.sort();
            v.join(",")
        };
        match self {
            Command::SetColor { ids: i, values } => format!("set_color|{}|{}", ids(i), values.keys()),
            Command::Preset { ids: i, n, speed } => {
                format!("preset|{}|{}|{}|{:?}", ids(i), n.is_some(), speed.is_some(), n)
            }
            Command::SetMaster(_) => "set_master".into(),
            Command::SetEffect(p) => format!(
                "set_effect|{}|{:?}|{}{}{}{}{}{}",
                p.ids.as_deref().map(ids).unwrap_or_default(),
                p.kind,
                p.speed.is_some(),
                p.spread.is_some(),
                p.colors.is_some(),
                p.by_segment.is_some(),
                p.beat.is_some(),
                p.beat_pulse.is_some()
            ),
            other => other.name().into(),
        }
    }

    /// Назва дії для підказки «Скасувати …» (знахідний відмінок).
    pub fn undo_label(name: &str) -> &'static str {
        match name {
            "set_color" => "зміну кольору",
            "static_color" => "статичний колір",
            "clear" => "очищення",
            "preset" => "пресет",
            "set_master" => "зміну майстра",
            "add_static_color" => "збереження кольору",
            "delete_static_color" => "видалення кольору",
            "blackout" => "Blackout",
            "set_effect" => "зміну ефекту",
            "add_fixture" => "додавання приладу",
            "update_fixture" => "зміну приладу",
            "delete_fixture" => "видалення приладу",
            "move_fixture" => "переміщення приладу",
            "place_fixture" => "переміщення на плані",
            "save_scene" => "збереження сцени",
            "recall_scene" => "виклик сцени",
            "delete_scene" => "видалення сцени",
            "import_scene" => "відкриття сцени з файлу",
            "save_profile" => "збереження профілю",
            "delete_profile" => "видалення профілю",
            "set_groups" => "зміну груп",
            _ => "дію",
        }
    }
}

#[derive(Debug, Clone)]
struct Fade {
    start: f64,
    dur: f64,
    /// крива на момент виклику сцени: зміна в налаштуваннях не смикає перехід, що вже йде
    curve: FadeCurve,
    /// прилад → (стан на початку, стан сцени)
    targets: Vec<(String, SceneState, SceneState)>,
}

/// Живий колір приладу для превʼю: RGBW на кожен сегмент.
pub type Live = IndexMap<String, Vec<[u8; 4]>>;

pub struct Engine {
    pub show: Show,
    /// Нещодавно використані моделі бібліотеки, найновіші першими.
    pub recent_models: Vec<Recent>,
    pub output_cfg: OutputConfig,
    /// `SetOutput` змінив вихід — програма має перевідкрити його.
    pub output_changed: bool,
    pub prefs: Prefs,
    /// «Спалах»: прилади, що зараз показують інший стан (без майстра й ефекту). Не зберігається.
    pub flash: IndexMap<String, Fixture>,
    /// Коли натиснули «Спалах» (відлік мерехтіння).
    flash_t0: f64,
    /// Плавний Blackout: (початок, рівень світла на початку: 1 — світло, 0 — темно).
    blackout_ramp: Option<(f64, f64)>,
    /// Початок ефекту: від нього відлічується фаза, щоб ефект починався з одного кольору.
    eff_t0: f64,
    /// Темп для ефектів «у такт музиці». Не зберігається.
    pub tempo: Tempo,
    /// Джерело ударів, крім тапу. Зберігається.
    pub audio: AudioSource,
    /// Прив'язки MIDI-контролера. Зберігаються.
    pub midi: Vec<crate::midi::MidiBinding>,
    /// Розклад і місце для сходу/заходу сонця. Зберігаються.
    pub schedule: Vec<crate::schedule::Event>,
    pub place: Option<crate::schedule::Place>,
    /// Порт сервера OS2L (VirtualDJ); `None` — вимкнено. Зберігається.
    pub os2l: Option<u16>,
    /// Порт пульта з телефона; `None` — вимкнено. Зберігається.
    pub remote: Option<u16>,
    /// Режим оператора: лише сцени, кольори, групи й майстер. Зберігається.
    pub operator: bool,
    /// PIN виходу з режиму оператора: `сіль:sha256`; `None` — без PIN.
    pub pin: Option<String>,
    /// Власні гарячі клавіші: дія → комбінації; стандартні — в інтерфейсі.
    pub hotkeys: IndexMap<String, Vec<String>>,
    /// Повідомлення для інтерфейсу про проблему з налаштуваннями.
    pub load_warning: Option<String>,
    pub universe: [u8; DMX_CHANNELS],
    /// Ручне керування окремими каналами (тест каналів); не зберігається.
    pub raw: BTreeMap<usize, u8>,
    pub live: Live,
    /// Шлях до config.json; `None` — нічого не зберігається (тести).
    pub config_path: Option<PathBuf>,
    fade: Option<Fade>,
    dirty: bool,
    last_save: f64,
    undo: Vec<(&'static str, Show)>,
    redo: Vec<(&'static str, Show)>,
    undo_key: Option<String>,
    undo_t: f64,
    pub(crate) notice: Option<String>,
    /// Назва останньої скасованої / повтореної дії.
    pub last_undo: Option<&'static str>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn pin_hash(salt: &str, pin: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(format!("{salt}:{pin}")).iter().map(|b| format!("{b:02x}")).collect()
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Перевірка гарячих клавіш: до 64 дій, назви й комбінації до 40 символів, до 4 комбінацій на дію.
pub fn check_hotkeys(raw: IndexMap<String, Vec<String>>) -> Result<IndexMap<String, Vec<String>>> {
    let mut out = IndexMap::new();
    for (action, keys) in raw.into_iter().take(64) {
        if action.chars().count() > 40 || !keys.iter().all(|k| !k.is_empty() && k.chars().count() <= 40) {
            bail!("неправильні гарячі клавіші");
        }
        out.insert(action, keys.into_iter().take(4).collect());
    }
    Ok(out)
}

impl Engine {
    /// Порожній рушій без файлу налаштувань.
    pub fn new() -> Engine {
        Engine {
            show: Show::default(),
            recent_models: vec![],
            output_cfg: OutputConfig::default(),
            output_changed: false,
            prefs: Prefs::default(),
            flash: IndexMap::new(),
            flash_t0: 0.0,
            blackout_ramp: None,
            eff_t0: 0.0,
            tempo: Tempo::default(),
            audio: AudioSource::default(),
            midi: vec![],
            schedule: vec![],
            place: None,
            os2l: None,
            remote: None,
            operator: false,
            pin: None,
            hotkeys: IndexMap::new(),
            load_warning: None,
            universe: [0; DMX_CHANNELS],
            raw: BTreeMap::new(),
            live: IndexMap::new(),
            config_path: None,
            fade: None,
            dirty: false,
            last_save: 0.0,
            undo: vec![],
            redo: vec![],
            undo_key: None,
            undo_t: f64::NEG_INFINITY,
            notice: None,
            last_undo: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self, now: f64) {
        self.dirty = false;
        self.last_save = now;
    }

    /// Час зберегти налаштування: є зміни й минуло понад секунду з останнього збереження.
    pub fn save_due(&self, now: f64) -> bool {
        self.dirty && now - self.last_save > 1.0
    }

    // ---------------------------------------------------------------- helpers --
    pub fn is_device(&self, name: &str) -> bool {
        if default_profiles().contains_key(name) {
            return false;
        }
        self.show.device_profiles.contains(name) || model_profiles().contains_key(name)
    }

    /// Профілі для меню: стандартні та власні. Профілі моделей обираються разом із приладом.
    pub fn menu_profiles(&self) -> Vec<String> {
        self.show.profiles.keys().filter(|n| !self.is_device(n)).cloned().collect()
    }

    pub fn remember(&mut self, model: &Model) {
        let key = Recent { m: model.m.clone(), n: model.n.clone() };
        self.recent_models.retain(|r| *r != key);
        self.recent_models.insert(0, key);
        self.recent_models.truncate(RECENT_LIMIT);
    }

    /// Профіль моделі (напр. WASH 243) додається, щойно він потрібен.
    pub fn ensure_profile(&mut self, name: &str) -> String {
        if !self.show.profiles.contains_key(name) {
            if let Some(ch) = model_profiles().get(name) {
                self.show.profiles.insert(name.to_string(), ch.clone());
            }
        }
        name.to_string()
    }

    pub fn fixture(&self, id: &str) -> Option<&Fixture> {
        self.show.fixtures.iter().find(|f| f.id == id)
    }

    fn fx_index(&self, id: &str) -> Result<usize> {
        self.show.fixtures.iter().position(|f| f.id == id).ok_or_else(|| err!("прилад {id} не знайдено"))
    }

    pub fn channels(&self, fx: &Fixture) -> &[String] {
        self.show.profiles.get(&fx.profile).map_or(&[], |v| v.as_slice())
    }

    pub fn has_presets(&self, fx: &Fixture) -> bool {
        has_presets(self.channels(fx))
    }

    pub fn has_role(&self, fx: &Fixture, role: &str) -> bool {
        self.channels(fx).iter().any(|c| c == role)
    }

    pub fn segments(&self, fx: &Fixture) -> usize {
        segment_count(self.channels(fx))
    }

    /// Розбирає вибір: індекси цілих приладів і (прилад, сегмент).
    fn resolve(&self, ids: &[String]) -> (Vec<usize>, Vec<(usize, usize)>) {
        let mut whole: BTreeSet<&str> = BTreeSet::new();
        let mut segs: Vec<(usize, usize)> = vec![];
        for item in ids {
            let (fid, k) = match item.split_once('#') {
                Some((f, k)) => (f, Some(k)),
                None => (item.as_str(), None),
            };
            let Some(i) = self.show.fixtures.iter().position(|f| f.id == fid) else { continue };
            match k {
                None => {
                    whole.insert(fid);
                }
                Some(k) => {
                    if let Ok(k) = k.parse::<usize>() {
                        if k < self.segments(&self.show.fixtures[i]) {
                            segs.push((i, k));
                        }
                    }
                }
            }
        }
        let w: Vec<usize> =
            (0..self.show.fixtures.len()).filter(|&i| whole.contains(self.show.fixtures[i].id.as_str())).collect();
        segs.retain(|(i, _)| !whole.contains(self.show.fixtures[*i].id.as_str()));
        (w, segs)
    }

    fn drop_from_effect(&mut self, fids: &[String]) {
        self.show.effect.ids.retain(|i| !fids.contains(i));
        if self.show.effect.ids.is_empty() {
            self.show.effect.kind = EffectKind::None;
        }
    }

    pub fn overlaps(&self) -> Vec<(String, String)> {
        find_overlaps(&self.show.fixtures, &self.show.profiles)
    }

    /// Прилади з каналом програми.
    pub fn preset_fixtures(&self) -> Vec<String> {
        self.show.fixtures.iter().filter(|f| self.has_presets(f)).map(|f| f.id.clone()).collect()
    }

    pub fn can_undo(&self) -> Option<&'static str> {
        self.undo.last().map(|u| u.0)
    }

    pub fn can_redo(&self) -> Option<&'static str> {
        self.redo.last().map(|u| u.0)
    }

    /// Повідомлення для того, хто надіслав останню команду.
    pub fn pop_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    // --------------------------------------------------------------- commands --
    /// Застосовує команду. Якщо команда не вдалася, стан лишається як був.
    pub fn handle(&mut self, cmd: Command, now: f64) -> Result<()> {
        let name = cmd.name();
        let mut pushed = false;
        if !cmd.no_undo() {
            let key = cmd.merge_key();
            if !cmd.mergeable() || self.undo_key.as_ref() != Some(&key) || now - self.undo_t > UNDO_MERGE_S {
                self.undo.push((name, self.show.clone()));
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
                pushed = true;
            }
            self.undo_key = Some(key);
            self.undo_t = now;
        }
        let no_undo = cmd.no_undo();
        // удари, «Спалах» і тест каналів не зберігаються — файл налаштувань не переписуємо
        let persist =
            !matches!(cmd, Command::Beat(_) | Command::Flash(_) | Command::RawSet { .. } | Command::RawClear(_));
        let prev = self.show.effect.clone();
        if let Err(e) = self.apply(cmd, now) {
            if pushed {
                // команда не виконалась — і кроку скасування немає
                let (_, snap) = self.undo.pop().expect("крок скасування");
                self.show = snap;
                self.undo_key = None;
            }
            return Err(e);
        }
        if !no_undo {
            self.redo.clear();
        }
        self.retime_effect(&prev, now);
        self.dirty |= persist;
        Ok(())
    }

    /// Новий ефект (чи нові прилади в ньому) починається з нуля — з одного кольору на всіх.
    /// Зміна швидкості не зсуває фазу: ефект продовжується з того ж кольору, без мерехтіння.
    fn retime_effect(&mut self, prev: &Effect, now: f64) {
        let e = &self.show.effect;
        if e.kind == EffectKind::None {
            return;
        }
        if e.kind != prev.kind || e.ids.iter().any(|id| !prev.ids.contains(id)) {
            self.eff_t0 = now;
        } else if e.speed != prev.speed {
            self.eff_t0 = now - (now - self.eff_t0) * prev.speed / e.speed;
        }
    }

    fn restore(&mut self, snap: Show) {
        self.show = snap;
        self.fade = None;
        self.blackout_ramp = None;
        self.undo_key = None;
    }

    fn apply(&mut self, cmd: Command, now: f64) -> Result<()> {
        match cmd {
            Command::Undo => {
                let (name, snap) = self.undo.pop().ok_or_else(|| err!("Немає дій для скасування"))?;
                self.redo.push((name, self.show.clone()));
                self.restore(snap);
                self.last_undo = Some(name);
            }
            Command::Redo => {
                let (name, snap) = self.redo.pop().ok_or_else(|| err!("Немає дій для повторення"))?;
                self.undo.push((name, self.show.clone()));
                self.restore(snap);
                self.last_undo = Some(name);
            }
            Command::SetColor { ids, values } => self.set_color(&ids, &values),
            Command::Preset { ids, n, speed } => self.preset(&ids, n, speed)?,
            Command::Clear { ids } => self.clear_faded(&ids, now),
            Command::StaticColor { ids, source } => self.static_color(&ids, source)?,
            Command::AddStaticColor { name, hex, w } => {
                if self.show.static_colors.len() >= STATIC_COLORS_MAX {
                    bail!("у палітрі вже {STATIC_COLORS_MAX} кольорів");
                }
                let (w, hex_w) = if w > 0 { (Some(w), Some(hex)) } else { (None, None) };
                if self.show.static_colors.iter().any(|c| c.hex == hex && c.w == w) {
                    bail!("такий колір уже є в палітрі");
                }
                let name = name.trim();
                let name = if name.is_empty() { hex.hex() } else { truncate_chars(name, 40) };
                self.show.static_colors.push(StaticColor { name, hex, hex_w, w });
            }
            Command::DeleteStaticColor { index } => {
                self.static_at(index)?;
                if index < default_static_colors().len() {
                    bail!("стандартний колір не можна видалити");
                }
                self.show.static_colors.remove(index);
            }
            Command::SetMaster(v) => self.show.master = v,
            Command::SetHotkeys(h) => self.hotkeys = check_hotkeys(h)?,
            Command::Blackout(on) => {
                let level = self.light_level(now);
                self.show.blackout = on.unwrap_or(!self.show.blackout);
                self.blackout_ramp = Some((now, level));
            }
            Command::AddFixture { spec, name, address, count } => self.add_fixture(spec, &name, address, count)?,
            Command::UpdateFixture { id, name, address, profile } => {
                self.update_fixture(&id, name, address, profile)?
            }
            Command::DeleteFixture { id } => {
                self.show.fixtures.retain(|f| f.id != id);
                self.drop_from_effect(std::slice::from_ref(&id));
                for ids in self.show.groups.values_mut() {
                    ids.retain(|i| *i != id);
                }
                self.flash.shift_remove(&id);
            }
            Command::MoveFixture { id, delta } => {
                let i = self.fx_index(&id)?;
                let last = self.show.fixtures.len() as i64 - 1;
                let j = (i as i64 + delta as i64).clamp(0, last) as usize;
                let fx = self.show.fixtures.remove(i);
                self.show.fixtures.insert(j, fx);
            }
            Command::PlaceFixture { id, pos } => {
                let i = self.fx_index(&id)?;
                self.show.fixtures[i].pos = pos;
            }
            Command::SaveProfile { name, channels, assign_to } => self.save_profile(&name, channels, assign_to)?,
            Command::RawSet { address, value } => {
                if !(1..=DMX_CHANNELS as u16).contains(&address) {
                    bail!("адреса має бути 1..512");
                }
                self.raw.insert(address as usize - 1, value);
            }
            Command::RawClear(addrs) => match addrs {
                None => self.raw.clear(),
                Some(a) => {
                    for a in a {
                        if a >= 1 {
                            self.raw.remove(&(a as usize - 1));
                        }
                    }
                }
            },
            Command::DeleteProfile { name } => {
                if self.show.fixtures.iter().any(|f| f.profile == name) {
                    bail!("профіль використовується приладами");
                }
                if default_profiles().contains_key(&name) {
                    bail!("вбудований профіль не можна видалити");
                }
                self.show.profiles.shift_remove(&name);
            }
            Command::SetEffect(p) => {
                let cur = self.show.effect.clone();
                let colors = p.colors.unwrap_or(cur.colors);
                self.show.effect = Effect {
                    kind: p.kind.unwrap_or(cur.kind),
                    ids: p.ids.unwrap_or(cur.ids),
                    speed: p.speed.unwrap_or(cur.speed).clamp(SPEED_MIN, SPEED_MAX),
                    spread: p.spread.unwrap_or(cur.spread).clamp(0.0, 1.0),
                    colors: colors.into_iter().take(PALETTE_MAX).collect(),
                    by_segment: p.by_segment.unwrap_or(cur.by_segment),
                    beat: p.beat.unwrap_or(cur.beat),
                    beat_pulse: p.beat_pulse.unwrap_or(cur.beat_pulse),
                    order: p.order.unwrap_or(cur.order),
                };
            }
            Command::SaveScene { name, with_effect } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    bail!("вкажіть назву сцени");
                }
                let fixtures = self.show.fixtures.iter().map(|f| (f.id.clone(), SceneState::of(f))).collect();
                let effect = with_effect.then(|| self.show.effect.clone());
                self.show.scenes.insert(name, Scene { fixtures, effect });
            }
            Command::RecallScene { name, fade } => self.recall_scene(&name, fade, now)?,
            Command::DeleteScene { name } => {
                self.show.scenes.shift_remove(&name);
            }
            Command::ImportScene { data, name } => {
                // усе або нічого: якщо файл зіпсований посередині, додані прилади й профілі прибираються
                let snap = self.show.clone();
                if let Err(e) = crate::scene_file::import(self, &data, name.as_deref()) {
                    self.restore(snap);
                    return Err(e);
                }
            }
            Command::SetOutput(cfg) => {
                self.output_cfg = cfg;
                self.output_changed = true;
            }
            Command::Flash(src) => self.flash_cmd(src, now)?,
            Command::Beat(at) => self.tempo.beat(at),
            Command::SetAudio(a) => self.audio = a,
            Command::SetMidi(m) => self.midi = crate::midi::check(m)?,
            Command::SetSchedule(s) => self.schedule = crate::schedule::check(s)?,
            Command::SetPlace(p) => self.place = crate::schedule::check_place(p)?,
            Command::SetOs2l(p) => self.os2l = p,
            Command::SetRemote(p) => self.remote = p,
            Command::SetGroups(groups) => self.show.groups = self.check_groups(groups)?,
            Command::Operator { on, pin } => {
                if !on && !self.pin_ok(&pin) {
                    bail!("неправильний PIN");
                }
                self.operator = on;
            }
            Command::SetPin(pin) => {
                let pin = pin.trim();
                self.pin = if pin.is_empty() {
                    None
                } else {
                    if !(4..=8).contains(&pin.len()) || !pin.chars().all(|c| c.is_ascii_digit()) {
                        bail!("PIN — від 4 до 8 цифр");
                    }
                    let salt = new_id();
                    Some(format!("{salt}:{}", pin_hash(&salt, pin)))
                };
            }
            Command::SetPrefs(p) => self.prefs = p.clamped(),
        }
        Ok(())
    }

    /// Прилади ефекту в порядку `order`. Прилад, ще не поставлений на план, стоїть праворуч від інших
    /// у порядку списку приладів.
    fn effect_order<'a>(&self, eff: &'a Effect) -> Vec<&'a String> {
        let mut ids: Vec<&String> = eff.ids.iter().collect();
        if eff.order == crate::effects::Order::Ids {
            return ids;
        }
        let at = |id: &str| -> (f64, f64) {
            let i = self.fx_index(id).unwrap_or(0);
            match self.show.fixtures.get(i).and_then(|f| f.pos) {
                Some([x, y]) => (x as f64, y as f64),
                None => (10_000.0 + i as f64, 0.0),
            }
        };
        let pts: Vec<(f64, f64)> = ids.iter().map(|id| at(id)).collect();
        let n = pts.len().max(1) as f64;
        let (cx, cy) = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n);
        ids.sort_by(|a, b| {
            let (pa, pb) = (at(a), at(b));
            eff.order
                .key(pa.0, pa.1, cx, cy)
                .partial_cmp(&eff.order.key(pb.0, pb.1, cx, cy))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ids
    }

    /// Дія розкладу: сцена (з плавним переходом; Blackout знімається), «Очистити» чи Blackout.
    pub fn scheduled(&mut self, action: &str, now: f64) -> Result<()> {
        match action {
            "clear" => self.handle(Command::Clear { ids: vec![] }, now),
            "blackout" => self.handle(Command::Blackout(Some(true)), now),
            a => {
                let name = a.strip_prefix("scene:").unwrap_or(a).to_string();
                self.handle(Command::Blackout(Some(false)), now)?;
                self.handle(Command::RecallScene { name, fade: 2.0 }, now)
            }
        }
    }

    /// PIN правильний (або його не задано).
    pub fn pin_ok(&self, pin: &str) -> bool {
        match self.pin.as_deref().and_then(|p| p.split_once(':')) {
            None => true,
            Some((salt, hash)) => pin_hash(salt, pin.trim()) == hash,
        }
    }

    /// Групи: до 32, назви до 40 символів, лише наявні прилади без повторів.
    pub(crate) fn check_groups(&self, raw: IndexMap<String, Vec<String>>) -> Result<IndexMap<String, Vec<String>>> {
        let mut out = IndexMap::new();
        for (name, ids) in raw {
            let name = truncate_chars(name.trim(), 40);
            if name.is_empty() {
                bail!("назва групи не може бути порожньою");
            }
            let mut v: Vec<String> = vec![];
            for id in ids {
                if self.fixture(&id).is_some() && !v.contains(&id) {
                    v.push(id);
                }
            }
            out.insert(name, v);
        }
        if out.len() > 32 {
            bail!("забагато груп (максимум 32)");
        }
        Ok(out)
    }

    /// «Спалах»: стан цілей рахується тими самими командами (колір чи сцена) на копії приладів,
    /// а шоу лишається незмінним — після відпускання все як було.
    fn flash_cmd(&mut self, src: Option<FlashSource>, now: f64) -> Result<()> {
        self.flash.clear();
        let Some(src) = src else { return Ok(()) };
        self.flash_t0 = now;
        let saved = (self.show.fixtures.clone(), self.show.effect.clone(), self.fade.take());
        let targets: Vec<String> = match &src {
            FlashSource::Color { ids, .. } if !ids.is_empty() => {
                ids.iter().map(|i| i.split('#').next().unwrap_or("").to_string()).collect()
            }
            FlashSource::Color { .. } => self.show.fixtures.iter().map(|f| f.id.clone()).collect(),
            FlashSource::Scene(name) => {
                self.show.scenes.get(name).map(|s| s.fixtures.keys().cloned().collect()).unwrap_or_default()
            }
        };
        let r = match src {
            FlashSource::Color { ids, source } => self.static_color(&ids, source).map(|()| {
                for f in self.show.fixtures.iter_mut().filter(|f| targets.contains(&f.id)) {
                    f.dimmer = 255; // спалах — на повну яскравість
                }
            }),
            FlashSource::Scene(name) => self.recall_scene(&name, 0.0, now),
        };
        let flashed = std::mem::replace(&mut self.show.fixtures, saved.0);
        (self.show.effect, self.fade) = (saved.1, saved.2);
        r?;
        self.flash = flashed.into_iter().filter(|f| targets.contains(&f.id)).map(|f| (f.id.clone(), f)).collect();
        Ok(())
    }

    fn static_at(&self, index: usize) -> Result<&StaticColor> {
        // список міг змінитися (інше вікно, скасування) — тоді зрозуміла помилка
        self.show.static_colors.get(index).ok_or_else(|| err!("цього кольору вже немає — список кольорів змінився"))
    }

    fn set_color(&mut self, ids: &[String], values: &ColorValues) {
        self.fade = None;
        let pairs = values.pairs();
        let rgb_set = pairs.iter().any(|(k, _)| matches!(*k, "r" | "g" | "b"));
        let (whole, segs) = self.resolve(ids);
        for i in whole {
            let f = &mut self.show.fixtures[i];
            for (k, v) in &pairs {
                f.set_channel(k, *v);
            }
            if rgb_set {
                // колір на весь прилад скидає кольори сегментів і пресет
                f.segs = None;
                f.preset = None;
            }
        }
        for (i, k) in segs {
            let n = self.segments(&self.show.fixtures[i]);
            let f = &mut self.show.fixtures[i];
            if rgb_set {
                f.preset = None;
                let cur = f.seg_color(k);
                let get = |key: &str, cur: f64| pairs.iter().find(|p| p.0 == key).map_or(cur as u8, |p| p.1);
                f.set_seg(k, [get("r", cur.r), get("g", cur.g), get("b", cur.b)], n);
            }
            for (key, v) in &pairs {
                // диммер, білий тощо — спільні для приладу
                if !matches!(*key, "r" | "g" | "b") {
                    f.set_channel(key, *v);
                }
            }
        }
    }

    fn preset(&mut self, ids: &[String], n: Option<Option<u8>>, speed: Option<u8>) -> Result<()> {
        if let Some(Some(n)) = n {
            preset_level(n)?; // перевірка діапазону
        }
        let (whole, segs) = self.resolve(ids);
        let mut targets: Vec<usize> = vec![];
        for i in whole.into_iter().chain(segs.into_iter().map(|s| s.0)) {
            if self.has_presets(&self.show.fixtures[i]) && !targets.contains(&i) {
                targets.push(i);
            }
        }
        if targets.is_empty() && ids.is_empty() {
            targets = (0..self.show.fixtures.len()).filter(|&i| self.has_presets(&self.show.fixtures[i])).collect();
        }
        if targets.is_empty() {
            bail!("Пресети доступні лише приладам з каналом програми");
        }
        self.fade = None;
        for &i in &targets {
            let f = &mut self.show.fixtures[i];
            if let Some(n) = n {
                f.preset = n;
            }
            if let Some(s) = speed {
                f.speed = s;
            }
        }
        if let Some(Some(_)) = n {
            let fids: Vec<String> = targets.iter().map(|&i| self.show.fixtures[i].id.clone()).collect();
            self.drop_from_effect(&fids);
        }
        Ok(())
    }

    /// «Очистити» з плавністю з налаштувань: кольори згасають через той самий кросфейд, що й сцени.
    fn clear_faded(&mut self, ids: &[String], now: f64) {
        let before: Vec<SceneState> = self.show.fixtures.iter().map(SceneState::of).collect();
        self.clear(ids);
        let dur = self.prefs.clear_fade;
        if dur <= 0.0 {
            return;
        }
        let targets: Vec<_> = self
            .show
            .fixtures
            .iter()
            .zip(before)
            .map(|(f, start)| (f.id.clone(), start, SceneState::of(f)))
            .filter(|(_, a, b)| a != b)
            .collect();
        self.apply_fade(&targets, 0.0);
        self.fade = Some(Fade { start: now, dur, curve: FadeCurve::Linear, targets });
    }

    /// Рівень світла з урахуванням плавного Blackout: 1 — світло, 0 — темно.
    fn light_level(&self, now: f64) -> f64 {
        let target = if self.show.blackout { 0.0 } else { 1.0 };
        let dur = self.prefs.blackout_fade;
        match self.blackout_ramp {
            Some((t0, from)) if dur > 0.0 => from + (target - from) * ((now - t0) / dur).clamp(0.0, 1.0),
            _ => target,
        }
    }

    fn clear(&mut self, ids: &[String]) {
        self.fade = None;
        let (mut whole, segs) = self.resolve(ids);
        if whole.is_empty() && segs.is_empty() {
            whole = (0..self.show.fixtures.len()).collect();
        }
        let mut fids = vec![];
        for &i in &whole {
            let f = &mut self.show.fixtures[i];
            (f.r, f.g, f.b, f.w, f.a, f.uv, f.strobe) = (0, 0, 0, 0, 0, 0, 0);
            f.segs = None;
            f.preset = None;
            fids.push(f.id.clone());
        }
        for &(i, k) in &segs {
            let n = self.segments(&self.show.fixtures[i]);
            let f = &mut self.show.fixtures[i];
            f.preset = None;
            f.set_seg(k, [0, 0, 0], n);
            fids.push(f.id.clone());
        }
        self.drop_from_effect(&fids);
    }

    fn static_color(&mut self, ids: &[String], source: StaticSource) -> Result<()> {
        let (hex, hex_w, w) = match source {
            StaticSource::Index(i) => {
                let c = self.static_at(i)?;
                (c.hex, c.hex_w, c.w.unwrap_or(0))
            }
            StaticSource::Custom { hex, hex_w, w } => (hex, hex_w, w),
        };
        let rgb_w = hex_w.unwrap_or(hex);
        let (mut whole, segs) = self.resolve(ids);
        if whole.is_empty() && segs.is_empty() {
            whole = (0..self.show.fixtures.len()).collect();
        }
        self.fade = None;
        let mut fids = vec![];
        for &i in &whole {
            let has_w = self.has_role(&self.show.fixtures[i], "w");
            let f = &mut self.show.fixtures[i];
            let c = if has_w && w > 0 { rgb_w } else { hex };
            (f.r, f.g, f.b) = (c.0, c.1, c.2);
            f.w = if has_w { w } else { 0 };
            f.a = 0;
            f.uv = 0;
            f.segs = None;
            f.preset = None;
            fids.push(f.id.clone());
        }
        for &(i, k) in &segs {
            let n = self.segments(&self.show.fixtures[i]);
            let f = &mut self.show.fixtures[i];
            f.preset = None;
            f.set_seg(k, hex.arr(), n);
            fids.push(f.id.clone());
        }
        self.drop_from_effect(&fids);
        Ok(())
    }

    /// Профіль і модель приладу з команди. Профіль моделі бібліотеки приходить разом із каналами
    /// й моделлю; він належить приладу і в меню профілів не потрапляє.
    pub(crate) fn profile_from(&mut self, spec: &ProfileSpec) -> Result<(String, Option<Model>)> {
        let model = spec.model.clone().and_then(Model::valid);
        let Some(channels) = &spec.channels else {
            let name = self.ensure_profile(&spec.profile);
            if !self.show.profiles.contains_key(&name) {
                bail!("невідомий профіль «{name}»");
            }
            let model = if self.is_device(&name) { model } else { None };
            return Ok((name, model));
        };
        let channels: Vec<String> = channels.iter().map(|c| c.trim().to_string()).collect();
        validate_profile(&channels)?;
        let mut name = spec.profile.trim().to_string();
        if name.is_empty() {
            if let Some(m) = &model {
                name = format!("{} {} · {} ({} кан.)", m.m, m.n, m.mode, channels.len());
            }
        }
        if name.is_empty() {
            bail!("вкажіть назву профілю");
        }
        let base = name.clone();
        let mut k = 2;
        while self.show.profiles.get(&name).is_some_and(|c| *c != channels) {
            name = format!("{base} ({k})");
            k += 1;
        }
        if !self.show.profiles.contains_key(&name) {
            self.show.profiles.insert(name.clone(), channels);
            self.show.device_profiles.insert(name.clone());
        }
        let model = if self.is_device(&name) { model } else { None };
        Ok((name, model))
    }

    fn add_fixture(&mut self, spec: ProfileSpec, name: &str, address: u16, count: u32) -> Result<()> {
        let count = count.clamp(1, 64) as usize;
        let (profile, model) = self.profile_from(&spec)?;
        let width = self.show.profiles[&profile].len();
        let given = name.trim();
        let base = {
            let n = truncate_chars(given, 80);
            if n.trim().is_empty() { "Прожектор".to_string() } else { n }
        };
        let mut new = vec![];
        for i in 0..count {
            let name = if count > 1 || given.is_empty() {
                format!("{base} {}", self.show.fixtures.len() + new.len() + 1)
            } else {
                base.clone()
            };
            let addr = address as usize + i * width;
            let fx = Fixture::new(new_id(), name, addr.min(u16::MAX as usize) as u16, profile.clone(), model.clone());
            validate_fixture(&fx, &self.show.profiles)?;
            new.push(fx);
        }
        self.show.fixtures.extend(new);
        if let Some(m) = model {
            self.remember(&m);
        }
        Ok(())
    }

    fn update_fixture(
        &mut self,
        id: &str,
        name: Option<String>,
        address: Option<u16>,
        profile: Option<ProfileSpec>,
    ) -> Result<()> {
        let i = self.fx_index(id)?;
        let mut trial = self.show.fixtures[i].clone();
        if let Some(n) = name {
            let n = truncate_chars(n.trim(), 80);
            if !n.is_empty() {
                trial.name = n;
            }
        }
        if let Some(a) = address {
            trial.address = a;
        }
        let mut model = trial.model.clone();
        let mut model_given = false;
        if let Some(spec) = &profile {
            model_given = spec.model.is_some();
            let (p, new_model) = self.profile_from(spec)?;
            if p != trial.profile || model_given {
                model = new_model;
            }
            trial.profile = p;
        }
        validate_fixture(&trial, &self.show.profiles)?;
        let is_device = self.is_device(&trial.profile);
        let has_presets = has_presets(&self.show.profiles[&trial.profile]);
        let f = &mut self.show.fixtures[i];
        if trial.profile != f.profile {
            f.segs = None;
        }
        f.name = trial.name;
        f.address = trial.address;
        f.profile = trial.profile;
        f.model = if is_device { model } else { None };
        if !has_presets {
            f.preset = None;
        }
        if let (Some(m), true) = (f.model.clone(), model_given) {
            self.remember(&m);
        }
        Ok(())
    }

    fn save_profile(&mut self, name: &str, channels: Vec<String>, assign_to: Option<String>) -> Result<()> {
        let name = name.trim().to_string();
        if name.is_empty() {
            bail!("вкажіть назву профілю");
        }
        if default_profiles().contains_key(&name) {
            bail!("вбудований профіль не можна змінити");
        }
        let channels: Vec<String> = channels.iter().map(|c| c.trim().to_string()).collect();
        validate_profile(&channels)?;
        if let Some(fid) = assign_to {
            // одразу призначити профіль приладу
            let i = self.fx_index(&fid)?;
            let mut trial = self.show.fixtures[i].clone();
            trial.profile = name.clone();
            let mut profiles = self.show.profiles.clone();
            profiles.insert(name.clone(), channels.clone());
            validate_fixture(&trial, &profiles)?;
            self.show.profiles.insert(name.clone(), channels);
            self.show.device_profiles.remove(&name);
            let f = &mut self.show.fixtures[i];
            f.profile = name;
            f.segs = None;
            f.model = None;
        } else {
            self.show.profiles.insert(name.clone(), channels);
            self.show.device_profiles.remove(&name);
        }
        Ok(())
    }

    fn recall_scene(&mut self, name: &str, fade: f64, now: f64) -> Result<()> {
        let scene = self.show.scenes.get(name).ok_or_else(|| err!("сцену «{name}» не знайдено"))?.clone();
        let fade = if fade.is_finite() { fade.max(0.0) } else { 0.0 };
        let targets: Vec<(String, SceneState, SceneState)> = scene
            .fixtures
            .iter()
            .filter_map(|(fid, end)| self.fixture(fid).map(|f| (fid.clone(), SceneState::of(f), end.clone())))
            .collect();
        if let Some(e) = scene.effect {
            self.show.effect = e.clamped();
        }
        if fade > 0.0 {
            self.fade = Some(Fade { start: now, dur: fade, curve: self.prefs.fade_curve, targets });
        } else {
            self.apply_fade(&targets, 1.0);
            self.fade = None;
        }
        Ok(())
    }

    // ----------------------------------------------------------------- render --
    fn apply_fade(&mut self, targets: &[(String, SceneState, SceneState)], k: f64) {
        for (fid, start, end) in targets {
            let Ok(i) = self.fx_index(fid) else { continue };
            let n = self.segments(&self.show.fixtures[i]);
            let f = &mut self.show.fixtures[i];
            for key in STATE_KEYS {
                let (a, b) = (start.get(key) as f64, end.get(key) as f64);
                f.set_channel(key, (a + (b - a) * k).round_ties_even() as u8);
            }
            if k >= 1.0 {
                f.segs = end.segs.clone();
                f.preset = end.preset.filter(|p| (1..=preset_count()).contains(p));
                f.speed = end.speed;
            } else if start.segs.is_some() || end.segs.is_some() {
                let (s0, s1) = (start.seg_rgbs(n), end.seg_rgbs(n));
                f.segs = Some(
                    s0.iter()
                        .zip(&s1)
                        .map(|(p, q)| {
                            let c = |j: usize| (p[j] as f64 + (q[j] as f64 - p[j] as f64) * k).round_ties_even() as u8;
                            Some([c(0), c(1), c(2)])
                        })
                        .collect(),
                );
            }
        }
    }

    /// Рендер кадру в момент `now`: кросфейд, ефекти, строб, блекаут, ручні канали.
    pub fn render(&mut self, now: f64) -> [u8; DMX_CHANNELS] {
        if let Some(fade) = self.fade.take() {
            let k = ((now - fade.start) / fade.dur).min(1.0);
            self.apply_fade(&fade.targets, fade.curve.apply(k));
            if k < 1.0 {
                self.fade = Some(fade);
            }
        }
        let mut uni = [0u8; DMX_CHANNELS];
        let mut live = Live::new();
        // «у такт музиці»: цикл ефекту відлічується від останнього удару зі швидкістю за темпом
        let beat = if self.show.effect.beat { self.tempo.beats_since(now) } else { None };
        let synced;
        let (eff, t) = match (beat, self.tempo.bpm) {
            (Some(_), Some(bpm)) => {
                let e = &self.show.effect;
                synced = Effect { speed: bpm / 60.0 * e.kind.cycles_per_beat(), ..e.clone() };
                (&synced, now - self.tempo.anchor)
            }
            _ => (&self.show.effect, now - self.eff_t0),
        };
        // ефект розходиться з одного кольору за перший цикл; у такт музиці — одразу
        let unfold = if beat.is_some() && self.tempo.bpm.is_some() { 1.0 } else { (t * eff.speed).clamp(0.0, 1.0) };
        let pulse = beat.filter(|_| eff.beat_pulse).map(beat_pulse);
        // «пікселі» ефекту: кожен сегмент кожного приладу (або прилад цілком)
        let mut pixel: IndexMap<(&str, usize), usize> = IndexMap::new();
        let mut count = 0usize;
        if eff.kind != EffectKind::None {
            for fid in self.effect_order(eff) {
                let Some(fx) = self.fixture(fid) else { continue };
                let n = self.segments(fx);
                if eff.by_segment {
                    for k in 0..n {
                        if !pixel.contains_key(&(fx.id.as_str(), k)) {
                            pixel.insert((fx.id.as_str(), k), pixel.len());
                        }
                    }
                } else {
                    let idx = pixel.values().collect::<BTreeSet<_>>().len();
                    for k in 0..n {
                        pixel.insert((fx.id.as_str(), k), idx);
                    }
                }
            }
            count = pixel.values().collect::<BTreeSet<_>>().len();
        }
        let level = self.light_level(now);
        let master = level * self.show.master as f64 / 255.0;
        let programs = programs();
        for fx in &self.show.fixtures {
            let Some(channels) = self.show.profiles.get(&fx.profile) else { continue };
            // «Спалах» перекриває стан приладу, ефект і майстер (але не Blackout)
            let over = self.flash.get(&fx.id);
            let fx = over.unwrap_or(fx);
            // «Спалах» з частотою мерехтить: половину періоду світить, половину — темно
            let flash_on =
                self.prefs.flash_rate <= 0.0 || ((now - self.flash_t0) * self.prefs.flash_rate).fract() < 0.5;
            let master = match over {
                Some(_) if !flash_on => 0.0,
                Some(_) => level,
                None => master,
            };
            if channels.is_empty() {
                continue;
            }
            let n_seg = segment_count(channels);
            if let (Some(p), true) = (fx.preset, has_presets(channels)) {
                // профіль з каналом програми (A001 / H001): програма — у свій канал
                let colors: Vec<Color> = (0..n_seg).map(|k| fx.seg_color(k)).collect();
                render_fixture(&mut uni, fx, channels, &colors, master);
                let rgb = programs.get(p as usize - 1).map_or([200, 200, 200], program_rgb);
                let lvl = if channels.iter().any(|c| c == "dim") { fx.dimmer as f64 / 255.0 } else { 1.0 } * master;
                let px = [
                    (rgb[0] as f64 * lvl).round_ties_even() as u8,
                    (rgb[1] as f64 * lvl).round_ties_even() as u8,
                    (rgb[2] as f64 * lvl).round_ties_even() as u8,
                    0,
                ];
                live.insert(fx.id.clone(), vec![px; n_seg]);
                continue;
            }
            let colors: Vec<Color> = (0..n_seg)
                .map(|k| {
                    let c = fx.seg_color(k);
                    match pixel.get(&(fx.id.as_str(), k)) {
                        Some(&p) if over.is_none() => {
                            let c = effect_color(eff, p, count, t, unfold, c);
                            pulse.map_or(c, |k| c.scaled(k))
                        }
                        _ => c,
                    }
                })
                .collect();
            render_fixture(&mut uni, fx, channels, &colors, master);
            let lvl = fx.dimmer as f64 / 255.0 * master;
            let px = |c: &Color| {
                let v = |x: f64| (x * lvl).min(255.0).round_ties_even() as u8;
                [v(c.r), v(c.g), v(c.b), v(c.w)]
            };
            live.insert(fx.id.clone(), colors.iter().map(px).collect());
        }
        for (&ch, &v) in &self.raw {
            uni[ch] = v;
        }
        self.universe = uni;
        self.live = live;
        uni
    }
}

/// Приблизний колір програми для превʼю на плитці приладу.
fn program_rgb(p: &crate::fixtures::Program) -> [u8; 3] {
    const COLORS: [(&str, [u8; 3]); 7] = [
        ("Червоний", [255, 0, 0]),
        ("Зелений", [0, 255, 0]),
        ("Синій", [0, 0, 255]),
        ("Жовтий", [255, 200, 0]),
        ("Фіолетовий", [170, 0, 255]),
        ("Блакитний", [0, 220, 255]),
        ("Білий", [255, 255, 255]),
    ];
    for (key, rgb) in COLORS {
        if p.name.starts_with(key) {
            return rgb;
        }
    }
    if p.group == "sound" { [255, 120, 255] } else { [200, 200, 200] }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(s)
    }
}
