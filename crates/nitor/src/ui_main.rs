//! Головне вікно: верхня панель, прилади, колір, статичні кольори, ефекти, сцени, DMX-монітор.

use egui::{
    Align, Align2, Color32, ColorImage, Id, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureOptions,
    Ui, pos2, vec2,
};
use nitor_core::color::hsv_to_rgb;
use nitor_core::engine::{ColorValues, EffectPatch};
use nitor_core::fixtures::programs;
use nitor_core::{Command, EffectKind, Engine, Rgb};

use crate::app::{Act, App, Pane, c32, cap, readable_text, speed_from_slider};
use crate::hotkeys::short_key;
use crate::menus::{fmt_num, paint_static_chip};
use nitor_ui::icons;
use nitor_ui::theme::*;
use nitor_ui::widgets::{self, Kind, Shown, btn, icon_btn, slider_row};

const SLIDERS: [(&str, &str, Color32); 8] = [
    ("r", "Червоний", Color32::from_rgb(0xff, 0x4d, 0x4d)),
    ("g", "Зелений", Color32::from_rgb(0x3e, 0xcf, 0x5e)),
    ("b", "Синій", Color32::from_rgb(0x4f, 0x8c, 0xff)),
    ("w", "Білий", Color32::WHITE),
    ("a", "Бурштин", Color32::from_rgb(0xff, 0xb0, 0x00)),
    ("uv", "УФ", Color32::from_rgb(0x9b, 0x5c, 0xff)),
    ("dimmer", "Яскравість", Color32::from_rgb(0xcc, 0xcc, 0xcc)),
    ("strobe", "Строб", Color32::from_rgb(0x88, 0x88, 0x88)),
];

const SWATCHES: [&str; 16] = [
    "#ff0000", "#ff6a00", "#ffc400", "#ffff00", "#80ff00", "#00ff00", "#00ffaa", "#00ffff", "#0080ff", "#0000ff",
    "#8000ff", "#ff00ff", "#ff0080", "#ffffff", "#ffb070", "#000000",
];

/// HSV → RGB так само, як колірне коло 1.x (округлення до цілих).
fn js_hsv(h: f64, s: f64, v: f64) -> Rgb {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Rgb((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

impl App {
    // ---------------------------------------------------------- верхня панель --
    pub fn topbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::symmetric(S4 as i8, S2 as i8))
                    .stroke(Stroke::new(1.0_f32, LINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = S3;
                    ui.set_min_height(CONTROL_H);
                    ui.label(RichText::new("Nitor").font(bold(FS_XL)));
                    self.output_pill(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = S3;
                        let h = self.v.hotkeys.clone();
                        let k = |id: &str| short_key(&h, id).map(|k| format!(" — {k}")).unwrap_or_default();
                        if icon_btn("settings").kind(Kind::Ghost).tip("Налаштування").show(ui).clicked() {
                            self.open_settings(None);
                        }
                        let kind = if self.v.show.blackout { Kind::DangerOn } else { Kind::Danger };
                        if icon_btn("bulbOff")
                            .kind(kind)
                            .tip(format!("Вимкнути все світло{}", k("blackout")))
                            .show(ui)
                            .clicked()
                        {
                            self.send(Command::Blackout(None));
                        }
                        let on = self.v.strobe.on;
                        let can = on || !self.v.strobe_fixtures.is_empty();
                        let tip = if !can {
                            "Немає приладів з каналом строба".to_string()
                        } else {
                            format!("{}{}", if on { "Вимкнути строб" } else { "Строб" }, k("strobe"))
                        };
                        if btn("Строб")
                            .icon("bolt")
                            .kind(if on { Kind::Toggled } else { Kind::Normal })
                            .enabled(can)
                            .tip(tip)
                            .show(ui)
                            .clicked()
                        {
                            self.toggle_strobe();
                        }
                        let label = if self.sel.is_empty() {
                            "Очистити все"
                        } else {
                            "Очистити вибрані"
                        };
                        if btn(label)
                            .icon("eraser")
                            .kind(Kind::Warn)
                            .tip(format!("Очистити кольори{}", k("clear")))
                            .show(ui)
                            .clicked()
                        {
                            self.clear_colors();
                        }
                        self.update_button(ui);
                        // майстер — праворуч від індикатора, перед кнопками
                        let mut m = self.v.show.master as f32;
                        let id = Id::new("master");
                        let mut changed = widgets::value_box(ui, id, &mut m, 0.0, 255.0, Shown::Pct, 44.0);
                        changed |= widgets::slider(ui, &mut m, 0.0, 255.0, ACCENT, 180.0).changed();
                        ui.label(RichText::new("Майстер").size(FS_MD));
                        if changed {
                            self.send(Command::SetMaster(m as u8));
                        }
                    });
                });
            });
    }

    fn output_pill(&mut self, ui: &mut Ui) {
        let o = &self.v.output;
        let status =
            if o.ok && o.paused { "пауза, світло вимкнене".to_string() } else { o.status.clone() };
        let names = |k: &str| match k {
            "udmx" => "uDMX",
            "none" => "Симулятор",
            "enttec_pro" => "DMX USB Pro",
            "open_dmx" => "Open DMX",
            "artnet" => "Art-Net",
            _ => "sACN",
        };
        let text = format!("{} · {}", names(o.kind), cap(&status));
        let tip = if o.ok && o.paused {
            "Кадри не відправляються, бо все світло вимкнене. Відправка продовжиться, щойно щось увімкнеться."
                .to_string()
        } else {
            o.status.clone()
        };
        let mut job = egui::text::LayoutJob::simple_singleline(text, font(FS_SM), TEXT);
        job.wrap = egui::text::TextWrapping::truncate_at_width(380.0);
        let g = ui.fonts(|f| f.layout_job(job));
        let w = S3 * 2.0 + 8.0 + S2 + g.size().x;
        let (r, resp) = ui.allocate_exact_size(vec2(w, CONTROL_H_SM), Sense::click());
        let p = ui.painter();
        p.rect(
            r,
            egui::CornerRadius::same(255),
            RAISED,
            Stroke::new(1.0_f32, if resp.hovered() { LINE_STRONG } else { LINE }),
            StrokeKind::Inside,
        );
        let dot = if o.ok { OK } else { DANGER };
        p.circle_filled(pos2(r.left() + S3 + 4.0, r.center().y), 4.0, dot);
        p.galley(pos2(r.left() + S3 + 8.0 + S2, r.center().y - g.size().y / 2.0), g, TEXT);
        // індикатор виходу — одразу до вибору DMX-виходу
        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.open_settings(Some(crate::dialogs::Tab::Output));
        }
    }

    fn update_button(&mut self, ui: &mut Ui) {
        let u = self.updater.info();
        let busy = matches!(u.status, crate::update::Status::Installing | crate::update::Status::Restarting);
        let failed = !u.error.is_empty() && self.update_tried && !busy;
        if !(u.available || busy || failed) {
            return;
        }
        let (icon, text, kind) = if busy {
            ("refresh", if u.progress.is_empty() { "Оновлення…".to_string() } else { u.progress.clone() }, Kind::Normal)
        } else if failed {
            ("alert", "Оновлення не вдалося".to_string(), Kind::Danger)
        } else {
            ("download", format!("Оновити до {}", u.latest), Kind::Update)
        };
        let tip = if failed {
            u.error.clone()
        } else {
            format!("Встановлена версія {}, доступна {}", u.current, u.latest)
        };
        if btn(text).icon(icon).kind(kind).enabled(!busy).tip(tip).show(ui).clicked() {
            self.open_update_dialog();
        }
    }

    // ------------------------------------------------------------ розкладка --
    pub fn main_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            let mon_h = if self.monitor_open { (full.height() * 0.28).clamp(120.0, 360.0) + 52.0 } else { 46.0 };
            let top = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - mon_h));
            let wide = full.width() > 1100.0;
            let (c1, c2, c3) = if wide {
                let total = 1.0 + 1.35 + 1.0;
                let w1 = (full.width() / total).max(240.0);
                let w2 = (full.width() * 1.35 / total).max(340.0);
                let r1 = Rect::from_min_size(top.min, vec2(w1, top.height()));
                let r2 = Rect::from_min_size(pos2(r1.right(), top.top()), vec2(w2, top.height()));
                let r3 = Rect::from_min_max(pos2(r2.right(), top.top()), top.max);
                (r1, r2, r3)
            } else {
                // вузьке вікно: дві колонки, праву (статичні кольори й ефекти) — під ними
                let h = top.height() * 0.62;
                let w1 = full.width() / 2.0;
                let r1 = Rect::from_min_size(top.min, vec2(w1, h));
                let r2 = Rect::from_min_max(pos2(r1.right(), top.top()), pos2(top.right(), top.top() + h));
                let r3 = Rect::from_min_max(pos2(top.left(), top.top() + h), top.max);
                (r1, r2, r3)
            };
            let p = ui.painter();
            if wide {
                p.vline(c2.left(), c2.y_range(), Stroke::new(1.0_f32, LINE));
                p.vline(c3.left(), c3.y_range(), Stroke::new(1.0_f32, LINE));
            } else {
                p.vline(c2.left(), c2.y_range(), Stroke::new(1.0_f32, LINE));
                p.hline(c3.x_range(), c3.top(), Stroke::new(1.0_f32, LINE));
            }
            p.hline(full.x_range(), top.bottom(), Stroke::new(1.0_f32, LINE));

            // фон панелей приймає правий клік (меню панелі) — вміст поверх нього має пріоритет
            let zone = |ui: &mut Ui, r: Rect, name: &str| ui.interact(r, Id::new(("zone", name)), Sense::click());
            let z1 = zone(ui, c1, "fixtures");
            let z2 = zone(ui, c2, "color");
            let z3 = zone(ui, c3, "side");

            self.column(ui, c1, "fixtures", |app, ui| app.fixtures_panel(ui));
            let scenes_rect = self.column(ui, c2, "color", |app, ui| app.color_panel(ui));
            let side_rects = self.column(ui, c3, "side", |app, ui| app.side_panel(ui));
            let mon = Rect::from_min_max(pos2(full.left(), top.bottom() + 1.0), full.max);
            self.monitor_panel(ui, mon);

            if self.pending_menu.is_none() {
                if z1.secondary_clicked() {
                    let m = self.fixtures_panel_menu();
                    self.want_menu(m);
                } else if z2.secondary_clicked() {
                    let pos = z2.interact_pointer_pos().unwrap_or_default();
                    let m = if scenes_rect.contains(pos) { self.scene_menu(None) } else { self.color_menu() };
                    self.want_menu(m);
                } else if z3.secondary_clicked() {
                    let pos = z3.interact_pointer_pos().unwrap_or_default();
                    let m = if side_rects.contains(pos) { self.effect_menu() } else { self.static_menu(None) };
                    self.want_menu(m);
                }
            }
        });
    }

    /// Колонка з прокруткою й відступами панелі. Повертає прямокутник, який позначив вміст.
    fn column(&mut self, ui: &mut Ui, rect: Rect, id: &str, add: impl FnOnce(&mut App, &mut Ui) -> Rect) -> Rect {
        let mut out = Rect::NOTHING;
        let inner = rect.shrink2(vec2(S5, 0.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).id_salt(id), |ui| {
            // смуги прокрутки приховано; прокрутка коліщатком лишається
            egui::ScrollArea::vertical()
                .id_salt(("scroll", id))
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_space(S4);
                    ui.set_width(inner.width() - 1.0);
                    out = add(self, ui);
                    ui.add_space(S4);
                });
        });
        out
    }

    // --------------------------------------------------------------- прилади --
    fn panel_head(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
        ui.horizontal(|ui| {
            ui.set_min_height(CONTROL_H_SM);
            widgets::h2(ui, title);
            ui.with_layout(Layout::right_to_left(Align::Center), right);
        });
        ui.add_space(S3 - S2);
    }

    fn fixtures_panel(&mut self, ui: &mut Ui) -> Rect {
        let mut acts: Vec<Act> = vec![];
        Self::panel_head(ui, "Прилади", |ui| {
            ui.spacing_mut().item_spacing.x = S1;
            if btn("Інверсія").small().show(ui).clicked() {
                acts.push(Act::SelectInvert);
            }
            if btn("Жодного").small().show(ui).clicked() {
                acts.push(Act::SelectNone);
            }
            if btn("Усі").small().show(ui).clicked() {
                acts.push(Act::SelectAll);
            }
            if btn("Додати").small().icon("plus").tip("Додати прилад").show(ui).clicked() {
                acts.push(Act::OpenAdd);
            }
        });
        for a in acts {
            self.act(ui.ctx(), a);
        }
        if self.v.show.fixtures.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(S5);
                ui.label(RichText::new("Приладів ще немає.").color(MUTED));
                ui.add_space(S3);
                if btn("Додати прилад").icon("plus").primary().show(ui).clicked() {
                    self.open_add();
                }
            });
            return Rect::NOTHING;
        }
        // сітка: auto-fill minmax(100px, 1fr), бари — на дві колонки
        let w = ui.available_width();
        let gap = S2;
        let cols = (((w + gap) / (100.0 + gap)).floor() as usize).max(1);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = 122.0;
        let fixtures = self.v.show.fixtures.clone();
        let mut positions = vec![];
        let (mut col, mut row) = (0usize, 0usize);
        for f in &fixtures {
            let span = if self.v.segs(f) > 1 { 2.min(cols) } else { 1 };
            if col + span > cols {
                col = 0;
                row += 1;
            }
            positions.push((col, row, span));
            col += span;
            if col >= cols {
                col = 0;
                row += 1;
            }
        }
        let rows = positions.last().map_or(0, |p| p.1 + 1) + usize::from(col == 0 && !positions.is_empty())
            - usize::from(col == 0 && !positions.is_empty());
        let rows = rows.max(positions.iter().map(|p| p.1 + 1).max().unwrap_or(0));
        let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (th + gap) - gap), Sense::hover());
        let eff_ids: Vec<String> = if self.v.effect_running() { self.v.show.effect.ids.clone() } else { vec![] };
        for (f, (c, r, span)) in fixtures.iter().zip(positions) {
            let rect = Rect::from_min_size(
                pos2(area.left() + c as f32 * (cw + gap), area.top() + r as f32 * (th + gap)),
                vec2(cw * span as f32 + gap * (span as f32 - 1.0), th),
            );
            self.tile(ui, rect, f, &eff_ids);
        }
        area
    }

    fn tile(&mut self, ui: &mut Ui, rect: Rect, f: &nitor_core::Fixture, eff_ids: &[String]) {
        let id = Id::new(("tile", &f.id));
        let resp = ui.interact(rect, id, Sense::click()).on_hover_text(&f.profile);
        let selected = self.sel.contains(&f.id);
        let p = ui.painter().clone();
        let (fill, stroke) = if selected {
            (ACCENT_SOFT, ACCENT)
        } else if resp.hovered() {
            (RAISED, LINE_STRONG)
        } else {
            (RAISED, LINE)
        };
        p.rect(rect, radius(R_LG), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
        let n = self.v.segs(f);
        let live = self.v.live.get(&f.id).cloned().unwrap_or_default();
        let paint = |c: Option<&[u8; 4]>| {
            let [r, g, b, w] = c.copied().unwrap_or([0, 0, 0, 0]);
            Color32::from_rgb(r.saturating_add(w), g.saturating_add(w), b.saturating_add(w))
        };
        let inner = rect.shrink(S2);
        let shift = ui.input(|i| i.modifiers.shift);
        let mut seg_clicked = false;
        let mut lamp_bottom;
        if n > 1 {
            let bar = Rect::from_min_size(pos2(inner.left(), inner.top() + S2), vec2(inner.width(), 32.0));
            p.rect_filled(bar, radius(R_SM), Color32::from_rgb(5, 6, 8));
            let cells = bar.shrink(3.0);
            let cw = (cells.width() - 3.0 * (n as f32 - 1.0)) / n as f32;
            for k in 0..n {
                let cr = Rect::from_min_size(
                    pos2(cells.left() + k as f32 * (cw + 3.0), cells.top()),
                    vec2(cw, cells.height()),
                );
                let sid = format!("{}#{k}", f.id);
                let cresp =
                    ui.interact(cr, Id::new(("seg", &sid)), Sense::click()).on_hover_text(format!("Сегмент {}", k + 1));
                p.rect_filled(cr, egui::CornerRadius::same(3), paint(live.get(k)));
                if self.sel.contains(&sid) {
                    p.rect_stroke(
                        cr.expand(1.0),
                        egui::CornerRadius::same(3),
                        Stroke::new(2.0_f32, ACCENT),
                        StrokeKind::Outside,
                    );
                } else if cresp.hovered() {
                    p.rect_stroke(cr, egui::CornerRadius::same(3), Stroke::new(1.0_f32, MUTED), StrokeKind::Outside);
                }
                if cresp.double_clicked() {
                    self.open_props(&f.id);
                    seg_clicked = true;
                } else if cresp.clicked() {
                    self.pick(&sid, shift);
                    seg_clicked = true;
                }
                if cresp.secondary_clicked() {
                    let m = self.fixture_menu(&sid);
                    self.want_menu(m);
                }
            }
            lamp_bottom = bar.bottom() + S3;
        } else {
            let c = pos2(inner.center().x, inner.top() + S1 + 22.0);
            let col = paint(live.first());
            let glow = (col.r() as f32 + col.g() as f32 + col.b() as f32) / 90.0;
            if glow > 0.5 {
                for i in (1..=4).rev() {
                    let a = 0.5 * (1.0 - i as f32 / 5.0) * 0.5;
                    p.circle_filled(
                        c,
                        22.0 + glow * i as f32 / 4.0,
                        Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (a * 255.0) as u8),
                    );
                }
            }
            p.circle(
                c,
                22.0,
                if col == Color32::BLACK { Color32::BLACK } else { col },
                Stroke::new(1.0_f32, LINE_STRONG),
            );
            lamp_bottom = c.y + 22.0 + S2;
        }
        let text = |s: &str, f: egui::FontId, col: Color32, y: &mut f32| {
            let mut job = egui::text::LayoutJob::simple_singleline(s.to_string(), f, col);
            job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width());
            job.halign = Align::Center;
            let g = ui.fonts(|fo| fo.layout_job(job));
            let h = g.size().y;
            p.galley(pos2(inner.center().x, *y), g, col);
            *y += h;
        };
        text(&f.name, bold(FS_SM), TEXT, &mut lamp_bottom);
        let end = self.v.end(f);
        text(&format!("DMX {}–{}", f.address, end), font(FS_XS), MUTED, &mut lamp_bottom);
        let badge = if let Some(n) = f.preset {
            programs().get(n as usize - 1).map_or(format!("Пресет {n}"), |p| p.name.clone())
        } else if eff_ids.contains(&f.id) {
            self.v.show.effect.kind.label().to_string()
        } else {
            String::new()
        };
        if !badge.is_empty() {
            let label = format!("●  {badge}");
            text(&label, font(FS_XS), ACCENT, &mut lamp_bottom);
        }
        if seg_clicked {
            return;
        }
        if resp.double_clicked() {
            self.select_only(&f.id);
            self.open_props(&f.id);
        } else if resp.clicked() {
            self.pick(&f.id, shift);
        }
        if resp.secondary_clicked() {
            let m = self.fixture_menu(&f.id);
            self.want_menu(m);
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    // ----------------------------------------------------------------- колір --
    fn color_panel(&mut self, ui: &mut Ui) -> Rect {
        let targets = self.preset_targets();
        let bars = !targets.is_empty();
        let show_presets = bars && self.pane == Pane::Presets;
        let desc = self.sel_description();
        ui.horizontal(|ui| {
            ui.set_min_height(CONTROL_H_SM);
            if bars {
                // перемикач «Пресети / Колір»
                egui::Frame::new()
                    .fill(RAISED)
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .corner_radius(radius(R_MD))
                    .inner_margin(egui::Margin::same(S1 as i8))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (pane, label) in [(Pane::Presets, "Пресети"), (Pane::Color, "Колір")] {
                            let on = (pane == Pane::Presets) == show_presets;
                            let w = widgets::text_width(ui, label, bold(FS_SM)) + 2.0 * S3;
                            let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                            if on {
                                ui.painter().rect_filled(r, radius(R_SM), ACCENT_STRONG);
                            }
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                label,
                                bold(FS_SM),
                                if on { WHITE } else { MUTED },
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.pane = pane;
                            }
                        }
                    });
            } else {
                widgets::h2(ui, "Колір");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(widgets::muted(if desc.is_empty() {
                    "Нічого не вибрано".into()
                } else {
                    format!("Вибрано {desc}")
                }));
            });
        });
        ui.add_space(S3 - S2);
        if show_presets {
            self.presets_pane(ui, &targets);
        } else {
            self.color_pane(ui);
        }
        ui.add_space(S5);
        self.scenes_block(ui)
    }

    fn wheel_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if let Some(t) = &self.wheel {
            return t.clone();
        }
        let n = 440usize;
        let r = n as f64 / 2.0;
        let mut img = ColorImage::filled([n, n], Color32::TRANSPARENT);
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f64 + 0.5 - r, y as f64 + 0.5 - r);
                let d = dx.hypot(dy);
                if d > r {
                    continue;
                }
                let h = (dy.atan2(dx) / (2.0 * std::f64::consts::PI) + 1.0) % 1.0;
                let c = js_hsv(h, d / r, 1.0);
                let a = if d > r - 2.0 { 128 } else { 255 };
                img[(x, y)] = Color32::from_rgba_unmultiplied(c.0, c.1, c.2, a);
            }
        }
        let t = ctx.load_texture("wheel", img, TextureOptions::LINEAR);
        self.wheel = Some(t.clone());
        t
    }

    fn color_pane(&mut self, ui: &mut Ui) {
        let tex = self.wheel_texture(ui.ctx());
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = S4;
            let (rect, resp) = ui.allocate_exact_size(vec2(220.0, 220.0), Sense::click_and_drag());
            ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let resp = resp.on_hover_cursor(egui::CursorIcon::Crosshair);
            if resp.is_pointer_button_down_on() && (resp.dragged() || resp.drag_started() || resp.clicked()) {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let d = pos - rect.center();
                    let rr = 110.0;
                    let s = (d.length() / rr).min(1.0) as f64;
                    let h = ((d.y as f64).atan2(d.x as f64) / (2.0 * std::f64::consts::PI) + 1.0) % 1.0;
                    let c = js_hsv(h, s, 1.0);
                    if c != self.last_color || resp.drag_started() || resp.clicked() {
                        self.pick_color(c);
                    }
                }
            } else if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let d = pos - rect.center();
                    let s = (d.length() / 110.0).min(1.0) as f64;
                    let h = ((d.y as f64).atan2(d.x as f64) / (2.0 * std::f64::consts::PI) + 1.0) % 1.0;
                    self.pick_color(js_hsv(h, s, 1.0));
                }
            }
            if resp.secondary_clicked() {
                let m = self.color_menu();
                self.want_menu(m);
            }
            // палітра 4 × 4
            let (grid, _) = ui.allocate_exact_size(vec2(4.0 * 32.0 + 3.0 * S2, 4.0 * 32.0 + 3.0 * S2), Sense::hover());
            for (i, hex) in SWATCHES.iter().enumerate() {
                let c = Rgb::from_hex(hex).unwrap();
                let r = Rect::from_min_size(
                    pos2(grid.left() + (i % 4) as f32 * (32.0 + S2), grid.top() + (i / 4) as f32 * (32.0 + S2)),
                    vec2(32.0, 32.0),
                );
                let resp = ui.interact(r, Id::new(("swatch", i)), Sense::click()).on_hover_text(*hex);
                ui.painter().rect(
                    r,
                    radius(R_SM),
                    c32(c),
                    Stroke::new(1.0_f32, rgba(0, 0, 0, 0.4)),
                    StrokeKind::Inside,
                );
                if resp.hovered() {
                    ui.painter().rect_stroke(
                        r.expand(1.0),
                        radius(R_SM),
                        Stroke::new(2.0_f32, TEXT),
                        StrokeKind::Outside,
                    );
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.pick_color(c);
                }
            }
        });
        ui.add_space(S4);
        // показувати лише ті канали, що є у вибраних приладів (або всі, якщо нічого не вибрано)
        let items = self.sel_items();
        let fixtures: Vec<String> = if items.is_empty() {
            self.v.show.fixtures.iter().map(|f| f.id.clone()).collect()
        } else {
            self.sel_fixtures()
        };
        let mut roles = std::collections::BTreeSet::new();
        for id in &fixtures {
            if let Some(f) = self.v.fixture(id) {
                roles.extend(self.v.channels(f).iter().cloned());
            }
        }
        let first = items.first().and_then(|i| self.item_color(i));
        ui.spacing_mut().item_spacing.y = S2;
        for (key, label, color) in SLIDERS {
            let always = ["r", "g", "b", "w", "dimmer"].contains(&key);
            if !always && !roles.contains(key) {
                continue;
            }
            let mut v = first.as_ref().map_or(0.0, |f| f.channel(key) as f32);
            if slider_row(ui, Id::new(("sl", key)), label, &mut v, 255.0, color, Shown::Raw, 84.0) {
                let mut cv = ColorValues::default();
                let b = v as u8;
                match key {
                    "r" => cv.r = Some(b),
                    "g" => cv.g = Some(b),
                    "b" => cv.b = Some(b),
                    "w" => cv.w = Some(b),
                    "a" => cv.a = Some(b),
                    "uv" => cv.uv = Some(b),
                    "dimmer" => cv.dimmer = Some(b),
                    _ => cv.strobe = Some(b),
                }
                self.set_color(cv);
            }
        }
    }

    fn presets_pane(&mut self, ui: &mut Ui, targets: &[String]) {
        let groups = [("color", "Кольори"), ("effect", "Ефекти"), ("gradient", "Градієнти"), ("sound", "Звук")];
        let fx: Vec<nitor_core::Fixture> = targets.iter().filter_map(|id| self.v.fixture(id).cloned()).collect();
        let mut used = std::collections::HashMap::new();
        for f in &fx {
            if let Some(p) = f.preset {
                *used.entry(p).or_insert(0usize) += 1;
            }
        }
        let progs = programs();
        let mut first = true;
        for (group, title) in groups {
            let list: Vec<(usize, &nitor_core::fixtures::Program)> =
                progs.iter().enumerate().filter(|(_, p)| p.group == group).collect();
            if list.is_empty() {
                continue;
            }
            if !first {
                ui.add_space(S4 - S2);
            }
            first = false;
            ui.label(widgets::muted(title));
            let w = ui.available_width();
            let cols = if group == "effect" { 9 } else { (((w + S2) / (96.0 + S2)).floor() as usize).max(1) };
            let cw = (w - S2 * (cols as f32 - 1.0)) / cols as f32;
            let rows = list.len().div_ceil(cols);
            let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (CONTROL_H + S2) - S2), Sense::hover());
            for (j, (i, p)) in list.iter().enumerate() {
                let n = (*i + 1) as u8;
                let r = Rect::from_min_size(
                    pos2(
                        area.left() + (j % cols) as f32 * (cw + S2),
                        area.top() + (j / cols) as f32 * (CONTROL_H + S2),
                    ),
                    vec2(cw, CONTROL_H),
                );
                let resp = ui
                    .interact(r, Id::new(("prog", n)), Sense::click())
                    .on_hover_text(format!("{} · DMX {}–{}", p.name, p.lo, p.hi));
                let c = used.get(&n).copied().unwrap_or(0);
                let (fill, stroke, fg) = if c > 0 && c == fx.len() {
                    (ACCENT_STRONG, ACCENT_STRONG, WHITE)
                } else if c > 0 {
                    (RAISED, ACCENT, ACCENT)
                } else {
                    (RAISED, if resp.hovered() { LINE_STRONG } else { LINE }, TEXT)
                };
                let painter = ui.painter();
                painter.rect(r, radius(R_MD), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
                let label = match group {
                    "effect" => p.name.replace("Ефект ", ""),
                    "gradient" => p.name.replace(" градієнт", ""),
                    _ => p.name.clone(),
                };
                let sw = program_swatch(p);
                let tw = widgets::text_width(ui, &label, font(FS_SM));
                let total = tw + if sw.is_some() { 12.0 + S2 } else { 0.0 };
                let mut x = r.center().x - total / 2.0;
                if let Some(sw) = sw {
                    let sr = Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(12.0, 12.0));
                    paint_swatch(painter, sr, sw, p.group == "gradient");
                    x += 12.0 + S2;
                }
                painter.text(pos2(x, r.center().y), Align2::LEFT_CENTER, label, font(FS_SM), fg);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.send(Command::Preset { ids: targets.to_vec(), n: Some(Some(n)), speed: None });
                }
            }
        }
        ui.add_space(S4);
        let mut speed = fx.first().map_or(128.0, |f| f.speed as f32);
        if slider_row(ui, Id::new("presetSpeed"), "Швидкість", &mut speed, 255.0, ACCENT, Shown::Pct, 84.0) {
            self.send(Command::Preset { ids: targets.to_vec(), n: None, speed: Some(speed as u8) });
        }
        ui.add_space(S3 - S2);
        let any = fx.iter().any(|f| f.preset.is_some());
        if btn("Вимкнути пресет").icon("stop").enabled(any).show(ui).clicked() {
            self.send(Command::Preset { ids: targets.to_vec(), n: Some(None), speed: None });
        }
    }

    // ------------------------------------------------------------------ сцени --
    fn scenes_block(&mut self, ui: &mut Ui) -> Rect {
        let start = ui.cursor().min;
        Self::panel_head(ui, "Сцени", |_| {});
        let w = ui.available_width();
        ui.horizontal(|ui| {
            let r = widgets::input(ui, &mut self.scene_name, "Назва сцени", w - 110.0);
            if r.secondary_clicked() {
                let m = self.input_menu(r.id);
                self.want_menu(m);
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let label = if self.flashing("sceneSave") { "Збережено" } else { "Зберегти" };
            let kind = if self.flashing("sceneSave") { Kind::Done } else { Kind::Normal };
            let mut b = btn(label).kind(kind).min_width(100.0);
            if self.flashing("sceneSave") {
                b = b.icon("check");
            }
            if b.show(ui).clicked() || enter {
                let name = self.scene_name.trim().to_string();
                if name.is_empty() {
                    self.toast("Вкажіть назву сцени", false);
                } else {
                    let with_effect = self.scene_with_effect;
                    if self.send(Command::SaveScene { name, with_effect }) {
                        self.scene_name.clear();
                        self.flash("sceneSave");
                    }
                }
            }
        });
        ui.add_space(S3 - S2);
        widgets::check(ui, &mut self.scene_with_effect, "Зберегти разом з ефектом");
        ui.add_space(S3 - S2);
        widgets::field_label(ui, "Час переходу, с");
        widgets::number_f(ui, &mut self.scene_fade, 0.0, 60.0, 0.05, w);
        ui.add_space(S3 - S2);
        if btn("Відкрити з файлу").small().icon("upload").show(ui).clicked() {
            self.act(ui.ctx(), Act::OpenSceneFile);
        }
        ui.add_space(S3 - S2);
        let names: Vec<String> = self.v.show.scenes.keys().cloned().collect();
        if names.is_empty() {
            ui.label(widgets::muted("Збережених сцен немає"));
        }
        ui.spacing_mut().item_spacing.y = S1;
        for name in names {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S1;
                let bw = ui.available_width() - 2.0 * (CONTROL_H_SM + S1);
                let r = btn(name.clone()).icon("play").min_width(bw).left().tip(name.clone()).show(ui);
                if r.clicked() {
                    let fade = self.scene_fade;
                    self.send(Command::RecallScene { name: name.clone(), fade });
                }
                if r.secondary_clicked() {
                    let m = self.scene_menu(Some(&name));
                    self.want_menu(m);
                }
                if icon_btn("download").small().tip("Зберегти сцену у файл").show(ui).clicked() {
                    self.act(ui.ctx(), Act::SaveSceneFile(name.clone()));
                }
                if icon_btn("trash").small().tip("Видалити сцену").show(ui).clicked() {
                    self.act(ui.ctx(), Act::AskDeleteScene(name.clone()));
                }
            });
        }
        Rect::from_min_max(start, pos2(start.x + w, ui.cursor().min.y))
    }

    // ------------------------------------------- статичні кольори й ефекти --
    fn side_panel(&mut self, ui: &mut Ui) -> Rect {
        let target = if self.sel.is_empty() { "Для всіх приладів" } else { "Для вибраних" };
        Self::panel_head(ui, "Статичні кольори", |ui| {
            ui.label(widgets::muted(target));
        });
        self.static_grid(ui);
        ui.add_space(S3 - S2);
        ui.horizontal_wrapped(|ui| {
            if btn("Зберегти поточний")
                .small()
                .icon("plus")
                .tip("Зберегти колір першого вибраного приладу")
                .show(ui)
                .clicked()
            {
                self.act(ui.ctx(), Act::SaveStaticCurrent);
            }
            let (icon, label) =
                if self.static_editing { ("check", "Готово") } else { ("edit", "Редагувати") };
            if btn(label).small().icon(icon).show(ui).clicked() {
                self.static_editing = !self.static_editing;
            }
            if self.static_editing && btn("Скинути до стандартних").small().show(ui).clicked() {
                self.act(ui.ctx(), Act::AskResetStatic);
            }
        });
        ui.add_space(S5);
        let start = ui.cursor().min;
        self.effects_block(ui);
        Rect::from_min_max(start, pos2(start.x + ui.available_width(), ui.cursor().min.y + 400.0))
    }

    fn static_grid(&mut self, ui: &mut Ui) {
        let colors = self.v.show.static_colors.clone();
        let w = ui.available_width();
        let gap = S2;
        let cols = (((w + gap) / (92.0 + gap)).floor() as usize).max(1);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let rows = colors.len().div_ceil(cols);
        let (area, _) = ui.allocate_exact_size(vec2(w, (rows as f32 * (48.0 + gap) - gap).max(0.0)), Sense::hover());
        for (i, c) in colors.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(area.left() + (i % cols) as f32 * (cw + gap), area.top() + (i / cols) as f32 * (48.0 + gap)),
                vec2(cw, 48.0),
            );
            let resp = ui.interact(r, Id::new(("static", i)), Sense::click()).on_hover_text(&c.name);
            let with_w = c.w.is_some_and(|w| w > 0);
            let rr = if resp.is_pointer_button_down_on() { r.shrink(1.0) } else { r };
            paint_static_chip(ui, rr, c.hex, with_w, R_MD);
            ui.painter().rect_stroke(rr, radius(R_MD), Stroke::new(1.0_f32, rgba(0, 0, 0, 0.4)), StrokeKind::Inside);
            let fg = readable_text(c.hex);
            if with_w {
                ui.painter().text(
                    rr.left_top() + vec2(6.0, 3.0),
                    Align2::LEFT_TOP,
                    "W",
                    font(FS_XS),
                    fg.gamma_multiply(0.75),
                );
            }
            let long = c.name.chars().count() > 10;
            let mut job = egui::text::LayoutJob::simple(
                c.name.clone(),
                bold(if long { FS_XS } else { FS_SM }),
                fg,
                rr.width() - 2.0 * S1,
            );
            job.wrap.max_rows = 2;
            job.halign = Align::Center;
            let g = ui.fonts(|f| f.layout_job(job));
            ui.painter().galley(pos2(rr.center().x, rr.bottom() - S1 - g.size().y), g, fg);
            if self.static_editing {
                ui.painter().rect_stroke(
                    r.expand(2.0),
                    radius(R_MD + 2),
                    Stroke::new(1.0_f32, MUTED),
                    StrokeKind::Outside,
                );
                let dr = Rect::from_center_size(pos2(r.right() - 4.0, r.top() + 4.0), vec2(20.0, 20.0));
                let dresp = ui.interact(dr, Id::new(("static-del", i)), Sense::click()).on_hover_text("Видалити");
                ui.painter().circle_filled(dr.center(), 10.0, DANGER_STRONG);
                icons::image("x", 12.0, WHITE).paint_at(ui, Rect::from_center_size(dr.center(), vec2(12.0, 12.0)));
                if dresp.clicked() {
                    self.send(Command::DeleteStaticColor { index: i });
                    return;
                }
            } else if resp.clicked() {
                let ids = self.sel.clone();
                self.act(ui.ctx(), Act::StaticTo { index: i, ids });
            }
            if resp.secondary_clicked() {
                let m = self.static_menu(Some(i));
                self.want_menu(m);
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }

    fn effects_block(&mut self, ui: &mut Ui) {
        let e = self.v.show.effect.clone();
        let state = if e.kind != EffectKind::None {
            format!("{} · {} прил.", e.kind.label(), e.ids.len())
        } else {
            String::new()
        };
        Self::panel_head(ui, "Ефекти", |ui| {
            ui.label(widgets::muted(state));
        });
        let w = ui.available_width();
        widgets::field_label(ui, "Ефект");
        let opts: Vec<(EffectKind, String)> =
            EffectKind::ALL.iter().skip(1).map(|k| (*k, k.label().to_string())).collect();
        widgets::select(ui, "effType", &mut self.eff.kind, &opts, w);
        ui.add_space(S3 - S2);
        let hz = speed_from_slider(self.eff.speed);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(widgets::muted("Швидкість"));
        });
        ui.horizontal(|ui| {
            let r = widgets::slider(ui, &mut self.eff.speed, 1.0, 100.0, ACCENT, w - 52.0 - S2)
                .on_hover_text(format!("{hz:.2} Гц"));
            changed |= r.changed();
            changed |= widgets::value_box(ui, Id::new("effSpeed"), &mut self.eff.speed, 1.0, 100.0, Shown::Pct, 52.0);
        });
        ui.add_space(S3 - S2);
        ui.label(widgets::muted("Розкид фази"));
        ui.horizontal(|ui| {
            changed |= widgets::slider(ui, &mut self.eff.spread, 0.0, 100.0, ACCENT, w - 52.0 - S2).changed();
            changed |= widgets::value_box(ui, Id::new("effSpread"), &mut self.eff.spread, 0.0, 100.0, Shown::Pct, 52.0);
        });
        ui.add_space(S3 - S2);
        ui.horizontal(|ui| {
            ui.label(widgets::muted("Палітра"));
            let mut remove = None;
            for (i, c) in self.eff.colors.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                ui.painter().rect(
                    r,
                    radius(R_SM),
                    c32(*c),
                    Stroke::new(1.0_f32, rgba(0, 0, 0, 0.4)),
                    StrokeKind::Inside,
                );
                if resp.hovered() {
                    ui.painter().rect_stroke(
                        r.expand(1.0),
                        radius(R_SM),
                        Stroke::new(2.0_f32, DANGER),
                        StrokeKind::Outside,
                    );
                }
                if resp.on_hover_text("Прибрати").clicked() {
                    remove = Some(i);
                }
            }
            if let Some(i) = remove {
                self.eff.colors.remove(i);
                changed = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_btn("plus").small().tip("Додати поточний колір у палітру").show(ui).clicked()
                {
                    self.act(ui.ctx(), Act::AddPaletteColor);
                }
            });
        });
        ui.add_space(S3 - S2);
        if widgets::check(ui, &mut self.eff.by_seg, "По сегментах").changed() {
            changed = true;
        }
        ui.add_space(S3 - S2);
        ui.horizontal(|ui| {
            let flash = self.flashing("effApply");
            let kind = if flash {
                Kind::Done
            } else if self.v.show.fixtures.is_empty() {
                Kind::Normal
            } else {
                Kind::Primary
            };
            let (icon, label) = if flash {
                ("check", "Запущено")
            } else {
                ("play", "Запустити на вибраних")
            };
            if btn(label).icon(icon).kind(kind).show(ui).clicked() {
                self.act(ui.ctx(), Act::RunEffectSelected);
            }
            if btn("Зупинити").icon("stop").show(ui).clicked() {
                self.send(Command::SetEffect(EffectPatch { kind: Some(EffectKind::None), ..Default::default() }));
            }
        });
        if changed {
            self.live_effect_update();
        }
    }

    // --------------------------------------------------------------- монітор --
    fn monitor_panel(&mut self, ui: &mut Ui, rect: Rect) {
        let inner = rect.shrink2(vec2(S5, S3));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).id_salt("monitor"), |ui| {
            let frames = self.v.output.frames;
            let (r, resp) = ui.allocate_exact_size(vec2(inner.width(), 22.0), Sense::click());
            let arrow = if self.monitor_open { "▾" } else { "▸" };
            ui.painter().text(r.left_center(), Align2::LEFT_CENTER, format!("{arrow} DMX-монітор"), bold(FS_MD), TEXT);
            let tw = widgets::text_width(ui, &format!("{arrow} DMX-монітор"), bold(FS_MD));
            ui.painter().text(
                pos2(r.left() + tw + S2, r.center().y),
                Align2::LEFT_CENTER,
                format!("· Відправлено кадрів: {frames}"),
                font(FS_SM),
                MUTED,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.monitor_open = !self.monitor_open;
            }
            if !self.monitor_open {
                return;
            }
            ui.add_space(S3 - S2);
            egui::ScrollArea::vertical().id_salt("monitor-scroll").auto_shrink([false, false]).show(ui, |ui| {
                let w = ui.available_width();
                let gap = 2.0;
                let cols = (((w + gap) / (40.0 + gap)).floor() as usize).max(1);
                let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
                let ch = 30.0;
                let rows = 512usize.div_ceil(cols);
                let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (ch + gap)), Sense::hover());
                let p = ui.painter();
                let clip = ui.clip_rect();
                for i in 0..512 {
                    let r = Rect::from_min_size(
                        pos2(area.left() + (i % cols) as f32 * (cw + gap), area.top() + (i / cols) as f32 * (ch + gap)),
                        vec2(cw, ch),
                    );
                    if !clip.intersects(r) {
                        continue;
                    }
                    let v = self.v.dmx[i];
                    p.rect_filled(r, egui::CornerRadius::same(3), RAISED);
                    let fill_h = v as f32 / 255.0 * r.height();
                    p.rect_filled(
                        Rect::from_min_max(pos2(r.left(), r.bottom() - fill_h), r.max),
                        egui::CornerRadius::same(3),
                        rgba(79, 140, 255, 0.35),
                    );
                    p.text(
                        pos2(r.center().x, r.top() + 2.0),
                        Align2::CENTER_TOP,
                        (i + 1).to_string(),
                        font(FS_XS),
                        MUTED,
                    );
                    p.text(
                        pos2(r.center().x, r.bottom() - 2.0),
                        Align2::CENTER_BOTTOM,
                        v.to_string(),
                        font(FS_XS),
                        TEXT,
                    );
                }
            });
        });
    }

    // ---------------------------------------------------------------- сповіщення --
    pub fn show_toast(&mut self, ctx: &egui::Context) {
        let Some(t) = &self.toast else { return };
        if std::time::Instant::now() > t.until {
            self.toast = None;
            return;
        }
        let screen = ctx.screen_rect();
        egui::Area::new(Id::new("toast"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -S4))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(if t.info { RAISED } else { TOAST_ERR_BG })
                    .stroke(Stroke::new(1.0_f32, if t.info { LINE_STRONG } else { DANGER }))
                    .corner_radius(radius(R_MD))
                    .inner_margin(egui::Margin::symmetric(S4 as i8, S2 as i8))
                    .show(ui, |ui| {
                        ui.set_max_width(screen.width() * 0.9);
                        ui.label(RichText::new(&t.text).size(FS_MD));
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// Колір плашки програми LED-бару.
enum Swatch {
    Solid(Color32),
    Rainbow,
}

fn program_swatch(p: &nitor_core::fixtures::Program) -> Option<Swatch> {
    let table: [(&str, Color32); 7] = [
        ("Червоний", Color32::from_rgb(0xff, 0x20, 0x20)),
        ("Зелений", Color32::from_rgb(0x20, 0xe0, 0x40)),
        ("Синій", Color32::from_rgb(0x20, 0x50, 0xff)),
        ("Жовтий", Color32::from_rgb(0xff, 0xc8, 0x00)),
        ("Фіолетовий", Color32::from_rgb(0xa0, 0x20, 0xff)),
        ("Блакитний", Color32::from_rgb(0x00, 0xd0, 0xff)),
        ("Білий", Color32::WHITE),
    ];
    if p.name.starts_with("Круговий") {
        return Some(Swatch::Rainbow);
    }
    table.iter().find(|(k, _)| p.name.starts_with(k)).map(|(_, c)| Swatch::Solid(*c))
}

fn paint_swatch(p: &egui::Painter, r: Rect, sw: Swatch, gradient: bool) {
    let c = r.center();
    match sw {
        Swatch::Rainbow => {
            let cols = [
                Color32::from_rgb(0xff, 0x22, 0x22),
                Color32::from_rgb(0xff, 0xcc, 0x00),
                Color32::from_rgb(0x22, 0xee, 0x44),
                Color32::from_rgb(0x22, 0x55, 0xff),
                Color32::from_rgb(0xaa, 0x22, 0xff),
            ];
            for (i, col) in cols.iter().enumerate() {
                let a0 = i as f32 / 5.0 * std::f32::consts::TAU;
                let a1 = (i + 1) as f32 / 5.0 * std::f32::consts::TAU;
                let pts: Vec<Pos2> = std::iter::once(c)
                    .chain((0..=6).map(|k| {
                        let a = a0 + (a1 - a0) * k as f32 / 6.0;
                        c + vec2(a.sin(), -a.cos()) * 6.0
                    }))
                    .collect();
                p.add(egui::Shape::convex_polygon(pts, *col, Stroke::NONE));
            }
        }
        Swatch::Solid(col) if gradient => {
            // linear-gradient(90deg, #000, колір)
            let mut mesh = egui::Mesh::default();
            let n = 12;
            for k in 0..=n {
                let a = std::f32::consts::PI * (k as f32 / n as f32) - std::f32::consts::FRAC_PI_2;
                let top = c + vec2(a.sin() * 6.0, -a.cos() * 6.0);
                let bot = c + vec2(a.sin() * 6.0, a.cos() * 6.0);
                let t = (top.x - r.left()) / r.width();
                let shade = Color32::from_rgb(
                    (col.r() as f32 * t) as u8,
                    (col.g() as f32 * t) as u8,
                    (col.b() as f32 * t) as u8,
                );
                mesh.colored_vertex(top, shade);
                mesh.colored_vertex(bot, shade);
                if k > 0 {
                    let i = (k * 2) as u32;
                    mesh.add_triangle(i - 2, i - 1, i);
                    mesh.add_triangle(i - 1, i, i + 1);
                }
            }
            p.add(egui::Shape::mesh(mesh));
        }
        Swatch::Solid(col) => {
            p.circle_filled(c, 6.0, col);
        }
    }
    p.circle_stroke(c, 6.0, Stroke::new(1.0_f32, rgba(255, 255, 255, 0.4)));
}

/// Значення числа з плаваючою крапкою для підказки.
pub fn _fmt(v: f64) -> String {
    fmt_num(v)
}

#[allow(dead_code)]
fn _unused(_: &Engine) {}
