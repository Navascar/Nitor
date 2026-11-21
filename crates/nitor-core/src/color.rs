//! Кольори: RGB із записом `#rrggbb` і перетворення HSV → RGB.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);

    /// Розбирає `#rrggbb` (решітка необов'язкова).
    pub fn from_hex(s: &str) -> Result<Rgb> {
        let h = s.trim_start_matches('#');
        if h.len() != 6 || !h.is_ascii() {
            bail!("некоректний колір {s:?}");
        }
        let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16);
        match (p(0), p(2), p(4)) {
            (Ok(r), Ok(g), Ok(b)) => Ok(Rgb(r, g, b)),
            _ => bail!("некоректний колір {s:?}"),
        }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn arr(self) -> [u8; 3] {
        [self.0, self.1, self.2]
    }
}

impl From<[u8; 3]> for Rgb {
    fn from(a: [u8; 3]) -> Self {
        Rgb(a[0], a[1], a[2])
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// HSV (усе 0..1) → RGB (0..1), точно як `colorsys.hsv_to_rgb`.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).trunc();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// RGB (0..255) → HSV (0..1) — для колірного кола інтерфейсу.
pub fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(Rgb::from_hex("#ff8001").unwrap(), Rgb(255, 128, 1));
        assert_eq!(Rgb::from_hex("00ff00").unwrap(), Rgb(0, 255, 0));
        assert_eq!(Rgb(1, 2, 3).hex(), "#010203");
        assert!(Rgb::from_hex("#fff").is_err());
        assert!(Rgb::from_hex("#gg0000").is_err());
        assert!(Rgb::from_hex("#ффф").is_err());
    }

    #[test]
    fn hsv_matches_colorsys() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (1.0, 0.0, 0.0));
        let (r, g, b) = hsv_to_rgb(0.5, 1.0, 1.0);
        assert_eq!((r, g, b), (0.0, 1.0, 1.0));
    }
}
