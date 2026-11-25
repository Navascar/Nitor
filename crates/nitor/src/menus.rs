//! Контекстні меню. Вміст залежить від того, де клікнули: прилад, колір палітри,
//! ефекти, сцени, поле вводу або порожнє місце.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use nitor_core::Command;
use nitor_core::engine::ColorValues;

use crate::app::{Act, App, TextOp, c32};
use crate::hotkeys::short_key;
use nitor_ui::icons;
use nitor_ui::theme::*;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Item {
    Title(String),
    Sep,
    /// кольори палітри: клік — застосувати до `ids`
    Colors(Vec<String>),
    Entry {
        icon: Option<&'static str>,
        label: String,
        key: Option<String>,
        danger: bool,
        disabled: bool,
        act: Act,
    },
}

/// Пункт вибору: позначений галочкою, якщо вибраний.
fn pick(on: bool, label: impl Into<String>, act: Act) -> Item {
    Item::Entry { icon: on.then_some("check"), label: label.into(), key: None, danger: false, disabled: false, act }
}

/// Меню знака «?»: сховати пояснення.
pub fn help_menu() -> Vec<Item> {
    vec![Item::Title("Підказки «?»".into()), item("help", "Сховати підказки", Act::HideHelp)]
}

fn item(icon: &'static str, label: impl Into<String>, act: Act) -> Item {
    Item::Entry { icon: Some(icon), label: label.into(), key: None, danger: false, disabled: false, act }
}

trait ItemExt {
    fn key(self, k: Option<String>) -> Self;
    fn danger(self) -> Self;
    fn disabled(self, d: bool) -> Self;
}

impl ItemExt for Item {
    fn key(mut self, k: Option<String>) -> Self {
        if let Item::Entry { key, .. } = &mut self {
            *key = k;
        }
        self
    }
    fn danger(mut self) -> Self {
        if let Item::Entry { danger, .. } = &mut self {
            *danger = true;
        }
        self
    }
    fn disabled(mut self, d: bool) -> Self {
        if let Item::Entry { disabled, .. } = &mut self {
            *disabled = d;
        }
        self
    }
}

pub struct MenuState {
    pub pos: Pos2,
    pub items: Vec<Item>,
    pub opened: f64,
}

impl App {
    /// Запит меню з елемента інтерфейсу; найглибший елемент виграє (його обробник — першим).
    pub fn want_menu(&mut self, items: Vec<Item>) {
        // у режимі оператора контекстних меню немає: через них можна редагувати
        if self.pending_menu.is_none() && !self.v.operator {
            self.pending_menu = Some(items);
        }
    }

    /// Меню «Очистити», «Спалаху» чи Blackout: плавність (частота) і показ назви.
    pub fn light_button_menu(&self, id: &'static str, label: &str, rect: Rect) -> Vec<Item> {
        let p = &self.v.prefs;
        let mut m = vec![];
        if id == "flash" {
            m.push(Item::Title("Спалах".into()));
            let presets = [(0.0, "Постійне світло"), (5.0, "Строб 5 Гц"), (10.0, "Строб 10 Гц"), (15.0, "Строб 15 Гц")];
            for (v, l) in presets {
                m.push(pick((p.flash_rate - v).abs() < 0.5, l, Act::LightPref(id, v)));
            }
            let other = presets.iter().all(|(v, _)| (p.flash_rate - v).abs() >= 0.5);
            let l = if other {
                format!("Інша частота: {} Гц…", fmt_num(p.flash_rate.round()))
            } else {
                "Інша частота…".into()
            };
            m.push(pick(other, l, Act::FlashConfig(rect)));
        } else {
            m.push(Item::Title(format!("{label}: плавність")));
            let cur = if id == "clear" { p.clear_fade } else { p.blackout_fade };
            for (v, l) in [(0.0, "Одразу"), (0.5, "0,5 с"), (1.0, "1 с"), (2.0, "2 с"), (5.0, "5 с")] {
                m.push(pick((cur - v).abs() < 0.01, l, Act::LightPref(id, v)));
            }
        }
        m.push(Item::Sep);
        let short = p.short_buttons.iter().any(|b| b == id);
        m.push(item("edit", if short { "Показувати назву" } else { "Згорнути назву" }, Act::ToggleShortButton(id)));
        m
    }

    fn label_sel(&self) -> String {
        self.sel_description()
    }

    pub fn fixture_menu(&mut self, id: &str) -> Vec<Item> {
        let (fid, seg) = match id.split_once('#') {
            Some((f, k)) => (f.to_string(), k.parse::<usize>().ok()),
            None => (id.to_string(), None),
        };
        let Some(f) = self.v.fixture(&fid).cloned() else { return vec![] };
        // правий клік по невибраному елементу вибирає лише його (як у провіднику)
        if !self.sel.iter().any(|s| s == id) {
            self.select_only(id);
        }
        let ids = self.sel.clone();
        let items = self.sel_items();
        let h = self.v.hotkeys.clone();
        let mut whole: Vec<String> = vec![];
        for i in items.iter().filter(|i| i.seg.is_none()) {
            if !whole.contains(&i.id) {
                whole.push(i.id.clone());
            }
        }
        let in_sel = self.sel_fixtures();
        let running = self.v.effect_running();
        let in_effect = running && in_sel.iter().any(|x| self.v.show.effect.ids.contains(x));
        let eff_name = self.eff.kind.label();
        let head = if items.len() > 1 {
            format!("Вибрано: {}", self.label_sel())
        } else if let Some(k) = seg {
            format!("{} · сегмент {}", f.name, k + 1)
        } else {
            f.name.clone()
        };
        let mut m = vec![
            Item::Title(head),
            Item::Colors(ids.clone()),
            item("eraser", "Очистити колір", Act::Cmd(Command::Clear { ids: ids.clone() })).key(short_key(&h, "clear")),
            Item::Sep,
            item("play", format!("Запустити ефект «{eff_name}»"), Act::RunEffect(in_sel.clone())),
        ];
        if in_effect {
            m.push(item("stop", "Зупинити ефект", Act::StopEffectOn(in_sel.clone())));
        }
        m.push(Item::Sep);
        if seg.is_some() && items.len() == 1 {
            m.push(item("selectAll", "Вибрати весь прилад", Act::SelectWhole(fid.clone())));
        }
        m.push(item("list", "Вибрати всі такі самі прилади", Act::SelectSameProfile(f.profile.clone())));
        if self.v.preset_fixtures.iter().any(|x| in_sel.contains(x)) {
            m.push(item("grid", "Пресети", Act::OpenPresets));
        }
        if in_sel.len() == 1 {
            m.push(item("settings", "Властивості…", Act::OpenProps(fid.clone())).key(Some("2×клік".into())));
            m.push(item("copy", "Дублювати", Act::Duplicate(fid.clone())));
        }
        if !whole.is_empty() {
            m.push(Item::Sep);
            let label = if whole.len() > 1 {
                format!("Видалити прилади ({})", whole.len())
            } else {
                "Видалити прилад".into()
            };
            m.push(
                item("trash", label, Act::AskDeleteFixtures { ids: whole, close_props: false })
                    .danger()
                    .key(short_key(&h, "deleteFixtures")),
            );
        }
        m
    }

    pub fn color_menu(&self) -> Vec<Item> {
        let has = !self.sel.is_empty();
        let ids = self.sel.clone();
        let cur = self.first_sel_color();
        let mut m = vec![Item::Title(if has {
            format!("Колір · вибрано {}", self.label_sel())
        } else {
            "Колір · нічого не вибрано".into()
        })];
        if has {
            m.push(Item::Colors(ids.clone()));
        }
        m.push(item("copy", "Копіювати колір", Act::CopyColor).disabled(cur.is_none()));
        let paste = match self.color_clipboard {
            Some((h, _)) => format!("Вставити колір {}", h.hex()),
            None => "Вставити колір".into(),
        };
        m.push(item("clipboard", paste, Act::PasteColor).disabled(self.color_clipboard.is_none() || !has));
        m.push(Item::Sep);
        let dim = |v: u8| {
            Act::Cmd(Command::SetColor {
                ids: ids.clone(),
                values: ColorValues { dimmer: Some(v), ..Default::default() },
            })
        };
        m.push(item("sun", "Яскравість 100%", dim(255)).disabled(!has));
        m.push(item("half", "Яскравість 50%", dim(128)).disabled(!has));
        m.push(item("plus", "Зберегти колір у палітру", Act::SaveColor).disabled(cur.is_none()));
        m.push(Item::Sep);
        m.push(
            item("eraser", if has { "Очистити вибрані" } else { "Очистити все" }, Act::Clear)
                .key(short_key(&self.v.hotkeys, "clear")),
        );
        m
    }

    pub fn static_menu(&self, i: usize) -> Vec<Item> {
        let ids = self.sel.clone();
        let Some(c) = self.v.show.static_colors.get(i) else { return vec![] };
        let label = if self.sel.is_empty() {
            "Застосувати до всіх"
        } else {
            "Застосувати до вибраних"
        };
        let mut m = vec![
            Item::Title(format!("Колір · {}", c.name)),
            item("dot", label, Act::StaticTo { index: i, ids: ids.clone() })
                .key(short_key(&self.v.hotkeys, &format!("static{}", i + 1))),
        ];
        if !self.sel.is_empty() {
            m.push(item("circle", "Застосувати до всіх приладів", Act::StaticTo { index: i, ids: vec![] }));
        }
        m.push(Item::Sep);
        m.push(
            item("plus", "Зберегти колір вибраного приладу", Act::SaveColor).disabled(self.first_sel_color().is_none()),
        );
        // стандартні кольори лишаються; видаляються лише збережені користувачем
        if i >= nitor_core::engine::default_static_colors().len() {
            m.push(item("trash", "Видалити колір з палітри", Act::AskDeleteColor(i)).danger());
        }
        m.push(Item::Sep);
        m.push(
            item(
                "eraser",
                if self.sel.is_empty() {
                    "Очистити всі прилади"
                } else {
                    "Очистити вибрані прилади"
                },
                Act::Clear,
            )
            .key(short_key(&self.v.hotkeys, "clear")),
        );
        m
    }

    pub fn effect_menu(&self) -> Vec<Item> {
        let running = self.v.effect_running();
        let e = &self.v.show.effect;
        let eff_name = self.eff.kind.label();
        let picked = self.sel_fixtures();
        let mut m = vec![Item::Title(if running {
            format!("Ефект · {} ({} прил.)", e.kind.label(), e.ids.len())
        } else {
            "Ефекти".into()
        })];
        if !picked.is_empty() {
            m.push(item("play", format!("«{eff_name}» на вибраних"), Act::RunEffectSelected));
        }
        let all: Vec<String> = self.v.show.fixtures.iter().map(|f| f.id.clone()).collect();
        m.push(
            item("play", format!("«{eff_name}» на всіх приладах"), Act::RunEffect(all))
                .disabled(self.v.show.fixtures.is_empty()),
        );
        if running {
            m.push(item("stop", "Зупинити ефект", Act::StopEffect));
        }
        m.push(Item::Sep);
        m.push(item("plus", "Додати поточний колір у палітру", Act::AddPaletteColor));
        if !self.eff.colors.is_empty() {
            m.push(item("eraser", "Очистити палітру", Act::ClearPalette));
        }
        m
    }

    pub fn scene_menu(&self, name: Option<&str>) -> Vec<Item> {
        let fade = self.scene_fade;
        let mut m = vec![Item::Title(match name {
            Some(n) => format!("Сцена · {n}"),
            None => "Сцени".into(),
        })];
        if let Some(n) = name {
            let label = if fade > 0.0 {
                format!("Викликати (перехід {} с)", fmt_num(fade))
            } else {
                "Викликати".into()
            };
            m.push(item("play", label, Act::RecallScene { name: n.into(), fade }));
            if fade > 0.0 {
                m.push(item("bolt", "Викликати миттєво", Act::RecallScene { name: n.into(), fade: 0.0 }));
            }
            m.push(item("refresh", "Перезаписати поточним станом", Act::AskOverwriteScene(n.into())));
            m.push(item("download", "Зберегти у файл…", Act::SaveSceneFile(n.into())));
            let file = self.v.show.scenes.get(n).is_some_and(|s| s.file.is_some());
            m.push(item("folder", "Показати файл у папці", Act::RevealSceneFile(n.into())).disabled(!file));
            m.push(item("trash", "Видалити сцену", Act::AskDeleteScene(n.into())).danger());
            m.push(Item::Sep);
        }
        m.push(item("plus", "Зберегти нову сцену…", Act::AskNewScene));
        m.push(item("upload", "Відкрити сцену з файлу…", Act::OpenSceneFile));
        m
    }

    pub fn group_menu(&self, name: &str) -> Vec<Item> {
        vec![
            Item::Title(format!("Група · {name}")),
            item("refresh", "Замінити вибраними приладами", Act::SaveGroup(name.into())).disabled(self.sel.is_empty()),
            item("edit", "Перейменувати…", Act::AskRenameGroup(name.into())),
            item("trash", "Видалити групу", Act::DeleteGroup(name.into())).danger(),
            Item::Sep,
            item("plus", "Нова група з вибраних…", Act::AskNewGroup).disabled(self.sel.is_empty()),
        ]
    }

    /// Меню для текстових полів: вирізати / копіювати / вставити / виділити все.
    pub fn input_menu(&self, id: Id) -> Vec<Item> {
        vec![
            item("scissors", "Вирізати", Act::TextOp(id, TextOp::Cut)).key(Some("Ctrl+X".into())),
            item("copy", "Копіювати", Act::TextOp(id, TextOp::Copy)).key(Some("Ctrl+C".into())),
            item("clipboard", "Вставити", Act::TextOp(id, TextOp::Paste)).key(Some("Ctrl+V".into())),
            Item::Sep,
            item("selectAll", "Виділити все", Act::TextOp(id, TextOp::SelectAll)).key(Some("Ctrl+A".into())),
        ]
    }

    pub fn fixtures_panel_menu(&self) -> Vec<Item> {
        let h = &self.v.hotkeys;
        let mut m = vec![Item::Title("Прилади".into()), item("plus", "Додати прилад…", Act::OpenAdd)];
        if !self.v.show.fixtures.is_empty() {
            m.push(item("selectAll", "Вибрати всі прилади", Act::SelectAll).key(short_key(h, "selectAll")));
        }
        if !self.sel.is_empty() {
            m.push(item("circle", "Зняти вибір", Act::SelectNone).key(short_key(h, "selectNone")));
        }
        if !self.v.show.fixtures.is_empty() {
            m.push(item("swap", "Інвертувати вибір", Act::SelectInvert));
        }
        m.push(Item::Sep);
        m.push(
            item(
                "eraser",
                if self.sel.is_empty() {
                    "Очистити всі кольори"
                } else {
                    "Очистити вибрані"
                },
                Act::Clear,
            )
            .key(short_key(h, "clear")),
        );
        m.push(item("layers", "Патч приладів…", Act::OpenSettings(Some(crate::dialogs::Tab::Patch))));
        m
    }

    pub fn general_menu(&self) -> Vec<Item> {
        let h = &self.v.hotkeys;
        let u = self.updater.info();
        let name = |n: Option<&str>| n.map(nitor_core::engine::Command::undo_label);
        let mut m = vec![
            item("undo", name(self.v.undo).map_or("Скасувати".into(), |n| format!("Скасувати {n}")), Act::Undo)
                .key(short_key(h, "undo"))
                .disabled(self.v.undo.is_none()),
            item("redo", name(self.v.redo).map_or("Повторити".into(), |n| format!("Повторити {n}")), Act::Redo)
                .key(short_key(h, "redo"))
                .disabled(self.v.redo.is_none()),
            Item::Sep,
            item("plus", "Додати прилад…", Act::OpenAdd),
        ];
        if !self.v.show.fixtures.is_empty() {
            m.push(item("selectAll", "Вибрати всі прилади", Act::SelectAll).key(short_key(h, "selectAll")));
        }
        if !self.sel.is_empty() {
            m.push(item("circle", "Зняти вибір", Act::SelectNone).key(short_key(h, "selectNone")));
        }
        m.push(Item::Sep);
        m.push(item("eraser", "Очистити всі кольори", Act::Cmd(Command::Clear { ids: vec![] })));
        if self.v.effect_running() {
            m.push(item("stop", "Зупинити всі ефекти", Act::StopEffect));
        }
        m.push(
            item("bulbOff", if self.v.show.blackout { "Вимкнути Blackout" } else { "Blackout" }, Act::Blackout)
                .key(short_key(h, "blackout")),
        );
        m.push(Item::Sep);
        if u.available && u.blocked.is_none() {
            m.push(item("download", format!("Оновити до {}…", u.latest), Act::OpenUpdate));
        } else {
            m.push(item("refresh", "Перевірити оновлення", Act::CheckUpdate));
        }
        m.push(item("settings", "Налаштування", Act::OpenSettings(None)));
        m
    }

    /// Показ відкритого меню. Клік поза меню, Esc або вибір пункту — закрити.
    pub fn show_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.menu.take() else { return };
        let mut chosen: Option<Act> = None;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let area = egui::Area::new(Id::new("ctx-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0_f32, LINE_STRONG))
                    .corner_radius(radius(R_LG))
                    .inner_margin(egui::Margin::same(S1 as i8))
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: rgba(0, 0, 0, 0.45) })
                    .show(ui, |ui| {
                        ui.set_min_width(230.0);
                        ui.set_max_width(320.0);
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let w = 252.0f32.max(ui.min_rect().width());
                        for it in &menu.items {
                            match it {
                                Item::Title(t) => {
                                    let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
                                    let mut job =
                                        egui::text::LayoutJob::simple_singleline(t.clone(), font(FS_SM), MUTED);
                                    job.wrap = egui::text::TextWrapping::truncate_at_width(w - 2.0 * S2);
                                    let g = ui.fonts(|f| f.layout_job(job));
                                    ui.painter().galley(pos2(r.left() + S2, r.center().y - g.size().y / 2.0), g, MUTED);
                                }
                                Item::Sep => {
                                    let (r, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
                                    ui.painter().hline(
                                        r.left() + S2..=r.right() - S2,
                                        r.center().y,
                                        Stroke::new(1.0_f32, LINE),
                                    );
                                }
                                Item::Colors(ids) => {
                                    let colors = self.v.show.static_colors.clone();
                                    let cols = 7usize;
                                    let gap = S1;
                                    let cw = (w - 2.0 * S2 - gap * (cols as f32 - 1.0)) / cols as f32;
                                    let rows = colors.len().div_ceil(cols);
                                    let (r, _) = ui
                                        .allocate_exact_size(vec2(w, rows as f32 * (22.0 + gap) + S2), Sense::hover());
                                    for (i, c) in colors.iter().enumerate() {
                                        let x = r.left() + S2 + (i % cols) as f32 * (cw + gap);
                                        let y = r.top() + S1 + (i / cols) as f32 * (22.0 + gap);
                                        let cr = Rect::from_min_size(pos2(x, y), vec2(cw, 22.0));
                                        let resp = ui
                                            .interact(cr, Id::new(("menu-chip", i)), Sense::click())
                                            .on_hover_text(&c.name);
                                        paint_static_chip(ui, cr, c.hex, c.w.is_some_and(|w| w > 0), R_SM);
                                        ui.painter().rect_stroke(
                                            cr,
                                            radius(R_SM),
                                            Stroke::new(1.0_f32, rgba(0, 0, 0, 0.45)),
                                            StrokeKind::Inside,
                                        );
                                        if resp.hovered() {
                                            ui.painter().rect_stroke(
                                                cr.expand(1.0),
                                                radius(R_SM),
                                                Stroke::new(2.0_f32, TEXT),
                                                StrokeKind::Outside,
                                            );
                                        }
                                        if resp.clicked() {
                                            chosen = Some(Act::StaticTo { index: i, ids: ids.clone() });
                                        }
                                    }
                                }
                                Item::Entry { icon, label, key, danger, disabled, act } => {
                                    let sense = if *disabled { Sense::hover() } else { Sense::click() };
                                    let (r, resp) = ui.allocate_exact_size(vec2(w, CONTROL_H_SM), sense);
                                    let hot = resp.hovered() && !*disabled;
                                    let a = if *disabled { 0.45 } else { 1.0 };
                                    if hot {
                                        ui.painter().rect_filled(
                                            r,
                                            radius(R_MD),
                                            if *danger { DANGER_STRONG } else { ACCENT_STRONG },
                                        );
                                    }
                                    let fg = if hot {
                                        WHITE
                                    } else if *danger {
                                        DANGER
                                    } else {
                                        TEXT
                                    };
                                    let ifg = if hot {
                                        WHITE
                                    } else if *danger {
                                        DANGER
                                    } else {
                                        MUTED
                                    };
                                    if let Some(name) = icon {
                                        let ir = Rect::from_center_size(
                                            pos2(r.left() + S2 + 8.0, r.center().y),
                                            vec2(16.0, 16.0),
                                        );
                                        icons::image(name, 16.0, ifg.gamma_multiply(a)).paint_at(ui, ir);
                                    }
                                    let kw = key.as_ref().map_or(0.0, |k| {
                                        ui.fonts(|f| f.layout_no_wrap(k.clone(), font(FS_XS), MUTED).size().x) + S2
                                    });
                                    let mut job = egui::text::LayoutJob::simple_singleline(
                                        label.clone(),
                                        font(FS_MD),
                                        fg.gamma_multiply(a),
                                    );
                                    job.wrap = egui::text::TextWrapping::truncate_at_width(w - 16.0 - 3.0 * S2 - kw);
                                    let g = ui.fonts(|f| f.layout_job(job));
                                    ui.painter().galley(
                                        pos2(r.left() + 2.0 * S2 + 16.0, r.center().y - g.size().y / 2.0),
                                        g,
                                        fg,
                                    );
                                    if let Some(k) = key {
                                        ui.painter().text(
                                            pos2(r.right() - S2, r.center().y),
                                            Align2::RIGHT_CENTER,
                                            k,
                                            font(FS_XS),
                                            if hot { WHITE } else { MUTED }.gamma_multiply(a),
                                        );
                                    }
                                    if resp.clicked() {
                                        chosen = Some(act.clone());
                                    }
                                    if !*disabled {
                                        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                    }
                                }
                            }
                        }
                    });
            });
        let rect = area.response.rect;
        // меню розгортається від точки кліку
        nitor_ui::anim::pop_in(ctx, area.response.layer_id, menu.pos, 0.96);
        let clicked_out = ctx.input(|i| {
            (i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)))
                && i.time - menu.opened > 0.05
        });
        if clicked_out || ctx.input(|i| i.viewport().focused == Some(false)) {
            close = true;
        }
        if let Some(a) = chosen {
            self.act(ctx, a);
        } else if !close {
            self.menu = Some(menu);
        }
    }
}

/// Плашка статичного кольору: з білим каналом — градієнт до білого.
pub fn paint_static_chip(ui: &egui::Ui, r: Rect, hex: nitor_core::Rgb, with_w: bool, rad: u8) {
    let p = ui.painter();
    p.rect_filled(r, radius(rad), c32(hex));
    if with_w {
        // linear-gradient(135deg, колір 55–60%, білий)
        let mut mesh = egui::Mesh::default();
        let tr = r.right_top();
        let bl = r.left_bottom();
        let br = r.right_bottom();
        let mid_a = pos2(r.left() + r.width() * 0.6, r.bottom());
        let mid_b = pos2(r.right(), r.top() + r.height() * 0.4);
        let clear = Color32::from_rgba_unmultiplied(255, 255, 255, 0);
        mesh.colored_vertex(mid_a, clear);
        mesh.colored_vertex(mid_b, clear);
        mesh.colored_vertex(br, Color32::WHITE);
        mesh.add_triangle(0, 1, 2);
        let _ = (tr, bl);
        p.add(egui::Shape::mesh(mesh));
    }
}

pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}
