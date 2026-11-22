//! Попередження для другої копії програми: Nitor уже відкрито. Дві копії одночасно керували б
//! одним DMX-виходом, тому друга не запускається. Кнопка «Перейти до Nitor» показує вікно першої
//! (подія `Local\Nitor.Show`).
//!
//! Вікно має сталий розмір і не змінює його після появи (див. `splash.rs`).

use egui::{Align, Layout, RichText, Sense, Stroke, vec2};

use crate::winshell;
use nitor_ui::theme::*;
use nitor_ui::widgets::{Kind, btn};

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 222.0;

/// Показує попередження і повертається, коли його закрили.
pub fn run() {
    let icon = image::load_from_memory(include_bytes!("../../../assets/icon.ico"))
        .ok()
        .map(|i| i.into_rgba8())
        .map(|i| egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Nitor")
        .with_app_id("Nitor")
        .with_inner_size([WIDTH, HEIGHT])
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, persist_window: false, centered: true, ..Default::default() };
    crate::set_renderer(&mut options);
    let _ = eframe::run_native(
        "Nitor",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            nitor_ui::theme::install_fonts(&cc.egui_ctx);
            nitor_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Already::default()))
        }),
    );
}

#[derive(Default)]
struct Already {
    closing: bool,
}

impl eframe::App for Already {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut show = false;
        let mut close = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, LINE))).show(
            ctx,
            |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // вікно без рамки: перетягується за будь-яке місце, крім кнопок
                let drag = ui.interact(ui.max_rect(), egui::Id::new("already-drag"), Sense::click_and_drag());
                if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 20, bottom: 0 }).show(
                    ui,
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Image::new(egui::include_image!("../../../assets/icon.svg"))
                                    .fit_to_exact_size(vec2(28.0, 28.0)),
                            );
                            ui.add_space(10.0);
                            ui.label(RichText::new("Nitor").font(bold(FS_LG)));
                        });
                        ui.add_space(18.0);
                        ui.label(RichText::new("Nitor уже відкрито").font(bold(FS_XL)));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "Друга копія не запускається, щоб дві програми не керували світлом одночасно. \
                                 Перейдіть до вже відкритого вікна.",
                            )
                            .size(FS_SM)
                            .color(MUTED),
                        );
                    },
                );
                // кнопки — внизу вікна
                let footer_h = 1.0 + 14.0 + CONTROL_H + 14.0;
                ui.add_space((ui.available_height() - footer_h).max(0.0));
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, LINE);
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 14)).show(ui, |ui| {
                    ui.allocate_ui_with_layout(vec2(w - 48.0, CONTROL_H), Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        show = btn("Перейти до Nitor").kind(Kind::Primary).key(Some("Enter".into())).show(ui).clicked();
                        close = btn("Закрити").key(Some("Esc".into())).show(ui).clicked();
                    });
                });
            },
        );
        show |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
        close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if show {
            winshell::signal_existing();
        }
        if (show || close) && !self.closing {
            self.closing = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        screenshot(ctx);
    }
}

/// Знімок вікна для CI (`NITOR_SCREENSHOT`): після кількох кадрів зберігає PNG і закривається.
fn screenshot(ctx: &egui::Context) {
    let Some(path) = std::env::var_os("NITOR_SCREENSHOT") else { return };
    if ctx.cumulative_frame_nr() == 20 {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
    }
    let shot = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = shot {
        let [w, h] = img.size;
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
            let _ = buf.save(std::path::PathBuf::from(&path));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    ctx.request_repaint();
}
