//! Вікна: налаштування (DMX-вихід, патч, профілі, гарячі клавіші, про програму), новий прилад,
//! бібліотека, режим приладу, властивості приладу з тестом каналів, оновлення, підтвердження.

use egui::{Align, Align2, Color32, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use nitor_core::engine::{ColorValues, ProfileSpec};
use nitor_core::fixtures::{Model, segment_count};
use nitor_core::library::{self, LibModel, Mode};
use nitor_core::outputs::{self, OutputConfig};
use nitor_core::{Command, DMX_CHANNELS};

use crate::app::{Act, App, cap};
use crate::hotkeys::{self, Entry};
use nitor_ui::theme::*;
use nitor_ui::widgets::{self, Kind, Shown, btn, icon_btn};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    General,
    Access,
    Output,
    Patch,
    Library,
    Profiles,
    Keys,
    Midi,
    Os2l,
    Remote,
    Schedule,
    About,
}

pub struct SettingsState {
    pub tab: Tab,
    /// чернетка виходу (поки користувач не натиснув «Застосувати»)
    pub out_kind: String,
    pub udmx_index: usize,
    pub port: String,
    pub host: String,
    pub universe: i64,
    pub touched: bool,
    pub prof_name: String,
    pub prof_channels: String,
    pub capturing: Option<String>,
    /// коли перемкнули вкладку (час egui) — для плавної появи
    pub tab_t: f64,
    /// прокрутити до розділу (MIDI і VirtualDJ — одне поле, два розділи)
    pub jump: Option<Tab>,
    /// вкладка «Бібліотека приладів»: розділ і пошук
    pub lib: LibState,
    /// поля патчу, що редагуються: id → (назва, адреса)
    pub patch_names: std::collections::HashMap<String, String>,
    /// пошук налаштувань (фільтрує вкладки й гарячі клавіші)
    pub q: String,
}

pub struct AddState {
    pub device: Option<(LibModel, Mode)>,
    pub profile: String,
    pub name: String,
    pub name_auto: bool,
    pub address: i64,
    pub count: i64,
}

pub struct LibState {
    pub kind: String,
    pub q: String,
    pub shown: usize,
    /// модель вибирають для наявного приладу (вікно властивостей), а не для нового
    pub for_props: bool,
}

impl LibState {
    /// Бібліотека завжди відкривається на «Усі моделі».
    pub fn new(for_props: bool) -> LibState {
        LibState { kind: String::new(), q: String::new(), shown: 60, for_props }
    }
}

/// Висота вмісту вкладок налаштувань: однакова для всіх, щоб вікно не стрибало (довгі — прокручуються).
const SETTINGS_BODY_H: f32 = 520.0;
/// Екран вкладки: VirtualDJ показується разом з MIDI, нижче.
fn page(t: Tab) -> Tab {
    if t == Tab::Os2l { Tab::Midi } else { t }
}

/// Назва розділу з «?», що підказує про розділ.
fn title_help(ui: &mut Ui, tab: Tab) {
    let (_, title, help, _) = TABS.iter().find(|t| t.0 == tab).copied().unwrap_or(TABS[0]);
    ui.spacing_mut().item_spacing.x = S2;
    ui.label(RichText::new(title).font(bold(FS_LG)));
    widgets::help(ui, help);
}

/// Чи підходить вкладка під пошук: назва, пояснення, ключові слова (для клавіш — ще й назви дій).
fn tab_matches(tab: Tab, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    let Some((_, title, help, words)) = TABS.iter().find(|t| t.0 == tab) else { return false };
    q.is_empty()
        || [*title, *help, *words].iter().any(|t| t.to_lowercase().contains(&q))
        || tab == Tab::Keys
            && hotkeys::table()
                .iter()
                .any(|e| matches!(e, Entry::Action { label, .. } if label.to_lowercase().contains(&q)))
}

/// Значок вкладки в списку налаштувань.
fn tab_icon(tab: Tab) -> &'static str {
    match tab {
        Tab::General => "settings",
        Tab::Access => "eye",
        Tab::Output => "plug",
        Tab::Patch => "list",
        Tab::Library => "book",
        Tab::Profiles => "sliders",
        Tab::Keys => "keyboard",
        Tab::Midi => "music",
        Tab::Os2l => "disc",
        Tab::Remote => "phone",
        Tab::Schedule => "clock",
        Tab::About => "info",
    }
}

/// Ширина списку вкладок налаштувань.
const SIDE_W: f32 = 240.0;

/// Вкладки налаштувань: назва, пояснення для новачка (знак питання біля назви) і слова для пошуку.
const TABS: [(Tab, &str, &str, &str); 12] = [
    (
        Tab::General,
        "Загальні",
        "Що показувати у вікні: гарячі клавіші на кнопках, знаки «?», DMX-монітор; і чи запускати Nitor разом із Windows. Сховані підказки повертаються тут.",
        "підказки клавіші кнопки пояснення автозапуск windows запуск dmx-монітор монітор",
    ),
    (
        Tab::Output,
        "DMX-вихід",
        "Виберіть адаптер і натисніть «Застосувати». Немає в списку — підключіть його й натисніть «Знайти». Art-Net і sACN працюють через мережу, «Симулятор» — без обладнання.",
        "інтерфейс пристрій usb udmx enttec open dmx порт com art-net sacn ip адреса всесвіт симулятор",
    ),
    (
        Tab::Patch,
        "Патч приладів",
        "Усі прилади з адресами DMX. Адреса й режим мають збігатися з тими, що налаштовані на самому прожекторі. Прилади з адресами, що перетинаються, підсвічено.",
        "прилади адреса канали перетин порядок назва профіль",
    ),
    (
        Tab::Library,
        "Бібліотека приладів",
        "Понад тисяча моделей, працює без інтернету. Знайдіть свою й виберіть режим DMX, як на прожекторі (зазвичай це кількість каналів).",
        "моделі виробник режим каналів ofl qlc пошук",
    ),
    (
        Tab::Profiles,
        "Профілі",
        "Потрібні, лише якщо вашої моделі немає в бібліотеці. Перелічіть канали за інструкцією прожектора: r g b w a uv dim strobe; zero і full — канали, що стоять на 0 чи на максимумі.",
        "канали власний профіль red green blue dimmer",
    ),
    (
        Tab::Keys,
        "Гарячі клавіші",
        "Натисніть «Змінити», потім клавішу чи поєднання з Ctrl, Shift або Alt. Esc скасовує.",
        "клавіатура поєднання ctrl shift alt blackout спалах тап",
    ),
    (
        Tab::Midi,
        "MIDI",
        "Натисніть «Навчити» біля дії, потім кнопку чи повзунок на контролері. Для Akai APC mini mk2 є готовий профіль.",
        "контролер пульт кнопки повзунки навчити прив'язка",
    ),
    (
        Tab::Os2l,
        "VirtualDJ",
        "Позначте «Приймати з VirtualDJ» — він сам знайде Nitor. Удари задають темп ефектів «у такт музиці», кнопка з назвою сцени вмикає цю сцену.",
        "os2l dj музика такт темп bpm",
    ),
    (
        Tab::Remote,
        "Пульт з телефона",
        "Керування з телефона в тій самій мережі Wi-Fi. Відкрийте показану адресу або QR-код; вхід — за PIN режиму оператора. Якщо Windows спитає дозвіл для мережі, дозвольте для приватних.",
        "телефон планшет wi-fi мережа браузер qr pin віддалено",
    ),
    (
        Tab::Schedule,
        "Розклад",
        "Сцена, «Очистити» чи Blackout у певний час або на сході й заході сонця, у вибрані дні. Працює, поки Nitor відкритий, навіть згорнутий.",
        "час таймер схід захід сонця автоматично",
    ),
    (
        Tab::Access,
        "Доступність",
        "Розмір тексту, кнопок і повзунків, від 75 до 200 %. Діє одразу.",
        "масштаб розмір шрифт крупніше дрібніше збільшити зменшити екран дисплей зір",
    ),
    (
        Tab::About,
        "Про програму",
        "Версія й оновлення (перевіряються під час запуску і щогодини). Якщо щось не працює, надішліть журнал nitor.log з теки %LOCALAPPDATA%\\Nitor.",
        "версія оновлення журнал",
    ),
];

pub enum ModesFor {
    Add,
    Props,
}

pub struct ModesState {
    pub fx: LibModel,
    pub current: Option<String>,
    pub target: ModesFor,
}

pub struct PropsState {
    pub id: String,
    pub name: String,
    pub address: i64,
    pub dimmer: f32,
    pub profile: String,
    /// профіль моделі приладу: (назва, канали, модель, змінено)
    pub device: Option<(String, Vec<String>, Option<Model>, bool)>,
    pub test_open: bool,
    pub test_count: i64,
    pub test_rows: Vec<(String, f32)>,
    pub test_prof_name: String,
}

pub struct UiDlg {
    pub title: String,
    pub text: String,
    pub input: Option<String>,
    pub ok: String,
    pub cancel: Option<String>,
    pub danger: bool,
    pub on_ok: Act,
    pub focused: bool,
}

impl UiDlg {
    pub fn confirm(title: &str, text: &str, ok: &str, danger: bool, on_ok: Act) -> UiDlg {
        UiDlg {
            title: title.into(),
            text: text.into(),
            input: None,
            ok: ok.into(),
            cancel: Some("Скасувати".into()),
            danger,
            on_ok,
            focused: false,
        }
    }

    pub fn prompt(title: &str, value: &str, on_ok: Act) -> UiDlg {
        UiDlg { input: Some(value.into()), ..UiDlg::confirm(title, "", "Зберегти", false, on_ok) }
    }
}

const TEST_ROLES: [(&str, &str); 13] = [
    ("r", "Червоний"),
    ("g", "Зелений"),
    ("b", "Синій"),
    ("w", "Білий"),
    ("a", "Бурштин"),
    ("uv", "УФ"),
    ("dim", "Диммер"),
    ("strobe", "Строб"),
    ("macro", "Програма"),
    ("speed", "Швидкість програми"),
    ("zero", "Не використовується"),
    ("full", "Завжди 255"),
    ("fixed", "Фіксоване значення"),
];

fn dialog_frame(width: f32) -> egui::Frame {
    let _ = width;
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(radius(R_LG))
        .inner_margin(egui::Margin::same(S5 as i8))
}

/// Заголовок вікна з кнопкою «Закрити». `true` — натиснуто «Закрити».
fn dialog_head(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(bold(FS_LG)));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            close = icon_btn("x").kind(Kind::Ghost).tip("Закрити").show(ui).clicked();
        });
    });
    ui.add_space(S4 - S2);
    close
}

fn modal(ctx: &egui::Context, id: &str, width: f32, add: impl FnOnce(&mut Ui)) -> bool {
    modal_with(ctx, id, width, dialog_frame(width), add)
}

fn modal_with(ctx: &egui::Context, id: &str, width: f32, frame: egui::Frame, add: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    let w = width.min(screen.width() * 0.96);
    let pad = frame.inner_margin.sum();
    let r = egui::Modal::new(Id::new(id)).frame(frame).backdrop_color(BACKDROP).show(ctx, |ui| {
        ui.set_width(w - pad.x);
        ui.set_max_height(screen.height() * 0.9 - pad.y);
        add(ui);
    });
    // вікно з'являється з легким збільшенням від центру (затемнення — той самий шар, тому ледь-ледь)
    nitor_ui::anim::pop_in(ctx, r.response.layer_id, r.response.rect.center(), 0.98);
    r.should_close()
}

/// Фокус у поле в перший кадр після відкриття вікна: одразу можна друкувати й ходити Tab.
/// Текст у полі виділено: друк замінює його, а не дописує.
fn focus_on_open(ui: &Ui, r: &egui::Response, key: &str, len: usize) {
    if nitor_ui::anim::shown_for(ui.ctx(), Id::new(("open-focus", key))) == 0.0 {
        r.request_focus();
        let mut st = egui::TextEdit::load_state(ui.ctx(), r.id).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(len),
        )));
        st.store(ui.ctx(), r.id);
    }
}

/// «Режим: …» без повтору кількості каналів, якщо вона вже є в назві режиму.
fn mode_text(mode: &str, n: usize) -> String {
    if mode.contains("кан.") { format!("Режим: {mode}") } else { format!("Режим: {mode}, {n} кан.") }
}

pub fn model_label(m: &Model) -> String {
    let t = library::title(&m.m, &m.n);
    if m.mode.is_empty() { t } else { format!("{t}, {}", m.mode) }
}

impl App {
    // --------------------------------------------------------------- відкриття --
    pub fn open_settings(&mut self, tab: Option<Tab>) {
        let o = self.v.output_cfg.clone();
        let (index, port, host, universe) = match &o {
            OutputConfig::Udmx { index } => (*index, String::new(), String::new(), 0),
            OutputConfig::EnttecPro { port } | OutputConfig::OpenDmx { port } => (0, port.clone(), String::new(), 0),
            OutputConfig::Artnet { host, universe, .. } => (0, String::new(), host.clone(), *universe as i64),
            OutputConfig::Sacn { host, universe } => (0, String::new(), host.clone(), *universe as i64),
            OutputConfig::None => (0, String::new(), String::new(), 0),
        };
        self.settings = Some(SettingsState {
            // кнопка налаштувань завжди відкриває «Загальні»; інші вкладки — лише за прямим запитом
            tab: tab.unwrap_or(Tab::General),
            out_kind: o.kind().into(),
            udmx_index: index,
            port,
            host,
            universe,
            touched: false,
            prof_name: String::new(),
            prof_channels: String::new(),
            capturing: None,
            tab_t: f64::NEG_INFINITY,
            jump: tab.filter(|t| *t == Tab::Os2l),
            lib: LibState::new(false),
            patch_names: Default::default(),
            q: String::new(),
        });
        if o.kind() == "udmx" {
            self.find_udmx(false);
        }
    }

    fn find_udmx(&mut self, notify: bool) {
        self.udmx_devices = outputs::find_udmx_devices();
        if notify {
            let n = self.udmx_devices.len();
            self.toast(&if n > 0 { format!("Знайдено uDMX: {n}") } else { "uDMX не знайдено".into() }, true);
        }
    }

    pub fn open_add(&mut self) {
        let profile = self.v.menu_profiles.first().cloned().unwrap_or_default();
        self.add = Some(AddState {
            device: None,
            profile,
            name: String::new(),
            name_auto: false,
            address: self.v.next_free_address(),
            count: 1,
        });
    }

    pub fn open_props(&mut self, id: &str) {
        let Some(f) = self.v.fixture(id).cloned() else { return };
        let device = if self.v.menu_profiles.contains(&f.profile) {
            None
        } else {
            Some((f.profile.clone(), self.v.channels(&f).to_vec(), f.model.clone(), false))
        };
        self.props = Some(PropsState {
            id: id.into(),
            name: f.name.clone(),
            address: f.address as i64,
            dimmer: f.dimmer as f32,
            profile: f.profile.clone(),
            device,
            test_open: false,
            test_count: 0,
            test_rows: vec![],
            test_prof_name: String::new(),
        });
    }

    /// Закриття вікна властивостей повертає канали тесту програмі.
    pub fn close_props(&mut self) {
        self.release_test();
        self.props = None;
    }

    fn release_test(&mut self) {
        if !self.tested.is_empty() {
            let addrs: Vec<u16> = self.tested.iter().copied().collect();
            self.send(Command::RawClear(Some(addrs)));
        }
        self.tested.clear();
    }

    // ----------------------------------------------------------------- показ --
    pub fn dialogs(&mut self, ctx: &egui::Context) {
        if self.settings.is_some() {
            self.settings_dialog(ctx);
        }
        if self.add.is_some() {
            self.add_dialog(ctx);
        }
        if self.props.is_some() {
            self.props_dialog(ctx);
        }
        if self.lib.is_some() {
            self.library_dialog(ctx);
        }
        if self.modes.is_some() {
            self.modes_dialog(ctx);
        }
        if self.upd_dlg {
            self.update_dialog(ctx);
        }
        if self.ui_dlg.is_some() {
            self.ui_dialog(ctx);
        }
    }

    pub fn any_dialog(&self) -> bool {
        self.settings.is_some()
            || self.add.is_some()
            || self.props.is_some()
            || self.lib.is_some()
            || self.modes.is_some()
            || self.upd_dlg
            || self.ui_dlg.is_some()
    }

    // ---------------------------------------------------- підтвердження / ввід --
    fn ui_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ui_dlg.take() else { return };
        let mut result: Option<bool> = None;
        let close = modal(ctx, "ui-dlg", 440.0, |ui| {
            ui.label(RichText::new(&d.title).font(bold(FS_LG)));
            ui.add_space(S2);
            if !d.text.is_empty() {
                ui.label(RichText::new(&d.text).size(FS_MD));
                ui.add_space(S4 - S2);
            }
            if let Some(text) = d.input.as_mut() {
                let w = ui.available_width();
                let secret = matches!(d.on_ok, Act::EnterOperator(_) | Act::ExitOperator(_));
                let r = if secret {
                    // PIN — окремі клітинки на 8 цифр, перші 4 обов'язкові (якщо PIN задають)
                    widgets::pin_cells(ui, text, 8, 4)
                } else {
                    widgets::input_secret(ui, text, "", w, false)
                };
                if !d.focused {
                    r.request_focus();
                    if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(text.chars().count()),
                        );
                        st.cursor.set_char_range(Some(all));
                        st.store(ui.ctx(), r.id);
                    }
                    d.focused = true;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    result = Some(true);
                }
                ui.add_space(S4 - S2);
            } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                result = Some(true);
            }
            let w = ui.available_width();
            ui.allocate_ui_with_layout(vec2(w, CONTROL_H), Layout::right_to_left(Align::Center), |ui| {
                let kind = if d.danger { Kind::DangerOn } else { Kind::Primary };
                if btn(d.ok.clone()).kind(kind).key(Some("Enter".into())).show(ui).clicked() {
                    result = Some(true);
                }
                if let Some(c) = &d.cancel {
                    if btn(c.clone()).key(Some("Esc".into())).show(ui).clicked() {
                        result = Some(false);
                    }
                }
            });
        });
        match result {
            Some(true) => {
                let act = match (d.on_ok.clone(), d.input.clone()) {
                    (Act::SaveScene(_), Some(name)) => Act::SaveScene(name),
                    (Act::SaveGroup(_), Some(name)) => Act::SaveGroup(name),
                    (Act::RenameGroup(old, _), Some(name)) => Act::RenameGroup(old, name),
                    (Act::EnterOperator(_), Some(pin)) => Act::EnterOperator(pin),
                    (Act::ExitOperator(_), Some(pin)) => Act::ExitOperator(pin),
                    (a, _) => a,
                };
                self.act(ctx, act);
            }
            Some(false) => {}
            None if close => {}
            None => self.ui_dlg = Some(d),
        }
    }

    // ------------------------------------------------------------ оновлення --
    fn update_dialog(&mut self, ctx: &egui::Context) {
        let u = self.updater.info();
        let busy = matches!(u.status, crate::update::Status::Installing | crate::update::Status::Restarting);
        if busy {
            self.upd_dlg = false;
            return;
        }
        let mut install = false;
        let mut later = false;
        let close = modal(ctx, "upd-dlg", 560.0, |ui| {
            later |= dialog_head(ui, &format!("Nitor {}", u.latest));
            ui.label(widgets::muted(format!("Встановлено: {}  →  нова версія: {}", u.current, u.latest)));
            notes_box(ui, if u.notes.is_empty() { "Опис змін відсутній." } else { &u.notes });
            if let Some(b) = &u.blocked {
                widgets::alert(ui, widgets::Tone::Warn, "Автооновлення недоступне", &cap(b), None);
                ui.add_space(S2);
            }
            ui.horizontal(|ui| {
                install = btn("Встановити оновлення")
                    .primary()
                    .enabled(u.blocked.is_none() && u.available)
                    .show(ui)
                    .clicked();
                later |= btn("Пізніше").key(Some("Esc".into())).show(ui).clicked();
            });
        });
        if install {
            self.update_tried = true;
            self.updater.apply();
            self.upd_dlg = false;
        } else if later || close {
            self.upd_dlg = false;
        }
    }

    // ---------------------------------------------------------- налаштування --
    fn settings_dialog(&mut self, ctx: &egui::Context) {
        let mut st = self.settings.take().unwrap();
        let mut close = false;
        let capturing = st.capturing.is_some();
        let now = ctx.input(|i| i.time);
        let body_h = SETTINGS_BODY_H.min(ctx.screen_rect().height() * 0.9 - 160.0).max(200.0);
        // без внутрішніх полів: підкладка вкладок і картка налаштувань доходять до країв вікна
        let frame = dialog_frame(1080.0).fill(BG).inner_margin(0);
        let mut lib_pick = None;
        let esc_close = modal_with(ctx, "settings", 1080.0, frame, |ui| {
            // ліворуч на темній підкладці — назва вікна й вкладки; праворуч на всю висоту — картка налаштувань
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let side_h = ui
                    .vertical(|ui| {
                        ui.set_width(SIDE_W);
                        ui.add_space(S5);
                        ui.horizontal(|ui| {
                            ui.add_space(S2 + S3); // на одній лінії з назвами вкладок
                            ui.label(RichText::new("Налаштування").font(bold(FS_LG)));
                        });
                        ui.add_space(S3);
                        let mut go = None;
                        ui.horizontal(|ui| {
                            ui.add_space(S2);
                            // поле з лупою ліворуч
                            let pad = egui::Margin { left: (S2 + 16.0 + S2) as i8, right: S2 as i8, top: 8, bottom: 8 };
                            let r = ui.add_sized(
                                vec2(SIDE_W - 2.0 * S2, CONTROL_H),
                                egui::TextEdit::singleline(&mut st.q)
                                    .hint_text(RichText::new("Пошук").color(MUTED))
                                    .margin(pad)
                                    .vertical_align(Align::Center),
                            );
                            let ir = Rect::from_center_size(
                                pos2(r.rect.left() + S2 + 8.0, r.rect.center().y),
                                vec2(16.0, 16.0),
                            );
                            nitor_ui::icons::image("search", 16.0, MUTED).paint_at(ui, ir);
                            focus_on_open(ui, &r, "settings", st.q.chars().count());
                            // Enter — перейти до першої знайденої вкладки
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                go = TABS.iter().map(|t| t.0).find(|t| tab_matches(*t, &st.q));
                            }
                        });
                        ui.add_space(S3);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let mut found = false;
                        // підсвітка вибраної вкладки переїжджає до нової
                        let pill = ui.painter().add(egui::Shape::Noop);
                        let pill_id = Id::new("settings-tab-pill");
                        let mut pill_shown = false;
                        for (tab, label, _, _) in TABS {
                            if !tab_matches(tab, &st.q) {
                                continue;
                            }
                            found = true;
                            let on = st.tab == tab;
                            let (r, resp) = ui.allocate_exact_size(vec2(SIDE_W, CONTROL_H), Sense::click());
                            let r = r.shrink2(vec2(S2, 0.0));
                            resp.widget_info(|| {
                                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, label)
                            });
                            widgets::focus_ring(ui, &resp, r, R_MD);
                            if on {
                                let r = nitor_ui::anim::glide_rect(ui.ctx(), pill_id, r, 0.2);
                                ui.painter().set(pill, egui::Shape::rect_filled(r, radius(R_MD), ACCENT_SOFT));
                                pill_shown = true;
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, radius(R_MD), RAISED);
                            }
                            // VirtualDJ — підрозділ MIDI: з відступом
                            let indent = if tab == Tab::Os2l { S4 } else { 0.0 };
                            let fg = if on { ACCENT } else { MUTED };
                            let ir = egui::Rect::from_center_size(
                                pos2(r.left() + S3 + indent + 8.0, r.center().y),
                                vec2(16.0, 16.0),
                            );
                            nitor_ui::icons::image(tab_icon(tab), 16.0, fg).paint_at(ui, ir);
                            ui.painter().text(
                                pos2(r.left() + S3 + indent + 16.0 + S2, r.center().y),
                                Align2::LEFT_CENTER,
                                label,
                                font(FS_MD),
                                fg,
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                go = Some(tab);
                            }
                        }
                        if !pill_shown {
                            nitor_ui::anim::forget(ui.ctx(), pill_id);
                        }
                        if !found {
                            ui.horizontal(|ui| {
                                ui.add_space(S2 + S3);
                                ui.label(widgets::muted("Нічого не знайдено"));
                            });
                        }
                        if let Some(tab) = go.filter(|t| *t != st.tab) {
                            if page(tab) == page(st.tab) {
                                st.jump = Some(tab); // той самий екран — лише прокрутка
                            } else {
                                st.jump = (tab == Tab::Os2l).then_some(tab);
                                st.tab_t = now;
                            }
                            st.tab = tab;
                            st.capturing = None;
                        }
                    })
                    .response
                    .rect
                    .height();
                // права частина — крок тла (BG → SURFACE), без власної рамки: не картка в картці
                egui::Frame::new()
                    .fill(SURFACE)
                    .corner_radius(egui::CornerRadius { nw: 0, ne: R_LG, sw: 0, se: R_LG })
                    .inner_margin(egui::Margin::same(S5 as i8))
                    .show(ui, |ui| {
                        // нульовий відступ потрібен лише між бічною панеллю й карткою
                        ui.spacing_mut().item_spacing.x = ui.ctx().style().spacing.item_spacing.x;
                        ui.set_min_width(ui.available_width());
                        // картка не нижча за список вкладок (великий масштаб, низький екран)
                        ui.set_min_height(side_h - 2.0 * S5);
                        ui.vertical(|ui| {
                            let ctx = ui.ctx().clone();
                            // назва вкладки й підказка про розділ
                            let pg = page(st.tab);
                            ui.horizontal(|ui| {
                                title_help(ui, pg);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    close |= icon_btn("x").kind(Kind::Ghost).tip("Закрити").show(ui).clicked();
                                });
                            });
                            ui.add_space(S3);
                            // новий вміст проявляється за 0,18 с, трохи з'їжджаючи вгору
                            let t = nitor_ui::anim::progress(&ctx, st.tab_t, 0.18);
                            let ease = 1.0 - (1.0 - t).powi(3);
                            if t < 1.0 {
                                ctx.request_repaint();
                            }
                            if pg == Tab::Library {
                                // у бібліотеки своя прокрутка карток — без зовнішньої
                                let w = ui.available_width();
                                ui.allocate_ui(vec2(w, body_h), |ui| {
                                    ui.set_height(body_h);
                                    lib_pick = self.library_body(ui, &mut st.lib, 260.0);
                                });
                            } else {
                                let mut area = egui::ScrollArea::vertical()
                                    .id_salt(("settings-scroll", pg as u8))
                                    .auto_shrink([false, false])
                                    .max_height(body_h);
                                let jumped = st.jump.take();
                                if jumped == Some(Tab::Midi) {
                                    area = area.vertical_scroll_offset(0.0);
                                }
                                let mut vdj_top = None;
                                let out = area.show(ui, |ui| {
                                    ui.multiply_opacity(ease);
                                    ui.add_space((1.0 - ease) * S2);
                                    match st.tab {
                                        Tab::Output => self.tab_output(ui, &mut st),
                                        Tab::Patch => self.tab_patch(ui, &mut st),
                                        Tab::Library => {}
                                        Tab::Profiles => self.tab_profiles(ui, &mut st),
                                        Tab::Keys => self.tab_keys(ui, &mut st),
                                        Tab::Midi | Tab::Os2l => {
                                            self.tab_midi(ui);
                                            ui.add_space(S5);
                                            let h = ui.horizontal(|ui| title_help(ui, Tab::Os2l)).response;
                                            if jumped == Some(Tab::Os2l) {
                                                h.scroll_to_me_animation(
                                                    Some(Align::TOP),
                                                    egui::style::ScrollAnimation::none(),
                                                );
                                            }
                                            vdj_top = Some(h.rect.top());
                                            ui.add_space(S3);
                                            self.os2l_block(ui);
                                        }
                                        Tab::Schedule => self.tab_schedule(ui),
                                        Tab::Remote => self.tab_remote(ui),
                                        Tab::General => self.tab_general(ui),
                                        Tab::Access => self.tab_access(ui),
                                        Tab::About => self.tab_about(ui),
                                    }
                                });
                                widgets::scroll_shadow(ui, &out, SURFACE);
                                // прокрутка сама перемикає MIDI ↔ VirtualDJ у списку вкладок
                                if let (Some(top), None) = (vdj_top, jumped) {
                                    let vp = out.inner_rect;
                                    let bottom = out.state.offset.y + vp.height() >= out.content_size.y - 2.0;
                                    let vdj = top <= vp.top() + vp.height() * 0.35 || bottom;
                                    st.tab = if vdj { Tab::Os2l } else { Tab::Midi };
                                }
                            }
                        });
                    });
            });
        });
        // модель з бібліотеки — нове вікно приладу поверх налаштувань і вибір режиму DMX
        if let Some(fx) = lib_pick {
            self.open_add();
            self.modes = Some(ModesState { fx, current: None, target: ModesFor::Add });
        }
        // Esc під час вибору клавіші скасовує лише вибір, а не закриває вікно
        let esc_close = esc_close && !capturing && self.ui_dlg.is_none() && self.add.is_none();
        if !(close || esc_close) && self.settings.is_none() {
            self.settings = Some(st);
        }
    }

    fn tab_general(&mut self, ui: &mut Ui) {
        let mut keys = !self.v.prefs.hide_keys;
        if widgets::switch(ui, &mut keys, "Показувати підказки клавіш на кнопках").changed()
        {
            self.set_prefs(|p| p.hide_keys = !keys);
        }
        ui.label(widgets::muted(
            "Біля назви кнопки видно клавішу, якою її можна натиснути, наприклад «B» для Blackout.",
        ));
        ui.add_space(S3);
        let mut help = !self.v.prefs.hide_help;
        if widgets::switch(ui, &mut help, "Показувати підказки").changed() {
            self.set_prefs(|p| p.hide_help = !help);
        }
        ui.label(widgets::muted("Якщо навести мишу на знак питання, з'явиться коротке пояснення."));
        ui.add_space(S3);
        let mut mon = !self.v.prefs.hide_monitor;
        if widgets::switch(ui, &mut mon, "Показувати DMX-монітор унизу вікна").changed() {
            self.set_prefs(|p| p.hide_monitor = !mon);
        }
        ui.label(widgets::muted("Монітор лише показує, що йде на прожектори; без нього світло працює так само."));
        ui.add_space(S3);
        let mut auto = *self.autostart.get_or_insert_with(crate::winshell::autostart);
        if widgets::switch(ui, &mut auto, "Запускати Nitor разом із Windows").changed() {
            match crate::winshell::set_autostart(auto) {
                Ok(()) => self.autostart = Some(auto),
                Err(e) => self.toast(&format!("Не вдалося змінити автозапуск: {e}"), false),
            }
        }
    }

    fn tab_access(&mut self, ui: &mut Ui) {
        widgets::field_label(ui, "Масштаб інтерфейсу");
        // повзунок 0–100: посередині 100 %, ліворуч до 50 %, праворуч до 200 %
        let to_pos = |z: f32| if z < 1.0 { (z - 0.5) * 100.0 } else { 50.0 + (z - 1.0) * 50.0 };
        let to_zoom = |p: f32| {
            let z = if p < 50.0 { 0.5 + p / 100.0 } else { 1.0 + (p - 50.0) / 50.0 };
            (z * 20.0).round() / 20.0
        };
        let zoom = if self.v.prefs.zoom == 0.0 { 1.0 } else { self.v.prefs.zoom };
        // під час перетягування масштаб не змінюється, інакше повзунок тікав би з-під миші
        let drag_id = Id::new("zoom-drag");
        let mut pos = ui.data(|d| d.get_temp::<f32>(drag_id)).unwrap_or_else(|| to_pos(zoom).clamp(0.0, 100.0));
        let w = 320.0;
        let mut apply = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S2;
            ui.allocate_ui_with_layout(vec2(w, CONTROL_H), Layout::left_to_right(Align::Center), |ui| {
                let r = widgets::slider_ticked(ui, "Масштаб інтерфейсу", &mut pos, 0.0, 100.0, ACCENT, w);
                if r.dragged() {
                    ui.data_mut(|d| d.insert_temp(drag_id, pos));
                } else {
                    ui.data_mut(|d| d.remove::<f32>(drag_id));
                    if r.changed() || r.drag_stopped() {
                        apply = Some(to_zoom(pos));
                    }
                }
            });
            let shown = if ui.data(|d| d.get_temp::<f32>(drag_id)).is_some() { to_zoom(pos) } else { zoom };
            ui.label(RichText::new(format!("{:.0}\u{a0}%", shown * 100.0)).size(FS_MD));
            let std = (zoom - 1.0).abs() < 0.001;
            if btn("Скинути").small().icon("refresh").enabled(!std).tip("Стандартний масштаб 100 %").show(ui).clicked()
            {
                apply = Some(0.0);
            }
        });
        if let Some(z) = apply {
            self.set_prefs(|p| p.zoom = if (z - 1.0).abs() < 0.001 { 0.0 } else { z });
        }
        ui.add_space(S2);
        ui.label(widgets::muted(
            "Посередині — стандартний масштаб 100 %. Праворуч усе більше, ліворуч — менше. Зміна діє, щойно відпустите повзунок.",
        ));
        ui.add_space(S5);
        widgets::field_label(ui, "Рух в інтерфейсі");
        let mut on = self.reduce_motion();
        if widgets::switch(ui, &mut on, "Зменшити рух").changed() {
            self.set_prefs(|p| p.reduce_motion = Some(on));
        }
        ui.label(widgets::muted(
            "Кнопки, вкладки й меню не ковзають і не збільшуються; кольори й підсвітка змінюються одразу, без руху.",
        ));
    }

    fn tab_output(&mut self, ui: &mut Ui, st: &mut SettingsState) {
        let w = ui.available_width();
        widgets::field_label(ui, "Інтерфейс");
        let opts: Vec<(String, String)> =
            outputs::OUTPUT_KINDS.iter().map(|k| (k.to_string(), outputs::kind_label(k).to_string())).collect();
        if widgets::select(ui, "outType", &mut st.out_kind, &opts, w) {
            st.touched = true;
            if st.out_kind == "udmx" {
                self.find_udmx(false);
            }
        }
        ui.add_space(S3 - S2);
        match st.out_kind.as_str() {
            "udmx" => {
                widgets::field_label(ui, "Пристрій");
                ui.horizontal(|ui| {
                    let mut opts: Vec<(usize, String)> = self
                        .udmx_devices
                        .iter()
                        .enumerate()
                        .map(|(i, d)| {
                            let serial = if d.serial.is_empty() { String::new() } else { format!(" #{}", d.serial) };
                            (i, format!("{}{serial} — USB {}:{}", d.product, d.bus, d.address))
                        })
                        .collect();
                    if opts.is_empty() {
                        opts.push((0, "uDMX не знайдено".into()));
                    }
                    if widgets::select(ui, "udmxIndex", &mut st.udmx_index, &opts, w - 100.0) {
                        st.touched = true;
                    }
                    if btn("Знайти").min_width(90.0).show(ui).clicked() {
                        self.find_udmx(true);
                    }
                });
                self.winusb_block(ui);
            }
            "enttec_pro" | "open_dmx" => {
                widgets::field_label(ui, "Послідовний порт");
                ui.horizontal(|ui| {
                    let r = widgets::input(ui, &mut st.port, "/dev/ttyUSB0 або COM3", w - 200.0);
                    if r.changed() {
                        st.touched = true;
                    }
                    let ports = self.ports.clone();
                    egui::ComboBox::from_id_salt("portList").selected_text("Порти").width(90.0).show_ui(ui, |ui| {
                        for p in &ports {
                            if ui
                                .selectable_label(st.port == p.device, format!("{} — {}", p.device, p.description))
                                .clicked()
                            {
                                st.port = p.device.clone();
                                st.touched = true;
                            }
                        }
                    });
                    if btn("Оновити").show(ui).clicked() {
                        self.ports = outputs::list_serial_ports();
                        let n = self.ports.len();
                        self.toast(
                            &if n > 0 {
                                format!("Знайдено портів: {n}")
                            } else {
                                "Послідовні порти не знайдено".into()
                            },
                            true,
                        );
                    }
                });
            }
            "artnet" | "sacn" => {
                widgets::field_label(ui, "IP-адреса вузла");
                if widgets::input(ui, &mut st.host, "Порожньо — broadcast / multicast", w).changed() {
                    st.touched = true;
                }
                ui.add_space(S3 - S2);
                widgets::field_label(ui, "Всесвіт");
                // Art-Net адресує 0..32767 всесвітів, sACN — 1..63999
                let (min, max) = if st.out_kind == "sacn" { (1, 63999) } else { (0, 32767) };
                if widgets::number(ui, &mut st.universe, min, max, w).changed() {
                    st.touched = true;
                }
            }
            _ => {}
        }
        ui.add_space(S3);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S3;
            let flash = self.flashing("outApply");
            let b = if flash {
                btn("Застосовано").icon("check").kind(Kind::Done)
            } else {
                btn("Застосувати").primary()
            };
            if b.show(ui).clicked() {
                let cfg = match st.out_kind.as_str() {
                    "udmx" => OutputConfig::Udmx { index: st.udmx_index },
                    "enttec_pro" => OutputConfig::EnttecPro { port: st.port.trim().into() },
                    "open_dmx" => OutputConfig::OpenDmx { port: st.port.trim().into() },
                    "artnet" => OutputConfig::Artnet {
                        host: st.host.trim().into(),
                        universe: st.universe.max(0) as u32,
                        port: 6454,
                    },
                    "sacn" => OutputConfig::Sacn { host: st.host.trim().into(), universe: st.universe.max(0) as u32 },
                    _ => OutputConfig::None,
                };
                st.touched = false;
                if self.send(Command::SetOutput(cfg)) {
                    self.flash("outApply");
                }
            }
            if self.v.output.ok {
                ui.label(widgets::muted(cap(&self.v.output.status)));
            }
        });
        // назва помилки виходу — лише тут; на головному екрані кнопка виходу просто блимає червоним
        if !self.v.output.ok {
            ui.add_space(S3);
            let text = format!(
                "{}. Перевірте кабель і вибраний пристрій, потім натисніть «Застосувати».",
                cap(&self.v.output.status).trim_end_matches('.')
            );
            widgets::alert(ui, widgets::Tone::Danger, "DMX-вихід не працює", &text, None);
        }
        ui.add_space(S4);
        let mut dev = self.v.prefs.output_device;
        if widgets::switch(ui, &mut dev, "Показувати назву пристрою біля назви програми").changed()
        {
            self.set_prefs(|p| p.output_device = dev);
        }
        ui.label(widgets::muted("Без цього вгорі видно лише тип інтерфейсу, наприклад «uDMX»."));
    }

    fn tab_patch(&mut self, ui: &mut Ui, st: &mut SettingsState) {
        let overlap: std::collections::BTreeSet<String> =
            self.v.overlaps.iter().flat_map(|(a, b)| [a.clone(), b.clone()]).collect();
        let fixtures = self.v.show.fixtures.clone();
        let w = ui.available_width();
        let cols = [w * 0.26, 88.0, w * 0.3, 0.0, 3.0 * (CONTROL_H_SM + S1)];
        let chs_w = (w - cols[0] - cols[1] - cols[2] - cols[4] - 4.0 * S2).max(60.0);
        ui.horizontal(|ui| {
            for (i, h) in ["Назва", "Адреса", "Профіль", "Канали", ""].iter().enumerate() {
                let cw = if i == 3 { chs_w } else { cols[i] };
                let (r, _) = ui.allocate_exact_size(vec2(cw, 20.0), Sense::hover());
                ui.painter().text(r.left_center(), Align2::LEFT_CENTER, *h, font(FS_SM), MUTED);
            }
        });
        let mut acts = vec![];
        for f in &fixtures {
            let row_top = ui.cursor().min.y;
            if overlap.contains(&f.id) {
                let r = Rect::from_min_size(pos2(ui.cursor().min.x, row_top), vec2(w, CONTROL_H + 2.0 * S1));
                ui.painter().rect_filled(r, 0.0, alpha(WARN, 0.08));
            }
            ui.painter().hline(ui.cursor().min.x..=ui.cursor().min.x + w, row_top, Stroke::new(1.0_f32, LINE));
            ui.add_space(S1);
            ui.horizontal(|ui| {
                let name = st.patch_names.entry(f.id.clone()).or_insert_with(|| f.name.clone());
                let r = widgets::input(ui, name, "Назва приладу", cols[0]);
                if r.lost_focus() && *name != f.name {
                    acts.push(Command::UpdateFixture {
                        id: f.id.clone(),
                        name: Some(name.clone()),
                        address: None,
                        profile: None,
                    });
                }
                if !r.has_focus() && !r.lost_focus() {
                    *name = f.name.clone();
                }
                let mut addr = f.address as i64;
                let ar = widgets::number(ui, &mut addr, 1, 512, cols[1]);
                if (ar.changed() && !ar.dragged() || ar.drag_stopped()) && addr != f.address as i64 {
                    acts.push(Command::UpdateFixture {
                        id: f.id.clone(),
                        name: None,
                        address: Some(addr as u16),
                        profile: None,
                    });
                }
                let mut prof = f.profile.clone();
                if self.profile_select(ui, ("patch-prof", &f.id), &mut prof, f, None, cols[2]) {
                    acts.push(Command::UpdateFixture {
                        id: f.id.clone(),
                        name: None,
                        address: None,
                        profile: Some(ProfileSpec::named(prof)),
                    });
                }
                let chs = self.v.channels(f).join(" ");
                let (r, _) = ui.allocate_exact_size(vec2(chs_w, CONTROL_H), Sense::hover());
                let mut job = egui::text::LayoutJob::simple_singleline(chs, mono(FS_SM), MUTED);
                job.wrap = egui::text::TextWrapping::truncate_at_width(chs_w);
                let g = ui.fonts(|fo| fo.layout_job(job));
                ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, MUTED);
                ui.spacing_mut().item_spacing.x = S1;
                if icon_btn("arrowUp").small().tip("Вище").show(ui).clicked() {
                    acts.push(Command::MoveFixture { id: f.id.clone(), delta: -1 });
                }
                if icon_btn("arrowDown").small().tip("Нижче").show(ui).clicked() {
                    acts.push(Command::MoveFixture { id: f.id.clone(), delta: 1 });
                }
                if icon_btn("trash").small().tip("Видалити прилад").show(ui).clicked() {
                    self.act(ui.ctx(), Act::AskDeleteFixtures { ids: vec![f.id.clone()], close_props: false });
                }
            });
            ui.add_space(S1);
        }
        for c in acts {
            self.send(c);
        }
        if !self.v.overlaps.is_empty() {
            let name = |id: &str| self.v.fixture(id).map(|f| f.name.clone()).unwrap_or_default();
            let text = self
                .v
                .overlaps
                .iter()
                .map(|(a, b)| format!("{} ↔ {}", name(a), name(b)))
                .collect::<Vec<_>>()
                .join(", ");
            ui.add_space(S2);
            ui.label(RichText::new(format!("Перетин адрес: {text}")).size(FS_SM).color(WARN));
        }
        ui.add_space(S3);
        if btn("Додати прилад").icon("plus").primary().show(ui).clicked() {
            self.open_add();
        }
    }

    /// Вибір профілю приладу: меню + власний профіль моделі цього приладу (якщо є).
    pub fn profile_select(
        &self,
        ui: &mut Ui,
        id: impl std::hash::Hash,
        value: &mut String,
        f: &nitor_core::Fixture,
        device: Option<&(String, Vec<String>, Option<Model>, bool)>,
        width: f32,
    ) -> bool {
        let names = &self.v.menu_profiles;
        let mut opts: Vec<(String, String)> = names.iter().map(|p| (p.clone(), p.clone())).collect();
        let own = device
            .map(|d| (d.0.clone(), d.2.clone()))
            .or_else(|| (!names.contains(&f.profile)).then(|| (f.profile.clone(), f.model.clone())));
        if let Some((name, model)) = own {
            let label = model.as_ref().map_or(name.clone(), model_label);
            opts.insert(0, (name, label));
        }
        widgets::select(ui, id, value, &opts, width)
    }

    fn tab_profiles(&mut self, ui: &mut Ui, st: &mut SettingsState) {
        let names = self.v.menu_profiles.clone();
        let w = ui.available_width();
        for (i, p) in names.iter().enumerate() {
            if i > 0 {
                widgets::hline(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(CONTROL_H_SM + S2);
                ui.label(RichText::new(p).font(bold(FS_MD)));
                let chs = self.v.show.profiles.get(p).map(|c| c.join(", ")).unwrap_or_default();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_btn("trash").small().tip("Видалити профіль").show(ui).clicked() {
                        self.send(Command::DeleteProfile { name: p.clone() });
                    }
                    if btn("Змінити").small().show(ui).clicked() {
                        st.prof_name = p.clone();
                        st.prof_channels = chs.clone();
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(RichText::new(chs).font(mono(FS_SM)).color(MUTED));
                    });
                });
            });
        }
        ui.add_space(S4);
        ui.label(RichText::new("Новий / змінити профіль").font(bold(FS_MD)));
        ui.add_space(S2);
        widgets::field_label(ui, "Назва");
        widgets::input(ui, &mut st.prof_name, "Мій PAR 7 кан.", w);
        ui.add_space(S3 - S2);
        widgets::field_label(ui, "Канали через кому");
        widgets::input(ui, &mut st.prof_channels, "dim, r, g, b, w, strobe, zero", w);
        ui.add_space(S3 - S2);
        if btn("Зберегти профіль").primary().show(ui).clicked() {
            let channels: Vec<String> = st
                .prof_channels
                .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            self.send(Command::SaveProfile { name: st.prof_name.trim().into(), channels, assign_to: None });
        }
    }

    fn tab_schedule(&mut self, ui: &mut Ui) {
        use nitor_core::schedule::{At, CITIES, Event, Place, all_days, minute_of};
        // місце для сходу й заходу сонця
        widgets::field_label(ui, "Місто для сходу й заходу сонця");
        let custom = "Свої координати".to_string();
        let mut opts: Vec<(String, String)> = vec![(String::new(), "Не вибрано".into())];
        opts.extend(CITIES.iter().map(|c| (c.0.to_string(), c.0.to_string())));
        opts.push((custom.clone(), custom.clone()));
        let cur = self.v.place.clone();
        let mut name = cur.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        if widgets::select(ui, "place", &mut name, &opts, 320.0) {
            let p = CITIES.iter().find(|c| c.0 == name).map(|c| Place { name: c.0.into(), lat: c.1, lon: c.2 });
            let p = if name == custom { Some(Place { name: custom.clone(), lat: 50.45, lon: 30.52 }) } else { p };
            self.send(Command::SetPlace(p));
        }
        if let Some(mut p) = cur.clone().filter(|p| p.name == custom) {
            ui.horizontal(|ui| {
                ui.label(widgets::muted("Широта"));
                let a = widgets::number_f(ui, &mut p.lat, -90.0, 90.0, 0.01, 2, 120.0).changed();
                ui.label(widgets::muted("Довгота"));
                let b = widgets::number_f(ui, &mut p.lon, -180.0, 180.0, 0.01, 2, 120.0).changed();
                if a || b {
                    self.send(Command::SetPlace(Some(p)));
                }
            });
        }
        let off = crate::winshell::local_offset_min();
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64 / 60);
        let today = (now + off as i64).div_euclid(1440);
        let hm = |m: i64| format!("{:02}:{:02}", m / 60, m % 60);
        if let Some(p) = &cur {
            let at = |at| {
                minute_of(
                    &Event { on: true, at, minutes: 0, days: all_days(), action: String::new() },
                    Some(p),
                    today,
                    off,
                )
            };
            if let (Some(r), Some(s)) = (at(At::Sunrise), at(At::Sunset)) {
                ui.label(widgets::muted(format!("Сьогодні схід о {}, захід о {}", hm(r), hm(s))));
            }
        }
        ui.add_space(S4);
        // події
        let mut ev = self.v.schedule.clone();
        let mut changed = false;
        let mut remove = None;
        let mut actions: Vec<(String, String)> =
            vec![("blackout".into(), "Blackout".into()), ("clear".into(), "Очистити".into())];
        actions.extend(self.v.show.scenes.keys().map(|n| (format!("scene:{n}"), format!("Сцена «{n}»"))));
        for (i, e) in ev.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S2;
                changed |= widgets::switch(ui, &mut e.on, "").on_hover_text("Подія увімкнена").changed();
                let whens = [
                    (At::Time, "О часі".to_string()),
                    (At::Sunrise, "На сході сонця".to_string()),
                    (At::Sunset, "На заході сонця".to_string()),
                ];
                let was = e.at;
                if widgets::select(ui, ("at", i), &mut e.at, &whens, 150.0) {
                    e.minutes = if e.at == At::Time { 19 * 60 } else { 0 };
                    changed |= e.at != was;
                }
                // час і зсув — колонка однакової ширини, щоб дні й дії стояли рівно
                ui.allocate_ui_with_layout(vec2(124.0, CONTROL_H), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_width(124.0);
                    if e.at == At::Time {
                        let (mut h, mut m) = ((e.minutes / 60) as i64, (e.minutes % 60) as i64);
                        let a = widgets::number(ui, &mut h, 0, 23, 54.0).changed();
                        ui.label(":");
                        let b = widgets::number(ui, &mut m, 0, 59, 54.0).changed();
                        if a || b {
                            e.minutes = (h * 60 + m) as i32;
                            changed = true;
                        }
                    } else {
                        let mut m = e.minutes as i64;
                        if widgets::number(ui, &mut m, -720, 720, 96.0)
                            .on_hover_text("Зсув у хвилинах: мінус — раніше")
                            .changed()
                        {
                            e.minutes = m as i32;
                            changed = true;
                        }
                        ui.label(widgets::muted("хв"));
                    }
                });
                for (d, label) in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Нд"].iter().enumerate() {
                    let on = e.days & (1 << d) != 0;
                    let kind = if on { Kind::Toggled } else { Kind::Normal };
                    if btn(*label).small().min_width(36.0).kind(kind).show(ui).clicked() {
                        e.days ^= 1 << d;
                        changed = true;
                    }
                }
                let mut a = e.action.clone();
                if !actions.iter().any(|o| o.0 == a) {
                    actions.push((a.clone(), format!("Сцени «{}» вже немає", a.trim_start_matches("scene:"))));
                }
                if widgets::select(ui, ("act", i), &mut a, &actions, 150.0) {
                    e.action = a;
                    changed = true;
                }
                if icon_btn("trash").small().tip("Видалити подію").show(ui).clicked() {
                    remove = Some(i);
                }
            });
            ui.add_space(S1);
        }
        if let Some(i) = remove {
            ev.remove(i);
            changed = true;
        }
        if btn("Додати подію").small().icon("plus").show(ui).clicked() {
            let action = self.v.show.scenes.keys().next().map_or("blackout".into(), |n| format!("scene:{n}"));
            ev.push(Event { on: true, at: At::Time, minutes: 19 * 60, days: all_days(), action });
            changed = true;
        }
        if changed {
            self.send(Command::SetSchedule(ev));
        }
    }

    /// uDMX знайдено, але драйвер не той: кнопка ставить WinUSB (Windows спитає дозвіл адміністратора).
    fn winusb_block(&mut self, ui: &mut Ui) {
        let done = self.winusb.as_ref().and_then(|r| r.try_recv().ok());
        if let Some(r) = done {
            self.winusb = None;
            match r {
                Ok(()) => self.toast("Драйвер WinUSB встановлено", true),
                Err(e) => self.toast(&format!("Драйвер не встановлено: {e}. Його можна поставити через Zadig."), false),
            }
            self.find_udmx(false);
        }
        if !cfg!(windows) || self.udmx_devices.iter().all(|d| d.driver_ok) {
            return;
        }
        ui.add_space(S3);
        if self.winusb.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            widgets::alert(ui, widgets::Tone::Info, "Встановлюю драйвер…", "Дозвольте зміни у вікні Windows.", None);
        } else if widgets::alert(
            ui,
            widgets::Tone::Warn,
            "uDMX потрібен драйвер WinUSB",
            "Адаптер підключено, але Windows поставила для нього інший драйвер. Nitor може встановити WinUSB сам — Windows спитає дозвіл адміністратора.",
            Some("Встановити драйвер"),
        ) {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::winshell::install_winusb_elevated());
                ctx.request_repaint();
            });
            self.winusb = Some(rx);
        }
    }

    fn tab_remote(&mut self, ui: &mut Ui) {
        if !self.v.has_pin {
            widgets::alert(
                ui,
                widgets::Tone::Warn,
                "Спершу задайте PIN",
                "Пульт відкривається лише за PIN. Його задають, коли вмикають режим оператора (замок угорі вікна).",
                None,
            );
            return;
        }
        let mut on = self.v.remote.is_some();
        let mut port = self.v.remote.unwrap_or(crate::remote::PORT) as i64;
        ui.horizontal(|ui| {
            let a = widgets::switch(ui, &mut on, "Пульт з телефона").changed();
            ui.add_space(S3);
            ui.label(widgets::muted("Порт"));
            ui.add_space(S1);
            let b = widgets::number(ui, &mut port, 1024, 65535, 80.0).changed();
            if a || (b && on) {
                self.send(Command::SetRemote(on.then_some(port as u16)));
            }
        });
        ui.add_space(S3);
        if let Some((p, e)) = &self.remote_error {
            let t = format!(
                "Порт {p}: {e}. Вкажіть інший порт, наприклад {}, і ввімкніть пульт знову.",
                p.saturating_add(1)
            );
            ui.label(RichText::new(t).size(FS_SM).color(DANGER));
            return;
        }
        let Some(srv) = &self.remote else { return };
        let Some(ip) = crate::remote::lan_ip() else {
            ui.label(widgets::muted(
                "Комп'ютер не підключено до мережі. Підключіть його до тієї самої Wi-Fi, що й телефон.",
            ));
            return;
        };
        let url = format!("http://{ip}:{}/", srv.port);
        ui.label(RichText::new("Відкрийте на телефоні:").size(FS_MD));
        // адреса — звичайним текстом (синій лише для дій), поруч кнопка копіювання
        ui.horizontal(|ui| {
            ui.label(RichText::new(&url).font(bold(FS_LG)));
            if icon_btn("copy").small().tip("Копіювати адресу").show(ui).clicked() {
                ui.ctx().copy_text(url.clone());
                self.toast("Адресу скопійовано", true);
            }
        });
        ui.add_space(S3);
        // QR-код адреси: темні модулі на білому, з полем у 4 модулі, як вимагає стандарт
        if let Ok(qr) = qrcodegen::QrCode::encode_text(&url, qrcodegen::QrCodeEcc::Medium) {
            let n = qr.size();
            let m = 5.0;
            let side = (n + 8) as f32 * m;
            let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, radius(R_SM), WHITE);
            for y in 0..n {
                for x in 0..n {
                    if qr.get_module(x, y) {
                        let min = r.min + vec2((x + 4) as f32 * m, (y + 4) as f32 * m);
                        p.rect_filled(Rect::from_min_size(min, vec2(m, m)), 0.0, INK);
                    }
                }
            }
        }
    }

    /// VirtualDJ (OS2L): удари й кнопки сцен.
    fn os2l_block(&mut self, ui: &mut Ui) {
        let mut on = self.v.os2l.is_some();
        let mut port = self.v.os2l.unwrap_or(nitor_core::os2l::PORT) as i64;
        ui.horizontal(|ui| {
            let a = widgets::switch(ui, &mut on, "Приймати з VirtualDJ").changed();
            ui.add_space(S3);
            ui.label(widgets::muted("Порт"));
            ui.add_space(S1);
            let b = widgets::number(ui, &mut port, 1024, 65535, 80.0).changed();
            if a || (b && on) {
                self.send(Command::SetOs2l(on.then_some(port as u16)));
            }
        });
        if let Some((p, e)) = &self.os2l_error {
            ui.label(widgets::muted(format!("Порт {p}: {e}")));
        } else if self.os2l.is_some() {
            ui.label(widgets::muted("Чекаю на VirtualDJ"));
        }
    }

    fn tab_midi(&mut self, ui: &mut Ui) {
        ui.label(widgets::muted("Натисніть «Навчити» біля дії, а потім кнопку чи повзунок на контролері."));
        ui.add_space(S3 - S2);
        let status = if !self.midi_error.is_empty() {
            format!("Контролер: {}", self.midi_error)
        } else if let Some(m) = &self.midi {
            if m.names.is_empty() {
                "Контролерів не знайдено".into()
            } else {
                format!("Підключено: {}", m.names.join(", "))
            }
        } else {
            "Контролер не підключено".into()
        };
        ui.label(RichText::new(status).size(FS_MD));
        ui.add_space(S2);
        ui.horizontal_wrapped(|ui| {
            if btn("Профіль APC mini mk2")
                .small()
                .tip("Пади — кольори палітри, сцени й групи; кнопки праворуч — Blackout, «Очистити», «Спалах», тап; повзунки — швидкість і майстер")
                .show(ui)
                .clicked()
            {
                let scenes: Vec<String> = self.v.show.scenes.keys().cloned().collect();
                let groups: Vec<String> = self.v.show.groups.keys().cloned().collect();
                let p = nitor_core::midi::apc_mini_mk2(self.v.show.static_colors.len(), &scenes, &groups);
                self.set_midi(p);
            }
            if btn("Перепідключити").small().icon("refresh").show(ui).clicked() {
                self.midi_error.clear();
                self.midi_connect(ui.ctx());
            }
            if !self.v.midi.is_empty() && btn("Прибрати все").small().kind(Kind::Ghost).show(ui).clicked() {
                self.midi_learn = None;
                self.set_midi(vec![]);
            }
        });
        ui.add_space(S3 - S2);
        let mut rows: Vec<(Option<&str>, String, String)> = vec![(Some("Світло"), String::new(), String::new())];
        for (id, label) in [
            ("blackout", "Blackout"),
            ("clear", "Очистити кольори"),
            ("flash", "Спалах — поки кнопку тримають"),
            ("tap", "Тап темпу"),
            ("master", "Майстер (повзунок)"),
            ("speed", "Швидкість ефекту (повзунок)"),
        ] {
            rows.push((None, id.into(), label.into()));
        }
        rows.push((Some("Кольори палітри"), String::new(), String::new()));
        for (i, c) in self.v.show.static_colors.iter().enumerate() {
            rows.push((None, format!("static{}", i + 1), c.name.clone()));
        }
        if !self.v.show.scenes.is_empty() {
            rows.push((Some("Сцени"), String::new(), String::new()));
            rows.extend(self.v.show.scenes.keys().map(|n| (None, format!("scene:{n}"), n.clone())));
        }
        if !self.v.show.groups.is_empty() {
            rows.push((Some("Групи"), String::new(), String::new()));
            rows.extend(self.v.show.groups.keys().map(|n| (None, format!("group:{n}"), n.clone())));
        }
        for (group, id, label) in rows {
            if let Some(g) = group {
                ui.add_space(S2);
                ui.label(widgets::muted(g));
                continue;
            }
            let bound = self.v.midi.iter().find(|b| b.action == id).map(|b| nitor_core::midi::key_label(&b.key));
            ui.horizontal(|ui| {
                ui.set_min_height(CONTROL_H_SM + S1);
                ui.add_space(S2);
                ui.label(RichText::new(&label).size(FS_MD));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = S1;
                    if bound.is_some() {
                        if btn("Прибрати").small().kind(Kind::Ghost).min_width(84.0).show(ui).clicked() {
                            let m = self.v.midi.iter().filter(|b| b.action != id).cloned().collect();
                            self.set_midi(m);
                        }
                    } else {
                        ui.allocate_exact_size(vec2(84.0, CONTROL_H_SM), Sense::hover());
                    }
                    let learning = self.midi_learn.as_deref() == Some(id.as_str());
                    if btn(if learning { "Скасувати" } else { "Навчити" }).small().min_width(84.0).show(ui).clicked()
                    {
                        self.midi_learn = if learning { None } else { Some(id.clone()) };
                    }
                    ui.add_space(S2);
                    if learning {
                        ui.label(RichText::new("Натисніть кнопку на контролері…").size(FS_SM).color(ACCENT));
                    } else {
                        ui.label(widgets::muted(bound.unwrap_or_else(|| "Не призначено".into())));
                    }
                });
            });
        }
    }

    fn tab_keys(&mut self, ui: &mut Ui, st: &mut SettingsState) {
        ui.label(widgets::muted(
            "Натисніть «Змінити» і потім потрібну клавішу або поєднання з Ctrl, Shift чи Alt. Esc — скасувати.",
        ));
        ui.add_space(S3 - S2);
        // вибір клавіші
        if let Some(id) = st.capturing.clone() {
            let mut got: Option<String> = None;
            let mut cancel = false;
            ui.input(|i| {
                for e in &i.events {
                    if let egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } = e {
                        let key = physical_key.unwrap_or(*key);
                        if key == egui::Key::Escape && !modifiers.any() {
                            cancel = true;
                        } else if let Some(c) = hotkeys::combo(key, *modifiers) {
                            got = Some(c);
                        }
                    }
                }
            });
            if cancel {
                st.capturing = None;
            } else if let Some(c) = got {
                if c == "Tab" || c == "Shift+Tab" {
                    self.toast("Tab потрібен для переходу між елементами", true);
                } else {
                    st.capturing = None;
                    self.assign_keys(&id, vec![c], false);
                }
            }
        }
        let custom = self.v.hotkeys.clone();
        // пошук за назвою дії; якщо шукали саму вкладку («клавіші») — показуємо все
        let q = st.q.trim().to_lowercase();
        let (_, title, help, words) = TABS.iter().find(|t| t.0 == Tab::Keys).copied().unwrap_or(TABS[0]);
        let all = q.is_empty() || [title, help, words].iter().any(|t| t.to_lowercase().contains(&q));
        let mut groups: Vec<(&str, Vec<Entry>)> = vec![];
        for e in hotkeys::table() {
            match e {
                Entry::Group(g) => groups.push((g, vec![])),
                Entry::Action { ref label, .. } => {
                    if all || label.to_lowercase().contains(&q) {
                        if let Some(g) = groups.last_mut() {
                            g.1.push(e);
                        }
                    }
                }
            }
        }
        for (g, rows) in groups.iter().filter(|g| !g.1.is_empty()) {
            ui.add_space(S2);
            ui.label(widgets::muted(*g));
            ui.add_space(S1);
            // група — на окремій плашці
            egui::Frame::new()
                .fill(BG)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(radius(R_MD))
                .inner_margin(egui::Margin::same(S1 as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for e in rows {
                        if let Entry::Action { id, label, keys } = e {
                            self.key_row(ui, st, &custom, id, label, keys);
                        }
                    }
                });
        }
        ui.add_space(S3);
        if btn("Відновити стандартні").icon("reset").show(ui).clicked() && !self.v.hotkeys.is_empty()
        {
            self.ui_dlg = Some(UiDlg::confirm(
                "Підтвердження",
                "Повернути стандартні гарячі клавіші для всіх дій?",
                "Відновити",
                false,
                Act::ResetKeys,
            ));
        }
    }

    /// Рядок дії: назва · клавіші · «Змінити» · «Прибрати» · повернути стандартну.
    fn key_row(
        &mut self,
        ui: &mut Ui,
        st: &mut SettingsState,
        custom: &indexmap::IndexMap<String, Vec<String>>,
        id: &str,
        label: &str,
        def: &[String],
    ) {
        let keys = hotkeys::keys_for(custom, id);
        let changed = custom.contains_key(id);
        // підсвітка рядка малюється під вмістом: місце для неї резервуємо заздалегідь
        let row_bg = ui.painter().add(egui::Shape::Noop);
        let row = ui.horizontal(|ui| {
            ui.set_min_height(CONTROL_H_SM + S1);
            ui.add_space(S2);
            ui.label(RichText::new(label).size(FS_MD));
            if changed {
                ui.label(widgets::muted("змінено"));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = S1;
                ui.add_space(S1);
                // стовпці однакові: повернути стандартну · «Прибрати» · «Змінити» — і клавіші поруч
                if changed {
                    if icon_btn("reset")
                        .small()
                        .kind(Kind::Ghost)
                        .tip("Повернути стандартну клавішу")
                        .show(ui)
                        .clicked()
                    {
                        self.assign_keys(id, def.to_vec(), true);
                    }
                } else {
                    ui.allocate_exact_size(vec2(CONTROL_H_SM, CONTROL_H_SM), Sense::hover());
                }
                if !keys.is_empty() {
                    if btn("Прибрати")
                        .small()
                        .kind(Kind::Ghost)
                        .min_width(84.0)
                        .tip("Залишити дію без клавіші")
                        .show(ui)
                        .clicked()
                    {
                        self.assign_keys(id, vec![], false);
                    }
                } else {
                    ui.allocate_exact_size(vec2(84.0, CONTROL_H_SM), Sense::hover());
                }
                let capturing = st.capturing.as_deref() == Some(id);
                if btn(if capturing { "Скасувати" } else { "Змінити" }).small().min_width(84.0).show(ui).clicked()
                {
                    st.capturing = if capturing { None } else { Some(id.to_string()) };
                }
                ui.add_space(S2);
                if capturing {
                    ui.label(RichText::new("Натисніть клавішу…").size(FS_SM).color(ACCENT));
                } else if keys.is_empty() {
                    ui.label(widgets::muted("Не призначено"));
                } else {
                    for k in keys.iter().rev() {
                        kbd(ui, &hotkeys::key_label(k));
                    }
                }
            });
        });
        if row.response.contains_pointer() {
            ui.painter().set(row_bg, egui::Shape::rect_filled(row.response.rect, radius(R_SM), RAISED));
        }
    }

    fn tab_about(&mut self, ui: &mut Ui) {
        let u = self.updater.info();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S4;
            ui.add(
                egui::Image::new(egui::include_image!("../../../assets/icon.svg")).fit_to_exact_size(vec2(64.0, 64.0)),
            );
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("Nitor v{}", u.current)).font(bold(FS_MD)));
                ui.label(widgets::muted("Керування RGB-прожекторами."));
                ui.horizontal(|ui| {
                    ui.label(widgets::muted("Оновлення:"));
                    let repo = format!("github.com/{}", nitor_core::GITHUB_REPO);
                    if ui.link(RichText::new(repo).size(FS_SM).color(ACCENT)).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&u.page));
                        self.toast("Відкриваю в браузері…", true);
                    }
                });
            });
        });
        ui.add_space(S4);
        let busy = matches!(u.status, crate::update::Status::Installing | crate::update::Status::Restarting);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S3;
            let label = if u.available {
                format!("Оновити до {}", u.latest)
            } else {
                "Перевірити оновлення".into()
            };
            let checking = u.status == crate::update::Status::Checking;
            let b = btn(label).icon(if u.available { "download" } else { "refresh" }).busy(checking || busy);
            if b.primary().enabled(!busy && !checking).show(ui).clicked() {
                if u.available && u.blocked.is_none() {
                    self.settings = None;
                    self.open_update_dialog();
                } else {
                    self.updater.check();
                }
            }
            let status = if u.status == crate::update::Status::Checking {
                "Перевірка…".to_string()
            } else if busy {
                u.progress.clone()
            } else if !u.error.is_empty() {
                u.error.clone()
            } else if u.available {
                format!("Доступна версія {}", u.latest)
            } else if !u.latest.is_empty() {
                "Встановлено останню версію".into()
            } else {
                String::new()
            };
            ui.label(widgets::muted(cap(&status)));
        });
        if u.available && !u.notes.is_empty() {
            notes_box(ui, &u.notes);
        }
    }

    // ----------------------------------------------------------- новий прилад --
    fn add_channels(&self, a: &AddState) -> Vec<String> {
        match &a.device {
            Some((_, mode)) => mode.channels().to_vec(),
            None => self.v.show.profiles.get(&a.profile).cloned().unwrap_or_default(),
        }
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        let mut a = self.add.take().unwrap();
        let mut close = false;
        let mut submit = false;
        let esc = modal(ctx, "add-dlg", 560.0, |ui| {
            close |= dialog_head(ui, "Новий прилад");
            let w = ui.available_width();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S3;
                let bw = 200.0 + if a.device.is_some() { 100.0 } else { 0.0 };
                ui.allocate_ui(vec2(w - bw - S3, CONTROL_H + 8.0), |ui| match &a.device {
                    Some((fx, mode)) => {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(library::title(&fx.m, &fx.n)).font(bold(FS_MD)));
                            ui.label(widgets::muted(mode_text(mode.name(), mode.channels().len())));
                        });
                    }
                    None => {
                        ui.label(widgets::muted("Модель не вибрано"));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if btn("Вибрати з бібліотеки").icon("grid").show(ui).clicked() {
                        self.lib = Some(LibState::new(false));
                    }
                    if a.device.is_some() && btn("Скинути").show(ui).clicked() {
                        a.device = None;
                        if a.name_auto {
                            a.name.clear();
                        }
                    }
                });
            });
            ui.add_space(S3 - S2);
            if a.device.is_none() {
                widgets::field_label(ui, "Стандартний профіль");
                let opts: Vec<(String, String)> = self.v.menu_profiles.iter().map(|p| (p.clone(), p.clone())).collect();
                widgets::select(ui, "addProfile", &mut a.profile, &opts, w);
                ui.add_space(S3 - S2);
            }
            let ch = self.add_channels(&a);
            let n = segment_count(&ch);
            let count = a.count.max(1);
            let start = a.address.max(1);
            let last = start + ch.len() as i64 * count - 1;
            // факти — окремими рядками, а не ланцюжком через крапки
            let segs = if n > 1 { format!(", {n} сегм.") } else { String::new() };
            let mut info = format!("Займе адреси {start}–{last}: {} кан.{segs}", ch.len());
            if last > DMX_CHANNELS as i64 {
                info += ", за межами 512";
            }
            ui.label(widgets::muted(info));
            ui.label(RichText::new(ch.join(" ")).font(mono(FS_SM)).color(MUTED));
            ui.add_space(S3 - S2);
            // сітка 2fr 1fr 1fr
            let cw = (w - 2.0 * S3) / 4.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = S3;
                ui.vertical(|ui| {
                    widgets::field_label(ui, "Назва");
                    let r = widgets::input(ui, &mut a.name, "Прожектор", cw * 2.0);
                    focus_on_open(ui, &r, "add", a.name.chars().count());
                    if r.changed() {
                        a.name_auto = false;
                    }
                });
                ui.vertical(|ui| {
                    widgets::field_label(ui, "Стартова адреса");
                    widgets::number(ui, &mut a.address, 1, 512, cw);
                });
                ui.vertical(|ui| {
                    widgets::field_label(ui, "Кількість");
                    widgets::number(ui, &mut a.count, 1, 64, cw);
                });
            });
            ui.add_space(S3);
            ui.horizontal(|ui| {
                submit |= btn("Додати").primary().show(ui).clicked();
                close |= btn("Скасувати").key(Some("Esc".into())).show(ui).clicked();
            });
        });
        let esc = esc && self.lib.is_none() && self.modes.is_none();
        if submit {
            let before = self.v.show.fixtures.len();
            let spec = match &a.device {
                // профіль моделі йде разом із приладом і не додається в меню профілів
                Some((fx, mode)) => ProfileSpec {
                    profile: fx.profile_name(mode),
                    channels: Some(mode.channels().to_vec()),
                    model: Some(Model { m: fx.m.clone(), n: fx.n.clone(), mode: mode.name().into() }),
                },
                None => ProfileSpec::named(a.profile.clone()),
            };
            let cmd = Command::AddFixture {
                spec,
                name: a.name.trim().into(),
                address: a.address.clamp(1, 512) as u16,
                count: a.count.max(1) as u32,
            };
            if self.send(cmd) {
                // після успішного додавання — закрити вікно і вибрати новий прилад
                if let Some(f) = self.v.show.fixtures.get(before) {
                    let id = f.id.clone();
                    self.select_only(&id);
                }
                self.lib = None;
                return;
            }
        }
        if !(close || esc) {
            if self.add.is_none() {
                self.add = Some(a);
            }
        } else {
            self.lib = None;
        }
    }

    pub fn choose_device(&mut self, fx: LibModel, mode: Mode) {
        if let Some(a) = self.add.as_mut() {
            if a.name.trim().is_empty() || a.name_auto {
                a.name = fx.n.clone();
                a.name_auto = true;
            }
            a.device = Some((fx, mode));
        }
    }

    /// Розділи, пошук і картки моделей бібліотеки; `Some` — модель вибрано.
    fn library_body(&mut self, ui: &mut Ui, st: &mut LibState, side: f32) -> Option<LibModel> {
        let mut pick = None;
        let counts = library::counts(&st.q, &self.v.recent);
        let all: usize = counts.iter().filter(|(k, _)| *k != "recent").map(|x| x.1).sum();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = S5;
            // розділи з кількістю моделей для поточного пошуку
            ui.vertical(|ui| {
                ui.set_width(side);
                ui.spacing_mut().item_spacing.y = 2.0;
                for (key, label) in library::SECTIONS {
                    let n =
                        if key.is_empty() { all } else { counts.iter().find(|(k, _)| *k == key).map_or(0, |x| x.1) };
                    let on = st.kind == key;
                    let enabled = n > 0 || on;
                    let (r, resp) = ui.allocate_exact_size(
                        vec2(side, CONTROL_H),
                        if enabled { Sense::click() } else { Sense::hover() },
                    );
                    let a = if enabled { 1.0 } else { 0.45 };
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            enabled,
                            on,
                            format!("{label}, {n}"),
                        )
                    });
                    widgets::focus_ring(ui, &resp, r, R_MD);
                    if on {
                        ui.painter().rect_filled(r, radius(R_MD), ACCENT_SOFT);
                    } else if resp.hovered() && enabled {
                        ui.painter().rect_filled(r, radius(R_MD), RAISED);
                    }
                    let fg = if on { ACCENT } else { TEXT }.gamma_multiply(a);
                    ui.painter().text(pos2(r.left() + S3, r.center().y), Align2::LEFT_CENTER, label, font(FS_MD), fg);
                    ui.painter().text(
                        pos2(r.right() - S3, r.center().y),
                        Align2::RIGHT_CENTER,
                        n.to_string(),
                        font(FS_SM),
                        if on { ACCENT } else { MUTED }.gamma_multiply(a),
                    );
                    if resp.clicked() && enabled {
                        st.kind = key.into();
                        st.shown = 60;
                    }
                    if key == "recent" {
                        ui.add_space(S3);
                    }
                }
            });
            ui.vertical(|ui| {
                let mw = ui.available_width();
                let r = widgets::input(ui, &mut st.q, "Пошук за виробником або моделлю", mw);
                if r.changed() {
                    st.shown = 60;
                }
                if !r.has_focus() && ui.memory(|m| m.focused().is_none()) {
                    r.request_focus();
                }
                ui.add_space(S3 - S2);
                let found = library::search(&st.q, st.shown + 1, &st.kind, 0, &self.v.recent);
                let more = found.len() > st.shown;
                let sa = egui::ScrollArea::vertical()
                    .id_salt(("lib-grid", &st.kind, &st.q))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let gw = ui.available_width();
                        let gap = S3;
                        let cols = (((gw + gap) / (230.0 + gap)).floor() as usize).max(1);
                        let cw = (gw - gap * (cols as f32 - 1.0)) / cols as f32;
                        let ch = 96.0 + 2.0 * S3 + 4.0 * S2 + 18.0 + 40.0 + 20.0 + 18.0;
                        let list: Vec<&LibModel> = found.iter().take(st.shown).copied().collect();
                        if list.is_empty() {
                            ui.add_space(S5);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(if st.kind == "recent" && st.q.is_empty() {
                                        "Тут з'являться прилади, які ви додавали з бібліотеки."
                                    } else {
                                        "Нічого не знайдено"
                                    })
                                    .color(MUTED),
                                );
                            });
                        }
                        let rows = list.len().div_ceil(cols);
                        let (area, _) =
                            ui.allocate_exact_size(vec2(gw, (rows as f32 * (ch + gap) - gap).max(0.0)), Sense::hover());
                        for (i, fx) in list.iter().enumerate() {
                            let r = Rect::from_min_size(
                                pos2(
                                    area.left() + (i % cols) as f32 * (cw + gap),
                                    area.top() + (i / cols) as f32 * (ch + gap),
                                ),
                                vec2(cw, ch),
                            );
                            if !ui.clip_rect().intersects(r) {
                                continue;
                            }
                            if lib_card(ui, r, fx).clicked() {
                                pick = Some((*fx).clone());
                            }
                        }
                        if more {
                            ui.add_space(S3);
                            ui.vertical_centered(|ui| {
                                if btn("Показати ще").show(ui).clicked() {
                                    st.shown += 60;
                                }
                            });
                        }
                    });
                widgets::scroll_shadow(ui, &sa, SURFACE);
            });
        });
        pick
    }

    // ----------------------------------------------------------- бібліотека --
    fn library_dialog(&mut self, ctx: &egui::Context) {
        let mut st = self.lib.take().unwrap();
        let mut close = false;
        let mut pick: Option<LibModel> = None;
        let screen = ctx.screen_rect();
        let w = 1180f32.min(screen.width() * 0.96);
        let h = 860f32.min(screen.height() * 0.92);
        let modal =
            egui::Modal::new(Id::new("lib-dlg")).frame(dialog_frame(w)).backdrop_color(BACKDROP).show(ctx, |ui| {
                ui.set_width(w - 2.0 * S5);
                ui.set_height(h - 2.0 * S5);
                close |= dialog_head(ui, "Бібліотека приладів");
                pick = self.library_body(ui, &mut st, 260.0);
            });
        nitor_ui::anim::pop_in(ctx, modal.response.layer_id, modal.response.rect.center(), 0.98);
        let esc = modal.should_close();
        if let Some(fx) = pick {
            let target = if st.for_props { ModesFor::Props } else { ModesFor::Add };
            self.modes = Some(ModesState { fx, current: None, target });
        }
        let esc = esc && self.modes.is_none();
        if !(close || esc) {
            self.lib = Some(st);
        }
    }

    fn modes_dialog(&mut self, ctx: &egui::Context) {
        let st = self.modes.take().unwrap();
        let mut close = false;
        let mut chosen: Option<Mode> = None;
        let esc = modal(ctx, "mode-dlg", 560.0, |ui| {
            close |= dialog_head(ui, &st.fx.n);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S4;
                let (r, _) = ui.allocate_exact_size(vec2(200.0, 96.0), Sense::hover());
                lib_image(ui, r, &st.fx);
                ui.vertical(|ui| {
                    ui.label(widgets::muted(library::maker(&st.fx.m)));
                    color_chips(ui, &st.fx);
                });
            });
            ui.add_space(S3);
            let sa = egui::ScrollArea::vertical()
                .id_salt("modes")
                .max_height(ui.ctx().screen_rect().height() * 0.5)
                .show(ui, |ui| {
                    let w = ui.available_width();
                    ui.spacing_mut().item_spacing.y = S2;
                    for mode in &st.fx.modes {
                        let picked = st.current.as_deref() == Some(mode.name());
                        let chs = mode.channels().join(" ");
                        let galley = ui.fonts(|f| f.layout(chs.clone(), font(FS_XS), MUTED, w - 2.0 * S3));
                        let h = S3 * 2.0 + 20.0 + galley.size().y + 2.0;
                        let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                        resp.widget_info(|| {
                            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, picked, mode.name())
                        });
                        widgets::focus_ring(ui, &resp, r, R_MD);
                        let stroke = if picked {
                            ACCENT
                        } else if resp.hovered() {
                            LINE_STRONG
                        } else {
                            LINE
                        };
                        ui.painter().rect(
                            r,
                            radius(R_MD),
                            if resp.hovered() { RAISED } else { Color32::TRANSPARENT },
                            Stroke::new(1.0_f32, stroke),
                            StrokeKind::Inside,
                        );
                        ui.painter().text(
                            pos2(r.left() + S3, r.top() + S3),
                            Align2::LEFT_TOP,
                            mode.name(),
                            bold(FS_MD),
                            TEXT,
                        );
                        ui.painter().text(
                            pos2(r.right() - S3, r.top() + S3 + 2.0),
                            Align2::RIGHT_TOP,
                            format!("{} кан.", mode.channels().len()),
                            font(FS_SM),
                            MUTED,
                        );
                        ui.painter().galley(pos2(r.left() + S3, r.top() + S3 + 22.0), galley, MUTED);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            chosen = Some(mode.clone());
                        }
                    }
                });
            widgets::scroll_shadow(ui, &sa, SURFACE);
        });
        if let Some(mode) = chosen {
            match st.target {
                ModesFor::Add => {
                    self.choose_device(st.fx.clone(), mode);
                    self.lib = None;
                }
                ModesFor::Props => {
                    self.lib = None;
                    if let Some(p) = self.props.as_mut() {
                        let name = st.fx.profile_name(&mode);
                        let model = Model { m: st.fx.m.clone(), n: st.fx.n.clone(), mode: mode.name().into() };
                        p.device = Some((name.clone(), mode.channels().to_vec(), Some(model), true));
                        p.profile = name;
                        p.test_count = mode.channels().len() as i64;
                        p.test_rows.clear();
                    }
                }
            }
            return;
        }
        if !(close || esc) {
            self.modes = Some(st);
        }
    }

    // ------------------------------------------------------ властивості приладу --
    /// Тест каналів у властивостях приладу: значення каналів вручну й збереження їх як профілю.
    /// `true` — натиснули «Зберегти профіль і призначити».
    fn channel_test(
        &mut self,
        ui: &mut Ui,
        p: &mut PropsState,
        ch: &[String],
        start: i64,
        w: f32,
        raw_cmds: &mut Vec<Command>,
    ) -> bool {
        let mut save = false;
        ui.add_space(S3 - S2);
        ui.horizontal(|ui| {
            ui.label(widgets::muted("Каналів для тесту"));
            widgets::number(ui, &mut p.test_count, 1, 64, 72.0);
            if btn("Усі на 0").small().show(ui).clicked() {
                for (i, row) in p.test_rows.iter_mut().enumerate() {
                    let addr = start as u16 + i as u16;
                    if addr as usize > DMX_CHANNELS {
                        break; // рядки за межами всесвіту не показані — і не надсилаються
                    }
                    row.1 = 0.0;
                    raw_cmds.push(Command::RawSet { address: addr, value: 0 });
                }
            }
            if btn("Скинути тест").small().tip("Повернути керування програмі").show(ui).clicked()
            {
                raw_cmds.push(Command::RawClear(Some(self.tested.iter().copied().collect())));
                self.tested.clear();
            }
        });
        // рядки тесту: попередні значення лишаються, нові — з профілю
        let count = p.test_count.clamp(1, 64) as usize;
        while p.test_rows.len() < count {
            let i = p.test_rows.len();
            let addr = start as usize + i;
            let mut role = ch.get(i).cloned().unwrap_or_else(|| "zero".into());
            let mut value = self.v.dmx.get(addr.wrapping_sub(1)).copied().unwrap_or(0) as f32;
            if let Some(v) = nitor_core::fixtures::fixed_value(&role) {
                value = v as f32;
                role = "fixed".into();
            }
            if !TEST_ROLES.iter().any(|(k, _)| *k == role) {
                role = "zero".into();
            }
            p.test_rows.push((role, value));
        }
        p.test_rows.truncate(count);
        ui.add_space(S3 - S2);
        egui::ScrollArea::vertical().id_salt("test-rows").max_height(300.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = S1;
            for (i, row) in p.test_rows.iter_mut().enumerate() {
                let addr = start as u16 + i as u16;
                if addr as usize > DMX_CHANNELS {
                    break;
                }
                ui.horizontal(|ui| {
                    let live = self.tested.contains(&addr);
                    let (r, _) = ui.allocate_exact_size(vec2(96.0, CONTROL_H_SM), Sense::hover());
                    let g1 = format!("CH{}", i + 1);
                    let w1 = widgets::text_width(ui, &g1, bold(FS_SM));
                    ui.painter().text(
                        r.left_center(),
                        Align2::LEFT_CENTER,
                        &g1,
                        bold(FS_SM),
                        if live { ACCENT } else { TEXT },
                    );
                    ui.painter().text(
                        pos2(r.left() + w1, r.center().y),
                        Align2::LEFT_CENTER,
                        format!("  DMX {addr}"),
                        font(FS_SM),
                        MUTED,
                    );
                    let opts: Vec<(String, String)> =
                        TEST_ROLES.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
                    widgets::select(ui, ("test-role", i), &mut row.0, &opts, 160.0);
                    let sw = ui.available_width() - 40.0 - S2;
                    if widgets::slider(ui, &mut row.1, 0.0, 255.0, ACCENT, sw).changed() {
                        self.tested.insert(addr);
                        raw_cmds.push(Command::RawSet { address: addr, value: row.1 as u8 });
                    }
                    ui.label(RichText::new(format!("{}", row.1 as u8)).size(FS_SM));
                });
            }
        });
        ui.add_space(S3 - S2);
        ui.horizontal(|ui| {
            widgets::input(ui, &mut p.test_prof_name, "Назва профілю", w - 280.0);
            save = btn("Зберегти профіль і призначити").show(ui).clicked();
        });
        ui.add_space(S3 - S2);
        save
    }

    fn props_channels(&self, p: &PropsState) -> Vec<String> {
        match &p.device {
            Some(d) if d.0 == p.profile => d.1.clone(),
            _ => self.v.show.profiles.get(&p.profile).cloned().unwrap_or_default(),
        }
    }

    fn props_dialog(&mut self, ctx: &egui::Context) {
        let mut p = self.props.take().unwrap();
        let Some(f) = self.v.fixture(&p.id).cloned() else {
            self.release_test();
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut dup = false;
        let mut del = false;
        let mut mode_btn = false;
        let mut lib_btn = false;
        let mut test_save = false;
        let mut raw_cmds: Vec<Command> = vec![];
        let mut dimmer_changed = false;
        let esc = modal(ctx, "prop-dlg", 560.0, |ui| {
            close |= dialog_head(ui, "Властивості приладу");
            let w = ui.available_width();
            widgets::field_label(ui, "Назва");
            widgets::input(ui, &mut p.name, "", w);
            ui.add_space(S3 - S2);
            let half = (w - S3) / 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = S3;
                ui.vertical(|ui| {
                    widgets::field_label(ui, "Стартова адреса");
                    widgets::number(ui, &mut p.address, 1, 512, half);
                });
                ui.vertical(|ui| {
                    widgets::field_label(ui, "Яскравість");
                    ui.horizontal(|ui| {
                        ui.set_min_height(CONTROL_H);
                        dimmer_changed |=
                            widgets::slider(ui, &mut p.dimmer, 0.0, 255.0, ACCENT, half - 52.0 - S2).changed();
                        dimmer_changed |=
                            widgets::value_box(ui, Id::new("propDimmer"), &mut p.dimmer, 0.0, 255.0, Shown::Pct, 52.0);
                    });
                });
            });
            ui.add_space(S3 - S2);
            // модель приладу: змінити режим або взяти іншу модель з бібліотеки
            let model = match &p.device {
                Some((_, ch, Some(m), _)) => Some((m.clone(), ch.len())),
                _ => None,
            };
            ui.allocate_ui_with_layout(vec2(w, CONTROL_H + 8.0), Layout::right_to_left(Align::Center), |ui| {
                // кнопки першими, щоб довга назва моделі обрізалась, а не залазила під них
                let label =
                    if model.is_some() { "Інша модель" } else { "Вибрати з бібліотеки" };
                lib_btn = btn(label).icon("grid").show(ui).clicked();
                if model.is_some() {
                    mode_btn = btn("Змінити режим").show(ui).clicked();
                }
                ui.add_space(S2);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (title, sub) = match &model {
                        Some((m, n)) => (library::title(&m.m, &m.n), mode_text(&m.mode, *n)),
                        None => ("Модель не вибрано".into(), "Прилад працює за стандартним профілем".into()),
                    };
                    ui.add(egui::Label::new(RichText::new(&title).font(bold(FS_MD))).truncate()).on_hover_text(&title);
                    ui.add(egui::Label::new(widgets::muted(&sub)).truncate());
                });
            });
            ui.add_space(S3 - S2);
            widgets::field_label(ui, "Профіль");
            let device = p.device.clone();
            if self.profile_select(ui, "propProfile", &mut p.profile, &f, device.as_ref(), w) {
                p.test_count = self.props_channels(&p).len() as i64;
                p.test_rows.clear();
            }
            ui.add_space(S2);
            let ch = self.props_channels(&p);
            let start = p.address.max(1);
            let end = start + ch.len() as i64 - 1;
            let n = segment_count(&ch);
            let segs = if n > 1 { format!(", {n} сегм.") } else { String::new() };
            ui.label(widgets::muted(format!("Адреси {start}–{end}: {} кан.{segs}", ch.len())));
            ui.label(RichText::new(ch.join(" ")).font(mono(FS_SM)).color(MUTED));
            let clash: Vec<String> = self
                .v
                .show
                .fixtures
                .iter()
                .filter(|o| o.id != f.id)
                .filter(|o| start <= self.v.end(o) as i64 && (o.address as i64) <= end)
                .map(|o| o.name.clone())
                .collect();
            let warn = if end > 512 {
                "Прилад виходить за межі 512 каналів.".to_string()
            } else if !clash.is_empty() {
                format!("Адреси перетинаються з: {}", clash.join(", "))
            } else {
                String::new()
            };
            if !warn.is_empty() {
                ui.label(RichText::new(warn).size(FS_SM).color(WARN));
            }
            ui.add_space(S3 - S2);
            // тест каналів
            widgets::hline(ui);
            ui.add_space(S3 - S2);
            let (r, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
            let open = p.test_open;
            resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::CollapsingHeader,
                    true,
                    open,
                    "Тест каналів і власний профіль",
                )
            });
            widgets::focus_ring(ui, &resp, r, R_SM);
            nitor_ui::icons::image("wrench", 18.0, MUTED)
                .paint_at(ui, Rect::from_min_size(pos2(r.left(), r.center().y - 9.0), vec2(18.0, 18.0)));
            ui.painter().text(
                pos2(r.left() + 18.0 + S2, r.center().y),
                Align2::LEFT_CENTER,
                "Тест каналів і власний профіль",
                font(FS_MD),
                TEXT,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                p.test_open = !p.test_open;
                if p.test_open {
                    if p.test_count == 0 {
                        p.test_count = ch.len() as i64;
                    }
                    if p.test_prof_name.is_empty() {
                        p.test_prof_name = format!("{} власний", f.name);
                    }
                }
            }
            if p.test_open {
                test_save = self.channel_test(ui, &mut p, &ch, start, w, &mut raw_cmds);
            }
            widgets::hline(ui);
            ui.add_space(S3);
            ui.horizontal(|ui| {
                save = btn("Зберегти").primary().show(ui).clicked();
                dup = btn("Дублювати").show(ui).clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    del = btn("Видалити").icon("trash").kind(Kind::Danger).show(ui).clicked();
                });
            });
        });
        for c in raw_cmds {
            self.send(c);
        }
        if dimmer_changed {
            let id = p.id.clone();
            self.send(Command::SetColor {
                ids: vec![id],
                values: ColorValues { dimmer: Some(p.dimmer as u8), ..Default::default() },
            });
        }
        if lib_btn {
            self.lib = Some(LibState::new(true));
        }
        if mode_btn {
            if let Some((_, _, Some(m), _)) = &p.device {
                match library::find(&m.m, &m.n) {
                    Some(fx) => {
                        self.modes =
                            Some(ModesState { fx: fx.clone(), current: Some(m.mode.clone()), target: ModesFor::Props })
                    }
                    None => self.toast("Модель не знайдено в бібліотеці", false),
                }
            }
        }
        if save {
            let name = if p.name.trim().is_empty() { "Прилад".to_string() } else { p.name.trim().to_string() };
            let mut spec = ProfileSpec::named(p.profile.clone());
            if let Some((dname, ch, model, true)) = &p.device {
                if *dname == p.profile {
                    spec.channels = Some(ch.clone());
                    spec.model = model.clone();
                }
            }
            self.send(Command::UpdateFixture {
                id: p.id.clone(),
                name: Some(name),
                address: Some(p.address.clamp(1, 512) as u16),
                profile: Some(spec),
            });
            self.close_props();
            return;
        }
        if test_save {
            if p.test_rows.is_empty() {
                self.toast("Немає каналів для збереження", false);
            } else if p.test_prof_name.trim().is_empty() {
                self.toast("Вкажіть назву профілю", false);
            } else {
                let channels: Vec<String> = p
                    .test_rows
                    .iter()
                    .map(|(r, v)| if r == "fixed" { format!("={}", *v as u8) } else { r.clone() })
                    .collect();
                let name = p.test_prof_name.trim().to_string();
                self.send(Command::UpdateFixture {
                    id: p.id.clone(),
                    name: None,
                    address: Some(p.address.clamp(1, 512) as u16),
                    profile: None,
                });
                if self.send(Command::SaveProfile {
                    name: name.clone(),
                    channels: channels.clone(),
                    assign_to: Some(p.id.clone()),
                }) {
                    self.toast(&format!("Профіль «{name}» збережено і призначено: {}", channels.join(" ")), true);
                }
                self.close_props();
                return;
            }
        }
        if dup {
            let id = p.id.clone();
            self.close_props();
            self.duplicate(&id);
            return;
        }
        if del {
            let id = p.id.clone();
            self.props = Some(p);
            self.act(ctx, Act::AskDeleteFixtures { ids: vec![id], close_props: true });
            return;
        }
        let esc = esc && self.modes.is_none() && self.lib.is_none() && self.ui_dlg.is_none();
        if close || esc {
            self.props = Some(p);
            self.close_props();
        } else if self.props.is_none() {
            self.props = Some(p);
        }
    }
}

fn notes_box(ui: &mut Ui, text: &str) {
    ui.add_space(S3 - S2);
    egui::Frame::new().fill(RAISED).corner_radius(radius(R_MD)).inner_margin(egui::Margin::same(S3 as i8)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            let sa = egui::ScrollArea::vertical().id_salt(("notes", text.len())).max_height(220.0).show(ui, |ui| {
                ui.label(RichText::new(text).size(FS_SM));
            });
            widgets::scroll_shadow(ui, &sa, RAISED);
        },
    );
    ui.add_space(S3 - S2);
}

fn kbd(ui: &mut Ui, text: &str) {
    let w = widgets::kbd_width(ui, text, FS_SM) + 2.0 * S1;
    let (r, _) = ui.allocate_exact_size(vec2(w, CONTROL_H_SM), Sense::hover());
    let p = ui.painter();
    p.rect(r, radius(R_SM), RAISED, Stroke::new(1.0_f32, LINE_STRONG), StrokeKind::Inside);
    p.hline(r.x_range(), r.bottom() - 1.0, Stroke::new(2.0_f32, LINE_STRONG));
    widgets::kbd_label(ui, r.translate(vec2(0.0, -1.0)), text, FS_SM, TEXT);
}

fn lib_image(ui: &Ui, r: Rect, fx: &LibModel) {
    ui.painter().rect_filled(r, radius(R_SM), INK);
    let uri = format!("bytes://art/{}/{}.svg", fx.m, fx.n);
    let img = nitor_ui::icons::svg(uri, crate::art::svg(fx));
    // 240 × 96 у центрі
    let scale = (r.width() / 240.0).min(r.height() / 96.0);
    let size = vec2(240.0 * scale, 96.0 * scale);
    img.paint_at(ui, Rect::from_center_size(r.center(), size));
}

fn color_chips(ui: &mut Ui, fx: &LibModel) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(S1, S1);
        for (_, hex, name) in crate::art::colors(fx) {
            let tw = widgets::text_width(ui, name, font(FS_XS));
            let (r, _) = ui.allocate_exact_size(vec2(tw + 8.0 + S1 + 2.0 * S2, 18.0), Sense::hover());
            let p = ui.painter();
            p.rect_stroke(r, egui::CornerRadius::same(255), Stroke::new(1.0_f32, LINE_STRONG), StrokeKind::Inside);
            let c = nitor_core::Rgb::from_hex(hex).unwrap();
            p.circle_filled(pos2(r.left() + S2 + 4.0, r.center().y), 4.0, Color32::from_rgb(c.0, c.1, c.2));
            p.text(pos2(r.left() + S2 + 8.0 + S1, r.center().y), Align2::LEFT_CENTER, name, font(FS_XS), MUTED);
        }
    });
}

/// Картка моделі: клік по будь-якій її частині відкриває вибір режиму.
fn lib_card(ui: &mut Ui, r: Rect, fx: &LibModel) -> egui::Response {
    let resp = ui
        .interact(r, Id::new(("lib-card", &fx.m, &fx.n)), Sense::click())
        .on_hover_text(format!("{} — вибрати режим", library::title(&fx.m, &fx.n)));
    // картка з дією піднімається на 2 px, рамка темнішає на крок
    let (h, _) = nitor_ui::anim::hover_press(ui.ctx(), resp.id, resp.hovered(), false);
    let r = nitor_ui::anim::lift_rect(r, h, 0.0, 2.0);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, library::title(&fx.m, &fx.n)));
    widgets::focus_ring(ui, &resp, r, R_LG);
    ui.painter().rect(
        r,
        radius(R_LG),
        RAISED,
        Stroke::new(1.0_f32, LINE.lerp_to_gamma(LINE_STRONG, h)),
        StrokeKind::Inside,
    );
    let inner = r.shrink(S3);
    let img = Rect::from_min_size(inner.min, vec2(inner.width(), 96.0));
    lib_image(ui, img, fx);
    let mut y = img.bottom() + S2;
    let p = ui.painter();
    p.text(pos2(inner.left(), y), Align2::LEFT_TOP, library::maker(&fx.m), font(FS_SM), MUTED);
    y += 18.0 + S2;
    let g = ui.fonts(|f| {
        let mut job = egui::text::LayoutJob::simple(fx.n.clone(), bold(FS_MD), TEXT, inner.width());
        job.wrap.max_rows = 2;
        f.layout_job(job)
    });
    let gh = g.size().y;
    p.galley(pos2(inner.left(), y), g, TEXT);
    y += gh.max(18.0) + S2;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(inner.left(), y), inner.max)));
    color_chips(&mut child, fx);
    let modes_y = inner.bottom() - 16.0;
    ui.painter().text(
        pos2(inner.left(), modes_y),
        Align2::LEFT_TOP,
        crate::art::modes_text(fx.modes.len()),
        font(FS_SM),
        MUTED,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_search() {
        assert!(tab_matches(Tab::Output, ""));
        assert!(tab_matches(Tab::Output, " ART-NET "));
        assert!(!tab_matches(Tab::Output, "автозапуск"));
        assert!(tab_matches(Tab::General, "автозапуск"));
        // назви дій гарячих клавіш ведуть на вкладку клавіш
        assert!(tab_matches(Tab::Keys, "інвертувати"));
    }
}
