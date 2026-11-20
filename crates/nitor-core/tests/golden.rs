//! Золоті тести: рушій на Rust має видавати ті самі DMX-кадри й живі кольори, що й Nitor 1.x.
//!
//! Кадри знято зі старого рушія на Python скриптом `tools/golden/gen_golden.py`
//! за сценаріями `tools/golden/scenarios.json`.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use nitor_core::config::ConfigFile;
use nitor_core::engine::{Command, Scene, SceneState, StrobeState};
use nitor_core::{Effect, Engine};
use serde_json::Value;

const SCENARIOS: &str = include_str!("../../../tools/golden/scenarios.json");
const GOLDEN: &str = include_str!("golden/golden.json");

fn make(spec: &Value, fixtures: &Value) -> Engine {
    // як у Python: прилади проходять через завантаження налаштувань (там чистяться неправильні пресети)
    let fixtures: Vec<Value> = fixtures
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let mut f = f.clone();
            f["name"] = f["id"].clone();
            f
        })
        .collect();
    let cfg = serde_json::json!({"version": 2, "profiles": spec["profiles"], "fixtures": fixtures});
    let mut e = Engine::new();
    e.apply_config(ConfigFile::parse(&cfg.to_string()).unwrap());
    e.set_epoch(0.0);
    e
}

fn check(key: &str, e: &mut Engine, expected: &Value) {
    let frames = expected.as_array().unwrap();
    for fr in frames {
        let t = fr["t"].as_f64().unwrap();
        let dmx = e.render(t);
        let want: Vec<u8> =
            (0..512).map(|i| u8::from_str_radix(&fr["dmx"].as_str().unwrap()[i * 2..i * 2 + 2], 16).unwrap()).collect();
        if dmx[..] != want[..] {
            let diff: Vec<String> = (0..512)
                .filter(|&i| dmx[i] != want[i])
                .map(|i| format!("кан.{}: {} ≠ {}", i + 1, dmx[i], want[i]))
                .take(12)
                .collect();
            panic!("{key} @ t={t}: {}", diff.join("; "));
        }
        let want_live: BTreeMap<String, Vec<[u8; 4]>> = serde_json::from_value(fr["live"].clone()).unwrap();
        let got_live: BTreeMap<String, Vec<[u8; 4]>> = e.live.clone().into_iter().collect();
        assert_eq!(got_live, want_live, "живі кольори {key} @ t={t}");
    }
}

#[test]
fn golden_frames_match_python_engine() {
    let spec: Value = serde_json::from_str(SCENARIOS).unwrap();
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    let results: BTreeMap<String, Value> = golden["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["key"].as_str().unwrap().to_string(), r["frames"].clone()))
        .collect();
    let mut checked = 0;

    for case in spec["cases"].as_array().unwrap() {
        let key = case["name"].as_str().unwrap();
        let mut e = make(&spec, &case["fixtures"]);
        e.show.master = case["master"].as_u64().unwrap_or(255) as u8;
        e.show.blackout = case["blackout"].as_bool().unwrap_or(false);
        if let Some(s) = case.get("strobe") {
            e.strobe =
                StrobeState { on: s["on"].as_bool().unwrap(), ids: serde_json::from_value(s["ids"].clone()).unwrap() };
        }
        if let Some(raw) = case.get("raw").and_then(Value::as_object) {
            for (k, v) in raw {
                e.raw.insert(k.parse::<usize>().unwrap() - 1, v.as_u64().unwrap() as u8);
            }
        }
        check(key, &mut e, &results[key]);
        checked += 1;
    }

    let ef = &spec["effects"];
    for kind in ef["types"].as_array().unwrap() {
        for (vi, var) in ef["variants"].as_array().unwrap().iter().enumerate() {
            let key = format!("effect_{}_{vi}", kind.as_str().unwrap());
            let mut e = make(&spec, &ef["fixtures"]);
            e.show.master = ef["master"].as_u64().unwrap() as u8;
            let mut v = var.clone();
            v["type"] = kind.clone();
            e.show.effect = serde_json::from_value::<Effect>(v).unwrap();
            check(&key, &mut e, &results[&key]);
            checked += 1;
        }
    }

    for fd in spec["fades"].as_array().unwrap() {
        let key = fd["name"].as_str().unwrap();
        let mut e = make(&spec, &fd["fixtures"]);
        let fixtures: IndexMap<String, SceneState> = serde_json::from_value(fd["scene"].clone()).unwrap();
        e.show.scenes.insert("s".into(), Scene { fixtures, effect: None });
        e.handle(Command::RecallScene { name: "s".into(), fade: fd["dur"].as_f64().unwrap() }, 0.0).unwrap();
        check(key, &mut e, &results[key]);
        checked += 1;
    }

    assert_eq!(checked, results.len(), "кожен сценарій золотого файлу перевірено");
}
