//! Програма: стан інтерфейсу, знімок стану рушія на кадр, вибір приладів і дії.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use egui::Context;
use nitor_core::engine::{ColorValues, EffectPatch, FlashSource, Live, Prefs, Show, StaticSource};
use nitor_core::fixtures::{Fixture, Segs, segment_count};
use nitor_core::library::Recent;
use nitor_core::midi::{MidiBinding, MidiValue};
use nitor_core::outputs::{OutputConfig, OutputInfo, SerialPortInfo, UdmxDevice};
use nitor_core::tempo::AudioSource;
use nitor_core::{Command, DMX_CHANNELS, EffectKind, Rgb, scene_file};

use crate::core::Core;
use crate::dialogs::{AddState, LibState, ModesState, PropsState, SettingsState, UiDlg};
use crate::menus::MenuState;
use crate::update::Updater;

/// Знімок стану рушія для одного кадру інтерфейсу.
pub struct View {
    pub show: Show,
    pub recent: Vec<Recent>,
    pub output_cfg: OutputConfig,
    pub prefs: Prefs,
    pub hotkeys: indexmap::IndexMap<String, Vec<String>>,
    pub live: Live,
    pub dmx: [u8; DMX_CHANNELS],
    pub undo: Option<&'static str>,
    pub redo: Option<&'static str>,
    pub menu_profiles: Vec<String>,
    pub preset_fixtures: Vec<String>,
    pub overlaps: Vec<(String, String)>,
    pub output: OutputInfo,
    pub flashing: bool,
    pub operator: bool,
    pub has_pin: bool,
    pub audio: AudioSource,
    pub bpm: Option<f64>,
    /// момент останнього удару (годинник рушія); 0 — ударів ще не було
    pub beat_at: f64,
    pub midi: Vec<MidiBinding>,
    pub schedule: Vec<nitor_core::schedule::Event>,
    pub place: Option<nitor_core::schedule::Place>,
    pub os2l: Option<u16>,
}

impl View {
    pub fn fixture(&self, id: &str) -> Option<&Fixture> {
        self.show.fixtures.iter().find(|f| f.id == id)
    }

    pub fn channels(&self, f: &Fixture) -> &[String] {
        self.show.profiles.get(&f.profile).map_or(&[], |v| v.as_slice())
    }

    pub fn segs(&self, f: &Fixture) -> usize {
        segment_count(self.channels(f))
    }

    /// Остання адреса приладу.
    pub fn end(&self, f: &Fixture) -> usize {
        f.address as usize + self.channels(f).len().saturating_sub(1)
    }

    pub fn next_free_address(&self) -> i64 {
        let end = self.show.fixtures.iter().map(|f| self.end(f)).max().unwrap_or(0);
        (end + 1).min(DMX_CHANNELS) as i64
    }

    pub fn effect_running(&self) -> bool {
        self.show.effect.kind != EffectKind::None
    }
}

/// Елемент вибору: прилад цілком (`seg: None`) або сегмент.
#[derive(Clone)]
pub struct SelItem {
    pub id: String,
    pub seg: Option<usize>,
}

/// Чернетка ефекту: повзунки належать користувачу, поки ефект не запущено.
pub struct EffDraft {
    pub kind: EffectKind,
    /// 1..100 (логарифмічна шкала швидкості)
    pub speed: f32,
    /// 0..100 %
    pub spread: f32,
    pub colors: Vec<Rgb>,
    pub by_seg: bool,
    pub beat: bool,
    pub beat_pulse: bool,
    pub synced: bool,
}

pub fn speed_from_slider(x: f32) -> f64 {
    0.05 * 10f64.powf(x as f64 / 40.0)
}

pub fn slider_from_speed(s: f64) -> f32 {
    (40.0 * (s / 0.05).log10()).round() as f32
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Color,
    Presets,
}

pub struct Toast {
    pub text: String,
    pub info: bool,
    pub born: Instant,
    pub until: Instant,
    /// номер для власної області egui: повідомлення рухаються незалежно
    pub seq: u64,
}

/// Скільки повідомлень видно одночасно (старіші зникають).
pub const TOASTS_MAX: usize = 3;

/// Дії з меню, гарячих клавіш і вікон підтвердження.
#[derive(Clone, Debug)]
pub enum Act {
    Cmd(Command),
    StaticTo {
        index: usize,
        ids: Vec<String>,
    },
    Clear,
    RunEffect(Vec<String>),
    RunEffectSelected,
    StopEffect,
    StopEffectOn(Vec<String>),
    SelectWhole(String),
    SelectSameProfile(String),
    OpenPresets,
    OpenProps(String),
    Duplicate(String),
    AskDeleteFixtures {
        ids: Vec<String>,
        close_props: bool,
    },
    DeleteFixtures {
        ids: Vec<String>,
        close_props: bool,
    },
    CopyColor,
    PasteColor,
    AddPaletteColor,
    /// Зберегти колір першого вибраного приладу в палітру.
    SaveColor,
    AskDeleteColor(usize),
    ClearPalette,
    RecallScene {
        name: String,
        fade: f64,
    },
    AskOverwriteScene(String),
    SaveScene(String),
    SaveSceneFile(String),
    AskDeleteScene(String),
    AskNewScene,
    OpenSceneFile,
    OpenAdd,
    SelectAll,
    SelectNone,
    SelectInvert,
    Undo,
    Redo,
    Blackout,
    /// «Видалити» на клавіатурі: вибрані прилади — після підтвердження.
    AskDeleteSelected,
    /// Поміняти місцями «Ефекти» і «Сцени».
    SwapSide,
    /// Показати чи сховати назву кнопки верхньої панелі (`clear`, `flash`, `blackout`).
    ToggleShortButton(&'static str),
    /// Плавність «Очистити» (`clear`) чи Blackout (`blackout`), секунди, або частота «Спалаху» (`flash`), Гц.
    LightPref(&'static str, f64),
    /// Вікно точної частоти «Спалаху» під кнопкою.
    FlashConfig(egui::Rect),
    /// Сховати знаки «?» з поясненнями.
    HideHelp,
    /// Закінчити редагування вигляду головного екрана.
    EndLayoutEdit,
    CheckUpdate,
    OpenUpdate,
    OpenSettings(Option<crate::dialogs::Tab>),
    TextOp(egui::Id, TextOp),
    ResetKeys,
    AskNewGroup,
    SaveGroup(String),
    AskRenameGroup(String),
    RenameGroup(String, String),
    DeleteGroup(String),
    AskOperator,
    EnterOperator(String),
    AskExitOperator,
    ExitOperator(String),
    Quit,
}

/// Чим тримають «Спалах»: клавішею, кнопкою миші чи кнопкою MIDI-контролера.
#[derive(Clone, PartialEq)]
pub enum FlashHold {
    Key(egui::Key),
    Pointer,
    Midi(String),
}

/// Повний білий «Спалаху» за замовчуванням (і RGB, і W).
pub const FLASH_WHITE: StaticSource =
    StaticSource::Custom { hex: Rgb(255, 255, 255), hex_w: Some(Rgb(255, 255, 255)), w: 255 };

#[derive(Clone, Copy, Debug)]
pub enum TextOp {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

pub struct App {
    pub core: Core,
    pub updater: Updater,
    pub v: View,
    pub sel: Vec<String>,
    pub last_color: Rgb,
    pub eff: EffDraft,
    pub pane: Pane,
    /// виділення рамкою: де почали тягнути і що було вибрано до того
    pub band: Option<(egui::Pos2, Vec<String>)>,
    /// плитки приладів цього кадру (для виділення рамкою)
    pub tile_rects: Vec<(String, egui::Rect)>,
    /// відкрите налаштування «Спалаху»: кнопка, під якою воно показане
    pub flash_cfg: Option<egui::Rect>,
    pub scene_name: String,
    pub scene_with_effect: bool,
    pub scene_fade: f64,
    pub monitor_open: bool,
    /// редагування вигляду головного екрана (олівець): ширина й порядок стовпців
    pub layout_edit: bool,
    /// ширина стовпців, поки тягнуть межу (у налаштування — коли відпустили)
    pub col_live: Option<std::collections::BTreeMap<String, f32>>,
    /// анімація обміну стовпців: коли почали і де стовпці були до того
    pub col_anim: Option<(f64, Vec<(&'static str, egui::Rect)>)>,
    /// коли «Ефекти» і «Сцени» помінялися місцями (для анімації)
    pub side_anim: Option<f64>,
    /// де стовпці намальовано в цьому кадрі
    pub col_rects: Vec<(&'static str, egui::Rect)>,
    /// у заголовку тісно: кнопки лише зі значками
    pub top_compact: bool,
    /// вільне місце в заголовку (без розгорнутих при наведенні назв): чи вміститься назва кнопки
    pub title_slack: f32,
    /// цього кадру в заголовку розгорнута назва кнопки під мишею
    pub title_open: bool,
    /// коли востаннє натиснули Esc (чи було відкрите вікно або меню) — для затримки перед «Закрити Nitor?»
    pub esc_t: f64,
    pub toasts: Vec<Toast>,
    /// у системі вимкнено анімацію (зчитано під час запуску)
    pub sys_reduce_motion: bool,
    /// елемент, що мав фокус перед відкриттям вікна: після закриття фокус повертається на нього
    pub focus_back: Option<egui::Id>,
    pub dialog_was: bool,
    pub toast_seq: u64,
    pub flash: HashMap<&'static str, Instant>,
    pub color_clipboard: Option<(Rgb, u8)>,
    pub settings: Option<SettingsState>,
    pub add: Option<AddState>,
    pub lib: Option<LibState>,
    pub modes: Option<ModesState>,
    pub props: Option<PropsState>,
    pub upd_dlg: bool,
    pub ui_dlg: Option<UiDlg>,
    pub menu: Option<MenuState>,
    /// меню, запитане в цьому кадрі (найглибший елемент — першим)
    pub pending_menu: Option<Vec<crate::menus::Item>>,
    pub udmx_devices: Vec<UdmxDevice>,
    pub ports: Vec<SerialPortInfo>,
    pub tested: BTreeSet<u16>,
    pub update_tried: bool,
    pub quitting: bool,
    pub wheel: Option<egui::TextureHandle>,
    pub pending_text_op: Option<(egui::Id, TextOp, u8)>,
    /// вікно запуску (поки воно є, програма ще не відкрита)
    pub splash: Option<crate::splash::Splash>,
    /// вікно запуску щойно стало головним: розмір вікна, якого чекаємо, і крайній строк
    pub main_pending: Option<(egui::Vec2, std::time::Instant)>,
    /// перший кадр ще має дочекатися перевірки оновлень (момент запуску програми)
    pub boot: Option<std::time::Instant>,
    /// інша копія програми просить показати вікно
    pub show_request: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// інсталятор просить завершити роботу
    pub quit_request: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// кадр, на якому зробити знімок (режим NITOR_SCREENSHOT)
    pub shot_frame: Option<u64>,
    pub flash_hold: Option<FlashHold>,
    pub audio: Option<crate::audio::Audio>,
    pub midi: Option<crate::midi::Midi>,
    pub midi_error: String,
    /// Дія, якій чекаємо повідомлення MIDI (режим «Навчити»).
    pub midi_learn: Option<String>,
    pub os2l: Option<crate::os2l::Server>,
    /// Помилка сервера OS2L для порту, який не вдалося відкрити.
    pub os2l_error: Option<(u16, String)>,
    /// Автозапуск з Windows (читається з реєстру, коли відкрили вкладку).
    pub autostart: Option<bool>,
    /// коли почали утримувати «Вийти з режиму оператора» (без PIN)
    pub exit_hold: Option<Instant>,
}

impl App {
    pub fn new(core: Core, updater: Updater) -> App {
        let v = snapshot(&core);
        let mut app = App {
            core,
            updater,
            v,
            sel: vec![],
            last_color: Rgb(255, 0, 0),
            eff: EffDraft {
                kind: EffectKind::Rainbow,
                speed: 40.0,
                spread: 100.0,
                colors: vec![],
                by_seg: true,
                beat: false,
                beat_pulse: false,
                synced: false,
            },
            pane: Pane::Color,
            band: None,
            tile_rects: vec![],
            flash_cfg: None,
            scene_name: String::new(),
            scene_with_effect: false,
            scene_fade: 1.0,
            monitor_open: false,
            layout_edit: false,
            col_live: None,
            col_anim: None,
            side_anim: None,
            col_rects: vec![],
            top_compact: false,
            title_slack: 0.0,
            title_open: false,
            esc_t: 0.0,
            toasts: vec![],
            sys_reduce_motion: crate::winshell::system_reduce_motion(),
            focus_back: None,
            dialog_was: false,
            toast_seq: 0,
            flash: HashMap::new(),
            color_clipboard: None,
            settings: None,
            add: None,
            lib: None,
            modes: None,
            props: None,
            upd_dlg: false,
            ui_dlg: None,
            menu: None,
            pending_menu: None,
            udmx_devices: vec![],
            ports: vec![],
            tested: BTreeSet::new(),
            update_tried: false,
            quitting: false,
            wheel: None,
            pending_text_op: None,
            splash: None,
            main_pending: None,
            boot: None,
            show_request: Default::default(),
            quit_request: Default::default(),
            shot_frame: None,
            flash_hold: None,
            audio: None,
            midi: None,
            midi_error: String::new(),
            midi_learn: None,
            os2l: None,
            os2l_error: None,
            autostart: None,
            exit_hold: None,
        };
        app.sync_effect_draft();
        let warning = app.core.lock().engine.load_warning.clone();
        if let Some(w) = warning {
            app.toast(&w, false);
        }
        app
    }

    pub fn refresh(&mut self) {
        self.v = snapshot(&self.core);
        nitor_ui::widgets::set_show_keys(!self.v.prefs.hide_keys);
        nitor_ui::widgets::set_show_help(!self.v.prefs.hide_help);
        nitor_ui::anim::set_reduced_motion(self.reduce_motion());
        // вибір без видалених приладів
        let ids: BTreeSet<String> = self.v.show.fixtures.iter().map(|f| f.id.clone()).collect();
        self.sel.retain(|s| ids.contains(s.split('#').next().unwrap_or("")));
        self.sync_effect_draft();
    }

    /// Поки ефект працює, чернетка — та, що в рушії (сцена могла її змінити).
    fn sync_effect_draft(&mut self) {
        let e = &self.v.show.effect;
        if e.kind != EffectKind::None {
            self.eff.kind = e.kind;
            self.eff.colors = e.colors.clone();
        }
        if e.kind != EffectKind::None || !self.eff.synced {
            self.eff.speed = slider_from_speed(e.speed).clamp(1.0, 100.0);
            self.eff.spread = (e.spread * 100.0).round() as f32;
            self.eff.by_seg = e.by_segment;
            self.eff.beat = e.beat;
            self.eff.beat_pulse = e.beat_pulse;
            self.eff.synced = true;
        }
    }

    // ---------------------------------------------------------- команди --
    /// Надсилає команду рушію; помилку показує сповіщенням. `true` — виконано.
    pub fn send(&mut self, cmd: Command) -> bool {
        let r = self.core.send(cmd);
        let ok = match r {
            Ok(Some(note)) => {
                self.toast(&note, true);
                true
            }
            Ok(None) => true,
            Err(e) => {
                self.toast(&e, false);
                false
            }
        };
        self.refresh();
        ok
    }

    pub fn toast(&mut self, text: &str, info: bool) {
        let (text, now) = (cap(text), Instant::now());
        let until = now + Duration::from_millis(3500);
        // те саме повідомлення ще раз — лише довше показується, без дубля
        if let Some(t) = self.toasts.iter_mut().find(|t| t.text == text && t.info == info) {
            t.until = until;
            return;
        }
        self.toast_seq += 1;
        self.toasts.push(Toast { text, info, born: now, until, seq: self.toast_seq });
        if self.toasts.len() > TOASTS_MAX {
            self.toasts.remove(0);
        }
    }

    pub fn flash(&mut self, key: &'static str) {
        self.flash.insert(key, Instant::now());
    }

    pub fn flashing(&self, key: &str) -> bool {
        self.flash.get(key).is_some_and(|t| t.elapsed() < Duration::from_millis(1400))
    }

    // ------------------------------------------------------------- вибір --
    pub fn sel_items(&self) -> Vec<SelItem> {
        self.sel
            .iter()
            .filter_map(|s| {
                let (fid, k) = match s.split_once('#') {
                    Some((f, k)) => (f, k.parse::<usize>().ok()),
                    None => (s.as_str(), None),
                };
                self.v.fixture(fid).map(|_| SelItem { id: fid.to_string(), seg: k })
            })
            .collect()
    }

    /// Прилади у виборі (без повторів, у порядку вибору).
    pub fn sel_fixtures(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for it in self.sel_items() {
            if !out.contains(&it.id) {
                out.push(it.id);
            }
        }
        out
    }

    /// Колір елемента вибору: r g b w (сегмент — свій RGB).
    pub fn item_color(&self, it: &SelItem) -> Option<Fixture> {
        let f = self.v.fixture(&it.id)?;
        let mut c = f.clone();
        if let Some(k) = it.seg {
            if let Some(Some(s)) = f.segs.as_ref().and_then(|s: &Segs| s.get(k)) {
                (c.r, c.g, c.b) = (s[0], s[1], s[2]);
            }
        }
        Some(c)
    }

    pub fn first_sel_color(&self) -> Option<(Rgb, u8)> {
        let it = self.sel_items().into_iter().next()?;
        let c = self.item_color(&it)?;
        Some((Rgb(c.r, c.g, c.b), c.w))
    }

    pub fn sel_description(&self) -> String {
        let items = self.sel_items();
        let whole = items.iter().filter(|i| i.seg.is_none()).count();
        let segs = items.len() - whole;
        let mut parts = vec![];
        if whole > 0 {
            parts.push(format!("приладів: {whole}"));
        }
        if segs > 0 {
            parts.push(format!("сегментів: {segs}"));
        }
        parts.join(", ")
    }

    /// Клік — вибрати лише цей прилад/сегмент (повторний клік знімає вибір).
    /// Shift + клік — додати до вибору або прибрати з нього.
    pub fn pick(&mut self, id: &str, multi: bool) {
        let fid = id.split('#').next().unwrap_or("").to_string();
        if multi {
            if let Some(i) = self.sel.iter().position(|s| s == id) {
                self.sel.remove(i);
            } else {
                // прилад цілком і його сегменти взаємовиключні
                let is_seg = id.contains('#');
                self.sel.retain(|x| !(x != id && x.split('#').next() == Some(&fid) && (!is_seg || !x.contains('#'))));
                self.sel.push(id.to_string());
            }
        } else {
            let only = self.sel.len() == 1 && self.sel[0] == id;
            self.sel.clear();
            if !only {
                self.sel.push(id.to_string());
            }
        }
        // клік по бару з програмами відкриває пресети
        if self.sel.iter().any(|s| s == id) && self.v.preset_fixtures.contains(&fid) {
            self.open_presets();
        }
    }

    pub fn select_only(&mut self, id: &str) {
        self.sel = vec![id.to_string()];
    }

    /// Прилади для пресетів: вибрані LED-бари, а якщо їх не вибрано — усі.
    pub fn preset_targets(&self) -> Vec<String> {
        let picked: Vec<String> =
            self.sel_fixtures().into_iter().filter(|id| self.v.preset_fixtures.contains(id)).collect();
        if picked.is_empty() { self.v.preset_fixtures.clone() } else { picked }
    }

    pub fn open_presets(&mut self) {
        if self.v.preset_fixtures.is_empty() {
            self.toast("Немає приладів із вбудованими програмами", false);
            return;
        }
        self.pane = Pane::Presets;
    }

    /// «Зменшити рух» діє: власний вибір, а без нього — як у системі.
    pub fn reduce_motion(&self) -> bool {
        self.v.prefs.reduce_motion.unwrap_or(self.sys_reduce_motion)
    }

    pub fn set_prefs(&mut self, f: impl FnOnce(&mut Prefs)) {
        let mut p = self.v.prefs.clone();
        f(&mut p);
        self.send(Command::SetPrefs(p));
    }

    pub fn effect_patch(&self) -> EffectPatch {
        EffectPatch {
            kind: None,
            ids: None,
            speed: Some((speed_from_slider(self.eff.speed) * 1000.0).round() / 1000.0),
            spread: Some(self.eff.spread as f64 / 100.0),
            colors: Some(self.eff.colors.clone()),
            by_segment: Some(self.eff.by_seg),
            beat: Some(self.eff.beat),
            beat_pulse: Some(self.eff.beat_pulse),
        }
    }

    /// Зміна параметрів працюючого ефекту.
    pub fn live_effect_update(&mut self) {
        if self.v.effect_running() {
            let p = self.effect_patch();
            self.send(Command::SetEffect(p));
        }
    }

    pub fn set_color(&mut self, values: ColorValues) {
        if self.sel.is_empty() {
            self.toast("Спершу виберіть прилади", false);
            return;
        }
        let ids = self.sel.clone();
        self.send(Command::SetColor { ids, values });
    }

    pub fn pick_color(&mut self, c: Rgb) {
        self.last_color = c;
        self.set_color(ColorValues { hex: Some(c), ..Default::default() });
    }

    pub fn clear_colors(&mut self) {
        let ids = self.sel.clone();
        self.send(Command::Clear { ids });
    }

    /// «Спалах» натиснули: на вибрані прилади (або всі), поки не відпустять.
    pub fn flash_start(&mut self, src: FlashSource, hold: FlashHold) {
        if self.flash_hold.is_none() && self.send(Command::Flash(Some(src))) {
            self.flash_hold = Some(hold);
        }
    }

    pub fn flash_color(&mut self, source: StaticSource, hold: FlashHold) {
        let ids = self.sel.clone();
        self.flash_start(FlashSource::Color { ids, source }, hold);
    }

    /// Відпускання «Спалаху»: клавішу відпустили, кнопку миші відпустили або вікно втратило фокус.
    pub fn flash_release(&mut self, ctx: &Context) {
        let Some(hold) = self.flash_hold.clone() else { return };
        let up = ctx.input(|i| match hold {
            // кнопку контролера відпускають у midi_tick; вікно при цьому може бути не у фокусі
            FlashHold::Midi(_) => false,
            FlashHold::Pointer => !i.focused || !i.pointer.any_down(),
            FlashHold::Key(k) => {
                !i.focused
                    || i.events.iter().any(|e| {
                        matches!(e, egui::Event::Key { key, physical_key, pressed: false, .. }
                            if physical_key.unwrap_or(*key) == k)
                    })
            }
        });
        if up {
            self.flash_hold = None;
            self.send(Command::Flash(None));
        }
    }

    /// Звук для ударів потрібен, лише поки працює ефект «у такт музиці».
    /// Сервер OS2L працює, поки його ввімкнено; порт, що не відкрився, не пробуємо щокадру.
    pub fn os2l_tick(&mut self) {
        let want = self.v.os2l;
        if self.os2l.as_ref().map(|s| s.port) == want
            || (want.is_some() && self.os2l_error.as_ref().map(|e| e.0) == want)
        {
            return;
        }
        self.os2l = None;
        self.os2l_error = None;
        if let Some(port) = want {
            match crate::os2l::Server::start(self.core.clone(), port) {
                Ok(s) => self.os2l = Some(s),
                Err(e) => {
                    log::warn!("OS2L: {e}");
                    self.os2l_error = Some((port, e));
                }
            }
        }
    }

    /// Розклад має хоч одну увімкнену подію.
    pub fn schedule_active(&self) -> bool {
        self.v.schedule.iter().any(|e| e.on)
    }

    pub fn audio_tick(&mut self) {
        let e = &self.v.show.effect;
        let want = (e.kind != EffectKind::None && e.beat && self.v.audio != AudioSource::Off).then_some(self.v.audio);
        if self.audio.as_ref().map(|a| a.source) != want {
            self.audio = want.map(|s| crate::audio::Audio::start(self.core.clone(), s));
        }
    }

    /// MIDI: порти відкриваються лише з прив'язками чи в навчанні; повідомлення — у дії.
    pub fn midi_tick(&mut self, ctx: &Context) {
        if self.midi.is_none() && self.midi_error.is_empty() && (!self.v.midi.is_empty() || self.midi_learn.is_some()) {
            self.midi_connect(ctx);
        }
        let msgs: Vec<(String, MidiValue)> = self.midi.as_ref().map(|m| m.rx.try_iter().collect()).unwrap_or_default();
        for (key, val) in msgs {
            if let Some(action) = self.midi_learn.take() {
                let mut m = self.v.midi.clone();
                m.retain(|b| b.action != action);
                m.push(MidiBinding { key, action });
                self.set_midi(m);
                continue;
            }
            let Some(action) = self.v.midi.iter().find(|b| b.key == key).map(|b| b.action.clone()) else { continue };
            self.midi_action(ctx, &key, &action, val);
        }
    }

    pub fn midi_connect(&mut self, ctx: &Context) {
        self.midi = None;
        match crate::midi::open(ctx.clone()) {
            Ok(m) => {
                self.midi_error.clear();
                self.midi = Some(m);
                self.midi_leds();
            }
            Err(e) => {
                log::warn!("MIDI недоступне: {e}");
                self.midi_error = e;
            }
        }
    }

    pub fn set_midi(&mut self, m: Vec<MidiBinding>) {
        self.send(Command::SetMidi(m));
        self.midi_leds();
    }

    /// Пади APC mini mk2 світяться кольором дії.
    pub fn midi_leds(&mut self) {
        let Some(midi) = self.midi.as_mut() else { return };
        if !midi.has_apc() {
            return;
        }
        for note in 0..64u8 {
            midi.send(&[0x90, note, 0]);
        }
        for b in &self.v.midi {
            let color = if let Some(n) = b.action.strip_prefix("static").and_then(|n| n.parse::<usize>().ok()) {
                self.v.show.static_colors.get(n.wrapping_sub(1)).map(|c| c.hex)
            } else if b.action.starts_with("scene:") {
                Some(Rgb(0, 0, 255))
            } else if b.action.starts_with("group:") {
                Some(Rgb(0, 255, 0))
            } else {
                None
            };
            if let Some(msg) = color.and_then(|c| nitor_core::midi::apc_led(&b.key, c)) {
                midi.send(&msg);
            }
        }
    }

    fn midi_action(&mut self, ctx: &Context, key: &str, action: &str, val: MidiValue) {
        let level = |v: u8| v as f32 / 127.0;
        match (action, val) {
            ("master", MidiValue::Level(v)) => {
                self.send(Command::SetMaster((level(v) * 255.0).round() as u8));
            }
            ("speed", MidiValue::Level(v)) => {
                self.eff.speed = 1.0 + (level(v) * 99.0).round();
                self.live_effect_update();
            }
            (_, MidiValue::Level(_)) if nitor_core::midi::is_level(action) => {}
            // кнопка, прив'язана до регулятора: натиснуто — понад половину
            (_, MidiValue::Level(v)) => self.midi_action(ctx, key, action, MidiValue::Button(v >= 64)),
            ("flash", MidiValue::Button(down)) => {
                if down {
                    self.flash_color(FLASH_WHITE, FlashHold::Midi(key.into()));
                } else if self.flash_hold == Some(FlashHold::Midi(key.into())) {
                    self.flash_hold = None;
                    self.send(Command::Flash(None));
                }
            }
            (_, MidiValue::Button(false)) => {}
            (a, MidiValue::Button(true)) => {
                if let Some(name) = a.strip_prefix("scene:") {
                    let fade = self.scene_fade;
                    self.send(Command::RecallScene { name: name.into(), fade });
                } else if let Some(name) = a.strip_prefix("group:") {
                    self.pick_group(name, false);
                } else {
                    self.run_hotkey(ctx, a);
                }
            }
        }
    }

    /// Групи з однією зміною (додати, перейменувати, видалити).
    fn set_groups(&mut self, f: impl FnOnce(&mut indexmap::IndexMap<String, Vec<String>>)) {
        let mut g = self.v.show.groups.clone();
        f(&mut g);
        self.send(Command::SetGroups(g));
    }

    /// Клік по групі виділяє її прилади; Shift + клік додає їх до виділення.
    pub fn pick_group(&mut self, name: &str, add: bool) {
        let ids = self.v.show.groups.get(name).cloned().unwrap_or_default();
        if !add {
            self.sel.clear();
        }
        for id in ids {
            if !self.sel.contains(&id) {
                self.sel.retain(|s| s.split('#').next() != Some(&id));
                self.sel.push(id);
            }
        }
    }

    pub fn undo(&mut self, redo: bool) {
        if self.send(if redo { Command::Redo } else { Command::Undo }) {
            self.toast(if redo { "Дію повторено" } else { "Дію скасовано" }, true);
        }
    }

    pub fn select_invert(&mut self) {
        // вибраний прилад → знімається; вибрані сегменти → вибираються решта сегментів того ж приладу;
        // невибраний прилад → вибирається повністю
        let items = self.sel_items();
        let whole: BTreeSet<String> = items.iter().filter(|i| i.seg.is_none()).map(|i| i.id.clone()).collect();
        let segs: BTreeSet<String> = items.iter().filter_map(|i| i.seg.map(|k| format!("{}#{k}", i.id))).collect();
        let mut out = vec![];
        for f in &self.v.show.fixtures {
            if whole.contains(&f.id) {
                continue;
            }
            let mine: Vec<String> = (0..self.v.segs(f)).map(|k| format!("{}#{k}", f.id)).collect();
            if mine.iter().any(|id| segs.contains(id)) {
                out.extend(mine.into_iter().filter(|id| !segs.contains(id)));
            } else {
                out.push(f.id.clone());
            }
        }
        self.sel = out;
    }

    pub fn run_effect(&mut self, ids: Vec<String>) {
        let mut p = self.effect_patch();
        p.kind = Some(self.eff.kind);
        p.ids = Some(ids);
        self.send(Command::SetEffect(p));
    }

    pub fn duplicate(&mut self, id: &str) {
        let Some(f) = self.v.fixture(id).cloned() else { return };
        let before = self.v.show.fixtures.len();
        let channels = self.v.channels(&f).to_vec();
        let spec = nitor_core::engine::ProfileSpec {
            profile: f.profile.clone(),
            channels: f.model.as_ref().map(|_| channels),
            model: f.model.clone(),
        };
        let address = self.v.next_free_address() as u16;
        if self.send(Command::AddFixture { spec, name: format!("{} (копія)", f.name), address, count: 1 }) {
            if let Some(n) = self.v.show.fixtures.get(before) {
                let id = n.id.clone();
                self.select_only(&id);
            }
        }
    }

    // --------------------------------------------------------------- дії --
    pub fn act(&mut self, ctx: &Context, a: Act) {
        match a {
            Act::Cmd(c) => {
                self.send(c);
            }
            Act::StaticTo { index, ids } => {
                if let Some(c) = self.v.show.static_colors.get(index) {
                    self.last_color = c.hex;
                }
                self.send(Command::StaticColor { ids, source: StaticSource::Index(index) });
            }
            Act::Clear => self.clear_colors(),
            Act::RunEffect(ids) => self.run_effect(ids),
            Act::RunEffectSelected => {
                let picked = self.sel_fixtures();
                let ids = if picked.is_empty() {
                    self.v.show.fixtures.iter().map(|f| f.id.clone()).collect()
                } else {
                    picked
                };
                self.run_effect(ids);
                self.flash("effApply");
            }
            Act::StopEffect => {
                self.send(Command::SetEffect(EffectPatch { kind: Some(EffectKind::None), ..Default::default() }));
            }
            Act::StopEffectOn(fids) => {
                let rest: Vec<String> = self.v.show.effect.ids.iter().filter(|x| !fids.contains(x)).cloned().collect();
                let p = if rest.is_empty() {
                    EffectPatch { kind: Some(EffectKind::None), ..Default::default() }
                } else {
                    EffectPatch { ids: Some(rest), ..Default::default() }
                };
                self.send(Command::SetEffect(p));
            }
            Act::SelectWhole(fid) => self.pick(&fid, false),
            Act::SelectSameProfile(p) => {
                self.sel = self.v.show.fixtures.iter().filter(|f| f.profile == p).map(|f| f.id.clone()).collect();
            }
            Act::OpenPresets => self.open_presets(),
            Act::OpenProps(id) => self.open_props(&id),
            Act::Duplicate(id) => self.duplicate(&id),
            Act::AskDeleteFixtures { ids, close_props } => {
                let names: Vec<String> =
                    self.v.show.fixtures.iter().filter(|f| ids.contains(&f.id)).map(|f| f.name.clone()).collect();
                if names.is_empty() {
                    return;
                }
                let title = if names.len() > 1 {
                    format!("Видалити прилади ({})?", names.len())
                } else {
                    "Видалити прилад?".into()
                };
                self.ui_dlg = Some(UiDlg::confirm(
                    &title,
                    &names.join(", "),
                    "Видалити",
                    true,
                    Act::DeleteFixtures { ids, close_props },
                ));
            }
            Act::DeleteFixtures { ids, close_props } => {
                for id in &ids {
                    self.send(Command::DeleteFixture { id: id.clone() });
                }
                self.sel.retain(|s| !ids.contains(&s.split('#').next().unwrap_or("").to_string()));
                if close_props {
                    self.close_props();
                }
            }
            Act::CopyColor => {
                if let Some(c) = self.first_sel_color() {
                    self.color_clipboard = Some(c);
                    self.toast(&format!("Скопійовано {}", c.0.hex()), true);
                }
            }
            Act::PasteColor => {
                if let Some((hex, w)) = self.color_clipboard {
                    self.set_color(ColorValues { hex: Some(hex), w: Some(w), ..Default::default() });
                }
            }
            Act::SaveColor => match self.first_sel_color().filter(|(c, w)| *c != Rgb::BLACK || *w > 0) {
                None => self.toast("Спершу виберіть прилад із кольором", false),
                Some((hex, w)) => {
                    if self.send(Command::AddStaticColor { name: String::new(), hex, w }) {
                        self.toast("Колір збережено в палітру", true);
                        self.flash("colorSave");
                    }
                }
            },
            Act::AskDeleteColor(i) => {
                if let Some(c) = self.v.show.static_colors.get(i) {
                    let t = format!("Видалити колір «{}» з палітри?", c.name);
                    let cmd = Act::Cmd(Command::DeleteStaticColor { index: i });
                    self.ui_dlg = Some(UiDlg::confirm(&t, "", "Видалити", true, cmd));
                }
            }
            Act::AddPaletteColor => {
                let c = self.first_sel_color().map(|c| c.0).filter(|c| *c != Rgb::BLACK).unwrap_or(self.last_color);
                self.eff.colors.push(c);
                self.live_effect_update();
            }
            Act::ClearPalette => {
                self.eff.colors.clear();
                self.live_effect_update();
            }
            Act::RecallScene { name, fade } => {
                self.send(Command::RecallScene { name, fade });
            }
            Act::AskOverwriteScene(name) => {
                let t = format!("Перезаписати сцену «{name}» поточними кольорами?");
                self.ui_dlg = Some(UiDlg::confirm(&t, "", "Перезаписати", false, Act::SaveScene(name)));
            }
            Act::SaveScene(name) => {
                if !name.trim().is_empty() {
                    let with_effect = self.scene_with_effect;
                    self.send(Command::SaveScene { name: name.trim().into(), with_effect });
                }
            }
            Act::SaveSceneFile(name) => self.save_scene_file(&name),
            Act::AskDeleteScene(name) => {
                let t = format!("Видалити сцену «{name}»?");
                self.ui_dlg = Some(UiDlg::confirm(&t, "", "Видалити", true, Act::Cmd(Command::DeleteScene { name })));
            }
            Act::AskNewScene => self.ui_dlg = Some(UiDlg::prompt("Назва сцени", "", Act::SaveScene(String::new()))),
            Act::OpenSceneFile => self.open_scene_file(),
            Act::OpenAdd => self.open_add(),
            Act::SelectAll => self.sel = self.v.show.fixtures.iter().map(|f| f.id.clone()).collect(),
            Act::SelectNone => self.sel.clear(),
            Act::SelectInvert => self.select_invert(),
            Act::Undo => self.undo(false),
            Act::Redo => self.undo(true),
            Act::Blackout => {
                self.send(Command::Blackout(None));
            }
            Act::AskDeleteSelected => {
                let ids = self.sel_fixtures();
                self.act(ctx, Act::AskDeleteFixtures { ids, close_props: false });
            }
            Act::SwapSide => {
                self.side_anim = Some(ctx.input(|i| i.time));
                self.set_prefs(|p| p.scenes_first = !p.scenes_first)
            }
            Act::ToggleShortButton(id) => self.set_prefs(|p| {
                if p.short_buttons.iter().any(|b| b == id) {
                    p.short_buttons.retain(|b| b != id);
                } else {
                    p.short_buttons.push(id.into());
                }
            }),
            Act::LightPref(id, v) => self.set_prefs(|p| match id {
                "flash" => p.flash_rate = v,
                "clear" => p.clear_fade = v,
                _ => p.blackout_fade = v,
            }),
            Act::FlashConfig(r) => self.flash_cfg = Some(r),
            Act::EndLayoutEdit => self.layout_edit = false,
            Act::HideHelp => {
                self.set_prefs(|p| p.hide_help = true);
                self.toast("Пояснення сховано. Повернути їх можна в Налаштуваннях, на вкладці «Загальні»", true);
            }
            Act::CheckUpdate => self.updater.check(),
            Act::OpenUpdate => self.open_update_dialog(),
            Act::OpenSettings(tab) => self.open_settings(tab),
            Act::TextOp(id, op) => {
                ctx.memory_mut(|m| m.request_focus(id));
                self.pending_text_op = Some((id, op, 2));
            }
            Act::ResetKeys => self.save_keys(indexmap::IndexMap::new()),
            Act::AskNewGroup => {
                if self.sel.is_empty() {
                    self.toast("Спершу виберіть прилади для групи", false);
                } else {
                    self.ui_dlg = Some(UiDlg::prompt("Назва групи", "", Act::SaveGroup(String::new())));
                }
            }
            Act::SaveGroup(name) => {
                let ids = self.sel_fixtures();
                if !name.trim().is_empty() {
                    self.set_groups(|g| {
                        g.insert(name.trim().into(), ids);
                    });
                }
            }
            Act::AskRenameGroup(name) => {
                self.ui_dlg =
                    Some(UiDlg::prompt("Нова назва групи", &name, Act::RenameGroup(name.clone(), String::new())));
            }
            Act::RenameGroup(old, new) => {
                let new = new.trim().to_string();
                if !new.is_empty() && new != old {
                    self.set_groups(|g| {
                        if let Some(i) = g.get_index_of(&old) {
                            let ids = g.shift_remove(&old).unwrap_or_default();
                            g.shift_insert(i.min(g.len()), new, ids);
                        }
                    });
                }
            }
            Act::DeleteGroup(name) => self.set_groups(|g| {
                g.shift_remove(&name);
            }),
            Act::AskOperator => {
                let mut d = UiDlg::prompt("Увімкнути режим оператора", "", Act::EnterOperator(String::new()));
                d.text = "Лишаться сцени, кольори палітри, групи й майстер. PIN — від 4 до 8 цифр. \
                          Без PIN вийти можна, утримуючи кнопку виходу 3 секунди."
                    .into();
                d.ok = "Увімкнути".into();
                self.ui_dlg = Some(d);
            }
            Act::EnterOperator(pin) => {
                if self.send(Command::SetPin(pin)) {
                    self.sel.clear();
                    self.send(Command::Operator { on: true, pin: String::new() });
                }
            }
            Act::AskExitOperator => {
                let mut d = UiDlg::prompt("Вийти з режиму оператора", "", Act::ExitOperator(String::new()));
                d.text = "Введіть PIN.".into();
                d.ok = "Вийти".into();
                self.ui_dlg = Some(d);
            }
            Act::ExitOperator(pin) => {
                self.send(Command::Operator { on: false, pin });
            }
            Act::Quit => self.quit(ctx),
        }
    }

    pub fn open_update_dialog(&mut self) {
        let u = self.updater.info();
        if !u.error.is_empty() && self.update_tried && !u.available {
            self.toast(&u.error, false);
            return;
        }
        self.upd_dlg = true;
    }

    // ------------------------------------------------------ файли сцен --
    fn save_scene_file(&mut self, name: &str) {
        let data = {
            let s = self.core.lock();
            scene_file::export(&s.engine, name)
        };
        let Ok(data) = data else {
            self.toast("Не вдалося підготувати файл сцени", false);
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Зберегти сцену у файл")
            .set_file_name(scene_file::file_name(name))
            .add_filter("Сцена Nitor", &[scene_file::SCENE_EXT])
            .save_file()
        else {
            return;
        };
        let text = serde_json::to_string_pretty(&data).unwrap_or_default();
        match std::fs::write(&path, text) {
            Ok(()) => self.toast(&format!("Файл збережено: {}", path.display()), true),
            Err(e) => self.toast(&format!("Не вдалося зберегти файл: {e}"), false),
        }
    }

    fn open_scene_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Відкрити сцену з файлу")
            .add_filter("Сцена Nitor", &[scene_file::SCENE_EXT])
            .add_filter("Усі файли", &["*"])
            .pick_file()
        else {
            return;
        };
        let data = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        match data {
            None => self.toast("Це не файл сцени Nitor", false),
            Some(data) => {
                self.send(Command::ImportScene { data, name: None });
            }
        }
    }

    // -------------------------------------------------- гарячі клавіші --
    /// Зберігає власні гарячі клавіші (збіг зі стандартними — не зберігається).
    pub fn save_keys(&mut self, mut map: indexmap::IndexMap<String, Vec<String>>) {
        map.retain(|id, ks| {
            let def = crate::hotkeys::defaults(id);
            crate::hotkeys::action_ids().contains(id) && def != *ks
        });
        self.send(Command::SetHotkeys(map));
    }

    /// Призначає дії клавіші; ці клавіші знімаються з інших дій.
    pub fn assign_keys(&mut self, id: &str, combos: Vec<String>, standard: bool) {
        let mut map = self.v.hotkeys.clone();
        for other in crate::hotkeys::action_ids() {
            if other == id {
                continue;
            }
            let ks = crate::hotkeys::keys_for(&self.v.hotkeys, &other);
            let left: Vec<String> = ks.iter().filter(|c| !combos.contains(c)).cloned().collect();
            if left.len() != ks.len() {
                let gone: Vec<String> =
                    ks.iter().filter(|c| combos.contains(c)).map(|c| crate::hotkeys::key_label(c)).collect();
                self.toast(
                    &format!("{} більше не діє для «{}»", gone.join(", "), crate::hotkeys::label_of(&other)),
                    true,
                );
                map.insert(other, left);
            }
        }
        if standard {
            map.shift_remove(id);
        } else {
            map.insert(id.to_string(), combos);
        }
        self.save_keys(map);
    }

    pub fn run_hotkey(&mut self, ctx: &Context, id: &str) {
        if let Some(n) = id.strip_prefix("static") {
            if let Ok(n) = n.parse::<usize>() {
                if n >= 1 && n - 1 < self.v.show.static_colors.len() {
                    let ids = self.sel.clone();
                    self.act(ctx, Act::StaticTo { index: n - 1, ids });
                }
            }
            return;
        }
        if id == "tap" {
            let now = self.core.now();
            self.send(Command::Beat(now));
            return;
        }
        let a = match id {
            "blackout" => Act::Blackout,
            "deleteFixtures" => Act::AskDeleteSelected,
            "clear" => Act::Clear,
            "effectStop" => Act::StopEffect,
            "selectAll" => Act::SelectAll,
            "selectNone" => Act::SelectNone,
            "selectInvert" => Act::SelectInvert,
            "undo" => Act::Undo,
            "redo" => Act::Redo,
            "settings" => Act::OpenSettings(None),
            _ => return,
        };
        self.act(ctx, a);
    }

    /// Закриття вікна (хрестик, Alt+F4) — лише після підтвердження.
    pub fn request_close(&mut self) {
        if self.ui_dlg.as_ref().is_some_and(|d| matches!(d.on_ok, Act::Quit)) {
            return;
        }
        self.menu = None;
        let text = if self.schedule_active() {
            "Прожектори згаснуть, розклад не працюватиме, поки Nitor закритий."
        } else {
            "Прожектори згаснуть."
        };
        self.ui_dlg = Some(UiDlg::confirm("Закрити Nitor?", text, "Закрити", true, Act::Quit));
    }

    pub fn quit(&mut self, ctx: &Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

pub fn snapshot(core: &Core) -> View {
    let s = core.lock();
    let e = &s.engine;
    View {
        show: e.show.clone(),
        recent: e.recent_models.clone(),
        output_cfg: e.output_cfg.clone(),
        prefs: e.prefs.clone(),
        hotkeys: e.hotkeys.clone(),
        live: e.live.clone(),
        dmx: e.universe,
        undo: e.can_undo(),
        redo: e.can_redo(),
        menu_profiles: e.menu_profiles(),
        preset_fixtures: e.preset_fixtures(),
        overlaps: e.overlaps(),
        output: s.output.info(),
        flashing: !e.flash.is_empty(),
        operator: e.operator,
        has_pin: e.pin.is_some(),
        audio: e.audio,
        bpm: e.tempo.bpm,
        beat_at: e.tempo.anchor,
        midi: e.midi.clone(),
        schedule: e.schedule.clone(),
        place: e.place.clone(),
        os2l: e.os2l,
    }
}

/// Усі написи — з великої літери.
pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Колір тексту на кольоровій кнопці — той, що дає більший контраст (WCAG).
pub fn readable_text(c: Rgb) -> egui::Color32 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2);
    if (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) { nitor_ui::theme::INK } else { nitor_ui::theme::WHITE }
}

pub fn c32(c: Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}
