//! Вікно запуску: з'являється одразу після старту, перевіряє оновлення. Якщо є новіша версія,
//! програма не відкривається, доки її не встановити: вікно показує, що нового, і кнопку
//! «Оновити». Без інтернету або якщо оновлення не можна встановити — програма відкривається як зазвичай.

use std::time::Instant;

use egui::{Align, Layout, RichText, Sense, Stroke, vec2};

use crate::app::App;
use crate::update::{SplashPhase, Status, decide, progress_percent, short_notes};
use nitor_ui::theme::*;
use nitor_ui::widgets::{Kind, btn};

pub const WIDTH: f32 = 560.0;

pub struct Splash {
    pub t0: Instant,
    pub requested: bool,
    pub height: f32,
    pub logged: Option<SplashPhase>,
}

impl Splash {
    pub fn new() -> Splash {
        Splash { t0: Instant::now(), requested: false, height: 0.0, logged: None }
    }
}

impl App {
    /// Кадр вікна запуску. `true` — пора відкривати програму.
    pub fn splash_frame(&mut self, ctx: &egui::Context) -> bool {
        let Some(mut sp) = self.splash.take() else { return true };
        let u = self.updater.info();
        let phase = decide(&u, sp.t0.elapsed().as_secs_f64(), sp.requested);
        if sp.logged != Some(phase) {
            log::info!("Вікно запуску: {phase:?}");
            sp.logged = Some(phase);
        }
        if phase == SplashPhase::Open {
            log::info!("Вікно запуску: відкриття програми через {:.1} с", sp.t0.elapsed().as_secs_f64());
            return true;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            // програму так і не відкрили — вимикаємо її повністю
            self.quitting = true;
            return false;
        }
        let mut close = false;
        let mut update = false;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, LINE)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // заголовок: логотип, назва, версія — за нього вікно можна перетягувати
                let head = ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.set_min_height(16.0 + 28.0);
                    ui.add_space(0.0);
                    ui.vertical(|ui| {
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Image::new(egui::include_image!("../../../assets/icon.svg"))
                                    .fit_to_exact_size(vec2(28.0, 28.0)),
                            );
                            ui.add_space(10.0);
                            ui.label(RichText::new("Nitor").font(bold(FS_LG)));
                            ui.add_space(8.0);
                            ui.label(RichText::new(&u.current).size(FS_SM).color(MUTED));
                        });
                    });
                });
                let drag = ui.interact(head.response.rect, egui::Id::new("splash-drag"), Sense::drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 18, bottom: 22 }).show(
                    ui,
                    |ui| {
                        let (heading, status) = match phase {
                            SplashPhase::Checking => ("Запуск Nitor".to_string(), "Перевірка оновлень…".to_string()),
                            SplashPhase::Update => (
                                format!("Доступна версія {}", u.latest),
                                format!(
                                    "Встановлена версія {}. Щоб працювати далі, оновіть програму: це займе хвилину, \
                                 сцени й налаштування збережуться.",
                                    u.current
                                ),
                            ),
                            SplashPhase::Installing => (
                                "Оновлення Nitor".to_string(),
                                if u.progress.is_empty() {
                                    "Підготовка…".into()
                                } else {
                                    u.progress.clone()
                                },
                            ),
                            SplashPhase::Failed => (
                                "Не вдалося оновити".to_string(),
                                "Перевірте підключення до інтернету й спробуйте ще раз.".to_string(),
                            ),
                            SplashPhase::Open => (String::new(), String::new()),
                        };
                        ui.label(RichText::new(heading).font(bold(FS_XL)));
                        ui.add_space(6.0);
                        ui.label(RichText::new(&status).size(FS_SM).color(MUTED));
                        ui.add_space(18.0);
                        match phase {
                            SplashPhase::Checking | SplashPhase::Installing => {
                                progress(ui, progress_percent(&status));
                            }
                            SplashPhase::Update => {
                                let notes = short_notes(&u.notes, 6);
                                if !notes.is_empty() {
                                    egui::Frame::new()
                                        .fill(RAISED)
                                        .corner_radius(radius(R_MD))
                                        .inner_margin(egui::Margin::symmetric(12, 10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.label(RichText::new(notes).size(FS_SM));
                                        });
                                }
                            }
                            SplashPhase::Failed => {
                                egui::Frame::new()
                                    .fill(TOAST_ERR_BG)
                                    .stroke(Stroke::new(1.0_f32, DANGER))
                                    .corner_radius(radius(R_MD))
                                    .inner_margin(egui::Margin::symmetric(12, 10))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(RichText::new(&u.error).size(FS_SM));
                                    });
                            }
                            SplashPhase::Open => {}
                        }
                    },
                );
                if matches!(phase, SplashPhase::Update | SplashPhase::Failed) {
                    let w = ui.available_width();
                    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, LINE);
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 14)).show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(w - 48.0, CONTROL_H),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let label = if phase == SplashPhase::Failed {
                                    "Спробувати ще раз"
                                } else {
                                    "Оновити"
                                };
                                update = btn(label).kind(Kind::Primary).show(ui).clicked();
                                close = btn("Закрити").show(ui).clicked();
                            },
                        );
                    });
                }
                ui.min_rect().height()
            });
        if update || (phase == SplashPhase::Update && ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
            sp.requested = true;
            self.update_tried = true;
            if self.updater.info().status != Status::Installing {
                self.updater.apply();
            }
        }
        if close {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // висота вікна — за вмістом
        let h = panel.inner.ceil();
        if (h - sp.height).abs() > 0.5 {
            sp.height = h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(WIDTH, h)));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        self.splash = Some(sp);
        false
    }

    /// Перехід від вікна запуску до вікна програми.
    /// Перетворює вікно запуску на головне. Поки система не змінить розмір вікна, малюємо лише
    /// тло (див. `main_window_ready`): інакше Windows розтягує кадр, намальований у маленькому
    /// вікні, на весь новий розмір — інтерфейс на мить стає збільшеним і напівпрозорим.
    pub fn open_main_window(&mut self, ctx: &egui::Context) {
        let size = vec2(1400.0, 900.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(900.0, 600.0)));
        let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1600.0, 1000.0));
        let size = vec2(size.x.min(monitor.x - 40.0), size.y.min(monitor.y - 80.0));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        let pos = ((monitor - size) / 2.0).max(vec2(0.0, 0.0));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos.to_pos2()));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.main_pending = Some((size, std::time::Instant::now() + std::time::Duration::from_millis(700)));
    }

    /// `false`, поки вікно ще не набуло розміру головного: тоді цей кадр — суцільне тло.
    pub fn main_window_ready(&mut self, ctx: &egui::Context) -> bool {
        let Some((size, deadline)) = self.main_pending else { return true };
        let now = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size());
        let resized = now.is_some_and(|s| (s.x - size.x).abs() <= 2.0 && (s.y - size.y).abs() <= 2.0);
        if resized || std::time::Instant::now() >= deadline {
            // розмір уже правильний: цей кадр — ще тло, наступний — інтерфейс у новому розмірі
            self.main_pending = None;
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |_| {});
        false
    }
}

/// Смуга прогресу: невідомо, скільки чекати, — смуга «біжить».
fn progress(ui: &mut egui::Ui, pct: Option<u32>) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, egui::CornerRadius::same(3), LINE);
    match pct {
        Some(pct) => {
            let fill = egui::Rect::from_min_size(r.min, vec2(w * pct as f32 / 100.0, 6.0));
            p.rect_filled(fill, egui::CornerRadius::same(3), ACCENT);
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let seg = w * 0.3;
            let x = ((t * 0.8).fract() * (w + seg)) - seg;
            let a = (r.left() + x).max(r.left());
            let b = (r.left() + x + seg).min(r.right());
            if b > a {
                p.rect_filled(egui::Rect::from_x_y_ranges(a..=b, r.y_range()), egui::CornerRadius::same(3), ACCENT);
            }
            ui.ctx().request_repaint();
        }
    }
}
