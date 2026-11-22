//! Гарячі клавіші. Комбінація — «Ctrl+Shift+Alt+код»: код фізичної клавіші, тож клавіші працюють
//! на будь-якій розкладці. Власні призначення зберігаються в рушії, стандартні — тут.

use egui::{Key, Modifiers};
use indexmap::IndexMap;

pub enum Entry {
    Group(&'static str),
    Action { id: String, label: String, keys: Vec<String> },
}

pub fn table() -> Vec<Entry> {
    let a = |id: &str, label: &str, keys: &[&str]| Entry::Action {
        id: id.into(),
        label: label.into(),
        keys: keys.iter().map(|s| s.to_string()).collect(),
    };
    let mut t = vec![
        Entry::Group("Світло"),
        a("blackout", "Blackout — вимкнути все світло", &["KeyB"]),
        a("flash", "Спалах — поки клавішу тримають", &["KeyF"]),
        a("tap", "Тап темпу для «у такт музиці»", &["KeyT"]),
        a("clear", "Очистити кольори", &["Backspace", "Digit0"]),
        a("effectStop", "Зупинити всі ефекти", &[]),
        Entry::Group("Вибір приладів"),
        a("selectAll", "Вибрати всі прилади", &["Ctrl+KeyA"]),
        a("selectNone", "Зняти вибір", &["Escape"]),
        a("selectInvert", "Інвертувати вибір", &[]),
        a("deleteFixtures", "Видалити вибрані прилади (з підтвердженням)", &["Delete"]),
        Entry::Group("Скасування"),
        a("undo", "Скасувати", &["Ctrl+KeyZ"]),
        a("redo", "Повторити", &["Ctrl+KeyY", "Ctrl+Shift+KeyZ"]),
        Entry::Group("Палітра кольорів"),
    ];
    for n in 1..=9 {
        t.push(Entry::Action {
            id: format!("static{n}"),
            label: format!("Колір палітри {n}"),
            keys: vec![format!("Digit{n}")],
        });
    }
    t.push(Entry::Group("Вікна"));
    t.push(a("settings", "Налаштування", &[]));
    t
}

pub fn defaults(id: &str) -> Vec<String> {
    table()
        .into_iter()
        .find_map(|e| match e {
            Entry::Action { id: i, keys, .. } if i == id => Some(keys),
            _ => None,
        })
        .unwrap_or_default()
}

pub fn label_of(id: &str) -> String {
    table()
        .into_iter()
        .find_map(|e| match e {
            Entry::Action { id: i, label, .. } if i == id => Some(label),
            _ => None,
        })
        .unwrap_or_default()
}

pub fn action_ids() -> Vec<String> {
    table()
        .into_iter()
        .filter_map(|e| match e {
            Entry::Action { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// Клавіші дії: власні або стандартні.
pub fn keys_for(custom: &IndexMap<String, Vec<String>>, id: &str) -> Vec<String> {
    custom.get(id).cloned().unwrap_or_else(|| defaults(id))
}

pub fn action_for(custom: &IndexMap<String, Vec<String>>, combo: &str) -> Option<String> {
    action_ids().into_iter().find(|id| keys_for(custom, id).iter().any(|k| k == combo))
}

/// Код фізичної клавіші (як `KeyboardEvent.code`).
pub fn code(key: Key) -> Option<String> {
    use Key::*;
    let s = match key {
        ArrowDown => "ArrowDown",
        ArrowLeft => "ArrowLeft",
        ArrowRight => "ArrowRight",
        ArrowUp => "ArrowUp",
        Escape => "Escape",
        Tab => "Tab",
        Backspace => "Backspace",
        Enter => "Enter",
        Space => "Space",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Minus => "Minus",
        Equals | Plus => "Equal",
        OpenBracket => "BracketLeft",
        CloseBracket => "BracketRight",
        Semicolon => "Semicolon",
        Quote => "Quote",
        Backtick => "Backquote",
        Backslash => "Backslash",
        Comma => "Comma",
        Period => "Period",
        Slash => "Slash",
        Num0 => "Digit0",
        Num1 => "Digit1",
        Num2 => "Digit2",
        Num3 => "Digit3",
        Num4 => "Digit4",
        Num5 => "Digit5",
        Num6 => "Digit6",
        Num7 => "Digit7",
        Num8 => "Digit8",
        Num9 => "Digit9",
        _ => {
            let n = key.name();
            if n.len() == 1 && n.chars().all(|c| c.is_ascii_uppercase()) {
                return Some(format!("Key{n}"));
            }
            if n.starts_with('F') && n[1..].chars().all(|c| c.is_ascii_digit()) {
                return Some(n.to_string());
            }
            return None;
        }
    };
    Some(s.to_string())
}

pub fn combo(key: Key, m: Modifiers) -> Option<String> {
    let code = code(key)?;
    let mut parts = vec![];
    if m.ctrl || m.command || m.mac_cmd {
        parts.push("Ctrl".to_string());
    }
    if m.shift {
        parts.push("Shift".into());
    }
    if m.alt {
        parts.push("Alt".into());
    }
    parts.push(code);
    Some(parts.join("+"))
}

/// Підпис комбінації для людини: «Ctrl+Z», «Del», «1».
pub fn key_label(combo: &str) -> String {
    combo
        .split('+')
        .map(|p| {
            let named = match p {
                "Escape" => "Esc",
                "Delete" => "Del",
                "Space" => "Пробіл",
                "ArrowUp" => "↑",
                "ArrowDown" => "↓",
                "ArrowLeft" => "←",
                "ArrowRight" => "→",
                "Insert" => "Ins",
                "PageUp" => "PgUp",
                "PageDown" => "PgDn",
                "Minus" => "-",
                "Equal" => "=",
                "BracketLeft" => "[",
                "BracketRight" => "]",
                "Semicolon" => ";",
                "Quote" => "'",
                "Backquote" => "`",
                "Backslash" => "\\",
                "Comma" => ",",
                "Period" => ".",
                "Slash" => "/",
                _ => "",
            };
            if !named.is_empty() {
                named.to_string()
            } else {
                p.trim_start_matches("Key").trim_start_matches("Digit").to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

pub fn short_key(custom: &IndexMap<String, Vec<String>>, id: &str) -> Option<String> {
    keys_for(custom, id).first().map(|k| key_label(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_and_labels() {
        assert_eq!(combo(Key::A, Modifiers::COMMAND).unwrap(), "Ctrl+KeyA");
        assert_eq!(combo(Key::Num1, Modifiers::NONE).unwrap(), "Digit1");
        assert_eq!(key_label("Ctrl+Shift+KeyZ"), "Ctrl+Shift+Z");
        assert_eq!(key_label("Delete"), "Del");
        let custom = IndexMap::new();
        assert_eq!(action_for(&custom, "KeyB").as_deref(), Some("blackout"));
        assert_eq!(action_for(&custom, "Digit3").as_deref(), Some("static3"));
        let mut custom = IndexMap::new();
        custom.insert("blackout".to_string(), vec!["KeyX".to_string()]);
        assert_eq!(action_for(&custom, "KeyB"), None);
        assert_eq!(action_for(&custom, "KeyX").as_deref(), Some("blackout"));
    }
}
