//! Прилади (прожектори), профілі каналів і рендер у DMX-всесвіт.
//!
//! Ролі каналів:
//! * `r g b w a uv` — колірні канали; `dim` — загальний диммер приладу;
//! * `strobe` — апаратний строб; `zero` — завжди 0; `full` — завжди 255;
//! * `=N` — завжди значення N (0..255), напр. канал режиму, що має стояти на «ручному RGB»;
//! * `macro` — вибір вбудованої програми (пресету) приладу; `speed` — швидкість програми.
//!
//! Якщо колірна роль повторюється (`r g b r g b …`), прилад має кілька сегментів:
//! k-те входження ролі належить сегменту k (LED-бари з незалежними секціями).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{DMX_CHANNELS, Result, bail};

/// Колірні ролі в порядку інтерфейсу.
pub const COLOR_KEYS: [&str; 6] = ["r", "g", "b", "w", "a", "uv"];
/// Усі ролі, крім `=N`.
pub const ROLES: [&str; 12] = ["r", "g", "b", "w", "a", "uv", "dim", "strobe", "zero", "full", "macro", "speed"];

/// Профілі: назва → ролі каналів (порядок важливий для меню).
pub type Profiles = IndexMap<String, Vec<String>>;

pub const WASH243: &str = "Free Color WASH 243";

/// Режими WASH 243 за інструкцією виробника: (режим, назва профілю, ролі).
pub fn wash243_modes() -> Vec<(String, String, Vec<String>)> {
    let rgb = |n: usize| roles(&["r", "g", "b"].repeat(n));
    vec![
        ("P001 · 8 сегментів".into(), format!("{WASH243} · 24 кан. P001 (8 сегментів)"), rgb(8)),
        ("L001 · 4 сегменти".into(), format!("{WASH243} · 12 кан. L001 (4 сегменти)"), rgb(4)),
        (
            "H001 · Dim, строб, програми, RGB".into(),
            format!("{WASH243} · 8 кан. H001 (Dim, строб, програми, RGB)"),
            // CH8 = 0, інакше CH5–7 не працюють
            roles(&["dim", "strobe", "macro", "speed", "r", "g", "b", "zero"]),
        ),
        ("E001 · 2 сегменти".into(), format!("{WASH243} · 6 кан. E001 (2 сегменти)"), rgb(2)),
        ("d001 · Dim+RGB".into(), format!("{WASH243} · 4 кан. d001 (Dim+RGB)"), roles(&["dim", "r", "g", "b"])),
        ("C001 · RGB".into(), format!("{WASH243} · 3 кан. C001 (RGB)"), rgb(1)),
        ("A001 · програми".into(), format!("{WASH243} · 2 кан. A001 (програми)"), roles(&["macro", "speed"])),
    ]
}

/// Профілі моделей, які програма знає сама (додаються, щойно потрібні приладу).
pub fn model_profiles() -> Profiles {
    wash243_modes().into_iter().map(|(_, name, r)| (name, r)).collect()
}

/// Загальні профілі — для невідомого приладу, якому потрібне лише базове керування кольором.
pub fn default_profiles() -> Profiles {
    [
        ("RGB (3 кан.)", &["r", "g", "b"][..]),
        ("RGBW (4 кан.)", &["r", "g", "b", "w"][..]),
        ("Dim+RGB (4 кан.)", &["dim", "r", "g", "b"][..]),
        ("RGB+Dim (4 кан.)", &["r", "g", "b", "dim"][..]),
    ]
    .into_iter()
    .map(|(n, r)| (n.to_string(), roles(r)))
    .collect()
}

pub fn roles(r: &[&str]) -> Vec<String> {
    r.iter().map(|s| s.to_string()).collect()
}

/// Модель з бібліотеки, з якою додано прилад. Її профіль належить приладу.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Model {
    /// виробник
    pub m: String,
    /// модель
    pub n: String,
    /// режим DMX
    #[serde(default)]
    pub mode: String,
}

impl Model {
    /// Модель лише з непорожніми виробником і назвою.
    pub fn valid(self) -> Option<Model> {
        (!self.m.is_empty() && !self.n.is_empty()).then_some(self)
    }
}

/// Колір сегмента або `None` — колір приладу.
pub type Segs = Vec<Option<[u8; 3]>>;

fn default_dimmer() -> u8 {
    255
}
fn default_speed() -> u8 {
    128
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fixture {
    pub id: String,
    pub name: String,
    /// 1..512
    pub address: u16,
    pub profile: String,
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
    #[serde(default = "default_dimmer")]
    pub dimmer: u8,
    #[serde(default)]
    pub strobe: u8,
    /// Кольори окремих сегментів; `None` — прилад світить одним кольором.
    #[serde(default)]
    pub segs: Option<Segs>,
    /// Вбудована програма (пресет): номер у [`PROGRAMS`] (1..N).
    #[serde(default)]
    pub preset: Option<u8>,
    /// Швидкість програми 0..255.
    #[serde(default = "default_speed")]
    pub speed: u8,
    #[serde(default)]
    pub model: Option<Model>,
}

/// Значення колірних каналів 0..255 (дробові — після ефектів і кросфейду).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub w: f64,
    pub a: f64,
    pub uv: f64,
}

impl Color {
    pub fn rgb(r: f64, g: f64, b: f64) -> Color {
        Color { r, g, b, ..Default::default() }
    }

    pub fn get(&self, role: &str) -> f64 {
        match role {
            "r" => self.r,
            "g" => self.g,
            "b" => self.b,
            "w" => self.w,
            "a" => self.a,
            "uv" => self.uv,
            _ => 0.0,
        }
    }

    pub fn scaled(self, k: f64) -> Color {
        Color { r: self.r * k, g: self.g * k, b: self.b * k, w: self.w * k, a: self.a * k, uv: self.uv * k }
    }
}

impl Fixture {
    pub fn new(id: String, name: String, address: u16, profile: String, model: Option<Model>) -> Fixture {
        Fixture {
            id,
            name,
            address,
            profile,
            r: 0,
            g: 0,
            b: 0,
            w: 0,
            a: 0,
            uv: 0,
            dimmer: 255,
            strobe: 0,
            segs: None,
            preset: None,
            speed: 128,
            model,
        }
    }

    pub fn color(&self) -> Color {
        Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            w: self.w as f64,
            a: self.a as f64,
            uv: self.uv as f64,
        }
    }

    pub fn channel(&self, key: &str) -> u8 {
        match key {
            "r" => self.r,
            "g" => self.g,
            "b" => self.b,
            "w" => self.w,
            "a" => self.a,
            "uv" => self.uv,
            "dimmer" => self.dimmer,
            "strobe" => self.strobe,
            _ => 0,
        }
    }

    pub fn set_channel(&mut self, key: &str, v: u8) {
        match key {
            "r" => self.r = v,
            "g" => self.g = v,
            "b" => self.b = v,
            "w" => self.w = v,
            "a" => self.a = v,
            "uv" => self.uv = v,
            "dimmer" => self.dimmer = v,
            "strobe" => self.strobe = v,
            _ => {}
        }
    }

    /// Колір сегмента k: власний, якщо задано, інакше колір приладу.
    pub fn seg_color(&self, k: usize) -> Color {
        let mut c = self.color();
        if let Some(Some(s)) = self.segs.as_ref().and_then(|s| s.get(k)) {
            c.r = s[0] as f64;
            c.g = s[1] as f64;
            c.b = s[2] as f64;
        }
        c
    }

    pub fn set_seg(&mut self, k: usize, rgb: [u8; 3], count: usize) {
        let mut segs = self.segs.take().unwrap_or_default();
        if segs.len() < count {
            segs.resize(count, None);
        }
        segs[k] = Some(rgb);
        segs.truncate(count);
        self.segs = Some(segs);
    }
}

/// Вбудована програма WASH 243: назва, група, початок і кінець DMX-діапазону.
pub struct Program {
    pub name: String,
    pub group: &'static str,
    pub lo: u8,
    pub hi: u8,
}

/// Вбудовані програми WASH 243 (CH1 у режимі A001, CH3 у H001). 000–005 — «без ефекту».
pub fn programs() -> &'static [Program] {
    use std::sync::OnceLock;
    static P: OnceLock<Vec<Program>> = OnceLock::new();
    P.get_or_init(|| {
        let p = |name: &str, group: &'static str, lo: u8, hi: u8| Program { name: name.into(), group, lo, hi };
        let mut v = vec![
            p("Червоний", "color", 6, 16),
            p("Зелений", "color", 17, 27),
            p("Синій", "color", 28, 38),
            p("Жовтий", "color", 39, 49),
            p("Фіолетовий", "color", 50, 60),
            p("Блакитний", "color", 61, 71),
            p("Білий", "color", 72, 82),
        ];
        for i in 1..=9u8 {
            v.push(p(&format!("Ефект {i}"), "effect", 83 + 11 * (i - 1), 93 + 11 * (i - 1)));
        }
        v.extend([
            p("Червоний градієнт", "gradient", 182, 192),
            p("Зелений градієнт", "gradient", 193, 203),
            p("Синій градієнт", "gradient", 204, 214),
            p("Фіолетовий градієнт", "gradient", 215, 225),
            p("Жовтий градієнт", "gradient", 226, 236),
            p("Круговий градієнт", "gradient", 237, 247),
            p("Від звуку", "sound", 248, 255),
        ]);
        v
    })
}

pub fn preset_count() -> u8 {
    programs().len() as u8
}

/// DMX-значення програми n (1..N) — середина її діапазону.
pub fn preset_level(n: u8) -> Result<u8> {
    let count = preset_count();
    if !(1..=count).contains(&n) {
        bail!("пресет має бути 1..{count}");
    }
    let p = &programs()[n as usize - 1];
    Ok(((p.lo as u16 + p.hi as u16) / 2) as u8)
}

/// Пресети (вбудовані програми) — лише в профілів з каналом програми.
pub fn has_presets(channels: &[String]) -> bool {
    channels.iter().any(|c| c == "macro")
}

/// Кількість сегментів = максимум повторів однієї колірної ролі.
pub fn segment_count(channels: &[String]) -> usize {
    COLOR_KEYS.iter().map(|k| channels.iter().filter(|c| c == k).count()).max().unwrap_or(0).max(1)
}

/// Округлення до 0..255 (банківське: .5 — до парного).
pub fn clamp8(v: f64) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v.round_ties_even() as u8
    }
}

/// Значення ролі «=N» або `None`, якщо роль не фіксована.
pub fn fixed_value(role: &str) -> Option<u8> {
    let digits = role.strip_prefix('=')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|v| *v <= 255).map(|v| v as u8)
}

pub fn validate_profile(channels: &[String]) -> Result<()> {
    if channels.is_empty() || channels.len() > DMX_CHANNELS {
        bail!("профіль має містити 1..512 каналів");
    }
    let bad: Vec<&str> = channels
        .iter()
        .filter(|c| !ROLES.contains(&c.as_str()) && fixed_value(c).is_none())
        .map(|c| c.as_str())
        .collect();
    if !bad.is_empty() {
        bail!("невідомі ролі каналів: {}", bad.join(", "));
    }
    Ok(())
}

pub fn validate_fixture(fx: &Fixture, profiles: &Profiles) -> Result<()> {
    let Some(ch) = profiles.get(&fx.profile) else {
        bail!("невідомий профіль «{}»", fx.profile);
    };
    let max = DMX_CHANNELS - ch.len() + 1;
    if !(1..=max).contains(&(fx.address as usize)) {
        bail!("адреса має бути в межах 1..{max} для профілю з {} каналів", ch.len());
    }
    Ok(())
}

/// Записує прилад у всесвіт.
///
/// `colors` — по одному кольору на сегмент (значення 0..255 уже з ефектами);
/// `master` — 0..1. Якщо в профілі є канал `dim`, яскравість іде туди,
/// інакше колірні канали масштабуються програмно.
pub fn render_fixture(universe: &mut [u8], fx: &Fixture, channels: &[String], colors: &[Color], master: f64) {
    let has_dim = channels.iter().any(|c| c == "dim");
    let level = fx.dimmer as f64 / 255.0 * master;
    // при нульовому рівні гасимо і кольорові канали — на випадок, якщо прилад ігнорує диммер
    let scale = if has_dim && level > 0.0 { 1.0 } else { level };
    let base = fx.address as usize - 1;
    // немає білого каналу — білий додається до R, G, B (повзунок «Білий» працює і для RGB)
    let mix_white = !channels.iter().any(|c| c == "w");
    let mut seen = [0usize; 6];
    for (i, role) in channels.iter().enumerate() {
        let ch = base + i;
        if ch >= universe.len() {
            break;
        }
        let role = role.as_str();
        let v = if let Some(ci) = COLOR_KEYS.iter().position(|k| *k == role) {
            let k = seen[ci];
            seen[ci] += 1;
            let c = colors[k.min(colors.len() - 1)];
            let mut raw = c.get(role);
            if mix_white && matches!(role, "r" | "g" | "b") {
                raw = (raw + c.w).min(255.0);
            }
            raw * scale
        } else {
            match role {
                "dim" => 255.0 * level,
                "strobe" => fx.strobe as f64,
                // 000–005 — без програми; програма гасне разом з блекаутом / майстром 0
                "macro" => match fx.preset {
                    Some(n) if master > 0.0 => preset_level(n).map(|v| v as f64).unwrap_or(0.0),
                    _ => 0.0,
                },
                "speed" => fx.speed as f64,
                "full" => 255.0,
                _ => fixed_value(role).unwrap_or(0) as f64,
            }
        };
        universe[ch] = clamp8(v);
    }
}

/// Пари приладів, у яких перетинаються діапазони адрес.
pub fn find_overlaps(fixtures: &[Fixture], profiles: &Profiles) -> Vec<(String, String)> {
    let mut spans: Vec<(usize, usize, &Fixture)> = fixtures
        .iter()
        .map(|fx| {
            let n = profiles.get(&fx.profile).map_or(0, |p| p.len());
            let a = fx.address as usize;
            (a, (a + n).saturating_sub(1), fx)
        })
        .collect();
    spans.sort_by_key(|s| s.0);
    let mut out = Vec::new();
    for (i, (_, e1, f1)) in spans.iter().enumerate() {
        for (s2, _, f2) in &spans[i + 1..] {
            if s2 > e1 {
                break;
            }
            out.push((f1.id.clone(), f2.id.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(address: u16, profile: &str) -> Fixture {
        Fixture::new("x".into(), "x".into(), address, profile.into(), None)
    }

    #[test]
    fn fixed_values() {
        assert_eq!(fixed_value("=32"), Some(32));
        assert_eq!(fixed_value("=255"), Some(255));
        assert_eq!(fixed_value("=256"), None);
        assert_eq!(fixed_value("="), None);
        assert_eq!(fixed_value("=-1"), None);
        assert_eq!(fixed_value("r"), None);
    }

    #[test]
    fn profile_validation() {
        assert!(validate_profile(&roles(&["r", "g", "=10", "macro"])).is_ok());
        assert!(validate_profile(&[]).is_err());
        let e = validate_profile(&roles(&["r", "x", "=300"])).unwrap_err();
        assert_eq!(e.0, "невідомі ролі каналів: x, =300");
    }

    #[test]
    fn address_bounds() {
        let p = default_profiles();
        assert!(validate_fixture(&fx(510, "RGB (3 кан.)"), &p).is_ok());
        assert!(validate_fixture(&fx(511, "RGB (3 кан.)"), &p).is_err());
        assert!(validate_fixture(&fx(0, "RGB (3 кан.)"), &p).is_err());
        assert!(validate_fixture(&fx(1, "немає"), &p).is_err());
    }

    #[test]
    fn segments_and_presets() {
        assert_eq!(segment_count(&roles(&["r", "g", "b", "r", "g", "b"])), 2);
        assert_eq!(segment_count(&roles(&["macro", "speed"])), 1);
        assert!(has_presets(&roles(&["macro"])));
        assert_eq!(preset_count(), 23);
        assert_eq!(preset_level(1).unwrap(), 11);
        assert_eq!(preset_level(23).unwrap(), 251);
        assert!(preset_level(24).is_err());
    }

    #[test]
    fn overlaps() {
        let p = default_profiles();
        let mut a = fx(1, "RGB (3 кан.)");
        a.id = "a".into();
        let mut b = fx(3, "RGB (3 кан.)");
        b.id = "b".into();
        let mut c = fx(4, "RGB (3 кан.)");
        c.id = "c".into();
        assert_eq!(find_overlaps(&[a, b, c], &p), vec![("a".to_string(), "b".to_string()), ("b".into(), "c".into())]);
    }

    #[test]
    fn segs() {
        let mut f = fx(1, "x");
        f.r = 9;
        f.set_seg(2, [1, 2, 3], 4);
        assert_eq!(f.segs.as_ref().unwrap().len(), 4);
        assert_eq!(f.seg_color(2).g, 2.0);
        assert_eq!(f.seg_color(0).r, 9.0);
    }
}
