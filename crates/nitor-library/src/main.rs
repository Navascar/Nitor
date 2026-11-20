//! Збирає бібліотеку приладів Nitor з відкритих баз описів DMX-приладів.
//!
//! Джерела (локальні клони):
//!   * Open Fixture Library — <https://github.com/OpenLightingProject/open-fixture-library> (MIT)
//!   * QLC+ fixtures        — <https://github.com/mcallegari/qlcplus/tree/master/resources/fixtures> (Apache-2.0)
//!
//! Кожен режим кожного приладу переводиться в ролі каналів програми: кольори (r g b w a uv),
//! dim, strobe, «=N» (фіксоване значення) або zero. Безпечні правила: канал строба/затвора
//! ставиться на «відкрито», макроси та програми — на «без функції», щоб прилад гарантовано
//! слухався кольорів. Беруться лише прилади з адитивним змішуванням кольорів (RGB/RGBW/…).
//!
//! Запуск:
//!   cargo run -p nitor-library -- --ofl ../open-fixture-library/fixtures --qlc ../qlcplus/resources/fixtures
//! Результат: crates/nitor-core/assets/library.json.gz

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

type Modes = Vec<(String, Vec<String>)>;

struct Fx {
    m: String,
    n: String,
    t: String,
    modes: Modes,
}

fn color_role(name: Option<&str>) -> Option<String> {
    let n = name?.trim().to_lowercase();
    let r = match n.as_str() {
        "red" => "r",
        "green" => "g",
        "blue" => "b",
        "white" | "warm white" | "cold white" => "w",
        "amber" => "a",
        "uv" | "ultraviolet" => "uv",
        "lime" | "cyan" | "indigo" | "magenta" | "yellow" | "lavender" | "orange" | "pink" => "zero",
        _ => return None,
    };
    Some(r.into())
}

static NEUTRAL_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)no ?func|no ?effect|^off$|disabled|^none|rgb|manual|dimmer mode|open|default|dmx mode|standard")
        .unwrap()
});

fn fixed(v: i64) -> String {
    let v = v.clamp(0, 255);
    if v == 0 { "zero".into() } else { format!("={v}") }
}

fn usable(roles: &[String]) -> bool {
    let has = |r: &str| roles.iter().any(|x| x == r);
    (has("r") && has("g") && has("b")) || has("w")
}

/// Ціле як `int()` у Python: число — з відкиданням дробової частини, рядок — розбір.
fn py_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0).trunc() as i64),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

// ------------------------------------------------------------------------ OFL --
/// defaultValue: число, «50%» або «x%».
fn ofl_value(v: &Value) -> i64 {
    if v.is_number() {
        return py_int(v);
    }
    static PCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([\d.]+)%").unwrap());
    let s = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match PCT.captures(&s).and_then(|c| c[1].parse::<f64>().ok()) {
        Some(x) => (x * 2.55).round_ties_even() as i64,
        None => 0,
    }
}

fn ofl_caps(ch: &Value) -> Vec<Value> {
    if let Some(c) = ch.get("capability") {
        let mut c = c.clone();
        if let Some(o) = c.as_object_mut() {
            o.insert("dmxRange".into(), json!([0, 255]));
        }
        return vec![c];
    }
    ch.get("capabilities").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn range0(c: &Value) -> i64 {
    c.get("dmxRange").and_then(|r| r.get(0)).map(py_int).unwrap_or(0)
}

fn s<'a>(c: &'a Value, k: &str) -> Option<&'a str> {
    c.get(k).and_then(Value::as_str)
}

fn ofl_role(ch: Option<&Value>) -> String {
    let Some(ch) = ch else { return "zero".into() };
    let caps = ofl_caps(ch);
    if caps.is_empty() {
        return "zero".into();
    }
    let first = &caps[0];
    let t = s(first, "type");
    if caps.len() == 1 && t == Some("ColorIntensity") {
        return color_role(s(first, "color")).unwrap_or_else(|| "zero".into());
    }
    if caps.len() == 1 && t == Some("Intensity") {
        return "dim".into();
    }
    let at0 = caps.iter().find(|c| range0(c) == 0).unwrap_or(first);
    if caps.iter().any(|c| matches!(s(c, "type"), Some("ShutterStrobe" | "StrobeSpeed" | "StrobeDuration"))) {
        let opened =
            caps.iter().find(|c| s(c, "type") == Some("ShutterStrobe") && s(c, "shutterEffect") == Some("Open"));
        if s(at0, "type") == Some("NoFunction") || s(at0, "shutterEffect") == Some("Open") {
            return "strobe".into();
        }
        if let Some(o) = opened {
            return fixed(range0(o));
        }
        return "strobe".into();
    }
    if s(at0, "type") == Some("NoFunction") {
        return "zero".into();
    }
    if let Some(nf) = caps.iter().find(|c| s(c, "type") == Some("NoFunction")) {
        return fixed(range0(nf));
    }
    if t == Some("Intensity") {
        return "dim".into();
    }
    if let Some(d) = ch.get("defaultValue") {
        return fixed(ofl_value(d));
    }
    "zero".into()
}

fn ofl_pixel_count(fx: &Value, repeat: Option<&Value>) -> usize {
    if let Some(Value::Array(a)) = repeat {
        return a.len();
    }
    let m = fx.get("matrix").cloned().unwrap_or(json!({}));
    if repeat.and_then(Value::as_str) == Some("eachPixelGroup") {
        return m.get("pixelGroups").and_then(Value::as_object).map_or(0, |g| g.len()).max(1);
    }
    if let Some(keys) = m.get("pixelKeys").and_then(Value::as_array) {
        let truthy = |x: &Value| match x {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            _ => true,
        };
        return keys
            .iter()
            .flat_map(|z| z.as_array().cloned().unwrap_or_default())
            .flat_map(|y| y.as_array().cloned().unwrap_or_default())
            .filter(truthy)
            .count();
    }
    if let Some(pc) = m.get("pixelCount").and_then(Value::as_array) {
        let mut v: Vec<i64> = pc.iter().map(py_int).collect();
        v.extend([1, 1, 1]);
        return (v[0] * v[1] * v[2]).max(0) as usize;
    }
    1
}

/// Файли `тека/*.ext`, відсортовані як `sorted(root.glob(...))` у Python (за частинами шляху).
fn glob2(root: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(dirs) = std::fs::read_dir(root) else { return out };
    for d in dirs.flatten() {
        if !d.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if p.is_file() && p.extension().is_some_and(|e| e == ext) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| {
        let parts = |p: &Path| -> (String, String) {
            (
                p.parent().and_then(|x| x.file_name()).unwrap_or_default().to_string_lossy().into(),
                p.file_name().unwrap_or_default().to_string_lossy().into(),
            )
        };
        parts(a).cmp(&parts(b))
    });
    out
}

fn load_ofl(root: &Path) -> Vec<Fx> {
    let mut out = vec![];
    let makers: Value = std::fs::read_to_string(root.join("manufacturers.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(json!({}));
    for path in glob2(root, "json") {
        let dir = path.parent().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy().to_string();
        let maker = makers.get(&dir).and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or(&dir).to_string();
        let Some(fx) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            continue;
        };
        let empty = Map::new();
        let chans = fx.get("availableChannels").and_then(Value::as_object).unwrap_or(&empty);
        let mut fine: HashSet<String> = HashSet::new();
        for ch in chans.values() {
            for a in ch.get("fineChannelAliases").and_then(Value::as_array).into_iter().flatten() {
                if let Some(a) = a.as_str() {
                    fine.insert(a.into());
                }
            }
            for a in ch.get("switchChannels").and_then(Value::as_object).into_iter().flatten() {
                fine.insert(a.0.clone());
            }
        }
        let templates = fx.get("templateChannels").and_then(Value::as_object).unwrap_or(&empty);
        let mut modes: Modes = vec![];
        for mode in fx.get("modes").and_then(Value::as_array).into_iter().flatten() {
            let mut roles: Vec<String> = vec![];
            for item in mode.get("channels").and_then(Value::as_array).into_iter().flatten() {
                if item.get("insert").and_then(Value::as_str) == Some("matrixChannels") {
                    let count = ofl_pixel_count(&fx, item.get("repeatFor"));
                    let tpl_roles: Vec<String> = item
                        .get("templateChannels")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|t| {
                            let t = t.as_str().unwrap_or("");
                            ofl_role(if fine.contains(t) { None } else { templates.get(t) })
                        })
                        .collect();
                    if item.get("channelOrder").and_then(Value::as_str) == Some("perChannel") {
                        for r in &tpl_roles {
                            roles.extend(std::iter::repeat_n(r.clone(), count));
                        }
                    } else {
                        for _ in 0..count {
                            roles.extend(tpl_roles.iter().cloned());
                        }
                    }
                } else {
                    match item.as_str() {
                        None => roles.push("zero".into()),
                        Some(name) if fine.contains(name) => roles.push("zero".into()),
                        Some(name) => roles.push(ofl_role(chans.get(name))),
                    }
                }
            }
            if !roles.is_empty() && usable(&roles) && roles.len() <= 512 {
                let name = s(mode, "shortName")
                    .filter(|x| !x.is_empty())
                    .or(s(mode, "name").filter(|x| !x.is_empty()))
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}ch", roles.len()));
                modes.push((name, roles));
            }
        }
        if !modes.is_empty() {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let name = s(&fx, "name").map(String::from).unwrap_or(stem);
            let cat = fx
                .get("categories")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
                .and_then(|a| a[0].as_str())
                .unwrap_or("");
            let t = fixture_type(cat, &name, &modes);
            out.push(Fx { m: maker, n: name, t, modes });
        }
    }
    out
}

// ----------------------------------------------------------------------- QLC+ --
fn qlc_color(preset: &str) -> Option<&'static str> {
    Some(match preset {
        "IntensityRed" => "r",
        "IntensityGreen" => "g",
        "IntensityBlue" => "b",
        "IntensityWhite" | "IntensityWarmWhite" | "IntensityColdWhite" => "w",
        "IntensityAmber" => "a",
        "IntensityUV" => "uv",
        "IntensityDimmer" | "IntensityMasterDimmer" => "dim",
        "IntensityCyan" | "IntensityMagenta" | "IntensityYellow" | "IntensityLime" | "IntensityIndigo" => "zero",
        _ => return None,
    })
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text(n: Option<roxmltree::Node>) -> String {
    n.and_then(|n| n.text()).unwrap_or("").trim().to_string()
}

fn qlc_role(ch: roxmltree::Node) -> String {
    let preset = ch.attribute("Preset").unwrap_or("");
    if let Some(r) = qlc_color(preset) {
        return r.into();
    }
    if preset.ends_with("Fine") || preset.starts_with("Position") || preset.starts_with("Speed") {
        return "zero".into();
    }
    let group = child(ch, "Group");
    let gname = text(group);
    if gname == "Intensity" {
        if group.and_then(|g| g.attribute("Byte")) == Some("1") {
            return "zero".into();
        }
        let col = child(ch, "Colour").and_then(|c| c.text());
        return color_role(col).unwrap_or_else(|| "dim".into());
    }
    let caps: Vec<(i64, i64, String, String)> = ch
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "Capability")
        .map(|c| {
            let num = |k: &str, d: i64| c.attribute(k).map_or(d, |v| v.trim().parse().unwrap_or(0));
            (
                num("Min", 0),
                num("Max", 255),
                c.attribute("Preset").unwrap_or("").into(),
                c.text().unwrap_or("").trim().into(),
            )
        })
        .collect();
    let at0 = caps.iter().find(|c| c.0 == 0);
    let neutral = |c: &(i64, i64, String, String)| c.2 == "NoFunction" || NEUTRAL_TEXT.is_match(&c.3);
    if gname == "Shutter" || caps.iter().any(|c| c.2.starts_with("Shutter") || c.2.starts_with("Strobe")) {
        if let Some(a) = at0 {
            if a.2 == "ShutterOpen" || a.2 == "NoFunction" || NEUTRAL_TEXT.is_match(&a.3) {
                return "strobe".into();
            }
        }
        if let Some(o) = caps.iter().find(|c| c.2 == "ShutterOpen") {
            return fixed(o.0);
        }
        return "strobe".into();
    }
    if let Some(a) = at0 {
        if a.2 == "NoFunction" || a.2 == "ShutterOpen" || NEUTRAL_TEXT.is_match(&a.3) {
            return "zero".into();
        }
    }
    if let Some(n) = caps.iter().find(|c| neutral(c)) {
        return fixed(n.0);
    }
    if let Some(d) = ch.attribute("Default").filter(|d| !d.is_empty()) {
        return fixed(d.trim().parse().unwrap_or(0));
    }
    "zero".into()
}

fn load_qlc(root: &Path) -> Vec<Fx> {
    let mut out = vec![];
    for path in glob2(root, "qxf") {
        let Ok(xml) = std::fs::read_to_string(&path) else { continue };
        let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
        let Ok(doc) = roxmltree::Document::parse_with_options(&xml, opts) else { continue };
        let t = doc.root_element();
        let dir = path.parent().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy().to_string();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let maker = child(t, "Manufacturer").and_then(|n| n.text()).map(|s| s.trim().to_string()).unwrap_or(dir);
        let model = child(t, "Model").and_then(|n| n.text()).map(|s| s.trim().to_string()).unwrap_or(stem);
        let mut chans = std::collections::HashMap::new();
        for c in t.children().filter(|c| c.is_element() && c.tag_name().name() == "Channel") {
            chans.insert(c.attribute("Name").unwrap_or("").to_string(), qlc_role(c));
        }
        let mut modes: Modes = vec![];
        for mode in t.children().filter(|c| c.is_element() && c.tag_name().name() == "Mode") {
            let mut items: Vec<(i64, String)> = mode
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "Channel")
                .map(|c| {
                    (
                        c.attribute("Number").map_or(0, |v| v.trim().parse().unwrap_or(0)),
                        c.text().unwrap_or("").trim().into(),
                    )
                })
                .collect();
            items.sort();
            let roles: Vec<String> =
                items.iter().map(|(_, n)| chans.get(n).cloned().unwrap_or_else(|| "zero".into())).collect();
            if !roles.is_empty() && usable(&roles) {
                let name = mode
                    .attribute("Name")
                    .filter(|n| !n.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}ch", roles.len()));
                modes.push((name, roles));
            }
        }
        if !modes.is_empty() {
            let ty = child(t, "Type").and_then(|n| n.text()).unwrap_or("");
            let t = fixture_type(ty, &model, &modes);
            out.push(Fx { m: maker, n: model, t, modes });
        }
    }
    out
}

// ------------------------------------------------------------------------ тип --
fn fixture_type(raw: &str, name: &str, modes: &Modes) -> String {
    let t = match raw.trim().to_lowercase().as_str() {
        "color changer" | "dimmer" => "par",
        "pixel bar" | "led bar (beams)" | "led bar (pixels)" => "bar",
        "moving head" => "head",
        "strobe" | "blinder" => "strobe",
        "matrix" => "matrix",
        "flower" | "effect" | "scanner" | "barrel scanner" | "laser" | "fan" => "effect",
        _ => "other",
    };
    static BAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bbar\b|batten|strip|tube|pixel").unwrap());
    static PANEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"matrix|panel|blinder").unwrap());
    let n = name.to_lowercase();
    if (t == "par" || t == "other") && BAR.is_match(&n) {
        return "bar".into();
    }
    if (t == "par" || t == "other") && PANEL.is_match(&n) {
        return if n.contains("matrix") || n.contains("panel") { "matrix" } else { "strobe" }.into();
    }
    if t == "par" && modes.iter().map(|m| m.1.iter().filter(|r| *r == "r").count()).max().unwrap_or(0) >= 4 {
        return "bar".into();
    }
    t.into()
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

fn build(ofl: &Path, qlc: &Path) -> (usize, usize, Vec<Fx>) {
    let (o, q) = (load_ofl(ofl), load_qlc(qlc));
    let (no, nq) = (o.len(), q.len());
    let mut seen = HashSet::new();
    let mut merged = vec![];
    for fx in o.into_iter().chain(q) {
        // OFL має пріоритет: точніша схема
        if seen.insert((norm(&fx.m), norm(&fx.n))) {
            merged.push(fx);
        }
    }
    merged.sort_by(|a, b| (a.m.to_lowercase(), a.n.to_lowercase()).cmp(&(b.m.to_lowercase(), b.n.to_lowercase())));
    (no, nq, merged)
}

fn to_json(list: &[Fx]) -> String {
    let fixtures: Vec<Value> = list
        .iter()
        .map(|f| {
            json!({"m": f.m, "n": f.n, "t": f.t,
                   "modes": f.modes.iter().map(|(n, r)| json!([n, r])).collect::<Vec<_>>()})
        })
        .collect();
    // ліцензії джерел — у THIRD_PARTY_LICENSES.md
    serde_json::to_string(&json!({ "fixtures": fixtures })).unwrap()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut ofl, mut qlc, mut out) = (None, None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ofl" => ofl = args.next().map(PathBuf::from),
            "--qlc" => qlc = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            _ => {
                eprintln!("невідомий параметр {a}");
                std::process::exit(2);
            }
        }
    }
    let (Some(ofl), Some(qlc)) = (ofl, qlc) else {
        eprintln!(
            "використання: nitor-library --ofl <тека fixtures OFL> --qlc <тека resources/fixtures QLC+> [--out файл]"
        );
        std::process::exit(2);
    };
    let out =
        out.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nitor-core/assets/library.json.gz"));
    let (no, nq, merged) = build(&ofl, &qlc);
    let json = to_json(&merged);
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(json.as_bytes()).unwrap();
    let gz = enc.finish().unwrap();
    std::fs::write(&out, &gz).unwrap_or_else(|e| panic!("не вдалося записати {}: {e}", out.display()));
    let modes: usize = merged.iter().map(|f| f.modes.len()).sum();
    println!(
        "OFL: {no}, QLC+: {nq}, разом без дублікатів: {} приладів, {modes} режимів → {} ({} КБ)",
        merged.len(),
        out.display(),
        gz.len() / 1024
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        assert_eq!(fixed(0), "zero");
        assert_eq!(fixed(300), "=255");
        assert_eq!(ofl_value(&json!("50%")), 127, "як round(50 * 2.55) у Python");
        assert_eq!(ofl_value(&json!(17.9)), 17);
        let strobe = json!({"capabilities": [
            {"dmxRange": [0, 9], "type": "ShutterStrobe", "shutterEffect": "Closed"},
            {"dmxRange": [10, 19], "type": "ShutterStrobe", "shutterEffect": "Open"},
            {"dmxRange": [20, 255], "type": "ShutterStrobe", "shutterEffect": "Strobe"}]});
        assert_eq!(ofl_role(Some(&strobe)), "=10", "затвор — на «відкрито»");
        let red = json!({"capability": {"type": "ColorIntensity", "color": "Red"}});
        assert_eq!(ofl_role(Some(&red)), "r");
        assert_eq!(ofl_role(None), "zero");
    }

    #[test]
    fn types() {
        let modes: Modes = vec![("3ch".into(), vec!["r".into(), "g".into(), "b".into()])];
        assert_eq!(fixture_type("Color Changer", "LED Bar 240", &modes), "bar");
        assert_eq!(fixture_type("Other", "LED Panel", &modes), "matrix");
        assert_eq!(fixture_type("Moving Head", "Spot", &modes), "head");
        assert_eq!(norm("ADJ Mega-Bar"), "adjmegabar");
    }
}
