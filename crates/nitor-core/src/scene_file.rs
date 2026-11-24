//! Сцена у файлі `.nitor` — для перенесення між комп'ютерами.
//!
//! Файл самодостатній: стан приладів + опис приладів (адреса, канали, модель). При відкритті
//! прилади зіставляються з тутешніми: за id, далі за адресою й каналами, далі за назвою й
//! каналами. Прилад, якого тут немає, додається на ту саму адресу, якщо вона вільна.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::effects::Effect;
use crate::engine::{Engine, ProfileSpec, Scene, SceneState};
use crate::fixtures::{Fixture, Model, preset_count, validate_profile};
use crate::{DMX_CHANNELS, Result, VERSION, bail, err};

pub const SCENE_FORMAT: &str = "nitor-scene";
pub const SCENE_VERSION: u64 = 2;
pub const SCENE_EXT: &str = "nitor";

/// Безпечна назва файлу для сцени.
pub fn file_name(scene: &str) -> String {
    let name: String =
        scene.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 32 { '_' } else { c }).collect();
    let name = name.trim_matches(|c| c == ' ' || c == '.');
    format!("{}.{SCENE_EXT}", if name.is_empty() { "scene" } else { name })
}

/// Сцена як самодостатній файл.
pub fn export(engine: &Engine, name: &str) -> Result<Value> {
    let scene = engine.show.scenes.get(name).ok_or_else(|| err!("сцену не знайдено"))?;
    let fixtures: Vec<Value> = scene
        .fixtures
        .iter()
        .map(|(fid, state)| {
            let mut item = json!({"id": fid, "state": state});
            if let Some(fx) = engine.fixture(fid) {
                item["name"] = json!(fx.name);
                item["address"] = json!(fx.address);
                item["profile"] = json!(fx.profile);
                item["channels"] = json!(engine.channels(fx));
                item["model"] = json!(fx.model);
            }
            item
        })
        .collect();
    Ok(json!({
        "format": SCENE_FORMAT, "version": SCENE_VERSION, "app": VERSION, "name": name,
        "fixtures": fixtures, "effect": scene.effect,
    }))
}

fn byte(v: Option<&Value>, default: u8) -> u8 {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() => x.trunc().clamp(0.0, 255.0) as u8,
        _ => default,
    }
}

/// Стан приладу зі сцени у файлі — лише відомі поля з допустимими значеннями.
fn scene_state(v: &Value, segments: usize) -> SceneState {
    let g = |k: &str, d: u8| byte(v.get(k), d);
    let segs = v.get("segs").and_then(Value::as_array).map(|a| {
        a.iter()
            .take(segments)
            .map(|s| match s.as_array() {
                Some(c) if c.len() >= 3 => Some([byte(c.first(), 0), byte(c.get(1), 0), byte(c.get(2), 0)]),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let segs = segs.filter(|s| s.iter().any(Option::is_some));
    let preset = v.get("preset").and_then(Value::as_u64).filter(|p| (1..=preset_count() as u64).contains(p));
    SceneState {
        r: g("r", 0),
        g: g("g", 0),
        b: g("b", 0),
        w: g("w", 0),
        a: g("a", 0),
        uv: g("uv", 0),
        dimmer: g("dimmer", 255),
        segs,
        preset: preset.map(|p| p as u8),
        speed: g("speed", 128),
    }
}

/// Ефект зі сцени — лише допустимі значення; невідомий тип — без ефекту.
fn clean_effect(v: Option<&Value>) -> Option<Effect> {
    let v = v?.as_object()?;
    let kind = serde_json::from_value(v.get("type")?.clone()).ok()?;
    let num = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d);
    let colors = v
        .get("colors")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|c| c.as_str().and_then(|s| crate::Rgb::from_hex(s).ok())).collect())
        .unwrap_or_default();
    let ids = v
        .get("ids")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|i| i.as_str().map(String::from).unwrap_or_else(|| i.to_string())).collect())
        .unwrap_or_default();
    Some(
        Effect {
            kind,
            ids,
            speed: num("speed", 0.5),
            spread: num("spread", 1.0),
            colors,
            by_segment: v.get("by_segment").and_then(Value::as_bool).unwrap_or(true),
            beat: v.get("beat").and_then(Value::as_bool).unwrap_or(false),
            beat_pulse: v.get("beat_pulse").and_then(Value::as_bool).unwrap_or(false),
        }
        .clamped(),
    )
}

fn id_str(v: &Value) -> String {
    v.as_str().map(String::from).unwrap_or_else(|| v.to_string())
}

/// Відкриває сцену з файлу (`data` — вміст `.nitor`). Повідомлення — у `engine.pop_notice()`.
pub(crate) fn import(engine: &mut Engine, data: &Value, name: Option<&str>) -> Result<()> {
    if data.get("format").and_then(Value::as_str) != Some(SCENE_FORMAT) {
        bail!("це не файл сцени Nitor");
    }
    if data.get("version").and_then(Value::as_u64).unwrap_or(0) > SCENE_VERSION {
        bail!("файл сцени створено новішою версією Nitor, оновіть програму");
    }
    let Some(list) = data.get("fixtures").and_then(Value::as_array) else {
        bail!("файл сцени пошкоджено");
    };
    struct Item<'a> {
        id: String,
        state: &'a Value,
        ch: Option<Vec<String>>,
        raw: &'a Value,
    }
    let items: Vec<Item> = list
        .iter()
        .filter(|i| i.get("state").is_some_and(Value::is_object) && i.get("id").is_some_and(|x| !x.is_null()))
        .map(|i| Item {
            id: id_str(&i["id"]),
            state: &i["state"],
            ch: i
                .get("channels")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
                .map(|a| a.iter().map(|c| c.as_str().map(String::from).unwrap_or_else(|| c.to_string())).collect()),
            raw: i,
        })
        .collect();

    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut mapping: Vec<(String, String)> = vec![];
    let (mut added, mut skipped) = (0usize, 0usize);
    let same_ch =
        |e: &Engine, f: &Fixture, ch: &Option<Vec<String>>| ch.as_ref().is_none_or(|c| e.channels(f) == c.as_slice());
    let mut pending = vec![];
    for it in &items {
        let addr = it.raw.get("address").and_then(Value::as_u64);
        let iname = it.raw.get("name").and_then(Value::as_str);
        let free = |x: &&Fixture| !used.contains(&x.id);
        let mut found =
            engine.show.fixtures.iter().filter(free).find(|f| f.id == it.id).filter(|f| same_ch(engine, f, &it.ch));
        if found.is_none() && it.ch.is_some() {
            found = engine
                .show
                .fixtures
                .iter()
                .filter(free)
                .find(|x| Some(x.address as u64) == addr && same_ch(engine, x, &it.ch));
        }
        if found.is_none() && it.ch.is_some() && iname.is_some_and(|n| !n.is_empty()) {
            found = engine
                .show
                .fixtures
                .iter()
                .filter(free)
                .find(|x| Some(x.name.as_str()) == iname && same_ch(engine, x, &it.ch));
        }
        match found {
            Some(f) => {
                used.insert(f.id.clone());
                mapping.push((it.id.clone(), f.id.clone()));
            }
            None => pending.push(it),
        }
    }
    for it in pending {
        // приладів немає — додаємо на їхні адреси, якщо ті вільні
        let (Some(ch), Some(address)) = (&it.ch, it.raw.get("address").and_then(Value::as_u64)) else {
            skipped += 1;
            continue;
        };
        if validate_profile(ch).is_err() {
            skipped += 1;
            continue;
        }
        let address = address as usize;
        let end = address + ch.len() - 1;
        let busy = engine.show.fixtures.iter().any(|f| {
            let a = f.address as usize;
            address <= a + engine.channels(f).len().saturating_sub(1) && a <= end
        });
        if address < 1 || end > DMX_CHANNELS || busy {
            skipped += 1;
            continue;
        }
        let model =
            it.raw.get("model").and_then(|m| serde_json::from_value::<Model>(m.clone()).ok()).and_then(Model::valid);
        let pname = it.raw.get("profile").and_then(Value::as_str).unwrap_or("");
        let (profile, model) = import_profile(engine, pname, ch, model)?;
        let fname: String =
            it.raw.get("name").and_then(Value::as_str).unwrap_or("Прожектор").chars().take(80).collect();
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        engine.show.fixtures.push(Fixture::new(id.clone(), fname, address as u16, profile, model));
        used.insert(id.clone());
        mapping.push((it.id.clone(), id));
        added += 1;
    }
    if mapping.is_empty() {
        bail!("жоден прилад зі сцени не вдалося зіставити: адреси зайняті іншими приладами");
    }
    let lookup = |from: &str| mapping.iter().find(|(f, _)| f == from).map(|(_, t)| t.clone());

    let mut states = indexmap::IndexMap::new();
    for it in &items {
        if let Some(fid) = lookup(&it.id) {
            let n = engine.fixture(&fid).map_or(1, |f| engine.segments(f));
            states.insert(fid, scene_state(it.state, n));
        }
    }
    let mut effect = clean_effect(data.get("effect"));
    if let Some(e) = effect.as_mut() {
        e.ids = e.ids.iter().filter_map(|i| lookup(i)).collect();
    }
    let given = name.map(str::to_string).or_else(|| data.get("name").and_then(Value::as_str).map(String::from));
    let base: String = given.unwrap_or_default().trim().chars().take(80).collect();
    let base = if base.is_empty() { "Сцена".to_string() } else { base };
    let (mut scene_name, mut k) = (base.clone(), 2);
    while engine.show.scenes.contains_key(&scene_name) {
        scene_name = format!("{base} ({k})");
        k += 1;
    }
    engine.show.scenes.insert(scene_name.clone(), Scene { fixtures: states, effect });
    let mut parts = vec![format!("зіставлено приладів: {}", mapping.len() - added)];
    if added > 0 {
        parts.push(format!("додано: {added}"));
    }
    if skipped > 0 {
        parts.push(format!("пропущено (адреса зайнята): {skipped}"));
    }
    engine.set_notice(format!("Сцену «{scene_name}» відкрито з файлу · {}", parts.join(", ")));
    Ok(())
}

/// Профіль для приладу з файлу сцени: наявний з тими самими каналами або новий.
fn import_profile(
    engine: &mut Engine,
    name: &str,
    channels: &[String],
    model: Option<Model>,
) -> Result<(String, Option<Model>)> {
    if model.is_some() {
        return engine.profile_from(&ProfileSpec { profile: name.into(), channels: Some(channels.to_vec()), model });
    }
    // стандартний або власний профіль з такими ж каналами
    if let Some(n) = engine
        .show
        .profiles
        .iter()
        .find(|(n, ch)| ch.as_slice() == channels && !engine.is_device(n))
        .map(|(n, _)| n.clone())
    {
        return Ok((n, None));
    }
    let base = if name.is_empty() { format!("Профіль ({} кан.)", channels.len()) } else { name.to_string() };
    let (mut n, mut k) = (base.clone(), 2);
    while engine.show.profiles.contains_key(&n) {
        n = format!("{base} ({k})");
        k += 1;
    }
    engine.show.profiles.insert(n.clone(), channels.to_vec());
    Ok((n, None))
}

impl Engine {
    pub(crate) fn set_notice(&mut self, text: String) {
        self.notice = Some(text);
    }
}
