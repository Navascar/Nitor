//! MIDI-контролери: розбір повідомлень, прив'язки «повідомлення → дія» і готовий профіль APC mini mk2.
//! Самі порти відкриває програма (лише Windows); тут — логіка, що перевіряється тестами.

use serde::{Deserialize, Serialize};

use crate::{Result, Rgb, bail};

/// Прив'язка: `key` — «note:канал:номер» або «cc:канал:номер», `action` — дія (як у гарячих клавішах:
/// `blackout`, `clear`, `flash`, `tap`, `static3`, а також `scene:Назва`, `group:Назва`, `master`, `speed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiBinding {
    pub key: String,
    pub action: String,
}

/// Що прийшло з контролера.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiValue {
    /// Кнопку натиснули (`true`) чи відпустили.
    Button(bool),
    /// Повзунок або регулятор 0..127.
    Level(u8),
}

/// Розбирає повідомлення MIDI. Інше (годинник, SysEx тощо) — `None`.
pub fn parse(msg: &[u8]) -> Option<(String, MidiValue)> {
    let (&status, rest) = msg.split_first()?;
    let ch = status & 0x0f;
    match (status & 0xf0, rest) {
        (0x90, [n, v, ..]) => Some((format!("note:{ch}:{n}"), MidiValue::Button(*v > 0))),
        (0x80, [n, ..]) => Some((format!("note:{ch}:{n}"), MidiValue::Button(false))),
        (0xb0, [n, v, ..]) => Some((format!("cc:{ch}:{n}"), MidiValue::Level(*v))),
        _ => None,
    }
}

/// Дія — повзунок (значення 0..127), а не кнопка.
pub fn is_level(action: &str) -> bool {
    matches!(action, "master" | "speed")
}

/// Підпис повідомлення для людини: «Нота 60», «CC 48 (канал 2)».
pub fn key_label(key: &str) -> String {
    let mut p = key.split(':');
    let (kind, ch, n) = (p.next().unwrap_or(""), p.next().unwrap_or("0"), p.next().unwrap_or(""));
    let kind = if kind == "cc" { "CC" } else { "Нота" };
    let ch: u8 = ch.parse().unwrap_or(0);
    if ch == 0 { format!("{kind} {n}") } else { format!("{kind} {n} (канал {})", ch + 1) }
}

/// Прив'язки з налаштувань: до 256, без повторів повідомлень (остання перемагає).
pub fn check(raw: Vec<MidiBinding>) -> Result<Vec<MidiBinding>> {
    let mut out: Vec<MidiBinding> = vec![];
    for b in raw {
        if parse_key(&b.key).is_none() || b.action.is_empty() || b.action.chars().count() > 80 {
            bail!("неправильна прив'язка MIDI");
        }
        out.retain(|o| o.key != b.key);
        out.push(b);
    }
    if out.len() > 256 {
        bail!("забагато прив'язок MIDI (максимум 256)");
    }
    Ok(out)
}

fn parse_key(key: &str) -> Option<(bool, u8, u8)> {
    let mut p = key.split(':');
    let note = match p.next()? {
        "note" => true,
        "cc" => false,
        _ => return None,
    };
    let ch: u8 = p.next()?.parse().ok().filter(|c| *c < 16)?;
    let n: u8 = p.next()?.parse().ok().filter(|n| *n < 128)?;
    p.next().is_none().then_some((note, ch, n))
}

// ------------------------------------------------------------------- APC mini mk2 --
/// Назва порту, за якою впізнаємо APC mini mk2.
pub const APC_PORT: &str = "APC mini mk2";

/// Готовий профіль APC mini mk2 (пади 8×8 — ноти 0..63, рядок 0 знизу):
/// два верхні рядки — статичні кольори, два наступні — сцени, ще один — групи;
/// кнопки праворуч (ноти 112..115) — Blackout, «Очистити», «Спалах», тап; повзунок 1 — швидкість,
/// повзунок 9 (майстер) — майстер-диммер.
pub fn apc_mini_mk2(statics: usize, scenes: &[String], groups: &[String]) -> Vec<MidiBinding> {
    let b = |key: String, action: String| MidiBinding { key, action };
    let mut out = vec![];
    for i in 0..statics.min(16) {
        let note = if i < 8 { 56 + i } else { 48 + i - 8 };
        out.push(b(format!("note:0:{note}"), format!("static{}", i + 1)));
    }
    for (i, s) in scenes.iter().take(16).enumerate() {
        let note = if i < 8 { 40 + i } else { 32 + i - 8 };
        out.push(b(format!("note:0:{note}"), format!("scene:{s}")));
    }
    for (i, g) in groups.iter().take(8).enumerate() {
        out.push(b(format!("note:0:{}", 24 + i), format!("group:{g}")));
    }
    for (n, a) in [(112, "blackout"), (113, "clear"), (114, "flash"), (115, "tap")] {
        out.push(b(format!("note:0:{n}"), a.into()));
    }
    out.push(b("cc:0:48".into(), "speed".into()));
    out.push(b("cc:0:56".into(), "master".into()));
    out
}

/// Повідомлення, що засвічує пад APC mini mk2 кольором (канал 6 — повна яскравість).
/// Не пад (кнопки праворуч, повзунки) — `None`.
pub fn apc_led(key: &str, color: Rgb) -> Option<[u8; 3]> {
    let (true, 0, note @ 0..=63) = parse_key(key)? else { return None };
    Some([0x96, note, apc_color(color)])
}

/// Найближчий колір палітри APC mini mk2.
// shortcut: 10 основних кольорів палітри замість усіх 128, якщо кольори падів не збігатимуться — RGB через SysEx
fn apc_color(c: Rgb) -> u8 {
    const PAL: [(u8, [i32; 3]); 10] = [
        (3, [255, 255, 255]),
        (5, [255, 0, 0]),
        (9, [255, 84, 0]),
        (13, [255, 255, 0]),
        (21, [0, 255, 0]),
        (33, [0, 255, 255]),
        (41, [0, 128, 255]),
        (45, [0, 0, 255]),
        (49, [128, 0, 255]),
        (53, [255, 0, 255]),
    ];
    if c == Rgb::BLACK {
        return 0;
    }
    let d = |p: [i32; 3]| {
        let (r, g, b) = (c.0 as i32 - p[0], c.1 as i32 - p[1], c.2 as i32 - p[2]);
        r * r + g * g + b * b
    };
    PAL.iter().min_by_key(|(_, p)| d(*p)).map_or(0, |(i, _)| *i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages() {
        assert_eq!(parse(&[0x90, 60, 100]), Some(("note:0:60".into(), MidiValue::Button(true))));
        assert_eq!(parse(&[0x91, 60, 0]), Some(("note:1:60".into(), MidiValue::Button(false))));
        assert_eq!(parse(&[0x80, 60, 64]), Some(("note:0:60".into(), MidiValue::Button(false))));
        assert_eq!(parse(&[0xb0, 48, 127]), Some(("cc:0:48".into(), MidiValue::Level(127))));
        assert_eq!(parse(&[0xf8]), None, "годинник MIDI ігнорується");
        assert_eq!(parse(&[0x90, 60]), None, "обрізане повідомлення");
        assert_eq!(key_label("note:0:60"), "Нота 60");
        assert_eq!(key_label("cc:1:48"), "CC 48 (канал 2)");
    }

    #[test]
    fn apc_profile_and_leds() {
        let p = apc_mini_mk2(13, &["Хор".into()], &["Сцена".into()]);
        let find = |a: &str| p.iter().find(|b| b.action == a).map(|b| b.key.as_str());
        assert_eq!(find("static1"), Some("note:0:56"), "лівий верхній пад");
        assert_eq!(find("static9"), Some("note:0:48"));
        assert_eq!(find("scene:Хор"), Some("note:0:40"));
        assert_eq!(find("group:Сцена"), Some("note:0:24"));
        assert_eq!(find("master"), Some("cc:0:56"));
        assert_eq!(check(p.clone()).unwrap().len(), p.len(), "профіль без конфліктів");
        assert_eq!(apc_led("note:0:56", Rgb(255, 0, 0)), Some([0x96, 56, 5]));
        assert_eq!(apc_led("note:0:56", Rgb(255, 180, 0)), Some([0x96, 56, 13]), "жовтий");
        assert_eq!(apc_led("note:0:112", Rgb(255, 0, 0)), None, "кнопка праворуч — не пад");
        assert_eq!(apc_led("cc:0:48", Rgb(255, 0, 0)), None);
    }

    #[test]
    fn check_rejects_bad_keys_and_dedups() {
        let b = |k: &str, a: &str| MidiBinding { key: k.into(), action: a.into() };
        assert!(check(vec![b("note:0:200", "clear")]).is_err());
        assert!(check(vec![b("pitch:0:1", "clear")]).is_err());
        let v = check(vec![b("note:0:1", "clear"), b("note:0:1", "blackout")]).unwrap();
        assert_eq!(v, vec![b("note:0:1", "blackout")]);
    }
}
