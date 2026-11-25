//! Короткі анімації інтерфейсу: ковзання підсвітки, поява шарів, числа, що прокручуються, кільце удару.
//! Стан зберігається в пам'яті egui за `Id`; поки анімація йде, програма просить наступний кадр.

use egui::emath::TSTransform;
use egui::{Align2, Color32, Context, FontId, Id, LayerId, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

static REDUCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// «Зменшити рух» (налаштування доступності): без ковзання, збільшення й прокручування —
/// елементи одразу на місці, зміни кольору й підсвітки миттєві.
pub fn set_reduced_motion(on: bool) {
    REDUCED.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn reduced_motion() -> bool {
    REDUCED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Як `animate_bool_with_time`, але зі «Зменшити рух» — одразу кінцеве значення.
pub fn anim_bool(ctx: &Context, id: Id, on: bool, time: f32) -> f32 {
    if reduced_motion() {
        return if on { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time(id, on, time)
}

/// Наведення й натискання: наведення проявляється за 150 мс і згасає за 120 мс,
/// натискання — 90 мс. Повертає (наведення 0..1, натискання 0..1).
pub fn hover_press(ctx: &Context, id: Id, hovered: bool, pressed: bool) -> (f32, f32) {
    let h = anim_bool(ctx, id.with("hover"), hovered, if hovered { 0.15 } else { 0.12 });
    let p = anim_bool(ctx, id.with("press"), pressed, 0.09);
    (h, p)
}

/// Зсув і масштаб кнопки чи картки: наведення піднімає на `lift` px, натискання опускає на 1 px нижче
/// спокою й стискає до 0,985. Зі «Зменшити рух» нічого не рухається (колір змінюється й так).
pub fn lift_rect(rect: Rect, hover: f32, press: f32, lift: f32) -> Rect {
    if reduced_motion() {
        return rect;
    }
    let dy = -lift * hover * (1.0 - press) + press;
    let s = 1.0 - 0.015 * press;
    Rect::from_center_size(rect.center() + egui::vec2(0.0, dy), rect.size() * s)
}

/// Частка 0..1 анімації, що почалася в `t0` і триває `dur` с; зі «Зменшити рух» — одразу 1.
pub fn progress(ctx: &Context, t0: f64, dur: f32) -> f32 {
    if reduced_motion() {
        return 1.0;
    }
    ((now(ctx) - t0) as f32 / dur).clamp(0.0, 1.0)
}

/// Плавне сповільнення в кінці (cubic ease-out).
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn now(ctx: &Context) -> f64 {
    ctx.input(|i| i.time)
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

#[derive(Clone, Copy)]
struct Glide {
    from: Rect,
    to: Rect,
    t0: f64,
}

/// Прямокутник, що плавно переїжджає до `target` за `dur` с. Уперше — одразу на місці.
/// Для підсвітки вибраного пункту: вкладки, перемикачі.
pub fn glide_rect(ctx: &Context, id: Id, target: Rect, dur: f32) -> Rect {
    let t = now(ctx);
    let g = ctx.data(|d| d.get_temp::<Glide>(id));
    let g = match g {
        None => Glide { from: target, to: target, t0: t },
        Some(g) if g.to != target => {
            let k = ease_out(((t - g.t0) as f32 / dur).min(1.0));
            Glide { from: lerp_rect(g.from, g.to, k), to: target, t0: t }
        }
        Some(g) => g,
    };
    ctx.data_mut(|d| d.insert_temp(id, g));
    let k = if reduced_motion() { 1.0 } else { ((t - g.t0) as f32 / dur).min(1.0) };
    if k < 1.0 {
        ctx.request_repaint();
    }
    lerp_rect(g.from, g.to, ease_out(k))
}

/// Забути положення підсвітки (наступна поява — одразу на місці).
pub fn forget(ctx: &Context, id: Id) {
    ctx.data_mut(|d| d.remove::<Glide>(id));
}

#[derive(Clone, Copy)]
struct Slot {
    slot: u64,
    shown: Rect,
    from: Rect,
    t0: f64,
}

/// Елемент сітки, що плавно переїжджає, коли змінюється його місце (`slot`): сусіди з'їжджають,
/// коли прилад додали чи видалили. Зміна розміру сітки (вікна) без зміни місця — без затримки.
/// Новий елемент (`appear`) виростає з трохи меншого.
pub fn slot_rect(ctx: &Context, id: Id, slot: u64, target: Rect, appear: bool) -> Rect {
    const DUR: f32 = 0.22;
    let t = now(ctx);
    let s = match ctx.data(|d| d.get_temp::<Slot>(id)) {
        None if appear => Slot { slot, shown: target, from: target.shrink2(target.size() * 0.08), t0: t },
        None => Slot { slot, shown: target, from: target, t0: t - DUR as f64 },
        Some(s) if s.slot != slot => Slot { slot, shown: s.shown, from: s.shown, t0: t },
        Some(s) => s,
    };
    let k = if reduced_motion() { 1.0 } else { ((t - s.t0) as f32 / DUR).min(1.0) };
    let shown = if k < 1.0 {
        ctx.request_repaint();
        lerp_rect(s.from, target, ease_out(k))
    } else {
        target
    };
    ctx.data_mut(|d| d.insert_temp(id, Slot { shown, ..s }));
    shown
}

/// Скільки секунд елемент показується без перерви: 0 у перший кадр після появи.
pub fn shown_for(ctx: &Context, id: Id) -> f32 {
    let (t, pass) = (now(ctx), ctx.cumulative_pass_nr());
    let start = match ctx.data(|d| d.get_temp::<(f64, u64)>(id)) {
        Some((t0, p)) if pass <= p + 2 => t0,
        _ => t,
    };
    ctx.data_mut(|d| d.insert_temp(id, (start, pass)));
    (t - start) as f32
}

/// Поява шару (меню, вікна): за 0,14 с збільшується від `from` (частка, напр. 0,96) до повного
/// розміру навколо `origin`. Прозорість egui наростає сам (fade_in у `Area`).
pub fn pop_in(ctx: &Context, layer: LayerId, origin: Pos2, from: f32) {
    let k = shown_for(ctx, Id::new(("pop", layer.id))) / 0.14;
    if k >= 1.0 || reduced_motion() {
        return;
    }
    let s = from + (1.0 - from) * ease_out(k);
    let o = origin.to_vec2();
    ctx.transform_layer_shapes(
        layer,
        TSTransform::from_translation(o) * TSTransform::from_scaling(s) * TSTransform::from_translation(-o),
    );
    ctx.request_repaint();
}

#[derive(Clone)]
struct Roll {
    cur: String,
    prev: String,
    up: bool,
    t0: f64,
}

/// Текст, вирівняний праворуч від `right`: змінені символи прокручуються вгору (число зросло)
/// або вниз (зменшилося), незмінні стоять на місці.
pub fn rolling_text(ui: &Ui, id: Id, right: Pos2, text: &str, font: FontId, color: Color32) {
    const DUR: f32 = 0.18;
    let ctx = ui.ctx();
    let t = now(ctx);
    let num = |s: &str| s.parse::<f64>().ok();
    let mut r = ctx.data(|d| d.get_temp::<Roll>(id)).unwrap_or(Roll {
        cur: text.into(),
        prev: text.into(),
        up: true,
        t0: t - DUR as f64,
    });
    if r.cur != text {
        r.up = num(text).zip(num(&r.cur)).is_none_or(|(a, b)| a >= b);
        r.prev = std::mem::replace(&mut r.cur, text.into());
        r.t0 = t;
    }
    ctx.data_mut(|d| d.insert_temp(id, r.clone()));
    let k = ((t - r.t0) as f32 / DUR).min(1.0);
    let p = ui.painter();
    if k >= 1.0 || reduced_motion() {
        p.text(right, Align2::RIGHT_CENTER, text, font, color);
        return;
    }
    ctx.request_repaint();
    let e = ease_out(k);
    let h = font.size * 1.25;
    let row = Rect::from_min_max(pos2(f32::MIN, right.y - h / 2.0), pos2(right.x + 1.0, right.y + h / 2.0));
    let p = p.with_clip_rect(row.intersect(p.clip_rect()));
    let dir = if r.up { 1.0 } else { -1.0 };
    let chars = |s: &str| s.chars().rev().collect::<Vec<char>>();
    let (new, old) = (chars(&r.cur), chars(&r.prev));
    let draw = |p: &Painter, s: &[char], i: usize, x: f32, dy: f32, c: Color32| -> f32 {
        let g = p.layout_no_wrap(s[i].to_string(), font.clone(), c);
        let w = g.size().x;
        p.galley(pos2(x - w, right.y - g.size().y / 2.0 + dy), g, c);
        w
    };
    let (mut xn, mut xo) = (right.x, right.x);
    for i in 0..new.len().max(old.len()) {
        let same = new.get(i) == old.get(i);
        if i < new.len() {
            let dy = if same { 0.0 } else { (1.0 - e) * h * dir };
            xn -= draw(&p, &new, i, xn, dy, if same { color } else { color.gamma_multiply(e) });
        }
        if i < old.len() && !same {
            xo -= draw(&p, &old, i, xo, -e * h * dir, color.gamma_multiply(1.0 - e));
        } else if i < old.len() {
            xo = xn;
        }
    }
}

/// Кільце, що розходиться від кнопки й гасне: `age` — секунди від удару (видно перші 0,45 с).
pub fn ring(ui: &Ui, rect: Rect, age: f32, color: Color32, radius: f32) {
    const DUR: f32 = 0.45;
    if !(0.0..DUR).contains(&age) {
        return;
    }
    let e = ease_out(age / DUR);
    // зі «Зменшити рух» кільце не розходиться, лише гасне на місці
    let grow = if reduced_motion() { 3.0 } else { 2.0 + 10.0 * e };
    ui.painter().rect_stroke(
        rect.expand(grow),
        egui::CornerRadius::same((radius + grow).min(255.0) as u8),
        Stroke::new(2.0 * (1.0 - e) + 0.5, color.gamma_multiply(1.0 - e)),
        egui::StrokeKind::Outside,
    );
    ui.ctx().request_repaint();
}

/// Обертовий сегмент кола (триває дія): `rect` — місце значка.
pub fn spinner(ui: &Ui, rect: Rect, color: Color32) {
    let t = now(ui.ctx()) as f32;
    let c = rect.center();
    let r = rect.width() * 0.38;
    let a0 = t * 5.0;
    let pts: Vec<Pos2> = (0..=20).map(|i| a0 + i as f32 / 20.0 * 4.2).map(|a| c + vec2(a.cos(), a.sin()) * r).collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(2.0_f32, color)));
    ui.ctx().request_repaint();
}
