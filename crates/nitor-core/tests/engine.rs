//! Поведінка рушія (перенесено з тестів pytest Nitor 1.x, без перенесення старих форматів).

use std::collections::BTreeSet;

use nitor_core::engine::{
    ColorValues, Command, EffectPatch, FlashSource, ProfileSpec, STROBE_LEVEL, StaticSource, StrobeState,
};
use nitor_core::fixtures::{Model, default_profiles, model_profiles, preset_level, segment_count};
use nitor_core::{EffectKind, Engine, Rgb, library, scene_file, userdata};
use serde_json::json;

const WASH8: &str = "Free Color WASH 243 · 8 кан. H001 (Dim, строб, програми, RGB)";
const WASH24: &str = "Free Color WASH 243 · 24 кан. P001 (8 сегментів)";

fn ok(e: &mut Engine, c: Command) {
    e.handle(c, 0.0).unwrap();
}

fn add(e: &mut Engine, profile: &str, address: u16) -> String {
    add_named(e, profile, address, "")
}

fn add_named(e: &mut Engine, profile: &str, address: u16, name: &str) -> String {
    ok(e, Command::AddFixture { spec: ProfileSpec::named(profile), name: name.into(), address, count: 1 });
    e.show.fixtures.last().unwrap().id.clone()
}

fn hex(ids: &[&str], h: &str) -> Command {
    Command::SetColor {
        ids: ids.iter().map(|s| s.to_string()).collect(),
        values: ColorValues { hex: Some(Rgb::from_hex(h).unwrap()), ..Default::default() },
    }
}

fn effect(kind: EffectKind, ids: &[&str]) -> Command {
    Command::SetEffect(EffectPatch {
        kind: Some(kind),
        ids: Some(ids.iter().map(|s| s.to_string()).collect()),
        ..Default::default()
    })
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn commands_and_scene_fade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut e = Engine::load(&path);
    assert!(e.show.fixtures.is_empty(), "перший запуск — без приладів");
    ok(
        &mut e,
        Command::AddFixture { spec: ProfileSpec::named("Dim+RGB (4 кан.)"), name: "".into(), address: 1, count: 4 },
    );
    let all: Vec<String> = e.show.fixtures.iter().map(|f| f.id.clone()).collect();
    assert_eq!(all.len(), 4);
    assert_eq!(e.show.fixtures[3].name, "Прожектор 4");
    ok(&mut e, hex(&[&all[0]], "#00ff00"));
    assert_eq!(&e.render(0.0)[0..4], &[255, 0, 255, 0]);
    ok(&mut e, Command::SaveScene { name: "green".into(), with_effect: false });
    ok(&mut e, hex(&[&all[0]], "#000000"));
    e.handle(Command::RecallScene { name: "green".into(), fade: 2.0 }, 100.0).unwrap();
    e.render(101.0);
    assert!((120..=135).contains(&e.show.fixtures[0].g));
    e.render(103.0);
    assert_eq!(e.show.fixtures[0].g, 255);

    ok(&mut e, Command::Blackout(Some(true)));
    assert!(e.render(10.0).iter().all(|&v| v == 0));
    ok(&mut e, Command::Blackout(Some(false)));

    ok(
        &mut e,
        Command::AddFixture { spec: ProfileSpec::named("RGB (3 кан.)"), name: "".into(), address: 100, count: 3 },
    );
    let addrs: Vec<u16> = e.show.fixtures[4..].iter().map(|f| f.address).collect();
    assert_eq!(addrs, [100, 103, 106]);
    assert!(
        e.handle(
            Command::AddFixture {
                spec: ProfileSpec::named("RGB (3 кан.)"), name: "".into(), address: 511, count: 1
            },
            0.0
        )
        .is_err()
    );

    e.save(0.0).unwrap();
    let again = Engine::load(&path);
    assert_eq!(again.show.fixtures.len(), 7);
    assert!(again.show.scenes.contains_key("green"));
    assert!(again.load_warning.is_none());
}

#[test]
fn render_with_and_without_dimmer() {
    let mut e = Engine::new();
    e.show.profiles.insert("D5".into(), ids(&["dim", "r", "g", "b", "strobe"]));
    let a = add(&mut e, "D5", 10);
    ok(
        &mut e,
        Command::SetColor {
            ids: ids(&[&a]),
            values: ColorValues { r: Some(255), g: Some(128), b: Some(0), dimmer: Some(128), ..Default::default() },
        },
    );
    assert_eq!(&e.render(0.0)[9..14], &[128, 255, 128, 0, 0]);

    let mut e = Engine::new();
    let a = add(&mut e, "RGBW (4 кан.)", 1);
    ok(
        &mut e,
        Command::SetColor {
            ids: ids(&[&a]),
            values: ColorValues { r: Some(200), g: Some(100), b: Some(50), w: Some(255), ..Default::default() },
        },
    );
    ok(&mut e, Command::SetMaster(128));
    // 0.5 майстра в Python ≈ 128/255 тут
    let f = e.render(0.0);
    assert_eq!(&f[0..4], &[100, 50, 25, 128]);
}

#[test]
fn static_color_stops_effect_and_uses_white_channel() {
    let mut e = Engine::new();
    let rgb = add(&mut e, "Dim+RGB (4 кан.)", 1);
    let rgbw = add(&mut e, "RGBW (4 кан.)", 100);
    ok(&mut e, effect(EffectKind::Rainbow, &[&rgb, &rgbw]));
    let white = e.show.static_colors.iter().position(|c| c.name == "Білий").unwrap();
    ok(&mut e, Command::StaticColor { ids: vec![], source: StaticSource::Index(white) });
    assert_eq!(e.show.effect.kind, EffectKind::None);
    let f = e.fixture(&rgb).unwrap();
    assert_eq!((f.r, f.g, f.b, f.w), (255, 255, 255, 0));
    let f = e.fixture(&rgbw).unwrap();
    assert_eq!((f.r, f.g, f.b, f.w), (0, 0, 0, 255));

    // статичний колір лише на частину приладів — ефект лишається на інших
    ok(&mut e, effect(EffectKind::Rainbow, &[&rgb, &rgbw]));
    ok(
        &mut e,
        Command::StaticColor {
            ids: ids(&[&rgb]),
            source: StaticSource::Custom { hex: Rgb(255, 0, 0), hex_w: None, w: 0 },
        },
    );
    assert!(!e.show.effect.ids.contains(&rgb) && e.show.effect.kind == EffectKind::Rainbow);
    assert_eq!(&e.render(0.0)[0..4], &[255, 255, 0, 0]);

    ok(&mut e, Command::AddStaticColor { name: "Мій".into(), hex: Rgb(0x12, 0x34, 0x56), w: 0 });
    let last = e.show.static_colors.last().unwrap();
    assert_eq!((last.name.as_str(), last.hex, last.w), ("Мій", Rgb(0x12, 0x34, 0x56), None));
}

#[test]
fn wash243_and_default_profiles() {
    let m = model_profiles();
    assert_eq!(segment_count(&m[WASH24]), 8);
    assert_eq!(m[WASH24].len(), 24);
    assert_eq!(segment_count(&m["Free Color WASH 243 · 6 кан. E001 (2 сегменти)"]), 2);
    assert_eq!(m["Free Color WASH 243 · 4 кан. d001 (Dim+RGB)"], ids(&["dim", "r", "g", "b"]));
    let d = default_profiles();
    assert_eq!(
        d.keys().cloned().collect::<BTreeSet<_>>(),
        ["RGB (3 кан.)", "RGBW (4 кан.)", "Dim+RGB (4 кан.)", "RGB+Dim (4 кан.)"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert!(d.values().flatten().all(|r| ["r", "g", "b", "w", "dim"].contains(&r.as_str())));
}

#[test]
fn segment_colors_and_clear() {
    let mut e = Engine::new();
    let fx = add_named(&mut e, WASH24, 1, "Бар");
    ok(&mut e, hex(&[&fx], "#0000ff"));
    ok(&mut e, hex(&[&format!("{fx}#2")], "#ff0000"));
    let f = e.render(0.0);
    assert_eq!(&f[0..3], &[0, 0, 255]);
    assert_eq!(&f[6..9], &[255, 0, 0]);
    assert_eq!(&f[21..24], &[0, 0, 255]);
    assert_eq!(e.live[&fx].len(), 8);

    ok(&mut e, Command::Clear { ids: ids(&[&format!("{fx}#0")]) });
    let f = e.render(0.0);
    assert_eq!(&f[0..6], &[0, 0, 0, 0, 0, 255]);

    ok(&mut e, hex(&[&fx], "#00ff00"));
    assert!(e.fixture(&fx).unwrap().segs.is_none(), "колір на весь прилад скидає сегменти");

    ok(&mut e, effect(EffectKind::Rainbow, &[&fx]));
    ok(&mut e, Command::Clear { ids: vec![] });
    assert_eq!(e.show.effect.kind, EffectKind::None);
    assert!(e.render(0.0).iter().all(|&v| v == 0));
}

#[test]
fn effect_runs_along_segments() {
    let mut e = Engine::new();
    let fx = add(&mut e, WASH24, 1);
    ok(
        &mut e,
        Command::SetEffect(EffectPatch {
            kind: Some(EffectKind::Rainbow),
            ids: Some(ids(&[&fx])),
            spread: Some(1.0),
            by_segment: Some(true),
            ..Default::default()
        }),
    );
    let f = e.render(0.0);
    let segs: BTreeSet<&[u8]> = (0..8).map(|i| &f[i * 3..i * 3 + 3]).collect();
    assert!(segs.len() > 4, "веселка розкладена по сегментах");
    ok(&mut e, Command::SetEffect(EffectPatch { by_segment: Some(false), ..Default::default() }));
    let f = e.render(0.0);
    let segs: BTreeSet<&[u8]> = (0..8).map(|i| &f[i * 3..i * 3 + 3]).collect();
    assert_eq!(segs.len(), 1, "увесь бар одним кольором");
}

#[test]
fn scene_restores_segments() {
    let mut e = Engine::new();
    let fx = add(&mut e, WASH24, 1);
    ok(&mut e, hex(&[&format!("{fx}#7")], "#ffffff"));
    ok(&mut e, Command::SaveScene { name: "s".into(), with_effect: false });
    ok(&mut e, Command::Clear { ids: vec![] });
    e.handle(Command::RecallScene { name: "s".into(), fade: 1.0 }, 50.0).unwrap();
    e.render(50.5);
    let s = e.fixture(&fx).unwrap().segs.as_ref().unwrap()[7].unwrap();
    assert!((120..=135).contains(&s[0]));
    e.render(52.0);
    assert_eq!(e.fixture(&fx).unwrap().segs.as_ref().unwrap()[7], Some([255, 255, 255]));
}

#[test]
fn fixed_roles_and_raw_channel_test() {
    let mut e = Engine::new();
    let fx = add(&mut e, "RGB (3 кан.)", 1);
    ok(
        &mut e,
        Command::SaveProfile {
            name: "Мій бар".into(),
            channels: ids(&["=10", "r", "g", "b"]),
            assign_to: Some(fx.clone()),
        },
    );
    assert_eq!(e.fixture(&fx).unwrap().profile, "Мій бар");
    ok(&mut e, hex(&[&fx], "#ff8000"));
    assert_eq!(&e.render(0.0)[0..4], &[10, 255, 128, 0]);
    ok(&mut e, Command::RawSet { address: 2, value: 7 });
    assert_eq!(e.render(0.0)[1], 7);
    ok(&mut e, Command::RawClear(None));
    assert_eq!(e.render(0.0)[1], 255);
    assert!(
        e.handle(Command::SaveProfile { name: "RGB (3 кан.)".into(), channels: ids(&["r"]), assign_to: None }, 0.0)
            .is_err()
    );
    assert!(e.handle(Command::RawSet { address: 513, value: 1 }, 0.0).is_err());
}

#[test]
fn presets() {
    assert_eq!([preset_level(1).unwrap(), preset_level(8).unwrap(), preset_level(23).unwrap()], [11, 88, 251]);
    let mut e = Engine::new();
    let bar = add(&mut e, WASH8, 1);
    ok(&mut e, Command::SetMaster(100));
    ok(&mut e, effect(EffectKind::Rainbow, &[&bar]));
    ok(&mut e, Command::Preset { ids: ids(&[&bar]), n: Some(Some(3)), speed: None });
    assert_eq!(e.render(0.0)[2], preset_level(3).unwrap(), "програма — у свій канал");
    assert_eq!(e.show.effect.kind, EffectKind::None);

    let mut e = Engine::new();
    let bar = add(&mut e, WASH8, 1);
    let rgb_bar = add(&mut e, WASH24, 9);
    let par = add(&mut e, "Dim+RGB (4 кан.)", 40);
    assert_eq!(e.preset_fixtures(), vec![bar.clone()]);
    ok(&mut e, Command::Preset { ids: vec![], n: Some(Some(2)), speed: None });
    assert_eq!(e.fixture(&bar).unwrap().preset, Some(2));
    assert_eq!(e.fixture(&rgb_bar).unwrap().preset, None);
    assert!(e.handle(Command::Preset { ids: ids(&[&par]), n: Some(Some(2)), speed: None }, 0.0).is_err());
    assert!(e.handle(Command::Preset { ids: vec![], n: Some(Some(99)), speed: None }, 0.0).is_err());
}

#[test]
fn presets_dropped_on_load_without_program_channel() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("config.json");
    std::fs::write(&p, json!({"version": 2, "fixtures": [{"id": "a", "name": "A", "address": 1, "profile": "RGB (3 кан.)", "preset": 4}]}).to_string()).unwrap();
    let e = Engine::load(&p);
    assert_eq!(e.show.fixtures[0].preset, None);
    assert!(e.preset_fixtures().is_empty());
}

#[test]
fn strobe_button_only_for_strobe_channel() {
    let mut e = Engine::new();
    let bar = add(&mut e, WASH8, 1);
    let par = add(&mut e, "RGB (3 кан.)", 20);
    assert_eq!(e.strobe_fixtures(), vec![bar.clone()]);
    ok(&mut e, Command::Strobe { on: Some(true), ids: vec![] });
    assert_eq!(e.render(0.0)[1], STROBE_LEVEL);
    assert_eq!(e.can_undo(), Some("add_fixture"), "строб не потрапляє в скасування");
    ok(&mut e, Command::Strobe { on: Some(false), ids: vec![] });
    assert_eq!(e.render(0.0)[1], 0);
    assert!(e.handle(Command::Strobe { on: Some(true), ids: ids(&[&par]) }, 0.0).is_err());
    ok(&mut e, Command::Strobe { on: None, ids: ids(&[&bar, &par]) });
    assert_eq!(e.strobe, StrobeState { on: true, ids: vec![bar] });
}

#[test]
fn hotkeys_saved() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("config.json");
    let mut e = Engine::load(&p);
    let mut h = indexmap::IndexMap::new();
    h.insert("blackout".to_string(), ids(&["KeyX"]));
    h.insert("strobe".to_string(), vec![]);
    ok(&mut e, Command::SetHotkeys(h.clone()));
    e.save(0.0).unwrap();
    assert_eq!(Engine::load(&p).hotkeys, h);
    let mut bad = indexmap::IndexMap::new();
    bad.insert("x".to_string(), ids(&[""]));
    assert!(e.handle(Command::SetHotkeys(bad), 0.0).is_err());
}

#[test]
fn undo_redo() {
    let mut e = Engine::new();
    let fx = add(&mut e, "RGB (3 кан.)", 1);
    ok(
        &mut e,
        Command::StaticColor {
            ids: ids(&[&fx]),
            source: StaticSource::Custom { hex: Rgb(255, 0, 0), hex_w: None, w: 0 },
        },
    );
    // серія рухів повзунка — один крок
    for (i, v) in [10, 50, 120].into_iter().enumerate() {
        e.handle(
            Command::SetColor { ids: ids(&[&fx]), values: ColorValues { g: Some(v), ..Default::default() } },
            1.0 + i as f64 * 0.2,
        )
        .unwrap();
    }
    assert_eq!(e.fixture(&fx).unwrap().g, 120);
    assert_eq!(e.can_undo(), Some("set_color"));
    ok(&mut e, Command::Undo);
    assert_eq!((e.fixture(&fx).unwrap().r, e.fixture(&fx).unwrap().g), (255, 0));
    ok(&mut e, Command::Undo);
    assert_eq!(e.fixture(&fx).unwrap().r, 0);
    ok(&mut e, Command::Redo);
    assert_eq!(e.fixture(&fx).unwrap().r, 255);
    ok(&mut e, Command::Undo);
    ok(&mut e, Command::Undo);
    assert!(e.show.fixtures.is_empty(), "скасовано і додавання приладу");
    assert!(e.handle(Command::Undo, 0.0).is_err());
    // невдала команда не створює кроку
    assert!(e.handle(Command::RecallScene { name: "немає".into(), fade: 0.0 }, 0.0).is_err());
    assert_eq!(e.can_undo(), None);
    // нова дія очищає «повторити»
    ok(&mut e, Command::Redo);
    ok(&mut e, Command::SetMaster(10));
    assert_eq!(e.can_redo(), None);
}

#[test]
fn undo_keeps_discrete_actions_separate() {
    let mut e = Engine::new();
    add(&mut e, "RGB (3 кан.)", 1);
    add(&mut e, "RGB (3 кан.)", 10);
    ok(&mut e, Command::Undo);
    assert_eq!(e.show.fixtures.len(), 1);
}

#[test]
fn white_mixes_into_rgb_without_white_channel() {
    let mut e = Engine::new();
    let rgb = add(&mut e, "RGB (3 кан.)", 1);
    let rgbw = add(&mut e, "RGBW (4 кан.)", 4);
    ok(
        &mut e,
        Command::SetColor {
            ids: ids(&[&rgb, &rgbw]),
            values: ColorValues { r: Some(200), g: Some(0), b: Some(50), w: Some(100), ..Default::default() },
        },
    );
    let f = e.render(0.0);
    assert_eq!(&f[0..3], &[255, 100, 150]);
    assert_eq!(&f[3..7], &[200, 0, 50, 100]);
}

#[test]
fn fixture_library_modes_are_valid() {
    let lib = library::models();
    assert!(lib.len() > 1000);
    for fx in lib {
        for mode in &fx.modes {
            nitor_core::fixtures::validate_profile(mode.channels()).unwrap();
        }
    }
    let found = library::search("adj mega bar", 40, "", 0, &[]);
    assert!(!found.is_empty() && found.iter().all(|f| f.n.contains("Mega")));
    assert_eq!(library::search("", 10, "", 0, &[]).len(), 10);
    assert!(library::search("", 50, "bar", 0, &[]).iter().all(|f| f.t == "bar"));
    assert_eq!(library::search("", 1, "", 0, &[])[0].n, "WASH 243");
}

fn add_model(e: &mut Engine, fx: &library::LibModel, mode: &library::Mode, address: u16) -> String {
    ok(
        e,
        Command::AddFixture {
            spec: ProfileSpec {
                profile: fx.profile_name(mode),
                channels: Some(mode.channels().to_vec()),
                model: Some(Model { m: fx.m.clone(), n: fx.n.clone(), mode: mode.name().into() }),
            },
            name: "".into(),
            address,
            count: 1,
        },
    );
    e.show.fixtures.last().unwrap().id.clone()
}

#[test]
fn library_device_profile_not_in_menu() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("config.json");
    let mut e = Engine::load(&p);
    let fx = library::search("adj mega bar 50rgb", 40, "", 0, &[])[0];
    let mode = &fx.modes[0];
    let name = fx.profile_name(mode);
    let id = add_model(&mut e, fx, mode, 1);
    let f = e.fixture(&id).unwrap().clone();
    assert_eq!(f.profile, name);
    assert_eq!(e.show.profiles[&name], mode.channels());
    assert_eq!(f.model, Some(Model { m: fx.m.clone(), n: fx.n.clone(), mode: mode.name().into() }));
    let menu = e.menu_profiles();
    assert!(!menu.contains(&name));
    assert!(default_profiles().keys().all(|k| menu.contains(k)));
    assert_eq!(e.recent_models, vec![library::Recent { m: fx.m.clone(), n: fx.n.clone() }]);
    assert_eq!(library::search("", 10, "recent", 0, &e.recent_models)[0].n, fx.n);
    e.save(0.0).unwrap();
    let mut e2 = Engine::load(&p);
    assert_eq!(e2.show.fixtures[0].model, f.model);
    assert!(!e2.menu_profiles().contains(&name));
    assert_eq!(e2.recent_models, e.recent_models);
    // стандартний профіль відв'язує прилад від моделі; невикористаний профіль моделі не зберігається
    ok(
        &mut e2,
        Command::UpdateFixture {
            id: id.clone(),
            name: None,
            address: None,
            profile: Some(ProfileSpec::named("RGB (3 кан.)")),
        },
    );
    assert_eq!(e2.show.fixtures[0].model, None);
    e2.save(0.0).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert!(saved["profiles"].get(&name).is_none());
    assert_eq!(saved["version"], 2);
}

#[test]
fn recent_models_order_and_mode_change() {
    let mut e = Engine::new();
    let found = library::search("", 2, "par", 0, &[]);
    let (a, b) = (found[0], found[1]);
    for fx in [a, b, a] {
        let addr = 1 + 20 * e.show.fixtures.len() as u16;
        add_model(&mut e, fx, fx.modes.last().unwrap(), addr);
    }
    let recent: Vec<&str> = e.recent_models.iter().map(|r| r.n.as_str()).collect();
    assert_eq!(recent, [a.n.as_str(), b.n.as_str()]);
    let id = e.show.fixtures[1].id.clone();
    let mode = &b.modes[0];
    ok(
        &mut e,
        Command::UpdateFixture {
            id: id.clone(),
            name: None,
            address: None,
            profile: Some(ProfileSpec {
                profile: b.profile_name(mode),
                channels: Some(mode.channels().to_vec()),
                model: Some(Model { m: b.m.clone(), n: b.n.clone(), mode: mode.name().into() }),
            }),
        },
    );
    let f = e.fixture(&id).unwrap();
    assert_eq!(e.show.profiles[&f.profile], mode.channels());
    assert_eq!(f.model.as_ref().unwrap().mode, mode.name());
    assert_eq!(e.recent_models[0].n, b.n);
}

fn import(e: &mut Engine, data: &serde_json::Value) -> nitor_core::Result<()> {
    e.handle(Command::ImportScene { data: data.clone(), name: None }, 0.0)
}

#[test]
fn scene_file_moves_to_other_computer() {
    let mut a = Engine::new();
    let left = add_named(&mut a, "RGBW (4 кан.)", 1, "Ліва");
    add_named(&mut a, "Dim+RGB (4 кан.)", 10, "Права");
    let wash = library::find("Free Color", "WASH 243").unwrap();
    let bar = add_model(&mut a, wash, &wash.modes[0], 20);
    ok(
        &mut a,
        Command::SetColor {
            ids: ids(&[&left]),
            values: ColorValues { r: Some(255), g: Some(10), b: Some(0), w: Some(40), ..Default::default() },
        },
    );
    ok(
        &mut a,
        Command::SetColor {
            ids: ids(&[&format!("{bar}#2")]),
            values: ColorValues { r: Some(0), g: Some(0), b: Some(200), ..Default::default() },
        },
    );
    ok(&mut a, Command::SaveScene { name: "Вечір".into(), with_effect: false });
    let data: serde_json::Value = serde_json::from_str(&scene_file::export(&a, "Вечір").unwrap().to_string()).unwrap();

    let mut b = Engine::new();
    add_named(&mut b, "RGBW (4 кан.)", 1, "L");
    add_named(&mut b, "Dim+RGB (4 кан.)", 10, "R");
    import(&mut b, &data).unwrap();
    let note = b.pop_notice().unwrap();
    assert!(note.contains("Вечір") && note.contains("додано: 1"), "{note}");
    assert_eq!(b.show.fixtures.len(), 3);
    let new_bar = b.show.fixtures[2].clone();
    assert_eq!(new_bar.address, 20);
    assert_eq!(new_bar.model.as_ref().unwrap().n, "WASH 243");
    assert!(!b.menu_profiles().contains(&new_bar.profile));
    ok(&mut b, Command::RecallScene { name: "Вечір".into(), fade: 0.0 });
    assert_eq!(&b.render(0.0)[0..4], &[255, 10, 0, 40]);
    assert_eq!(b.show.fixtures[2].segs.as_ref().unwrap()[2], Some([0, 0, 200]));
    import(&mut b, &data).unwrap();
    assert!(b.show.scenes.contains_key("Вечір (2)"));
    assert_eq!(b.show.fixtures.len(), 3);
    ok(&mut b, Command::Undo);
    assert!(!b.show.scenes.contains_key("Вечір (2)"));
}

#[test]
fn scene_file_busy_addresses_and_bad_file() {
    let mut a = Engine::new();
    add_named(&mut a, "RGB (3 кан.)", 1, "PAR");
    ok(&mut a, Command::SaveScene { name: "S".into(), with_effect: false });
    let data = scene_file::export(&a, "S").unwrap();
    let mut b = Engine::new();
    add_named(&mut b, "RGBW (4 кан.)", 1, "Інший");
    let err = import(&mut b, &data).unwrap_err();
    assert!(err.0.contains("зіставити"));
    assert!(!b.show.scenes.contains_key("S"));
    assert_eq!(b.show.fixtures.len(), 1);
    assert!(import(&mut b, &json!({"hello": 1})).unwrap_err().0.contains("не файл сцени"));
    // власний профіль з іншого комп'ютера приходить разом із приладом
    ok(
        &mut a,
        Command::SaveProfile {
            name: "Мій PAR".into(),
            channels: ids(&["dim", "r", "g", "b", "zero"]),
            assign_to: None,
        },
    );
    add(&mut a, "Мій PAR", 100);
    ok(&mut a, Command::SaveScene { name: "S2".into(), with_effect: false });
    import(&mut b, &scene_file::export(&a, "S2").unwrap()).unwrap();
    let added = b.show.fixtures.iter().find(|f| f.address == 100).unwrap().clone();
    assert_eq!(b.show.profiles[&added.profile], ids(&["dim", "r", "g", "b", "zero"]));
    assert!(b.menu_profiles().contains(&added.profile));
    // файли старого формату 1.x не відкриваються
    assert!(import(&mut b, &json!({"format": "sc-dmx-scene", "fixtures": []})).is_err());
}

fn scene_with(effect: serde_json::Value, items: serde_json::Value) -> serde_json::Value {
    json!({"format": "nitor-scene", "version": 2, "name": "X", "fixtures": items, "effect": effect})
}

#[test]
fn scene_file_with_broken_effect_does_not_freeze_light() {
    for effect in [
        json!({"type": "rainbow", "ids": ["f1"], "speed": "fast", "spread": null, "colors": ["#00ff00", 5, "zzz"]}),
        json!({"type": "rainbow", "ids": ["f1"], "speed": f64::MAX, "colors": ["#00ff00"]}),
        json!({"type": "laser", "ids": ["f1"]}),
        json!("rainbow"),
    ] {
        let mut e = Engine::new();
        add(&mut e, "RGB (3 кан.)", 1);
        let items = json!([{"id": "f1", "address": 1, "channels": ["r", "g", "b"], "state": {"r": 255}}]);
        import(&mut e, &scene_with(effect, items)).unwrap();
        ok(&mut e, Command::RecallScene { name: "X".into(), fade: 0.0 });
        e.render(1.0);
        assert!(e.show.effect.colors.iter().all(|c| *c == Rgb(0, 255, 0)));
        ok(&mut e, Command::SetEffect(EffectPatch { kind: Some(EffectKind::None), ..Default::default() }));
        assert_eq!(e.show.effect.kind, EffectKind::None);
        e.render(2.0);
    }
}

#[test]
fn scene_file_item_without_id_is_skipped() {
    let mut e = Engine::new();
    add(&mut e, "RGB (3 кан.)", 1);
    let items = json!([
        {"address": 1, "channels": ["r", "g", "b"], "state": {"r": 1}},
        {"id": "f1", "address": 1, "channels": ["r", "g", "b"], "state": {"r": 9}}
    ]);
    import(&mut e, &scene_with(json!(null), items)).unwrap();
    assert!(e.show.scenes.contains_key("X"));
}

#[test]
fn corrupt_config_does_not_block_start() {
    for text in [
        "{not json",
        "[1, 2]",
        r#"{"version": 2, "fixtures": 5}"#,
        r#"{"version": 2, "master": "max"}"#,
        r#"{"fixtures": []}"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        std::fs::write(&p, text).unwrap();
        let e = Engine::load(&p);
        assert!(e.show.fixtures.is_empty() && e.show.master == 255);
        assert!(e.load_warning.is_some(), "{text}");
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|f| f.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names.len(), 1);
        assert!(names[0].starts_with("config.broken-"));
    }
}

// ------------------------------------------------------------------ userdata --
fn full_config() -> serde_json::Value {
    json!({
        "version": 2,
        "output": {"type": "artnet", "host": "", "universe": 1, "port": 6454},
        "master": 200,
        "profiles": {"Мій": ["r", "g", "b", "zero"], "Модель · 3 кан.": ["r", "g", "b"], "Чужий": ["dim", "r", "g", "b"]},
        "device_profiles": ["Модель · 3 кан."],
        "recent_models": [{"m": "A", "n": "B"}],
        "fixtures": [{"id": "a", "name": "A", "address": 1, "profile": "Модель · 3 кан."},
                     {"id": "b", "name": "B", "address": 10, "profile": "Мій"}],
        "scenes": {"S": {"fixtures": {}, "effect": null}},
        "static_colors": [{"name": "X", "hex": "#ff0000"}],
        "hotkeys": {"blackout": ["KeyX"]}
    })
}

fn parts(p: &[userdata::Part]) -> BTreeSet<userdata::Part> {
    p.iter().copied().collect()
}

#[test]
fn userdata_strip() {
    use userdata::Part::*;
    let cfg = full_config();
    let c = cfg.as_object().unwrap();
    let s = userdata::strip(c, &parts(&[Scenes]));
    assert!(!s.contains_key("scenes") && s.contains_key("fixtures"));
    let s = userdata::strip(c, &parts(&[Fixtures]));
    assert!(!s.contains_key("fixtures") && !s.contains_key("recent_models"));
    assert!(s["profiles"].get("Модель · 3 кан.").is_none() && s["profiles"].get("Мій").is_some());
    let s = userdata::strip(c, &parts(&[Profiles]));
    // у використанні — лишаються; невикористаний власний — видаляється
    assert!(s["profiles"].get("Мій").is_some() && s["profiles"].get("Модель · 3 кан.").is_some());
    assert!(s["profiles"].get("Чужий").is_none());
    let s = userdata::strip(c, &parts(&[Settings, Colors]));
    assert!(
        !s.contains_key("output")
            && !s.contains_key("master")
            && !s.contains_key("hotkeys")
            && !s.contains_key("static_colors")
    );
}

#[test]
fn userdata_apply() {
    use userdata::Part::*;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("config.json");
    assert!(userdata::apply(&p, &parts(&[Scenes])).unwrap(), "файлу немає — нічого робити");
    std::fs::write(&p, full_config().to_string()).unwrap();
    assert!(!userdata::apply(&p, &parts(&[Scenes])).unwrap());
    let e = Engine::load(&p);
    assert!(e.load_warning.is_none(), "рушій читає очищений файл");
    assert!(e.show.scenes.is_empty() && e.show.fixtures.len() == 2 && e.show.master == 200);
    assert!(userdata::apply(&p, &parts(&[Fixtures, Profiles, Colors, Settings])).unwrap(), "нічого не лишилось");
    assert!(!p.exists());
    std::fs::write(&p, full_config().to_string()).unwrap();
    assert!(userdata::apply(&p, &parts(&userdata::Part::ALL)).unwrap());
    assert!(!p.exists());
}

#[test]
fn flash_overrides_and_restores() {
    let mut e = Engine::new();
    let a = add(&mut e, "Dim+RGB (4 кан.)", 1);
    let b = add(&mut e, "Dim+RGB (4 кан.)", 5);
    ok(&mut e, hex(&[&a, &b], "#ff0000"));
    ok(&mut e, Command::SetMaster(51));
    let before = e.render(0.0);
    assert_eq!(&before[0..4], &[51, 255, 0, 0]);
    let white = StaticSource::Custom { hex: Rgb(255, 255, 255), hex_w: None, w: 0 };
    ok(&mut e, Command::Flash(Some(FlashSource::Color { ids: ids(&[&a]), source: white })));
    let f = e.render(0.0);
    assert_eq!(&f[0..4], &[255, 255, 255, 255], "спалах — повний білий без майстра");
    assert_eq!(&f[4..8], &before[4..8], "інші прилади без змін");
    ok(&mut e, Command::Blackout(Some(true)));
    assert!(e.render(0.0)[0..8].iter().all(|&v| v == 0), "Blackout перекриває спалах");
    ok(&mut e, Command::Blackout(Some(false)));
    ok(&mut e, Command::Flash(None));
    assert_eq!(e.render(0.0), before, "відпустили — все як було");
    assert_eq!(e.can_undo(), Some("blackout"), "спалах у скасування не потрапляє");
    assert!(e.to_config().fixtures.iter().all(|f| (f.r, f.g) == (255, 0)), "у файл спалах не потрапляє");
}

#[test]
fn flash_scene_and_groups() {
    let mut e = Engine::new();
    let a = add(&mut e, "Dim+RGB (4 кан.)", 1);
    ok(&mut e, hex(&[&a], "#0000ff"));
    ok(&mut e, Command::SaveScene { name: "синя".into(), with_effect: false });
    ok(&mut e, hex(&[&a], "#00ff00"));
    ok(&mut e, effect(EffectKind::Rainbow, &[&a]));
    ok(&mut e, Command::Flash(Some(FlashSource::Scene("синя".into()))));
    assert_eq!(&e.render(3.0)[0..4], &[255, 0, 0, 255], "сцена поверх ефекту");
    ok(&mut e, Command::Flash(None));
    assert_eq!(e.show.effect.kind, EffectKind::Rainbow, "ефект лишився");

    let mut g = indexmap::IndexMap::new();
    g.insert(" Сцена ".to_string(), ids(&[&a, &a, "немає"]));
    ok(&mut e, Command::SetGroups(g));
    assert_eq!(e.show.groups["Сцена"], ids(&[&a]), "без повторів і зниклих приладів");
    ok(&mut e, Command::DeleteFixture { id: a.clone() });
    assert!(e.show.groups["Сцена"].is_empty());
    ok(&mut e, Command::Undo);
    assert_eq!(e.show.groups["Сцена"], ids(&[&a]), "скасування повертає прилад і в групу");
}

#[test]
fn operator_pin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut e = Engine::load(&path);
    assert!(e.handle(Command::SetPin("12".into()), 0.0).is_err(), "закороткий PIN");
    ok(&mut e, Command::SetPin("1234".into()));
    ok(&mut e, Command::Operator { on: true, pin: String::new() });
    assert!(e.handle(Command::Operator { on: false, pin: "0000".into() }, 0.0).is_err());
    assert!(e.operator);
    e.save(0.0).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("1234"), "PIN не зберігається відкритим текстом");
    let mut e = Engine::load(&path);
    assert!(e.operator, "після перезапуску — знову режим оператора");
    ok(&mut e, Command::Operator { on: false, pin: "1234".into() });
    assert!(!e.operator);
}
