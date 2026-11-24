//! Розклад: сцена, «Очистити» чи Blackout у задану хвилину, на сході чи заході сонця ± хвилини,
//! у вибрані дні тижня. Час — місцевий; програма передає його разом зі зсувом від UTC.

use serde::{Deserialize, Serialize};

use crate::{Result, bail};

/// Від чого відлічується подія.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum At {
    #[default]
    Time,
    Sunrise,
    Sunset,
}

/// Подія розкладу. `minutes`: для `Time` — хвилина доби 0..1439, для сходу/заходу — зсув −720..720.
/// `days` — біти днів тижня, понеділок — біт 0. `action`: `scene:Назва`, `clear` або `blackout`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    #[serde(default = "yes")]
    pub on: bool,
    #[serde(default)]
    pub at: At,
    #[serde(default)]
    pub minutes: i32,
    #[serde(default = "all_days")]
    pub days: u8,
    pub action: String,
}

fn yes() -> bool {
    true
}

pub fn all_days() -> u8 {
    0x7f
}

/// Місце для сходу й заходу сонця.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// Обласні центри України (широта, довгота).
pub const CITIES: [(&str, f64, f64); 25] = [
    ("Київ", 50.45, 30.52),
    ("Вінниця", 49.23, 28.47),
    ("Дніпро", 48.46, 35.05),
    ("Донецьк", 48.00, 37.80),
    ("Житомир", 50.25, 28.66),
    ("Запоріжжя", 47.84, 35.14),
    ("Івано-Франківськ", 48.92, 24.71),
    ("Кропивницький", 48.51, 32.26),
    ("Луганськ", 48.57, 39.31),
    ("Луцьк", 50.75, 25.34),
    ("Львів", 49.84, 24.03),
    ("Миколаїв", 46.98, 32.00),
    ("Одеса", 46.48, 30.72),
    ("Полтава", 49.59, 34.55),
    ("Рівне", 50.62, 26.25),
    ("Сімферополь", 44.95, 34.10),
    ("Суми", 50.91, 34.80),
    ("Тернопіль", 49.55, 25.59),
    ("Ужгород", 48.62, 22.29),
    ("Харків", 49.99, 36.23),
    ("Херсон", 46.64, 32.62),
    ("Хмельницький", 49.42, 26.99),
    ("Черкаси", 49.44, 32.06),
    ("Чернівці", 48.29, 25.94),
    ("Чернігів", 51.50, 31.29),
];

/// Дні від 1970-01-01 для дати (алгоритм Говарда Хіннанта).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Схід і захід сонця (хвилини від півночі UTC) у день `day` (дні від 1970-01-01).
/// `None` — полярний день чи ніч. Формула NOAA, точність — кілька хвилин.
pub fn sun_utc(day: i64, lat: f64, lon: f64) -> Option<(f64, f64)> {
    use std::f64::consts::PI;
    let jan1 = |y: i64| days_from_civil(y, 1, 1);
    let mut y = 1970 + day.div_euclid(366);
    while jan1(y + 1) <= day {
        y += 1;
    }
    while jan1(y) > day {
        y -= 1;
    }
    let n = (day - jan1(y)) as f64; // день року від 0
    let len = (jan1(y + 1) - jan1(y)) as f64;
    let g = 2.0 * PI / len * n;
    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos()
            - 0.032077 * g.sin()
            - 0.014615 * (2.0 * g).cos()
            - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    let lat = lat.to_radians();
    let c = 90.833_f64.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan();
    if !(-1.0..=1.0).contains(&c) {
        return None;
    }
    let ha = c.acos().to_degrees();
    Some((720.0 - 4.0 * (lon + ha) - eqtime, 720.0 - 4.0 * (lon - ha) - eqtime))
}

/// Хвилина місцевої доби, коли подія спрацьовує в день `day` (`None` — не в цей день або немає сонця).
pub fn minute_of(e: &Event, place: Option<&Place>, day: i64, offset_min: i32) -> Option<i64> {
    let weekday = (day + 3).rem_euclid(7); // 1970-01-01 — четвер, понеділок = 0
    if !e.on || e.days & (1 << weekday) == 0 {
        return None;
    }
    let sun = |rise: bool| {
        let p = place?;
        let (r, s) = sun_utc(day, p.lat, p.lon)?;
        Some((if rise { r } else { s }).round() as i64 + offset_min as i64 + e.minutes as i64)
    };
    let m = match e.at {
        At::Time => e.minutes as i64,
        At::Sunrise => sun(true)?,
        At::Sunset => sun(false)?,
    };
    (0..1440).contains(&m).then_some(m)
}

/// Дії, що мають спрацювати в хвилину `local_min` (хвилини місцевого часу від 1970-01-01).
pub fn due(events: &[Event], place: Option<&Place>, local_min: i64, offset_min: i32) -> Vec<String> {
    let (day, m) = (local_min.div_euclid(1440), local_min.rem_euclid(1440));
    events.iter().filter(|e| minute_of(e, place, day, offset_min) == Some(m)).map(|e| e.action.clone()).collect()
}

/// Розклад із налаштувань: до 64 подій у допустимих межах.
pub fn check(events: Vec<Event>) -> Result<Vec<Event>> {
    if events.len() > 64 {
        bail!("забагато подій розкладу (максимум 64)");
    }
    for e in &events {
        let range = if e.at == At::Time { 0..=1439 } else { -720..=720 };
        let action_ok = matches!(e.action.as_str(), "clear" | "blackout") || e.action.starts_with("scene:");
        if !range.contains(&e.minutes) || !action_ok || e.action.chars().count() > 80 {
            bail!("неправильна подія розкладу");
        }
    }
    Ok(events)
}

pub fn check_place(p: Option<Place>) -> Result<Option<Place>> {
    if let Some(p) = &p {
        if !(-90.0..=90.0).contains(&p.lat) || !(-180.0..=180.0).contains(&p.lon) || p.name.chars().count() > 60 {
            bail!("неправильні координати");
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kyiv() -> Place {
        Place { name: "Київ".into(), lat: 50.45, lon: 30.52 }
    }

    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 24), 20781);
        assert_eq!((days_from_civil(2026, 11, 24) + 3).rem_euclid(7), 1, "24.11.2026 — вівторок");
    }

    #[test]
    fn sun_in_kyiv() {
        // 21 червня: схід ≈ 04:46, захід ≈ 21:13 за київським (UTC+3)
        let (r, s) = sun_utc(days_from_civil(2026, 6, 21), 50.45, 30.52).unwrap();
        assert!((r + 180.0 - (4.0 * 60.0 + 46.0)).abs() < 5.0, "схід {r}");
        assert!((s + 180.0 - (21.0 * 60.0 + 13.0)).abs() < 5.0, "захід {s}");
        // 21 грудня: схід ≈ 07:56, захід ≈ 15:55 (UTC+2)
        let (r, s) = sun_utc(days_from_civil(2026, 12, 21), 50.45, 30.52).unwrap();
        assert!((r + 120.0 - (7.0 * 60.0 + 56.0)).abs() < 5.0, "схід {r}");
        assert!((s + 120.0 - (15.0 * 60.0 + 55.0)).abs() < 5.0, "захід {s}");
        assert_eq!(sun_utc(days_from_civil(2026, 6, 21), 80.0, 0.0), None, "полярний день");
    }

    #[test]
    fn events_fire_once_on_their_minute() {
        let day = days_from_civil(2026, 11, 24); // вівторок
        let at = |h: i64, m: i64| day * 1440 + h * 60 + m;
        let time = Event { on: true, at: At::Time, minutes: 23 * 60, days: all_days(), action: "blackout".into() };
        let sunset = Event { on: true, at: At::Sunset, minutes: -15, days: 1 << 1, action: "scene:Вечір".into() };
        let ev = [time.clone(), sunset.clone()];
        let p = kyiv();
        assert_eq!(due(&ev, Some(&p), at(23, 0), 120), vec!["blackout".to_string()]);
        assert!(due(&ev, Some(&p), at(23, 1), 120).is_empty());
        let m = minute_of(&sunset, Some(&p), day, 120).unwrap();
        assert!((15 * 60..16 * 60).contains(&m), "захід мінус 15 хв у листопаді — між 15:00 і 16:00, а не {m}");
        assert_eq!(due(&ev, Some(&p), day * 1440 + m, 120), vec!["scene:Вечір".to_string()]);
        assert_eq!(minute_of(&sunset, Some(&p), day + 1, 120), None, "лише у вівторок");
        assert_eq!(minute_of(&sunset, None, day, 120), None, "без місця сонце невідоме");
        assert_eq!(minute_of(&Event { on: false, ..time }, None, day, 120), None);
    }

    #[test]
    fn check_limits() {
        let e = |at, minutes, action: &str| Event { on: true, at, minutes, days: 1, action: action.into() };
        assert!(check(vec![e(At::Time, 1440, "clear")]).is_err());
        assert!(check(vec![e(At::Sunset, -30, "scene:Х")]).is_ok());
        assert!(check(vec![e(At::Time, 60, "rm -rf")]).is_err());
        assert!(check_place(Some(Place { name: "x".into(), lat: 91.0, lon: 0.0 })).is_err());
    }
}
