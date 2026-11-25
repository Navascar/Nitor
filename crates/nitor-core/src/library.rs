//! Бібліотека моделей приладів: вбудовані (WASH 243) + зібрана база (`nitor-library`).
//!
//! База зашита в програму (`assets/library.json.gz`) і працює без інтернету.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::fixtures::{WASH243, wash243_modes};

/// Типи приладів у порядку розділів бібліотеки.
pub const TYPES: [&str; 7] = ["bar", "par", "head", "strobe", "matrix", "effect", "other"];
pub const RECENT_LIMIT: usize = 24;

const MAKER_ALIASES: [(&str, &str); 6] = [
    ("american dj", "adj"),
    ("chauvet", "chauvet dj"),
    ("stairville", "thomann"),
    ("eurolite", "steinigke"),
    ("martin", "martin professional"),
    ("elation", "elation professional"),
];

/// Розділи бібліотеки з підписами (порожній ключ — «Усі моделі»).
pub const SECTIONS: [(&str, &str); 9] = [
    ("recent", "Нещодавно використані"),
    ("", "Усі моделі"),
    ("bar", "LED-бари"),
    ("par", "Прожектори PAR і заливка"),
    ("head", "Голови"),
    ("strobe", "Стробоскопи і блайндери"),
    ("matrix", "Матриці й панелі"),
    ("effect", "Ефекти"),
    ("other", "Інше"),
];

/// Режим DMX моделі: назва режиму, ролі каналів і (необов'язково) готова назва профілю.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mode(
    pub String,
    pub Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub Option<String>,
);

impl Mode {
    pub fn name(&self) -> &str {
        &self.0
    }
    pub fn channels(&self) -> &[String] {
        &self.1
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibModel {
    /// виробник
    pub m: String,
    /// модель
    pub n: String,
    /// тип (`TYPES`)
    #[serde(default = "other")]
    pub t: String,
    pub modes: Vec<Mode>,
    #[serde(skip)]
    pub builtin: bool,
}

fn other() -> String {
    "other".into()
}

impl LibModel {
    /// Назва профілю режиму моделі.
    pub fn profile_name(&self, mode: &Mode) -> String {
        match &mode.2 {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("{} · {} ({} кан.)", title(&self.m, &self.n), mode.0, mode.1.len()),
        }
    }

    pub fn mode(&self, name: &str) -> Option<&Mode> {
        self.modes.iter().find(|m| m.0 == name)
    }

    /// Колірні канали моделі (для підпису в бібліотеці): «RGB», «RGBW», «RGB+UV»…
    pub fn color_summary(&self) -> String {
        let mut has = [false; 6];
        for mode in &self.modes {
            for c in &mode.1 {
                if let Some(i) = crate::fixtures::COLOR_KEYS.iter().position(|k| k == c) {
                    has[i] = true;
                }
            }
        }
        let names = ["R", "G", "B", "W", "A", "UV"];
        let mut s: String = names[..4].iter().zip(&has[..4]).filter(|(_, h)| **h).map(|(n, _)| *n).collect();
        if has[4] {
            s.push('A');
        }
        if has[5] {
            s.push_str("+UV");
        }
        s
    }
}

#[derive(Deserialize)]
struct LibraryFile {
    fixtures: Vec<LibModel>,
}

static LIBRARY_GZ: &[u8] = include_bytes!("../assets/library.json.gz");

struct Library {
    models: Vec<LibModel>,
    by_key: HashMap<(String, String), usize>,
}

fn library() -> &'static Library {
    static L: OnceLock<Library> = OnceLock::new();
    L.get_or_init(|| {
        let builtin = LibModel {
            m: "Free Color".into(),
            n: WASH243.trim_start_matches("Free Color ").into(),
            t: "bar".into(),
            modes: wash243_modes().into_iter().map(|(mode, name, roles)| Mode(mode, roles, Some(name))).collect(),
            builtin: true,
        };
        let mut models = vec![builtin];
        models.extend(clones());
        match decode(LIBRARY_GZ) {
            Ok(base) => models.extend(base),
            Err(e) => log::error!("Не вдалося завантажити бібліотеку приладів: {e}"),
        }
        let mut by_key = HashMap::new();
        for (i, m) in models.iter().enumerate() {
            by_key.entry((m.m.clone(), m.n.clone())).or_insert(i);
        }
        Library { models, by_key }
    })
}

/// Виробник безіменних копій: лишається ключем у збережених налаштуваннях, але не показується.
pub const NO_MAKER: &str = "Китайська копія";

/// Виробник для показу (у безіменних копій — порожньо).
pub fn maker(m: &str) -> &str {
    if m == NO_MAKER { "" } else { m }
}

/// «Виробник модель» для показу.
pub fn title(m: &str, n: &str) -> String {
    if maker(m).is_empty() { n.to_string() } else { format!("{m} {n}") }
}

/// Безіменні китайські копії (продаються під різними назвами, напр. «Power Light 5433»).
/// Таблиця каналів — як у Betopper LPC007/LPC008 (посібник виробника): меню A001 — 7 кан.
/// (1 яскравість, 2–4 RGB, 5 строб, 6 вибір режиму: 0–50 — колір з DMX, 7 швидкість), d001 — 3 кан. RGB.
// shortcut: програми й швидкість тримаються на 0 (у копій різні таблиці програм), якщо попросять — окремі пресети
fn clones() -> Vec<LibModel> {
    let r = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let par = |n: &str| LibModel {
        m: NO_MAKER.into(),
        n: n.into(),
        t: "par".into(),
        modes: vec![
            Mode("A001 · 7 кан. (Dim, RGB, строб)".into(), r(&["dim", "r", "g", "b", "strobe", "zero", "zero"]), None),
            Mode("d001 · 3 кан. (RGB)".into(), r(&["r", "g", "b"]), None),
        ],
        builtin: true,
    };
    vec![par("LED PAR 54x3 Вт RGB (5433, Power Light)"), par("LED PAR 18x1 Вт RGB"), par("LED PAR 36x1 Вт RGB")]
}

/// Розпаковує файл бібліотеки (`{"fixtures": [...]}` у gzip).
pub fn decode(gz: &[u8]) -> Result<Vec<LibModel>, String> {
    let mut json = String::new();
    flate2::read::GzDecoder::new(gz).read_to_string(&mut json).map_err(|e| e.to_string())?;
    let file: LibraryFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(file.fixtures)
}

pub fn models() -> &'static [LibModel] {
    &library().models
}

pub fn total() -> usize {
    models().len()
}

/// Модель за виробником і назвою.
pub fn find(maker: &str, name: &str) -> Option<&'static LibModel> {
    let l = library();
    l.by_key.get(&(maker.to_string(), name.to_string())).map(|&i| &l.models[i])
}

fn alias(maker: &str) -> &'static str {
    let low = maker.to_lowercase();
    MAKER_ALIASES.iter().find(|(k, _)| *k == low).map_or("", |(_, v)| v)
}

/// Що пишуть на коробках китайських копій — щоб пошук знаходив відповідну модель бібліотеки.
const TAGS: [(&str, &str, &str); 4] = [
    ("Betopper", "LPC-007", "lpc007 lpc008 54x3 5433 54x1.5"),
    ("Shehds", "LED Flat Par 54x3W", "5433"),
    ("Eyourlife", "LED RGBW 54x3 Par64", "54x3w gohyo coidak gbgs"),
    ("Китайська копія", "LED PAR 54x3 Вт RGB (5433, Power Light)", "54x3w powerlight"),
];

fn matches(fx: &LibModel, words: &[String]) -> bool {
    let tags = TAGS.iter().find(|t| t.0 == fx.m && t.1 == fx.n).map_or("", |t| t.2);
    let text = format!("{} {} {} {tags}", fx.m, alias(&fx.m), fx.n).to_lowercase();
    words.iter().all(|w| text.contains(w.as_str()))
}

fn words(query: &str) -> Vec<String> {
    query.to_lowercase().split_whitespace().map(String::from).collect()
}

/// Ключ нещодавно використаної моделі.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Recent {
    pub m: String,
    pub n: String,
}

fn recent_models(recent: &[Recent]) -> Vec<&'static LibModel> {
    recent.iter().filter_map(|r| find(&r.m, &r.n)).collect()
}

/// Пошук моделей. `kind` — тип, `"recent"` (у порядку використання) або порожньо (усі).
pub fn search(query: &str, limit: usize, kind: &str, offset: usize, recent: &[Recent]) -> Vec<&'static LibModel> {
    let words = words(query);
    if kind == "recent" {
        return recent_models(recent).into_iter().filter(|fx| matches(fx, &words)).skip(offset).take(limit).collect();
    }
    let mut hits: Vec<&LibModel> =
        models().iter().filter(|fx| (kind.is_empty() || fx.t == kind) && matches(fx, &words)).collect();
    let first = words.first().cloned().unwrap_or_default();
    hits.sort_by_cached_key(|fx| {
        let starts =
            !first.is_empty() && (fx.m.to_lowercase().starts_with(&first) || fx.n.to_lowercase().starts_with(&first));
        (if starts { 0 } else { 1 }, maker(&fx.m).to_lowercase(), fx.n.to_lowercase())
    });
    hits.into_iter().skip(offset).take(limit).collect()
}

/// Кількість знайдених моделей за типами (і в «Нещодавно використаних»).
pub fn counts(query: &str, recent: &[Recent]) -> Vec<(&'static str, usize)> {
    let words = words(query);
    let mut out: Vec<(&'static str, usize)> = TYPES.iter().map(|t| (*t, 0)).collect();
    for fx in models() {
        if matches(fx, &words) {
            let t = TYPES.iter().position(|t| *t == fx.t).unwrap_or(TYPES.len() - 1);
            out[t].1 += 1;
        }
    }
    let n = recent_models(recent).into_iter().filter(|fx| matches(fx, &words)).count();
    out.push(("recent", n));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_library() {
        assert!(total() > 1000, "бібліотека має містити понад 1000 моделей");
        let w = find("Free Color", "WASH 243").unwrap();
        assert!(w.builtin);
        assert_eq!(w.modes.len(), 7);
        assert_eq!(w.profile_name(&w.modes[0]), "Free Color WASH 243 · 24 кан. P001 (8 сегментів)");
    }

    #[test]
    fn search_sorts_by_maker_and_uses_aliases() {
        let hits = search("", 500, "bar", 0, &[]);
        let makers: Vec<String> = hits.iter().map(|f| maker(&f.m).to_lowercase()).collect();
        assert!(makers.is_sorted(), "за алфавітом виробників");
        let adj = search("adj", 200, "", 0, &[]);
        assert!(adj.iter().any(|m| m.m.eq_ignore_ascii_case("american dj")) || adj.iter().any(|m| m.m == "ADJ"));
        let none = search("щось-якого-немає", 5, "", 0, &[]);
        assert!(none.is_empty());
    }

    #[test]
    fn recent_in_usage_order() {
        let recent = vec![
            Recent { m: "Free Color".into(), n: "WASH 243".into() },
            Recent { m: "немає".into(), n: "такої".into() },
        ];
        let hits = search("", 10, "recent", 0, &recent);
        assert_eq!(hits.len(), 1);
        let c = counts("", &recent);
        assert_eq!(c.last().unwrap(), &("recent", 1));
        assert_eq!(c.iter().take(7).map(|x| x.1).sum::<usize>(), total());
    }

    #[test]
    fn profile_name_without_explicit() {
        let m = LibModel {
            m: "Acme".into(),
            n: "Par".into(),
            t: "par".into(),
            modes: vec![Mode("3ch".into(), crate::fixtures::roles(&["r", "g", "b"]), None)],
            builtin: false,
        };
        assert_eq!(m.profile_name(&m.modes[0]), "Acme Par · 3ch (3 кан.)");
        assert_eq!(m.color_summary(), "RGB");
    }

    #[test]
    fn chinese_clones_are_found() {
        let hits = search("5433", 10, "", 0, &[]);
        assert!(hits.iter().any(|f| f.m == "Китайська копія"));
        assert!(hits.iter().any(|f| f.n == "LPC-007"), "та сама таблиця, що в Betopper");
        let own = hits.iter().find(|f| f.m == "Китайська копія").unwrap();
        let lpc = find("Betopper", "LPC-007").unwrap();
        assert_eq!(own.modes[0].channels(), lpc.mode("7 Channel").unwrap().channels(), "як у посібнику Betopper");
        assert!(!search("power light", 10, "par", 0, &[]).is_empty());
        assert!(!search("китайська", 10, "", 0, &[]).is_empty());
        assert_eq!(title(&own.m, &own.n), own.n, "виробник копій не показується");
    }
}
