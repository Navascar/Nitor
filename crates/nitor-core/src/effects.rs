//! Динамічні ефекти. Кожен ефект — чиста функція часу та позиції приладу в групі.

use serde::{Deserialize, Serialize};

use crate::Rgb;
use crate::color::hsv_to_rgb;
use crate::fixtures::Color;
use crate::pyrandom::PyRandom;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EffectKind {
    #[default]
    None,
    Rainbow,
    Fade,
    Chase,
    Pulse,
    Strobe,
    Random,
    Fire,
}

impl EffectKind {
    pub const ALL: [EffectKind; 8] = [
        EffectKind::None,
        EffectKind::Rainbow,
        EffectKind::Fade,
        EffectKind::Chase,
        EffectKind::Pulse,
        EffectKind::Strobe,
        EffectKind::Random,
        EffectKind::Fire,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EffectKind::None => "Без ефекту",
            EffectKind::Rainbow => "Веселка",
            EffectKind::Fade => "Плавна зміна кольорів",
            EffectKind::Chase => "Біжучий вогонь",
            EffectKind::Pulse => "Пульсація",
            EffectKind::Strobe => "Стробоскоп",
            EffectKind::Random => "Випадкові кольори",
            EffectKind::Fire => "Вогонь",
        }
    }
}

pub const SPEED_MIN: f64 = 0.01;
pub const SPEED_MAX: f64 = 20.0;
pub const PALETTE_MAX: usize = 16;

fn default_speed() -> f64 {
    0.5
}
fn default_spread() -> f64 {
    1.0
}
fn yes() -> bool {
    true
}

/// Ефект: тип, прилади, швидкість (циклів/с), розкид фази 0..1, палітра, «по сегментах».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    #[serde(rename = "type", default)]
    pub kind: EffectKind,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default = "default_spread")]
    pub spread: f64,
    /// Порожньо — власний колір приладу.
    #[serde(default)]
    pub colors: Vec<Rgb>,
    #[serde(default = "yes")]
    pub by_segment: bool,
}

impl Default for Effect {
    fn default() -> Self {
        Effect { kind: EffectKind::None, ids: vec![], speed: 0.5, spread: 1.0, colors: vec![], by_segment: true }
    }
}

impl Effect {
    /// Обмежує значення допустимими межами (ефект із файлу іншого комп'ютера тощо).
    pub fn clamped(mut self) -> Effect {
        let num = |v: f64, d: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.speed = num(self.speed, 0.5, SPEED_MIN, SPEED_MAX);
        self.spread = num(self.spread, 1.0, 0.0, 1.0);
        self.colors.truncate(PALETTE_MAX);
        self
    }
}

fn rgbc(c: Rgb) -> Color {
    Color::rgb(c.0 as f64, c.1 as f64, c.2 as f64)
}

fn lerp(c1: Rgb, c2: Rgb, t: f64) -> Color {
    let l = |a: u8, b: u8| a as f64 + (b as f64 - a as f64) * t;
    Color::rgb(l(c1.0, c2.0), l(c1.1, c2.1), l(c1.2, c2.2))
}

/// Залишок як `%` у Python (знак дільника).
fn pymod(a: f64, b: f64) -> f64 {
    a.rem_euclid(b)
}

/// Колір «пікселя» з номером `index` (0..count-1) у момент `t` (секунди).
/// `base` — власний колір приладу.
pub fn effect_color(effect: &Effect, index: usize, count: usize, t: f64, base: Color) -> Color {
    let speed = effect.speed;
    let spread = effect.spread;
    let palette = &effect.colors;
    let count = count.max(1);
    let (fi, fc) = (index as f64, count as f64);
    let offset = fi / fc * spread;
    let phase = t * speed + offset;

    match effect.kind {
        EffectKind::None => base,
        EffectKind::Rainbow => {
            let (r, g, b) = hsv_to_rgb(pymod(phase, 1.0), 1.0, 1.0);
            Color::rgb(r * 255.0, g * 255.0, b * 255.0)
        }
        EffectKind::Fade => {
            let default = [Rgb(255, 0, 0), Rgb(0, 255, 0), Rgb(0, 0, 255)];
            let pal: &[Rgb] = if palette.is_empty() { &default } else { palette };
            let pos = pymod(phase, 1.0) * pal.len() as f64;
            let i = (pos.trunc() as usize) % pal.len();
            let frac = pos - pos.trunc();
            let frac = 0.5 - 0.5 * (std::f64::consts::PI * frac).cos(); // м'які переходи
            lerp(pal[i], pal[(i + 1) % pal.len()], frac)
        }
        EffectKind::Chase => {
            // «голова» пробігає по приладах, позаду — згасаючий хвіст
            let head = pymod(t * speed * fc, fc);
            let dist = pymod(head - fi, fc);
            let k = (1.0 - dist / (fc * 0.35 * spread + 0.6).max(1.0)).max(0.0);
            let src =
                if palette.is_empty() { base } else { rgbc(palette[((t * speed).trunc() as usize) % palette.len()]) };
            src.scaled(k)
        }
        EffectKind::Pulse => {
            let k = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * phase).cos();
            let src = if palette.is_empty() { base } else { rgbc(palette[(phase.trunc() as usize) % palette.len()]) };
            src.scaled(k)
        }
        EffectKind::Strobe => {
            let on = pymod(phase, 1.0) < 0.2;
            let src =
                if palette.is_empty() { base } else { rgbc(palette[((t * speed).trunc() as usize) % palette.len()]) };
            if on { src } else { src.scaled(0.0) }
        }
        EffectKind::Random => {
            let step = (t * speed + offset).trunc() as u64;
            let mut rng = PyRandom::new(step * 7919 + index as u64 * 104729);
            if palette.is_empty() {
                let (r, g, b) = hsv_to_rgb(rng.random(), 1.0, 1.0);
                Color::rgb(r * 255.0, g * 255.0, b * 255.0)
            } else {
                rgbc(*rng.choice(palette))
            }
        }
        EffectKind::Fire => {
            let n = ((t * speed * 7.3 + fi * 1.7).sin()
                + (t * speed * 13.1 + fi * 3.1).sin()
                + (t * speed * 23.7 + fi * 0.7).sin())
                / 3.0;
            let k = 0.65 + 0.35 * n;
            Color::rgb(255.0 * k, (70.0 + 60.0 * n) * k, 0.0)
        }
    }
}
