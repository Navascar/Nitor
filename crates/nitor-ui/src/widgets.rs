//! Елементи керування в стилі дизайн-системи: кнопки, повзунки з заливкою, числа, що
//! вписуються з клавіатури, поля вводу.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, Vec2, WidgetText, pos2, vec2,
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
    pub fn small_if(mut self, yes: bool) -> Self {
        self.small = yes;
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

    pub fn show(self, ui: &mut Ui) -> Response {
        let h = if self.small { CONTROL_H_SM } else { CONTROL_H };
        let pad = if self.small {
            S2
        } else if self.kind == Kind::Warn {
            S4
        } else {
            S3
        };
        let isz = if self.small { 16.0 } else { 18.0 };
        let gap = if self.small { S1 } else { S2 };
        let fs = if self.small { FS_SM } else { FS_MD };
        let font = if self.bold || matches!(self.kind, Kind::Warn | Kind::Update) { bold(fs) } else { font(fs) };
        let galley = self
            .text
            .as_ref()
            .filter(|t| !t.text().is_empty())
            .map(|t| t.clone().into_galley(ui, Some(egui::TextWrapMode::Truncate), f32::INFINITY, font.clone()));
        let mut w = match (&galley, self.icon) {
            (None, Some(_)) => h,
            (Some(g), Some(_)) => pad * 2.0 + isz + gap + g.size().x,
            (Some(g), None) => pad * 2.0 + g.size().x,
            (None, None) => h,
        };
        w = w.max(self.min_width);
        if self.full_width {
            w = w.max(ui.available_width());
        }
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), sense);
        let hovered = resp.hovered() && self.enabled;
        let (fill, stroke, fg) = colors(self.kind, hovered);
        if ui.is_rect_visible(rect) {
            let a = if self.enabled { 1.0 } else { 0.45 };
            let p = ui.painter();
            p.rect(
                rect,
                radius(R_MD),
                fill.gamma_multiply(a),
                Stroke::new(1.0_f32, stroke.gamma_multiply(a)),
                StrokeKind::Inside,
            );
            if resp.has_focus() {
                p.rect_stroke(rect.expand(2.0), radius(R_MD + 2), Stroke::new(2.0_f32, ACCENT), StrokeKind::Outside);
            }
            let fg = fg.gamma_multiply(a);
            let content_w = match (&galley, self.icon) {
                (Some(g), Some(_)) => isz + gap + g.size().x,
                (Some(g), None) => g.size().x,
                _ => isz,
            };
            let mut x = if self.left_align { rect.left() + pad } else { rect.center().x - content_w / 2.0 };
            x = x.max(rect.left() + if galley.is_some() { pad } else { 0.0 });
            if let Some(name) = self.icon {
                let r = Rect::from_min_size(pos2(x, rect.center().y - isz / 2.0), Vec2::splat(isz));
                icons::image(name, isz, fg).paint_at(ui, r);
                x += isz + gap;
            }
            if let Some(g) = galley {
                let max_w = rect.right() - pad - x;
                let pos = pos2(x, rect.center().y - g.size().y / 2.0);
                if g.size().x > max_w + 0.5 {
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
        let resp = if self.enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
        match self.tooltip {
            Some(t) if !t.is_empty() => resp.on_hover_text(t),
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
        Kind::Danger => (if hovered { rgba(239, 91, 96, 0.12) } else { Color32::TRANSPARENT }, DANGER, DANGER),
        Kind::DangerOn => (DANGER_STRONG, DANGER_STRONG, WHITE),
        Kind::Ghost => (if hovered { RAISED } else { Color32::TRANSPARENT }, Color32::TRANSPARENT, TEXT),
        Kind::Warn => (if hovered { rgba(245, 165, 36, 0.12) } else { Color32::TRANSPARENT }, WARN, WARN),
        Kind::Update => (ACCENT_SOFT, ACCENT, ACCENT),
        Kind::Toggled => (ACCENT_SOFT, ACCENT, TEXT),
        Kind::Done => (Color32::TRANSPARENT, OK, OK),
    }
}

/// Повзунок із заливкою доріжки кольором `fill`. Повертає відповідь (змінено — `changed()`).
pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, fill: Color32, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width.max(40.0), 18.0), Sense::click_and_drag());
    let thumb = 16.0;
    let track = Rect::from_min_max(pos2(rect.left(), rect.center().y - 3.0), pos2(rect.right(), rect.center().y + 3.0));
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
        let cx = rect.left() + thumb / 2.0 + t * (rect.width() - thumb);
        p.rect_filled(track, CornerRadius::same(3), LINE_STRONG);
        // при максимумі — заливка до самого кінця
        let fill_right = if t >= 1.0 { track.right() } else { cx };
        p.rect_filled(Rect::from_min_max(track.min, pos2(fill_right, track.max.y)), CornerRadius::same(3), fill);
        p.circle(pos2(cx, rect.center().y), thumb / 2.0 - 1.0, fill, Stroke::new(2.0_f32, TEXT));
        if resp.has_focus() {
            p.circle_stroke(pos2(cx, rect.center().y), thumb / 2.0 + 2.0, Stroke::new(2.0_f32, ACCENT));
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Як показувати число біля повзунка.
#[derive(Clone, Copy, PartialEq)]
pub enum Shown {
    Raw,
    /// у відсотках від `max`
    Pct,
}

pub fn shown_text(v: f32, max: f32, shown: Shown) -> String {
    match shown {
        Shown::Raw => format!("{}", v.round() as i64),
        Shown::Pct => format!("{}%", (v / max * 100.0).round() as i64),
    }
}

/// Число біля повзунка: клік відкриває поле, куди можна вписати значення (у відсотках — для
/// значень у %). Enter або втрата фокусу — застосувати, Esc — скасувати.
pub fn value_box(ui: &mut Ui, id: Id, value: &mut f32, min: f32, max: f32, shown: Shown, width: f32) -> bool {
    let edit_id = id.with("edit");
    let mut editing: Option<String> = ui.data(|d| d.get_temp(edit_id));
    let mut changed = false;
    if let Some(text) = editing.as_mut() {
        let r = ui.add_sized(
            vec2(width, CONTROL_H_SM),
            TextEdit::singleline(text).font(font(FS_SM)).horizontal_align(egui::Align::Max),
        );
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            editing = None;
        } else if r.lost_focus() {
            if let Ok(raw) = text.trim().replace(',', ".").trim_end_matches('%').parse::<f32>() {
                let v = match shown {
                    Shown::Pct => (raw / 100.0 * max).round(),
                    Shown::Raw => raw.round(),
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
        let text = shown_text(*value, max, shown);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, radius(R_SM), RAISED);
        }
        ui.painter().text(rect.right_center() - vec2(S1, 0.0), Align2::RIGHT_CENTER, text.clone(), font(FS_SM), TEXT);
        let resp = resp.on_hover_cursor(CursorIcon::Text).on_hover_text("Клікніть, щоб ввести значення");
        if resp.clicked() {
            let start = match shown {
                Shown::Pct => format!("{}", (*value / max * 100.0).round() as i64),
                Shown::Raw => format!("{}", value.round() as i64),
            };
            ui.data_mut(|d| d.insert_temp(edit_id, start));
        }
    }
    changed
}

/// Рядок «підпис · повзунок · число» (сітка 84px · 1fr · 48px).
pub fn slider_row(
    ui: &mut Ui,
    id: Id,
    label: &str,
    value: &mut f32,
    max: f32,
    fill: Color32,
    shown: Shown,
    label_w: f32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = S2;
        let (r, _) = ui.allocate_exact_size(vec2(label_w, 18.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, label, font(FS_SM), MUTED);
        let w = ui.available_width() - 48.0 - S2;
        changed |= slider(ui, value, 0.0, max, fill, w).changed();
        changed |= value_box(ui, id, value, 0.0, max, shown, 48.0);
    });
    changed
}

/// Однорядкове поле вводу на всю ширину з підказкою.
pub fn input(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    ui.add_sized(
        vec2(width, CONTROL_H),
        TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(MUTED))
            .margin(vec2(S2, 8.0))
            .vertical_align(egui::Align::Center),
    )
}

/// Ціле число в полі вводу (як `<input type=number>`).
pub fn number(ui: &mut Ui, value: &mut i64, min: i64, max: i64, width: f32) -> Response {
    ui.add_sized(vec2(width, CONTROL_H), egui::DragValue::new(value).range(min..=max).speed(0.2))
}

/// Дробове число (час переходу).
pub fn number_f(ui: &mut Ui, value: &mut f64, min: f64, max: f64, step: f64, width: f32) -> Response {
    ui.add_sized(vec2(width, CONTROL_H), egui::DragValue::new(value).range(min..=max).speed(step).max_decimals(1))
}

/// Підпис поля (дрібний, приглушений).
pub fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(FS_SM).color(MUTED));
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
    let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - 8.0), Vec2::splat(16.0));
    let p = ui.painter();
    if *value {
        p.rect_filled(b, CornerRadius::same(4), ACCENT_STRONG);
        icons::image("check", 14.0, WHITE).paint_at(ui, b.shrink(1.0));
    } else {
        p.rect(b, CornerRadius::same(4), RAISED, Stroke::new(1.0_f32, LINE_STRONG), StrokeKind::Inside);
    }
    p.text(pos2(b.right() + S2, rect.center().y), Align2::LEFT_CENTER, text, font(FS_MD), TEXT);
    resp.on_hover_cursor(CursorIcon::PointingHand)
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
    egui::ComboBox::from_id_salt(id)
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
    changed
}

/// Розмір тексту одним рядком.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, TEXT).size().x)
}
