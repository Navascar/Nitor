//! Елементи керування в стилі дизайн-системи: кнопки, повзунки з заливкою, числа, що
//! вписуються з клавіатури, поля вводу.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, Vec2, WidgetInfo, WidgetText, WidgetType, pos2, vec2,
};

use crate::icons;
use crate::theme::*;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Kind {
    #[default]
    Normal,
    /// основна дія: синя заливка
    Primary,
    /// небезпечна дія: червона рамка
    Danger,
    /// небезпечна дія, увімкнена: червона заливка
    DangerOn,
    /// без рамки
    Ghost,
    /// «Очистити»: помітна, але не основна
    Warn,
    /// «Спалах»: світла рамка, як у «Очистити» й Blackout
    Outline,
    /// «Закрити» вікно: без рамки, при наведенні — червона
    Close,
    /// «Оновити до …»
    Update,
    /// перемикач увімкнено (строб)
    Toggled,
    /// підтвердження «✓ Готово»
    Done,
}

pub struct Btn<'a> {
    text: Option<WidgetText>,
    icon: Option<&'a str>,
    kind: Kind,
    small: bool,
    enabled: bool,
    tooltip: Option<String>,
    min_width: f32,
    full_width: bool,
    bold: bool,
    left_align: bool,
    /// гаряча клавіша дії — показується праворуч у кнопці
    kbd: Option<String>,
    /// підказка лише для екранного диктора, без спливаючого вікна
    quiet: bool,
    /// частка розгорнутої назви (0 — лише значок, 1 — кнопка з назвою): плавне розгортання
    reveal: f32,
    /// заливка зліва (0..1): скільки вже втримано кнопку
    progress: f32,
    /// замість значка — обертовий сегмент (дія триває)
    busy: bool,
    /// секунди від удару: кільце навколо кнопки
    ring: Option<f32>,
    /// кругла (кнопка відтворення: ▶/■)
    round: bool,
}

static SHOW_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Показувати підказки клавіш на кнопках (загальне налаштування програми).
pub fn set_show_keys(on: bool) {
    SHOW_KEYS.store(on, std::sync::atomic::Ordering::Relaxed);
}

static SHOW_HELP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Показувати знаки «?» з поясненнями (загальне налаштування програми).
pub fn set_show_help(on: bool) {
    SHOW_HELP.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Чи показуються знаки «?».
pub fn help_shown() -> bool {
    SHOW_HELP.load(std::sync::atomic::Ordering::Relaxed)
}

/// Позначка в пам'яті egui: на знаку «?» клацнули правою кнопкою — програма показує меню «Сховати пояснення».
pub fn help_menu_id() -> Id {
    Id::new("help-menu")
}

/// Знак «?», що пояснює розділ при наведенні або кліку (пояснення лишається відкритим).
/// Нічого не малює, якщо пояснення вимкнено.
pub fn help(ui: &mut Ui, text: &str) {
    if !SHOW_HELP.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text));
    if resp.secondary_clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(help_menu_id(), true));
    }
    let popup = egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_max_width(360.0);
            ui.label(text);
        });
    if let Some(p) = &popup {
        crate::anim::pop_in(ui.ctx(), p.response.layer_id, r.center(), 0.96);
    }
    let open = popup.is_some();
    let c = if resp.hovered() || open || resp.has_focus() { TEXT } else { MUTED };
    icons::image("help", 16.0, c).paint_at(ui, r);
    focus_ring(ui, &resp, r, 9);
    if !open {
        resp.on_hover_ui(|ui| {
            ui.set_max_width(360.0);
            ui.label(text);
        });
    }
}

pub fn btn<'a>(text: impl Into<WidgetText>) -> Btn<'a> {
    Btn {
        text: Some(text.into()),
        icon: None,
        kind: Kind::Normal,
        small: false,
        enabled: true,
        tooltip: None,
        min_width: 0.0,
        full_width: false,
        bold: false,
        left_align: false,
        kbd: None,
        quiet: false,
        reveal: 1.0,
        progress: 0.0,
        busy: false,
        ring: None,
        round: false,
    }
}

/// Квадратна кнопка-іконка.
pub fn icon_btn(icon: &str) -> Btn<'_> {
    Btn { text: None, icon: Some(icon), ..btn("") }
}

impl<'a> Btn<'a> {
    pub fn icon(mut self, name: &'a str) -> Self {
        self.icon = Some(name);
        self
    }
    pub fn kind(mut self, k: Kind) -> Self {
        self.kind = k;
        self
    }
    pub fn primary(self) -> Self {
        self.kind(Kind::Primary)
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn tip(mut self, t: impl Into<String>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
    /// Назва розгортається з кнопки-значка: 0 — лише значок, 1 — повністю (для анімації).
    pub fn reveal(mut self, t: f32) -> Self {
        self.reveal = t.clamp(0.0, 1.0);
        self
    }
    /// Заливка зліва направо, поки кнопку тримають: 0 — немає, 1 — уся кнопка.
    pub fn progress(mut self, t: f32) -> Self {
        self.progress = t.clamp(0.0, 1.0);
        self
    }
    /// Дія триває: значок замінює обертовий сегмент.
    pub fn busy(mut self, on: bool) -> Self {
        self.busy = on;
        self
    }
    /// Кільце, що розходиться від кнопки: `age` — секунди від удару.
    pub fn ring(mut self, age: Option<f32>) -> Self {
        self.ring = age;
        self
    }
    /// Підказка не спливає (назва кнопки показується інакше), але лишається для екранного диктора.
    pub fn quiet_tip(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn left(mut self) -> Self {
        self.left_align = true;
        self
    }
    /// Кругла кнопка-значок: так позначається відтворення (▶/■), щоб не плутати з іншими діями.
    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }
    /// Підказка гарячої клавіші в кнопці («F», «Ctrl+A»); не показується, якщо підказки вимкнено.
    pub fn key(mut self, k: Option<String>) -> Self {
        self.kbd = k.filter(|k| !k.is_empty() && SHOW_KEYS.load(std::sync::atomic::Ordering::Relaxed));
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let h = if self.small { CONTROL_H_SM } else { CONTROL_H };
        let pad = if self.small {
            S2
        } else if matches!(self.kind, Kind::Warn | Kind::Outline) {
            S4
        } else {
            S3
        };
        let isz = if self.small { 16.0 } else { 18.0 };
        let gap = if self.small { S1 } else { S2 };
        let fs = if self.small { FS_SM } else { FS_MD };
        let font = if self.bold || matches!(self.kind, Kind::Warn | Kind::Update | Kind::Outline) {
            bold(fs)
        } else {
            font(fs)
        };
        let galley = self
            .text
            .as_ref()
            .filter(|t| !t.text().is_empty())
            .map(|t| t.clone().into_galley(ui, Some(egui::TextWrapMode::Truncate), f32::INFINITY, font.clone()));
        let kbd = self.kbd.as_ref().map(|k| kbd_width(ui, k, FS_SM));
        let kbd_w = kbd.map_or(0.0, |w| w + gap);
        let mut w = match (&galley, self.icon) {
            (None, Some(_)) if kbd.is_some() => pad * 2.0 + isz + kbd_w,
            (None, Some(_)) => h,
            (Some(g), Some(_)) => pad * 2.0 + isz + gap + g.size().x + kbd_w,
            (Some(g), None) => pad * 2.0 + g.size().x + kbd_w,
            (None, None) => h,
        };
        // розгортання назви: ширина — між кнопкою-значком і повною, значок їде від центру до краю
        let icon_w = if kbd.is_some() { pad * 2.0 + isz + kbd_w } else { h };
        let revealing = self.reveal < 1.0 && galley.is_some() && self.icon.is_some();
        if revealing {
            w = icon_w + (w - icon_w) * self.reveal;
        }
        w = w.max(self.min_width);
        if self.full_width {
            w = w.max(ui.available_width());
        }
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), sense);
        // назва для екранного диктора: текст кнопки, а в кнопки-іконки — підказка
        let name = match (&self.text, &self.tooltip) {
            (Some(t), _) if !t.text().is_empty() => t.text().to_string(),
            (_, Some(tip)) => tip.clone(),
            _ => String::new(),
        };
        let enabled = self.enabled;
        let toggled = matches!(self.kind, Kind::Toggled | Kind::DangerOn);
        resp.widget_info(|| {
            if toggled {
                WidgetInfo::selected(WidgetType::Button, enabled, true, &name)
            } else {
                WidgetInfo::labeled(WidgetType::Button, enabled, &name)
            }
        });
        let hovered = resp.hovered() && self.enabled;
        let (h, pr) =
            crate::anim::hover_press(ui.ctx(), resp.id, hovered, resp.is_pointer_button_down_on() && self.enabled);
        let lerp3 = |a: (Color32, Color32, Color32), b: (Color32, Color32, Color32), t: f32| {
            (a.0.lerp_to_gamma(b.0, t), a.1.lerp_to_gamma(b.1, t), a.2.lerp_to_gamma(b.2, t))
        };
        let (mut fill, mut stroke, mut fg) = lerp3(colors(self.kind, false), colors(self.kind, true), h);
        // наведення піднімає кнопку на 1 px, натискання опускає; місце в розкладці не змінюється
        let rect = crate::anim::lift_rect(rect, h, pr, 1.0);
        // «Готово» проявляється плавно з попереднього вигляду кнопки, галочка виростає без відскоку
        let done = self.kind == Kind::Done;
        let kind_id = resp.id.with("kind");
        if !done {
            ui.data_mut(|d| d.insert_temp(kind_id, self.kind));
        }
        let dt = crate::anim::anim_bool(ui.ctx(), resp.id.with("done"), done, 0.2);
        if dt > 0.0 && dt < 1.0 {
            let base = ui.data(|d| d.get_temp::<Kind>(kind_id)).unwrap_or_default();
            let (bf, bs, bg) = lerp3(colors(base, false), colors(base, true), h);
            let (df, ds, dg) = lerp3(colors(Kind::Done, false), colors(Kind::Done, true), h);
            (fill, stroke, fg) = (bf.lerp_to_gamma(df, dt), bs.lerp_to_gamma(ds, dt), bg.lerp_to_gamma(dg, dt));
        }
        let pop = if done && dt < 1.0 { 0.6 + 0.4 * crate::anim::ease_out(dt) } else { 1.0 };
        let rr = if self.round { 255 } else { R_MD };
        if ui.is_rect_visible(rect) {
            let a = if self.enabled { 1.0 } else { 0.45 };
            let p = ui.painter();
            p.rect(
                rect,
                radius(rr),
                fill.gamma_multiply(a),
                Stroke::new(1.0_f32, stroke.gamma_multiply(a)),
                StrokeKind::Inside,
            );
            if self.progress > 0.0 {
                let clip =
                    Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * self.progress, rect.bottom()));
                p.with_clip_rect(clip.intersect(p.clip_rect())).rect_filled(rect, radius(rr), alpha(ACCENT, 0.35));
            }
            if let Some(age) = self.ring {
                crate::anim::ring(ui, rect, age, ACCENT, rr as f32);
            }
            if resp.has_focus() {
                p.rect_stroke(
                    rect.expand(2.0),
                    radius(rr.saturating_add(2)),
                    Stroke::new(2.0_f32, ACCENT),
                    StrokeKind::Outside,
                );
            }
            let fg = fg.gamma_multiply(a);
            let content_w = match (&galley, self.icon) {
                (Some(g), Some(_)) => isz + gap + g.size().x,
                (Some(g), None) => g.size().x,
                _ => isz,
            } + kbd_w;
            let mut x = if self.left_align { rect.left() + pad } else { rect.center().x - content_w / 2.0 };
            x = x.max(rect.left() + if galley.is_some() || kbd.is_some() { pad } else { 0.0 });
            if revealing {
                let from = (icon_w - isz - kbd_w) / 2.0;
                x = rect.left() + from + (pad - from) * self.reveal;
            }
            // клавіша — праворуч: рамка-клавіша, як на клавіатурі
            let text_right = rect.right() - pad - kbd_w;
            if let (Some(kw), Some(k)) = (kbd, &self.kbd) {
                let right = (x + content_w).min(rect.right() - pad);
                let kr =
                    Rect::from_min_max(pos2(right - kw, rect.center().y - 9.0), pos2(right, rect.center().y + 9.0));
                p.rect_stroke(
                    kr,
                    CornerRadius::same(4),
                    Stroke::new(1.0_f32, fg.gamma_multiply(0.45)),
                    StrokeKind::Inside,
                );
                kbd_label(ui, kr, k, FS_SM, fg.gamma_multiply(0.85));
            }
            if let Some(name) = self.icon {
                let r = Rect::from_min_size(pos2(x, rect.center().y - isz / 2.0), Vec2::splat(isz));
                if self.busy {
                    crate::anim::spinner(ui, r, fg);
                } else {
                    icons::image(name, isz, fg).paint_at(ui, Rect::from_center_size(r.center(), r.size() * pop));
                }
                x += isz + gap;
            }
            if let Some(g) = galley {
                let max_w = text_right - x;
                let pos = pos2(x, rect.center().y - g.size().y / 2.0);
                if revealing {
                    // назва проявляється з-під краю кнопки, без трикрапки
                    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(text_right, rect.bottom()));
                    let fg = fg.gamma_multiply(self.reveal);
                    p.with_clip_rect(clip.intersect(p.clip_rect())).galley(pos, g, fg);
                } else if g.size().x > max_w + 0.5 {
                    // довгий текст: обрізаємо з трикрапкою
                    let text = self.text.as_ref().map(|t| t.text().to_string()).unwrap_or_default();
                    let mut job = egui::text::LayoutJob::simple_singleline(text, font.clone(), fg);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(10.0));
                    let g2 = ui.fonts(|f| f.layout_job(job));
                    p.galley(pos, g2, fg);
                } else {
                    p.galley(pos, g, fg);
                }
            }
        }
        match self.tooltip {
            Some(t) if !t.is_empty() && !self.quiet => resp.on_hover_text(t),
            _ => resp,
        }
    }
}

fn colors(kind: Kind, hovered: bool) -> (Color32, Color32, Color32) {
    match kind {
        Kind::Normal => (RAISED, if hovered { LINE_STRONG } else { LINE }, TEXT),
        Kind::Primary => {
            let f = if hovered { ACCENT_HOVER } else { ACCENT_STRONG };
            (f, ACCENT_STRONG, WHITE)
        }
        Kind::Danger => (if hovered { alpha(DANGER, 0.12) } else { Color32::TRANSPARENT }, DANGER, DANGER),
        Kind::DangerOn => (DANGER_STRONG, DANGER_STRONG, WHITE),
        Kind::Ghost => (if hovered { RAISED } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, TEXT),
        Kind::Warn => (if hovered { alpha(WARN, 0.12) } else { Color32::TRANSPARENT }, WARN, WARN),
        Kind::Outline => (if hovered { alpha(TEXT, 0.12) } else { Color32::TRANSPARENT }, TEXT, TEXT),
        Kind::Close if hovered => (DANGER_STRONG, DANGER_STRONG, WHITE),
        Kind::Close => (Color32::TRANSPARENT, Color32::TRANSPARENT, TEXT),
        // текст світлий: синій на ACCENT_SOFT поверх SURFACE — лише 4.46:1
        Kind::Update => (ACCENT_SOFT, ACCENT, TEXT),
        Kind::Toggled => (ACCENT_SOFT, ACCENT, TEXT),
        Kind::Done => (Color32::TRANSPARENT, OK, OK),
    }
}

/// Клавіші, які показуються значком: у шрифтах Windows може не бути символів «⌫» і «↵».
fn kbd_icon(key: &str) -> Option<&'static str> {
    match key {
        "Backspace" => Some("backspace"),
        "Enter" => Some("enter"),
        _ => None,
    }
}

/// Ширина підпису клавіші разом із рамкою.
pub fn kbd_width(ui: &Ui, key: &str, size: f32) -> f32 {
    let w = match kbd_icon(key) {
        Some(_) => size + 3.0,
        None => ui.fonts(|f| f.layout_no_wrap(key.to_string(), mono(size), TEXT).size().x),
    };
    (w + 2.0 * S1).max(16.0)
}

/// Підпис клавіші по центру рамки `r`: за самими літерами, а не за висотою рядка шрифту
/// (у Cascadia й Consolas рядок вищий за літери, і текст здавався піднятим).
pub fn kbd_label(ui: &Ui, r: Rect, key: &str, size: f32, color: Color32) {
    if let Some(name) = kbd_icon(key) {
        let s = size + 3.0;
        icons::image(name, s, color).paint_at(ui, Rect::from_center_size(r.center(), Vec2::splat(s)));
        return;
    }
    let g = ui.fonts(|f| f.layout_no_wrap(key.to_string(), mono(size), color));
    let ink = if g.mesh_bounds.is_positive() { g.mesh_bounds } else { g.rect };
    ui.painter().galley(r.center() - ink.center().to_vec2(), g, color);
}

/// Повзунок із заливкою доріжки кольором `fill`. Повертає відповідь (змінено — `changed()`).
pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, fill: Color32, width: f32) -> Response {
    slider_named(ui, "", value, min, max, fill, width)
}

/// Повзунок з назвою для екранного диктора.
pub fn slider_named(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    fill: Color32,
    width: f32,
) -> Response {
    slider_impl(ui, name, value, min, max, fill, width, false, false)
}

/// Повзунок з рисками на 25, 50 і 75 %: легше повторити те саме значення.
pub fn slider_ticked(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    fill: Color32,
    width: f32,
) -> Response {
    slider_impl(ui, name, value, min, max, fill, width, false, true)
}

/// Повзунок каналу кольору: доріжка — градієнт від темряви до кольору каналу, бігунець — поточний колір.
pub fn slider_channel(ui: &mut Ui, name: &str, value: &mut f32, max: f32, color: Color32, width: f32) -> Response {
    slider_impl(ui, name, value, 0.0, max, color, width, true, false)
}

#[allow(clippy::too_many_arguments)]
fn slider_impl(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    fill: Color32,
    width: f32,
    grad: bool,
    ticks: bool,
) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width.max(40.0), 18.0), Sense::click_and_drag());
    let thumb = 16.0;
    // мишу тягнуть за край: повзунок трохи розтягується в той бік і стоншується, відпустили — пружинить назад
    let over = match resp.interact_pointer_pos() {
        Some(p) if resp.dragged() && !crate::anim::reduced_motion() => {
            let d = if p.x < rect.left() { p.x - rect.left() } else { (p.x - rect.right()).max(0.0) };
            d.signum() * 10.0 * (1.0 - (-d.abs() / 60.0).exp())
        }
        _ => 0.0,
    };
    let over = ui.ctx().animate_value_with_time(resp.id.with("over"), over, 0.12);
    let squash = (over.abs() / 10.0) * 1.5;
    let (sl, sr) = if over < 0.0 { (over, 0.0) } else { (0.0, over) };
    let track = Rect::from_min_max(
        pos2(rect.left() + sl, rect.center().y - 3.0 + squash),
        pos2(rect.right() + sr, rect.center().y + 3.0 - squash),
    );
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() || resp.drag_started() {
            let t = ((pos.x - rect.left() - thumb / 2.0) / (rect.width() - thumb)).clamp(0.0, 1.0);
            let v = (min + t * (max - min)).round();
            if v != *value {
                *value = v;
                resp.mark_changed();
            }
        }
    }
    if resp.has_focus() {
        let step = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
        let mut v = *value;
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp) {
                v += step;
            }
            if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown) {
                v -= step;
            }
        });
        let v = v.clamp(min, max);
        if v != *value {
            *value = v;
            resp.mark_changed();
        }
    }
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let t = if max > min { (*value - min) / (max - min) } else { 0.0 };
        let cx = rect.left() + thumb / 2.0 + t * (rect.width() - thumb) + over;
        if grad {
            // від темряви до кольору каналу: видно, що дасть кожне положення
            let dark = INK.lerp_to_gamma(fill, 0.12);
            let rr = track.height() / 2.0;
            let mut m = egui::Mesh::default();
            let (l, r) = (track.left() + rr, track.right() - rr);
            m.colored_vertex(pos2(l, track.top()), dark);
            m.colored_vertex(pos2(r, track.top()), fill);
            m.colored_vertex(pos2(l, track.bottom()), dark);
            m.colored_vertex(pos2(r, track.bottom()), fill);
            m.add_triangle(0, 1, 2);
            m.add_triangle(1, 3, 2);
            p.circle_filled(pos2(l, track.center().y), rr, dark);
            p.circle_filled(pos2(r, track.center().y), rr, fill);
            p.add(m);
            // бігунець — колір каналу, як у звичайного повзунка, без обводки
            p.circle_filled(pos2(cx, rect.center().y), thumb / 2.0 - 1.0, fill);
        } else {
            p.rect_filled(track, CornerRadius::same(3), LINE_STRONG);
            if ticks {
                for k in [0.25, 0.5, 0.75] {
                    let x = rect.left() + thumb / 2.0 + k * (rect.width() - thumb);
                    p.vline(x, track.bottom() + 3.0..=track.bottom() + 7.0, Stroke::new(1.0_f32, LINE_FIELD));
                }
            }
            // при максимумі — заливка до самого кінця
            let fill_right = if t >= 1.0 { track.right() } else { cx };
            p.rect_filled(Rect::from_min_max(track.min, pos2(fill_right, track.max.y)), CornerRadius::same(3), fill);
            p.circle_filled(pos2(cx, rect.center().y), thumb / 2.0 - 1.0, fill);
        }
        if resp.has_focus() {
            p.circle_stroke(pos2(cx, rect.center().y), thumb / 2.0 + 2.0, Stroke::new(2.0_f32, ACCENT));
        }
    }
    let v = *value as f64;
    resp.widget_info(|| WidgetInfo::slider(true, v, name));
    link_label(ui, &resp);
    resp
}

/// Як показувати число біля повзунка.
#[derive(Clone, Copy, PartialEq)]
pub enum Shown {
    Raw,
    /// у відсотках від `max`
    Pct,
    /// герци
    Hz,
}

impl Shown {
    /// Одиниця після числа: показується, але не редагується.
    fn unit(self) -> &'static str {
        match self {
            Shown::Raw => "",
            Shown::Pct => "%",
            Shown::Hz => " Гц",
        }
    }

    /// Число без одиниці.
    fn number(self, v: f32, max: f32) -> i64 {
        match self {
            Shown::Pct => (v / max * 100.0).round() as i64,
            _ => v.round() as i64,
        }
    }
}

/// Число біля повзунка: клік відкриває поле, куди можна вписати значення (у відсотках — для
/// значень у %). Поле на місці числа й того самого розміру; одиниця (%) лишається поруч і не редагується.
/// Enter або втрата фокусу — застосувати, Esc — скасувати.
pub fn value_box(ui: &mut Ui, id: Id, value: &mut f32, min: f32, max: f32, shown: Shown, width: f32) -> bool {
    value_box_sized(ui, id, value, min, max, shown, width, FS_SM)
}

/// Те саме число, але заданого розміру шрифту (майстер — крупніше).
#[allow(clippy::too_many_arguments)]
pub fn value_box_sized(
    ui: &mut Ui,
    id: Id,
    value: &mut f32,
    min: f32,
    max: f32,
    shown: Shown,
    width: f32,
    size: f32,
) -> bool {
    let edit_id = id.with("edit");
    let mut editing: Option<String> = ui.data(|d| d.get_temp(edit_id));
    let mut changed = false;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, size + 6.0), Sense::click());
    let unit = shown.unit();
    let unit_w = if unit.is_empty() { 0.0 } else { text_width(ui, unit, font(size)) };
    let right = rect.right() - S1;
    if let Some(text) = editing.as_mut() {
        ui.painter().rect(rect, radius(R_SM), RAISED, Stroke::new(1.0_f32, ACCENT), StrokeKind::Inside);
        ui.painter().text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, unit, font(size), MUTED);
        let field = Rect::from_min_max(pos2(rect.left() + S1, rect.top()), pos2(right - unit_w, rect.bottom()));
        // поле — поверх уже зайнятого місця, курсор розкладки не зсувається
        let r = ui.new_child(egui::UiBuilder::new().max_rect(field)).add_sized(
            field.size(),
            TextEdit::singleline(text)
                .id(edit_id.with("field"))
                .font(font(size))
                .frame(false)
                .margin(egui::Margin::ZERO)
                .horizontal_align(egui::Align::Max)
                .vertical_align(egui::Align::Center)
                .char_limit(6),
        );
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            editing = None;
        } else if r.lost_focus() {
            if let Ok(raw) = text.trim().replace(',', ".").trim_end_matches(unit.trim()).trim().parse::<f32>() {
                let v = match shown {
                    Shown::Pct => (raw / 100.0 * max).round(),
                    _ => raw.round(),
                };
                let v = v.clamp(min, max);
                if v != *value {
                    *value = v;
                    changed = true;
                }
            }
            editing = None;
        }
        ui.data_mut(|d| match &editing {
            Some(t) => d.insert_temp(edit_id, t.clone()),
            None => d.remove::<String>(edit_id),
        });
    } else {
        let num = shown.number(*value, max).to_string();
        if resp.hovered() {
            ui.painter().rect_filled(rect, radius(R_SM), RAISED);
        }
        ui.painter().text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, unit, font(size), MUTED);
        if size > FS_SM {
            // крупне число (майстер) прокручується до нового значення
            crate::anim::rolling_text(
                ui,
                id.with("roll"),
                pos2(right - unit_w, rect.center().y),
                &num,
                font(size),
                TEXT,
            );
        } else {
            ui.painter().text(pos2(right - unit_w, rect.center().y), Align2::RIGHT_CENTER, &num, font(size), TEXT);
        }
        let text = format!("{num}{unit}");
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("{text}, ввести значення")));
        let resp = resp.on_hover_cursor(CursorIcon::Text).on_hover_text("Клікніть, щоб ввести значення");
        if resp.clicked() {
            ui.data_mut(|d| d.insert_temp(edit_id, num));
        }
    }
    changed
}

/// Рядок «підпис · повзунок · число» (сітка 84px · 1fr · 48px).
#[allow(clippy::too_many_arguments)]
pub fn slider_row(
    ui: &mut Ui,
    id: Id,
    label: &str,
    value: &mut f32,
    max: f32,
    fill: Color32,
    shown: Shown,
    label_w: f32,
    grad: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = S2;
        let (r, _) = ui.allocate_exact_size(vec2(label_w, 18.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, font(FS_SM), MUTED);
        let w = ui.available_width() - 48.0 - S2;
        changed |= slider_impl(ui, label, value, 0.0, max, fill, w, grad, false).changed();
        changed |= value_box(ui, id, value, 0.0, max, shown, 48.0);
    });
    changed
}

/// Однорядкове поле вводу на всю ширину з підказкою.
pub fn input(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    input_secret(ui, text, hint, width, false)
}

/// Поле вводу; `secret` — ховає символи (PIN).
pub fn input_secret(ui: &mut Ui, text: &mut String, hint: &str, width: f32, secret: bool) -> Response {
    let r = ui.add_sized(
        vec2(width, CONTROL_H),
        TextEdit::singleline(text)
            .password(secret)
            .hint_text(RichText::new(hint).color(MUTED))
            .margin(vec2(S2, 8.0))
            .vertical_align(egui::Align::Center),
    );
    link_label(ui, &r);
    r
}

/// Ціле число в полі вводу (як `<input type=number>`), з кнопками «−» і «+», якщо поле не вузьке.
pub fn number(ui: &mut Ui, value: &mut i64, min: i64, max: i64, width: f32) -> Response {
    let mut v = *value as f64;
    let r = stepper(ui, &mut v, min as f64, max as f64, 1.0, 0, width);
    *value = (v.round() as i64).clamp(min, max);
    r
}

/// Дробове число: `step` — крок кнопок «−»/«+» і перетягування; `decimals` — знаків після коми.
pub fn number_f(ui: &mut Ui, value: &mut f64, min: f64, max: f64, step: f64, decimals: usize, width: f32) -> Response {
    stepper(ui, value, min, max, step, decimals, width)
}

/// Підпис поля (дрібний, приглушений). Поле одразу під ним отримує цей підпис як назву для диктора.
pub fn field_label(ui: &mut Ui, text: &str) {
    let r = ui.label(RichText::new(text).size(FS_SM).color(MUTED));
    ui.data_mut(|d| d.insert_temp(Id::new("field-label"), (r.id, r.rect)));
}

/// Зв'язує поле з підписом `field_label`, якщо підпис стоїть просто над полем (інакше підпис чужий).
fn link_label(ui: &Ui, resp: &Response) {
    let key = Id::new("field-label");
    let Some((id, r)) = ui.data(|d| d.get_temp::<(Id, Rect)>(key)) else { return };
    ui.data_mut(|d| d.remove::<(Id, Rect)>(key));
    let gap = resp.rect.top() - r.bottom();
    if (-2.0..=24.0).contains(&gap) && r.left() <= resp.rect.right() && resp.rect.left() <= r.right() {
        resp.clone().labelled_by(id);
    }
}

/// Рамка фокусу з клавіатури для власних клікабельних елементів (як у кнопок).
pub fn focus_ring(ui: &Ui, resp: &Response, rect: Rect, r: u8) {
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(r.saturating_add(2)),
            Stroke::new(2.0_f32, ACCENT),
            StrokeKind::Outside,
        );
    }
}

/// Підпис поля зі знаком «?» поруч.
pub fn field_label_help(ui: &mut Ui, text: &str, help_text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = S1;
        field_label(ui, text);
        help(ui, help_text);
    });
}

/// Підпис частини панелі, що згортається: клік по стрілці чи підпису. `open` — поточний стан;
/// повертає новий (програма зберігає його в налаштуваннях).
pub fn fold(ui: &mut Ui, key: &str, text: &str, help_text: Option<&str>, mut open: bool) -> bool {
    let id = Id::new(("fold", key));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = S1;
        let tw = text_width(ui, text, font(FS_SM));
        let (r, resp) = ui.allocate_exact_size(vec2(14.0 + S1 + tw, 18.0), Sense::click());
        if resp.clicked() {
            open = !open;
        }
        resp.widget_info(|| WidgetInfo::selected(WidgetType::CollapsingHeader, true, open, text));
        let c = if resp.hovered() || resp.has_focus() { TEXT } else { MUTED };
        let ic = Rect::from_min_size(pos2(r.left(), r.center().y - 7.0), vec2(14.0, 14.0));
        let t = crate::anim::anim_bool(ui.ctx(), id.with("t"), open, 0.15);
        icons::image("chevronDown", 14.0, c)
            .rotate((t - 1.0) * std::f32::consts::FRAC_PI_2, Vec2::splat(0.5))
            .paint_at(ui, ic);
        ui.painter().text(pos2(ic.right() + S1, r.center().y), Align2::LEFT_CENTER, text, font(FS_SM), c);
        if let Some(h) = help_text {
            help(ui, h);
        }
    });
    open
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(FS_SM).color(MUTED)
}

/// Заголовок панелі (h2).
pub fn h2(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).font(bold(FS_LG)))
}

/// Тонка горизонтальна лінія.
pub fn hline(ui: &mut Ui) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, LINE);
}

/// Чекбокс із підписом.
pub fn check(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(
        vec2(18.0 + S2 + ui.fonts(|f| f.layout_no_wrap(text.into(), font(FS_MD), TEXT).size().x), 22.0),
        Sense::click(),
    );
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    let on = *value;
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, text));
    let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - 8.0), Vec2::splat(16.0));
    // заливка проявляється, а галочка малюється лінією від кута до кінця (зі «Зменшити рух» — одразу)
    let t = crate::anim::anim_bool(ui.ctx(), resp.id.with("check"), on, 0.15);
    let p = ui.painter();
    let border = if resp.hovered() { MUTED } else { LINE_FIELD };
    p.rect(
        b,
        CornerRadius::same(4),
        RAISED.lerp_to_gamma(ACCENT_STRONG, t),
        Stroke::new(1.0_f32, border.lerp_to_gamma(ACCENT_STRONG, t)),
        StrokeKind::Inside,
    );
    if t > 0.0 {
        let pts = [pos2(4.0, 8.5), pos2(7.0, 11.5), pos2(12.5, 5.0)].map(|q| b.min + q.to_vec2());
        let (l1, l2) = (pts[0].distance(pts[1]), pts[1].distance(pts[2]));
        let d = crate::anim::ease_out(t) * (l1 + l2);
        let mut line = vec![pts[0], pts[0] + (pts[1] - pts[0]) * (d / l1).min(1.0)];
        if d > l1 {
            line.push(pts[1] + (pts[2] - pts[1]) * ((d - l1) / l2));
        }
        p.add(egui::Shape::line(line, Stroke::new(2.0_f32, WHITE)));
    }
    if resp.has_focus() {
        p.rect_stroke(b.expand(2.0), CornerRadius::same(6), Stroke::new(2.0_f32, ACCENT), StrokeKind::Outside);
    }
    p.text(pos2(b.right() + S2, rect.center().y), Align2::LEFT_CENTER, text, font(FS_MD), TEXT);
    resp
}

/// Вибір зі списку (як `<select>`). Повертає `true`, якщо значення змінилося.
pub fn select<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    options: &[(T, String)],
    width: f32,
) -> bool {
    let current = options.iter().find(|(v, _)| v == value).map(|(_, l)| l.clone()).unwrap_or_default();
    let mut changed = false;
    // ComboBox обрізає текст за доступною шириною, тож обмежуємо її: довгі назви не розширюють вікно
    ui.allocate_ui(vec2(width, CONTROL_H), |ui| {
        ui.set_max_width(width);
        let c = egui::ComboBox::from_id_salt(id)
            .selected_text(RichText::new(current).size(FS_MD))
            .width(width)
            .truncate()
            .height(360.0)
            .show_ui(ui, |ui| {
                ui.set_min_width(width - 8.0);
                for (v, label) in options {
                    if ui.selectable_label(v == value, label).clicked() && v != value {
                        *value = v.clone();
                        changed = true;
                    }
                }
            });
        link_label(ui, &c.response);
    });
    changed
}

/// Розмір тексту одним рядком.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, TEXT).size().x)
}

/// Перемикач для налаштувань, що діють одразу (на відміну від галочки — вибору перед дією).
pub fn switch(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    let (tw, th) = (32.0, 18.0);
    let text_w = if text.is_empty() { 0.0 } else { S2 + text_width(ui, text, font(FS_MD)) };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(tw + text_w, 22.0), Sense::click());
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    let on = *value;
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, text));
    let t = crate::anim::anim_bool(ui.ctx(), resp.id.with("sw"), on, 0.15);
    let track = Rect::from_min_size(pos2(rect.left(), rect.center().y - th / 2.0), vec2(tw, th));
    let p = ui.painter();
    let hovered = resp.hovered();
    let off_fill = if hovered { LINE_FIELD } else { LINE_STRONG };
    p.rect_filled(track, CornerRadius::same(9), off_fill.lerp_to_gamma(ACCENT_STRONG, t));
    let knob = pos2(track.left() + th / 2.0 + t * (tw - th), track.center().y);
    p.circle_filled(knob, th / 2.0 - 2.0, WHITE);
    if resp.has_focus() {
        p.rect_stroke(track.expand(2.0), CornerRadius::same(11), Stroke::new(2.0_f32, ACCENT), StrokeKind::Outside);
    }
    if !text.is_empty() {
        p.text(pos2(track.right() + S2, rect.center().y), Align2::LEFT_CENTER, text, font(FS_MD), TEXT);
    }
    resp
}

/// PIN окремими клітинками (лише цифри). Клітинки після `required` — необов'язкові (тонша рамка).
/// Повертає відповідь поля вводу, тож фокус і Enter працюють як у звичайного поля.
pub fn pin_cells(ui: &mut Ui, text: &mut String, max: usize, required: usize) -> Response {
    let (cw, ch, gap) = (36.0, 44.0, S2);
    let w = max as f32 * cw + (max as f32 - 1.0) * gap;
    let (rect, _) = ui.allocate_exact_size(vec2(w, ch), Sense::hover());
    // справжнє поле — невидиме, на всю ширину клітинок: клік будь-де ставить у нього фокус
    let r = ui
        .scope(|ui| {
            let v = ui.visuals_mut();
            v.text_cursor.stroke = Stroke::NONE;
            v.selection.bg_fill = Color32::TRANSPARENT;
            ui.put(
                rect,
                TextEdit::singleline(text).char_limit(max).frame(false).text_color(Color32::TRANSPARENT).password(true),
            )
        })
        .inner;
    text.retain(|c| c.is_ascii_digit());
    let n = text.chars().count();
    let focus = r.has_focus();
    let p = ui.painter();
    for i in 0..max {
        let c = Rect::from_min_size(pos2(rect.left() + i as f32 * (cw + gap), rect.top()), vec2(cw, ch));
        let current = focus && (i == n || (n == max && i == max - 1));
        let stroke = if current {
            Stroke::new(2.0_f32, ACCENT)
        } else if i < required || i < n {
            Stroke::new(1.0_f32, LINE_FIELD)
        } else {
            Stroke::new(1.0_f32, LINE_STRONG)
        };
        p.rect(c, radius(R_MD), RAISED, stroke, StrokeKind::Inside);
        if i < n {
            p.circle_filled(c.center(), 5.0, TEXT);
        } else if current {
            // миготливий курсор у поточній клітинці
            let on = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
            if on {
                p.vline(c.center().x, c.center().y - 9.0..=c.center().y + 9.0, Stroke::new(2.0_f32, ACCENT));
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    r
}

/// Число з кнопками «−» і «+» по боках (вузьке поле — без кнопок). Колесо й перетягування теж працюють.
fn stepper(ui: &mut Ui, value: &mut f64, min: f64, max: f64, step: f64, decimals: usize, width: f32) -> Response {
    fn drag(v: &mut f64, min: f64, max: f64, step: f64, decimals: usize) -> egui::DragValue<'_> {
        egui::DragValue::new(v).range(min..=max).speed(step).max_decimals(decimals).min_decimals(decimals)
    }
    if width < 96.0 {
        let r = ui.add_sized(vec2(width, CONTROL_H), drag(value, min, max, step, decimals));
        link_label(ui, &r);
        return r;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(width, CONTROL_H), Sense::hover());
    let b = CONTROL_H - 6.0;
    let minus = Rect::from_center_size(pos2(rect.left() + 3.0 + b / 2.0, rect.center().y), Vec2::splat(b));
    let plus = Rect::from_center_size(pos2(rect.right() - 3.0 - b / 2.0, rect.center().y), Vec2::splat(b));
    let mid = Rect::from_min_max(pos2(minus.right() + 2.0, rect.top()), pos2(plus.left() - 2.0, rect.bottom()));
    let mut r = ui
        .scope(|ui| {
            let ws = &mut ui.visuals_mut().widgets;
            for w in [&mut ws.inactive, &mut ws.hovered] {
                w.bg_fill = Color32::TRANSPARENT;
                w.weak_bg_fill = Color32::TRANSPARENT;
                w.bg_stroke = Stroke::NONE;
            }
            ui.put(mid, drag(value, min, max, step, decimals))
        })
        .inner;
    let stroke = if r.has_focus() {
        ACCENT
    } else if r.hovered() {
        MUTED
    } else {
        LINE_FIELD
    };
    ui.painter().rect_stroke(rect, radius(R_MD), Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
    for (br, icon, d) in [(minus, "minus", -step), (plus, "plus", step)] {
        let id = r.id.with(icon);
        let can = if d < 0.0 { *value > min } else { *value < max };
        let sense = if can { Sense::click() } else { Sense::hover() };
        let resp = ui.interact(br, id, sense);
        if resp.hovered() && can {
            ui.painter().rect_filled(br, radius(R_SM), LINE);
        }
        focus_ring(ui, &resp, br, R_SM);
        let a = if can { 1.0 } else { 0.35 };
        icons::image(icon, 16.0, TEXT.gamma_multiply(a))
            .paint_at(ui, Rect::from_center_size(br.center(), vec2(16.0, 16.0)));
        let name = if d < 0.0 { "Зменшити" } else { "Збільшити" };
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can, name));
        if resp.clicked() {
            let p = 10f64.powi(decimals as i32);
            *value = (((*value + d) * p).round() / p).clamp(min, max);
            r.mark_changed();
        }
    }
    link_label(ui, &r);
    r
}

/// Перемикач із кількох варіантів в одній рамці; підсвітка вибраного переїжджає плавно.
/// `width` — розтягнути на всю ширину, інакше кожен пункт — за довжиною назви.
pub fn segmented<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: Id,
    value: &mut T,
    options: &[(T, &str)],
    width: Option<f32>,
) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(RAISED)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(radius(R_MD))
        .inner_margin(egui::Margin::same(S1 as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let n = options.len().max(1) as f32;
                // на всю ширину: кожен пункт — за довжиною назви, а решта місця ділиться порівну
                // (якщо назви не вміщаються — пункти стискаються пропорційно)
                let pad = if width.is_some() { S2 } else { S3 };
                let mut natural: Vec<f32> =
                    options.iter().map(|(_, l)| text_width(ui, l, font(FS_MD)) + 2.0 * pad).collect();
                let mut extra = 0.0;
                if let Some(w) = width {
                    let room = w - 2.0 * S1 - 2.0 - 2.0 * (n - 1.0);
                    let sum: f32 = natural.iter().sum();
                    if sum > room {
                        natural.iter_mut().for_each(|x| *x *= room / sum);
                    } else {
                        extra = (room - sum) / n;
                    }
                }
                let slot = ui.painter().add(egui::Shape::Noop);
                for ((v, label), nw) in options.iter().zip(&natural) {
                    let on = v == value;
                    let w = nw + extra;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                    resp.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, on, *label));
                    focus_ring(ui, &resp, r, R_MD);
                    if on {
                        let r = crate::anim::glide_rect(ui.ctx(), id.with("pill"), r, 0.2);
                        ui.painter().set(slot, egui::Shape::rect_filled(r, radius(R_SM), ACCENT_STRONG));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, radius(R_SM), LINE);
                    }
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        font(FS_MD),
                        if on { WHITE } else { MUTED },
                    );
                    if resp.clicked() && !on {
                        *value = v.clone();
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// Тіні зверху й знизу прокручуваної області, коли там є ще вміст. `bg` — колір фону під областю.
pub fn scroll_shadow<R>(ui: &Ui, out: &egui::scroll_area::ScrollAreaOutput<R>, bg: Color32) {
    let r = out.inner_rect;
    let y = out.state.offset.y;
    let more = out.content_size.y - r.height() - y;
    let p = ui.painter().with_clip_rect(r);
    let fade = |top: f32, down: bool, k: f32| {
        let h = 24.0;
        let (a, b) = (bg.gamma_multiply(k), Color32::TRANSPARENT);
        let (c0, c1) = if down { (a, b) } else { (b, a) };
        let mut m = egui::Mesh::default();
        let rr = Rect::from_min_size(pos2(r.left(), top), vec2(r.width(), h));
        let i = m.vertices.len() as u32;
        m.colored_vertex(rr.left_top(), c0);
        m.colored_vertex(rr.right_top(), c0);
        m.colored_vertex(rr.left_bottom(), c1);
        m.colored_vertex(rr.right_bottom(), c1);
        m.add_triangle(i, i + 1, i + 2);
        m.add_triangle(i + 1, i + 3, i + 2);
        p.add(m);
    };
    if y > 0.5 {
        fade(r.top(), true, (y / 24.0).min(1.0));
    }
    if more > 0.5 {
        fade(r.bottom() - 24.0, false, (more / 24.0).min(1.0));
    }
}

/// Тон повідомлення чи мітки.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Neutral,
    Info,
    Ok,
    Warn,
    Danger,
}

impl Tone {
    fn color(self) -> Color32 {
        match self {
            Tone::Neutral => MUTED,
            Tone::Info => ACCENT,
            Tone::Ok => OK,
            Tone::Warn => WARN,
            Tone::Danger => DANGER,
        }
    }
}

/// Плашка-попередження: значок, заголовок, пояснення й необов'язкова кнопка праворуч.
/// Повертає `true`, якщо натиснули кнопку.
pub fn alert(ui: &mut Ui, tone: Tone, title: &str, text: &str, action: Option<&str>) -> bool {
    let c = tone.color();
    let icon = match tone {
        Tone::Ok => "check",
        Tone::Warn | Tone::Danger => "alert",
        _ => "info",
    };
    let mut clicked = false;
    egui::Frame::new()
        .fill(alpha(c, 0.10))
        .stroke(Stroke::new(1.0_f32, alpha(c, 0.35)))
        .corner_radius(radius(R_MD))
        .inner_margin(egui::Margin::same(S3 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = S2;
                let (ir, _) = ui.allocate_exact_size(vec2(18.0, 20.0), Sense::hover());
                icons::image(icon, 18.0, c).paint_at(ui, Rect::from_center_size(ir.center(), vec2(18.0, 18.0)));
                let bw = action.map_or(0.0, |a| text_width(ui, a, font(FS_SM)) + 2.0 * S2 + S2);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - bw);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(title).font(bold(FS_MD)).color(c));
                    if !text.is_empty() {
                        ui.label(RichText::new(text).size(FS_SM).color(TEXT));
                    }
                });
                if let Some(a) = action {
                    let kind = if tone == Tone::Danger { Kind::DangerOn } else { Kind::Primary };
                    clicked = btn(a).small().kind(kind).show(ui).clicked();
                }
            });
        });
    clicked
}

/// Невелика мітка стану в «пігулці».
pub fn chip(ui: &mut Ui, tone: Tone, text: &str) -> Response {
    let c = tone.color();
    let (fill, fg) = if tone == Tone::Neutral { (RAISED, MUTED) } else { (alpha(c, 0.16), c) };
    let w = text_width(ui, text, font(FS_SM)) + 2.0 * S2;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(11), fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, font(FS_SM), fg);
    resp
}

/// Порожній стан: значок у колі, заголовок і пояснення по центру. Кнопку дії додає викликач нижче.
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, text: &str) {
    empty_state_in(ui, icon, title, text, (RAISED, LINE, MUTED));
}

/// Порожній стан першої дії: значок синій у колі `ACCENT_SOFT` з рамкою `ACCENT`.
pub fn empty_state_accent(ui: &mut Ui, icon: &str, title: &str, text: &str) {
    empty_state_in(ui, icon, title, text, (ACCENT_SOFT, ACCENT, ACCENT));
}

fn empty_state_in(ui: &mut Ui, icon: &str, title: &str, text: &str, (fill, ring, ink): (Color32, Color32, Color32)) {
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 24.0, fill);
        ui.painter().circle_stroke(r.center(), 24.0, Stroke::new(1.0_f32, ring));
        icons::image(icon, 22.0, ink).paint_at(ui, Rect::from_center_size(r.center(), vec2(22.0, 22.0)));
        ui.add_space(S2);
        ui.label(RichText::new(title).font(bold(FS_MD)).color(TEXT));
        if !text.is_empty() {
            ui.add_space(S1);
            ui.scope(|ui| {
                ui.set_max_width(280.0);
                ui.label(RichText::new(text).size(FS_SM).color(MUTED));
            });
        }
    });
}
