//! Головне вікно: верхня панель, прилади, колір і палітра, ефекти, сцени, DMX-монітор.

use egui::{
    Align, Align2, Color32, ColorImage, Id, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureOptions,
    Ui, pos2, vec2,
};
use nitor_core::color::{hsv_to_rgb, rgb_to_hsv};
use nitor_core::engine::{ColorValues, EffectPatch};
use nitor_core::fixtures::programs;
use nitor_core::{Command, EffectKind, Rgb};

use crate::app::{Act, App, FLASH_WHITE, FlashHold, Pane, c32, cap, readable_text, speed_from_slider};
use crate::hotkeys::short_key;
use crate::menus::paint_static_chip;
use nitor_core::engine::{FlashSource, StaticSource};
use nitor_core::tempo::AudioSource;
use nitor_ui::anim;
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

/// Радіус колірного кола на екрані.
const WHEEL_R: f32 = 110.0;

/// HSV → RGB для колірного кола (округлення до цілих).
fn wheel_hsv(h: f64, s: f64, v: f64) -> Rgb {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Rgb((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

impl App {
    // ---------------------------------------------------------- верхня панель --
    /// Кнопка верхньої панелі: з назвою або лише значок (вибір — у меню правої кнопки миші).
    #[allow(clippy::too_many_arguments)]
    fn top_button(
        &mut self,
        ui: &mut Ui,
        id: &'static str,
        icon: &'static str,
        label: &str,
        kind: Kind,
        key: Option<String>,
        tip: &str,
    ) -> bool {
        let r = self.light_button(ui, id, icon, label, kind, key, tip);
        r.clicked()
    }

    /// «Очистити», «Спалах» чи Blackout: з назвою або лише значок; права кнопка миші — меню
    /// з плавністю (частотою) і показом назви.
    #[allow(clippy::too_many_arguments)]
    fn light_button(
        &mut self,
        ui: &mut Ui,
        id: &'static str,
        icon: &'static str,
        label: &str,
        kind: Kind,
        key: Option<String>,
        tip: &str,
    ) -> egui::Response {
        let short = self.top_compact || self.v.prefs.short_buttons.iter().any(|b| b == id);
        let key = key.filter(|_| !self.top_compact);
        let r = if short {
            self.title_icon(ui, id, icon, label, kind, key, &format!("{label} — {tip}"))
        } else {
            btn(label).icon(icon).kind(kind).key(key).tip(tip).show(ui)
        };
        if r.secondary_clicked() {
            let items = self.light_button_menu(id, label, r.rect);
            self.want_menu(items);
        }
        r
    }

    /// Кнопка заголовка без назви. Через секунду наведення назва розгортається, якщо в заголовку
    /// для неї є місце; якщо немає — спливає підказка, як раніше.
    #[allow(clippy::too_many_arguments)]
    fn title_icon(
        &mut self,
        ui: &mut Ui,
        id: &str,
        icon: &'static str,
        label: &str,
        kind: Kind,
        key: Option<String>,
        tip: &str,
    ) -> egui::Response {
        let hid = Id::new(("title-hover", id));
        let oid = Id::new(("title-open", id));
        let now = ui.input(|i| i.time);
        let since: Option<f64> = ui.data(|d| d.get_temp(hid));
        // розгорнута кнопка не згортається, поки миша поруч (на сусідній кнопці теж),
        // інакше кнопки під курсором стрибали б назад
        let last: Option<Rect> = ui.data(|d| d.get_temp(oid));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let near = last.zip(pointer).is_some_and(|(r, p)| r.expand2(vec2(44.0, 12.0)).contains(p));
        let text_w = ui.fonts(|f| f.layout_no_wrap(label.to_string(), bold(FS_MD), TEXT).size().x);
        // shortcut: ширина розгорнутої кнопки — оцінка з запасом на значок і відступи; рахувати точно, якщо кнопки стрибатимуть
        let fits = self.title_slack >= text_w + 3.0 * S4 + 18.0;
        let open = fits && (near || since.is_some_and(|t| now - t >= 1.0));
        let t = anim::anim_bool(ui.ctx(), oid.with("anim"), open, 0.22);
        // плавний початок і кінець руху
        let t = t * t * (3.0 - 2.0 * t);
        let b = if t > 0.0 { btn(label).icon(icon).reveal(t) } else { icon_btn(icon) };
        let r = b.kind(kind).key(key).tip(tip).quiet_tip(fits).show(ui);
        if r.hovered() {
            if since.is_none() {
                ui.data_mut(|d| d.insert_temp(hid, now));
            }
            if fits && !open {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
        } else if since.is_some() {
            ui.data_mut(|d| d.remove::<f64>(hid));
        }
        if open {
            ui.data_mut(|d| d.insert_temp(oid, r.rect));
        } else if last.is_some() {
            ui.data_mut(|d| d.remove::<Rect>(oid));
        }
        self.title_open |= t > 0.0;
        r
    }

    /// Олівець: увімкнути редагування вигляду; під час редагування — «Готово» і «Скинути вигляд».
    fn layout_buttons(&mut self, ui: &mut Ui) {
        if !self.layout_edit {
            let tip = "Змінити вигляд головного екрана";
            if self.title_icon(ui, "edit", "edit", "Змінити вигляд", Kind::Ghost, None, tip).clicked() {
                self.layout_edit = true;
            }
            return;
        }
        let c = self.top_compact;
        let done = if c { icon_btn("check") } else { btn("Готово").icon("check") };
        if done.primary().tip("Готово: закінчити редагування вигляду").show(ui).clicked()
        {
            self.layout_edit = false;
        }
        let reset = if c { icon_btn("reset") } else { btn("Скинути вигляд").icon("reset") };
        if reset.kind(Kind::Ghost).tip("Скинути вигляд: стандартні ширина й порядок стовпців").show(ui).clicked()
        {
            self.col_anim = Some((ui.input(|i| i.time), self.col_rects.clone()));
            self.set_prefs(|p| {
                p.columns.clear();
                p.col_width.clear();
                p.scenes_first = false;
            });
        }
    }

    /// Панель вікна, як у вікні запуску й інсталятора: логотип, «Nitor», DMX-вихід поруч,
    /// «Згорнути», «Розгорнути», «Закрити». Вікно перетягується за панель, подвійний клік — розгорнути.
    pub fn title_bar(&mut self, ctx: &egui::Context) {
        let max = ctx.input(|i| i.viewport().maximized == Some(true));
        egui::TopBottomPanel::top("titlebar")
            .exact_height(56.0)
            .frame(egui::Frame::new().fill(BG).stroke(Stroke::new(1.0_f32, LINE)).inner_margin(egui::Margin {
                left: S4 as i8,
                right: S2 as i8,
                top: 0,
                bottom: 0,
            }))
            .show(ctx, |ui| {
                self.title_open = false;
                let drag = ui.interact(ui.max_rect(), Id::new("title-drag"), Sense::click_and_drag());
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                } else if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                let row = ui.max_rect();
                // праворуч — кнопки програми, роздільник і кнопки вікна Windows
                let right = ui
                    .scope_builder(
                        egui::UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)),
                        |ui| {
                            self.window_controls(ui, max);
                            ui.add_space(S2);
                            let (sep, _) = ui.allocate_exact_size(vec2(1.0, 28.0), Sense::hover());
                            ui.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0_f32, LINE_STRONG));
                            ui.add_space(S2);
                            ui.spacing_mut().item_spacing.x = S3;
                            self.app_buttons(ui);
                        },
                    )
                    .response
                    .rect;
                // ліворуч — назва, вихід DMX, майстер і дії зі світлом; що не вмістилося — обрізається
                let left_max = Rect::from_min_max(row.min, pos2((right.left() - S3).max(row.left()), row.max.y));
                let left = ui
                    .scope_builder(
                        egui::UiBuilder::new().max_rect(left_max).layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            ui.set_clip_rect(left_max.intersect(ui.clip_rect()));
                            self.title_left(ui);
                        },
                    )
                    .response
                    .rect;
                // місце для повних назв міряємо, поки назви показано; не вистачає — лише значки
                let need_id = Id::new("title-need");
                if !self.top_compact {
                    // вихід DMX може звузитися до 160 px, перш ніж ховати назви кнопок
                    let pill: f32 = ui.data(|d| d.get_temp(Id::new("title-pill"))).unwrap_or(0.0);
                    let need = left.width() - pill + pill.min(160.0) + right.width() + S3;
                    ui.data_mut(|d| d.insert_temp(need_id, need));
                }
                let need: f32 = ui.data(|d| d.get_temp(need_id)).unwrap_or(0.0);
                // вільне місце міряємо без розгорнутих при наведенні назв, інакше вони б самі себе згортали
                if !self.title_open {
                    self.title_slack = row.width() - need;
                }
                let compact = need > row.width();
                if compact != self.top_compact {
                    self.top_compact = compact;
                    ui.ctx().request_repaint();
                }
            });
    }

    /// Ліва частина заголовка: логотип, назва, вихід DMX, майстер, «Очистити», «Спалах», Blackout.
    fn title_left(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.x = S3;
        let logo = ui.add(
            egui::Image::new(egui::include_image!("../../../assets/icon.svg")).fit_to_exact_size(vec2(32.0, 32.0)),
        );
        // назва й версія — однією лінією по низу літер
        ui.spacing_mut().item_spacing.x = S2;
        let mut brand = logo.rect;
        // у тісноті лишається лише логотип
        if !self.top_compact {
            brand |= ui.label(RichText::new("Nitor").font(bold(22.0))).rect;
            brand |= ui.label(RichText::new(format!("v{}", nitor_core::VERSION)).size(FS_SM).color(MUTED)).rect;
        }
        // логотип, назва й версія — до «Про програму»
        let about = ui.interact(brand, Id::new("title-about"), Sense::click());
        about.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Про програму"));
        if about.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.v.operator {
            self.open_settings(Some(crate::dialogs::Tab::About));
        }
        ui.add_space(S2);
        ui.spacing_mut().item_spacing.x = S3;
        // вихід DMX — до 320 px, але не більше, ніж лишається після майстра й кнопок
        let rest_id = Id::new("title-rest");
        let rest: f32 = ui.data(|d| d.get_temp(rest_id)).unwrap_or(0.0);
        let pill = self.output_pill(ui, (ui.available_width() - rest - S3).min(320.0));
        ui.data_mut(|d| d.insert_temp(Id::new("title-pill"), pill));
        let start = ui.cursor().min.x;
        ui.add_space(S2);
        let mut m = self.v.show.master as f32;
        ui.spacing_mut().item_spacing.x = S1;
        ui.label(RichText::new("Майстер").size(FS_MD));
        widgets::help(
            ui,
            "Загальна яскравість усього світла. 100 % — як задано в кольорах і сценах, 50 % — усе вдвічі тьмяніше, \
             0 % — темно. Кольори при цьому не губляться: підніміть майстер, і світло повернеться.",
        );
        ui.spacing_mut().item_spacing.x = S3;
        let sw = if self.top_compact { 90.0 } else { 160.0 };
        let mut changed = widgets::slider_named(ui, "Майстер", &mut m, 0.0, 255.0, ACCENT, sw).changed();
        changed |= widgets::value_box_sized(ui, Id::new("master"), &mut m, 0.0, 255.0, Shown::Pct, 56.0, FS_LG);
        if changed {
            self.send(Command::SetMaster(m as u8));
        }
        // дії зі світлом — одразу біля майстра
        let h = self.v.hotkeys.clone();
        let k = |id: &str| short_key(&h, id);
        ui.add_space(S1);
        let label = if self.sel.is_empty() { "Очистити все" } else { "Очистити вибрані" };
        if self.top_button(ui, "clear", "eraser", label, Kind::Warn, k("clear"), "Очистити кольори") {
            self.clear_colors();
        }
        self.flash_button(ui, k("flash"));
        let kind = if self.v.show.blackout { Kind::DangerOn } else { Kind::Danger };
        if self.top_button(ui, "blackout", "bulbOff", "Blackout", kind, k("blackout"), "Вимкнути все світло")
        {
            self.send(Command::Blackout(None));
        }
        let rest = ui.cursor().min.x - start;
        ui.data_mut(|d| d.insert_temp(rest_id, rest));
    }

    /// Кнопки програми праворуч у заголовку (справа наліво): налаштування, оператор, олівець, оновлення.
    fn app_buttons(&mut self, ui: &mut Ui) {
        if self.v.operator {
            self.exit_operator_button(ui);
            return;
        }
        let key = short_key(&self.v.hotkeys, "settings");
        let s = "Налаштування";
        if self.title_icon(ui, "settings", "settings", s, Kind::Ghost, key, s).clicked() {
            self.open_settings(None);
        }
        let s = "Режим оператора";
        if self.title_icon(ui, "lock", "lock", s, Kind::Ghost, None, s).clicked() {
            self.act(ui.ctx(), Act::AskOperator);
        }
        self.layout_buttons(ui);
        self.update_button(ui);
    }

    /// «Згорнути», «Розгорнути», «Закрити» (справа наліво).
    fn window_controls(&mut self, ui: &mut Ui, max: bool) {
        let ctx = ui.ctx().clone();
        ui.spacing_mut().item_spacing.x = S1;
        if icon_btn("x").kind(Kind::Close).tip("Закрити").show(ui).clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let tip = if max { "Відновити" } else { "Розгорнути" };
        if window_button(ui, tip, |p, c| {
            let s = Stroke::new(1.5_f32, TEXT);
            if max {
                // два вікна одне за одним
                p.rect_stroke(Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(9.0, 9.0)), 1.0, s, StrokeKind::Middle);
                p.line_segment([c + vec2(-3.0, -4.5), c + vec2(4.5, -4.5)], s);
                p.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 3.0)], s);
            } else {
                p.rect_stroke(Rect::from_center_size(c, vec2(11.0, 11.0)), 1.0, s, StrokeKind::Middle);
            }
        }) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
        if window_button(ui, "Згорнути", |p, c| {
            p.hline(c.x - 6.0..=c.x + 6.0, c.y, Stroke::new(2.0_f32, TEXT));
        }) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Вікно без системної рамки: його краї тягнуть, щоб змінити розмір (коли воно не розгорнуте).
    pub fn window_edges(&mut self, ctx: &egui::Context) {
        use egui::viewport::ResizeDirection as D;
        let (max, pos, pressed, screen) = ctx.input(|i| {
            (i.viewport().maximized == Some(true), i.pointer.hover_pos(), i.pointer.primary_pressed(), i.screen_rect())
        });
        let Some(p) = pos else { return };
        if max {
            return;
        }
        let e = 5.0;
        let (l, r, t, b) =
            (p.x <= screen.left() + e, p.x >= screen.right() - e, p.y <= screen.top() + e, p.y >= screen.bottom() - e);
        let dir = match (l, r, t, b) {
            (true, _, true, _) => D::NorthWest,
            (_, true, true, _) => D::NorthEast,
            (true, _, _, true) => D::SouthWest,
            (_, true, _, true) => D::SouthEast,
            (true, ..) => D::West,
            (_, true, ..) => D::East,
            (_, _, true, _) => D::North,
            (.., true) => D::South,
            _ => return,
        };
        let cursor = match dir {
            D::North | D::South => egui::CursorIcon::ResizeVertical,
            D::East | D::West => egui::CursorIcon::ResizeHorizontal,
            D::NorthWest | D::SouthEast => egui::CursorIcon::ResizeNwSe,
            _ => egui::CursorIcon::ResizeNeSw,
        };
        ctx.set_cursor_icon(cursor);
        if pressed {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    /// «Спалах»: тримати — повне світло; права кнопка миші — постійне світло чи мерехтіння з частотою.
    fn flash_button(&mut self, ui: &mut Ui, key: Option<String>) {
        let rate = self.v.prefs.flash_rate;
        let (icon, label, tip) = if rate > 0.0 {
            ("bolt", format!("Спалах {}\u{a0}Гц", rate.round()), "Строб, поки кнопку тримають")
        } else {
            ("sun", "Спалах".to_string(), "Повний білий, поки кнопку тримають")
        };
        let kind = if self.v.flashing { Kind::Toggled } else { Kind::Outline };
        let r = self.light_button(ui, "flash", icon, &label, kind, key, tip);
        // права кнопка відкриває меню, а не спалах
        if r.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) {
            self.flash_color(FLASH_WHITE, FlashHold::Pointer);
        }
    }

    /// Вікно точної частоти «Спалаху» під кнопкою (з її контекстного меню).
    pub fn flash_popup(&mut self, ctx: &egui::Context) {
        let Some(anchor) = self.flash_cfg else { return };
        let mut rate = self.v.prefs.flash_rate as f32;
        let mut changed = false;
        let area = egui::Area::new(Id::new("flash-cfg"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(anchor.right_bottom() + vec2(0.0, S1))
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0_f32, LINE_STRONG))
                    .corner_radius(radius(R_LG))
                    .inner_margin(egui::Margin::same(S4 as i8))
                    .shadow(SHADOW)
                    .show(ui, |ui| {
                        ui.set_width(300.0);
                        ui.label(RichText::new("Спалах").font(bold(FS_MD)));
                        ui.add_space(S1);
                        ui.label(widgets::muted(
                            "Поки кнопку тримають — повне світло. З частотою світло блимає, як строб.",
                        ));
                        ui.add_space(S3);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = S1;
                            for (v, label) in [(0.0, "Постійно"), (5.0, "5 Гц"), (10.0, "10 Гц"), (15.0, "15 Гц")]
                            {
                                let on = (rate - v).abs() < 0.5;
                                let b = btn(label).small().kind(if on { Kind::Toggled } else { Kind::Normal });
                                if b.show(ui).clicked() {
                                    rate = v;
                                    changed = true;
                                }
                            }
                        });
                        ui.add_space(S3);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = S2;
                            ui.label(widgets::muted("Частота"));
                            let w = ui.available_width() - 56.0 - S2;
                            let max = nitor_core::engine::FLASH_RATE_MAX as f32;
                            changed |=
                                widgets::slider_named(ui, "Частота спалаху", &mut rate, 0.0, max, ACCENT, w).changed();
                            changed |=
                                widgets::value_box(ui, Id::new("flashRate"), &mut rate, 0.0, max, Shown::Hz, 56.0);
                        });
                    });
            });
        if changed {
            self.set_prefs(|p| p.flash_rate = rate as f64);
        }
        let rect = area.response.rect;
        nitor_ui::anim::pop_in(ctx, area.response.layer_id, rect.right_top(), 0.96);
        let close = ctx.input(|i| {
            i.key_pressed(egui::Key::Escape)
                || (i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p)))
        });
        if close {
            self.flash_cfg = None;
        }
    }

    /// Вихід DMX біля назви програми: інтерфейс, а назва пристрою — якщо так задано в налаштуваннях.
    fn output_pill(&mut self, ui: &mut Ui, room: f32) -> f32 {
        let o = &self.v.output;
        let names = |k: &str| match k {
            "udmx" => "uDMX",
            "none" => "Симулятор",
            "enttec_pro" => "DMX USB Pro",
            "open_dmx" => "Open DMX",
            "artnet" => "Art-Net",
            _ => "sACN",
        };
        let kind = names(o.kind);
        // помилка: лише назва інтерфейсу й червоне переливання, сам текст помилки — у налаштуваннях виходу
        let text = match () {
            _ if !o.ok => kind.to_string(),
            _ if self.v.prefs.output_device => format!("{kind}: {}", o.status),
            _ => kind.to_string(),
        };
        let tip = if !o.ok {
            "Проблема з DMX-виходом. Клацніть, щоб побачити, що сталося, і змінити вихід.".to_string()
        } else if o.paused {
            "Кадри не відправляються, бо все світло вимкнене. Відправка продовжиться, щойно щось увімкнеться."
                .to_string()
        } else {
            cap(&o.status)
        };
        let chrome = S3 * 2.0 + 8.0 + S2;
        // без місця для тексту лишається лише кольорова крапка стану
        let show_text = room - chrome >= 48.0;
        let mut job = egui::text::LayoutJob::simple_singleline(text.clone(), font(FS_SM), TEXT);
        job.wrap = egui::text::TextWrapping::truncate_at_width((room - chrome).max(1.0));
        let g = ui.fonts(|f| f.layout_job(job));
        let w = if show_text { chrome + g.size().x } else { S3 * 2.0 + 8.0 };
        let tip = if show_text { tip } else { format!("{text}\n{tip}") };
        let (r, resp) = ui.allocate_exact_size(vec2(w, CONTROL_H_SM), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Вихід DMX: {text}")));
        let p = ui.painter();
        if resp.has_focus() {
            p.rect_stroke(
                r.expand(2.0),
                egui::CornerRadius::same(255),
                Stroke::new(2.0_f32, ACCENT),
                StrokeKind::Outside,
            );
        }
        let (fill, stroke) = if o.ok {
            (RAISED, if resp.hovered() { LINE_STRONG } else { LINE })
        } else {
            // плавне червоне переливання, період 1,6 с; зі «Зменшити рух» — рівний червоний
            let k = if anim::reduced_motion() {
                1.0
            } else {
                let t = ui.input(|i| i.time) as f32;
                ui.ctx().request_repaint();
                0.5 - 0.5 * (t * std::f32::consts::TAU / 1.6).cos()
            };
            (RAISED.lerp_to_gamma(DANGER_STRONG, 0.15 + 0.45 * k), alpha(DANGER, 0.4 + 0.6 * k))
        };
        p.rect(r, egui::CornerRadius::same(255), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
        let dot = if o.ok { OK } else { WHITE };
        let c = pos2(r.left() + S3 + 4.0, r.center().y);
        if o.ok && o.paused {
            // пауза — дві риски замість крапки, без зміни ширини
            for dx in [-2.25, 2.25] {
                p.rect_filled(Rect::from_center_size(c + vec2(dx, 0.0), vec2(2.5, 9.0)), 1.0, MUTED);
            }
        } else {
            p.circle_filled(c, 4.0, dot);
        }
        if show_text {
            p.galley(pos2(r.left() + S3 + 8.0 + S2, r.center().y - g.size().y / 2.0), g, TEXT);
        }
        // індикатор виходу — одразу до вибору DMX-виходу
        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.v.operator {
            self.open_settings(Some(crate::dialogs::Tab::Output));
        }
        w
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
        let b = if self.top_compact { icon_btn(icon) } else { btn(text.clone()).icon(icon) };
        let tip = if self.top_compact { format!("{text}\n{tip}") } else { tip };
        if b.kind(kind).enabled(!busy).tip(tip).show(ui).clicked() {
            self.open_update_dialog();
        }
    }

    // ------------------------------------------------------------ розкладка --
    pub fn main_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            let mon_h = match (self.v.prefs.hide_monitor, self.monitor_open) {
                (true, _) => 0.0,
                (false, true) => (full.height() * 0.28).clamp(120.0, 360.0) + 52.0,
                (false, false) => 46.0,
            };
            let top = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - mon_h));
            let order = col_order(&self.v.prefs.columns);
            let wide = full.width() >= COLS.iter().map(|c| c.2).sum::<f32>();
            let stored = self.col_live.clone().unwrap_or_else(|| self.v.prefs.col_width.clone());
            let widths = col_widths(&order, &stored, full.width());
            let mut target = [Rect::NOTHING; 3];
            if wide {
                let mut x = top.left();
                for i in 0..3 {
                    target[i] = Rect::from_min_size(pos2(x, top.top()), vec2(widths[i], top.height()));
                    x += widths[i];
                }
                target[2].max.x = top.right();
            } else {
                // вузьке вікно: два стовпці, третій — під ними на всю ширину
                let h = top.height() * 0.62;
                let w1 = full.width() / 2.0;
                target[0] = Rect::from_min_size(top.min, vec2(w1, h));
                target[1] = Rect::from_min_max(pos2(top.left() + w1, top.top()), pos2(top.right(), top.top() + h));
                target[2] = Rect::from_min_max(pos2(top.left(), top.top() + h), top.max);
            }
            // обмін стовпців — плавно, за 0,25 с
            let mut rects = target;
            if let Some((t0, from)) = &self.col_anim {
                let t = anim::progress(ui.ctx(), *t0, 0.25);
                let e = 1.0 - (1.0 - t).powi(3);
                for (i, id) in order.iter().enumerate() {
                    if let Some((_, f)) = from.iter().find(|(fid, _)| fid == id) {
                        rects[i] = Rect::from_min_max(f.min.lerp(target[i].min, e), f.max.lerp(target[i].max, e));
                    }
                }
                if t < 1.0 {
                    ui.ctx().request_repaint();
                } else {
                    self.col_anim = None;
                }
            }
            self.col_rects = order.iter().copied().zip(rects).collect();
            let at = |id: &str| rects[order.iter().position(|o| *o == id).unwrap_or(0)];
            let (c1, c2, c3) = (at("fixtures"), at("color"), at("side"));
            let p = ui.painter();
            for r in rects {
                if r.left() > top.left() + 1.0 {
                    p.vline(r.left(), r.y_range(), Stroke::new(1.0_f32, LINE));
                }
                if r.top() > top.top() + 1.0 {
                    p.hline(r.x_range(), r.top(), Stroke::new(1.0_f32, LINE));
                }
            }
            if !self.v.prefs.hide_monitor {
                p.hline(full.x_range(), top.bottom(), Stroke::new(1.0_f32, LINE));
            }

            // фон панелей приймає правий клік (меню панелі) — вміст поверх нього має пріоритет
            let zone = |ui: &mut Ui, r: Rect, name: &str| ui.interact(r, Id::new(("zone", name)), Sense::click());
            // порожнє місце серед приладів: клік знімає вибір, протягування — виділення рамкою
            let z1 = ui.interact(c1, Id::new(("zone", "fixtures")), Sense::click_and_drag());
            let z2 = zone(ui, c2, "color");
            let z3 = zone(ui, c3, "side");

            self.column(ui, c1, "fixtures", |app, ui| app.fixtures_panel(ui));
            self.column(ui, c2, "color", |app, ui| app.color_panel(ui));
            let scenes_rect = self.column(ui, c3, "side", |app, ui| app.side_panel(ui));
            if self.layout_edit && !self.v.operator {
                self.layout_overlay(ui, &order, &rects, &widths, wide);
            }
            self.band_select(ui, &z1, c1);
            if !self.v.prefs.hide_monitor {
                let mon = Rect::from_min_max(pos2(full.left(), top.bottom() + 1.0), full.max);
                self.monitor_panel(ui, mon);
            }

            if z1.clicked() && !ui.input(|i| i.modifiers.shift) {
                self.sel.clear();
            }
            if self.pending_menu.is_none() {
                if z1.secondary_clicked() {
                    let m = self.fixtures_panel_menu();
                    self.want_menu(m);
                } else if z2.secondary_clicked() {
                    let m = self.color_menu();
                    self.want_menu(m);
                } else if z3.secondary_clicked() {
                    let pos = z3.interact_pointer_pos().unwrap_or_default();
                    let m = if scenes_rect.contains(pos) { self.scene_menu(None) } else { self.effect_menu() };
                    self.want_menu(m);
                }
            }
        });
    }

    /// Редагування вигляду: межі між стовпцями тягнуть мишею, стрілки міняють стовпці місцями.
    fn layout_overlay(
        &mut self,
        ui: &mut Ui,
        order: &[&'static str; 3],
        rects: &[Rect; 3],
        widths: &[f32; 3],
        wide: bool,
    ) {
        for r in rects {
            ui.painter().rect_stroke(
                r.shrink(3.0),
                radius(R_LG),
                Stroke::new(1.0_f32, alpha(ACCENT, 0.5)),
                StrokeKind::Inside,
            );
        }
        if wide {
            for k in 0..2 {
                let x = rects[k].right();
                let hit = Rect::from_x_y_ranges(x - 5.0..=x + 5.0, rects[k].y_range());
                let resp = ui
                    .interact(hit, Id::new(("col-edge", k)), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                    .on_hover_text("Потягніть, щоб змінити ширину");
                let hot = resp.hovered() || resp.dragged();
                ui.painter().vline(x, hit.y_range(), Stroke::new(if hot { 3.0_f32 } else { 2.0 }, ACCENT));
                // ручка посередині межі
                let grip = Rect::from_center_size(pos2(x, hit.center().y), vec2(8.0, 40.0));
                ui.painter().rect_filled(grip, radius(R_SM), ACCENT);
                if resp.dragged() {
                    let mut w = *widths;
                    let dx = resp.drag_delta().x;
                    let pair = w[k] + w[k + 1];
                    w[k] = (w[k] + dx).clamp(col_min(order[k]), (pair - col_min(order[k + 1])).max(col_min(order[k])));
                    w[k + 1] = pair - w[k];
                    self.col_live = Some(order.iter().map(|id| id.to_string()).zip(w).collect());
                }
                if resp.drag_stopped() {
                    if let Some(w) = self.col_live.take() {
                        self.set_prefs(|p| p.col_width = w);
                    }
                }
            }
        }
        // посередині внизу стовпця — назва й стрілки «ліворуч / праворуч»
        let mut swap = None;
        for (i, r) in rects.iter().enumerate() {
            let name = COLS.iter().find(|c| c.0 == order[i]).map_or("", |c| c.3);
            let size =
                vec2(widgets::text_width(ui, name, bold(FS_SM)) + 2.0 * CONTROL_H_SM + 2.0 * S3, CONTROL_H_SM + S2);
            let bar = Rect::from_center_size(pos2(r.center().x, r.bottom() - S4 - size.y / 2.0), size);
            ui.painter().rect(bar, radius(R_MD), RAISED, Stroke::new(1.0_f32, ACCENT), StrokeKind::Inside);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(bar.shrink(S1)).layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = S1;
                    let left = if wide {
                        "Перемістити ліворуч"
                    } else {
                        "Перемістити раніше"
                    };
                    if icon_btn("arrowLeft").small().kind(Kind::Ghost).enabled(i > 0).tip(left).show(ui).clicked() {
                        swap = Some(i - 1);
                    }
                    ui.label(RichText::new(name).font(bold(FS_SM)));
                    let right = if wide {
                        "Перемістити праворуч"
                    } else {
                        "Перемістити далі"
                    };
                    if icon_btn("arrowRight").small().kind(Kind::Ghost).enabled(i < 2).tip(right).show(ui).clicked() {
                        swap = Some(i);
                    }
                },
            );
        }
        if let Some(i) = swap {
            self.col_anim = Some((ui.input(|t| t.time), self.col_rects.clone()));
            let mut o = *order;
            o.swap(i, i + 1);
            self.set_prefs(|p| p.columns = o.iter().map(|s| s.to_string()).collect());
        }
    }

    /// Виділення приладів рамкою, як файлів на робочому столі. Shift або Ctrl — додати до вибору.
    fn band_select(&mut self, ui: &Ui, zone: &egui::Response, clip: Rect) {
        if zone.drag_started_by(egui::PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).or(zone.interact_pointer_pos());
            if let Some(start) = start {
                let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                self.band = Some((start, if add { self.sel.clone() } else { vec![] }));
            }
        }
        let Some((start, base)) = self.band.clone() else { return };
        if !zone.dragged() {
            self.band = None;
            return;
        }
        let cur = ui.input(|i| i.pointer.interact_pos()).unwrap_or(start);
        let band = Rect::from_two_pos(start, cur);
        let mut sel = base;
        for (id, r) in &self.tile_rects {
            if r.intersects(band) && !sel.iter().any(|s| s.split('#').next() == Some(id.as_str())) {
                sel.push(id.clone());
            }
        }
        self.sel = sel;
        ui.painter_at(clip).rect(
            band,
            radius(R_SM),
            alpha(ACCENT, 0.12),
            Stroke::new(1.0_f32, ACCENT),
            StrokeKind::Inside,
        );
        ui.ctx().request_repaint();
    }

    /// Колонка з прокруткою й відступами панелі. Повертає прямокутник, який позначив вміст.
    fn column(&mut self, ui: &mut Ui, rect: Rect, id: &str, add: impl FnOnce(&mut App, &mut Ui) -> Rect) -> Rect {
        let mut out = Rect::NOTHING;
        let inner = rect.shrink2(vec2(S5, 0.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).id_salt(id), |ui| {
            // смуги прокрутки приховано; прокрутка коліщатком лишається
            let sa = egui::ScrollArea::vertical()
                .id_salt(("scroll", id))
                .auto_shrink([false, false])
                // мишею тягнуть рамку виділення, а не прокручують
                .scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() })
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_space(S4);
                    ui.set_width(inner.width() - 1.0);
                    out = add(self, ui);
                    ui.add_space(S4);
                });
            // смуг прокрутки немає — тінь угорі чи внизу підказує, що там є ще вміст
            widgets::scroll_shadow(ui, &sa, BG);
        });
        out
    }

    // --------------------------------------------------------------- прилади --
    /// Пояснення для новачка біля назви панелі (знак «?»).
    fn panel_help(title: &str) -> Option<&'static str> {
        Some(match title {
            "Прилади" => {
                "Ваші прожектори. Клацніть прилад, щоб вибрати його. Кілька приладів вибирають із Shift або рамкою мишею, усі одразу — Ctrl+A. Колір і ефекти діють на вибрані прилади, а якщо нічого не вибрано, то на всі. Подвійний клік відкриває властивості приладу, права кнопка миші — його меню. Кнопка «Група» зберігає вибрані прилади як групу."
            }
            "Сцени" => {
                "Сцена запам'ятовує кольори всіх приладів, а з галочкою «Зберегти разом з ефектом» ще й ефект. Налаштуйте світло, введіть назву і натисніть «Зберегти». Клік по сцені вмикає її, а «Час переходу» задає, за скільки секунд світло плавно зміниться. Якщо затиснути Alt і тримати сцену, вона світить лише поки кнопку тримають. Сцену можна зберегти у файл через її меню й відкрити на іншому комп'ютері кнопкою «Відкрити з файлу»."
            }
            "Ефекти" => {
                "Ефект сам змінює світло на вибраних приладах (або на всіх). Виберіть ефект, швидкість і кольори палітри та натисніть «Запустити». Статичний колір чи «Очистити» зупиняють ефект на своїх приладах."
            }
            "Кольори" => {
                "Готові кольори. Клік вмикає колір на вибраних приладах, а якщо нічого не вибрано, то на всіх, і зупиняє на них ефект. Якщо затиснути Alt і тримати колір, він світить лише поки кнопку тримають."
            }
            "Групи" => {
                "Група — кілька приладів під однією назвою, наприклад «Ліва сторона». Клік по групі вибирає її прилади, Shift + клік додає їх до вже вибраних. Створюють групу кнопкою «Група» над приладами; один прилад може бути в кількох групах."
            }
            "Колір" => {
                "Колір вибраних приладів (або всіх, якщо нічого не вибрано). Клацніть колір на колі чи в палітрі праворуч; клавіші 1–9 вмикають перші кольори палітри. Кнопка «+» додає поточний колір у палітру. Повзунки нижче налаштовують окремі канали: червоний, зелений, синій, білий, яскравість тощо. Видно лише ті повзунки, канали яких є у вибраних приладів. Щоб вписати точне значення, клацніть число біля повзунка."
            }
            "Пресети" => {
                "Вбудовані програми прожектора: він сам перемикає кольори чи ефекти, а Nitor лише вибирає програму. Є не в усіх приладів. Повернутися до звичайного керування кольором можна перемикачем «Колір» угорі."
            }
            _ => return None,
        })
    }

    /// Заголовок панелі з кнопками праворуч. Якщо кнопки не вміщаються поруч із назвою,
    /// вони переходять у рядок під нею (ширину кнопок пам'ятаємо з попереднього кадру).
    fn panel_head(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
        let id = ui.id().with(("panel-head", title));
        let right_w: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
        let title_w = widgets::text_width(ui, title, bold(FS_LG)) + 26.0; // з «?»
        let fits = title_w + S4 + right_w <= ui.available_width();
        let row = |ui: &mut Ui, right: Box<dyn FnOnce(&mut Ui) + '_>| -> f32 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.scope(right).response.rect.width()).inner
        };
        let used = if fits {
            ui.horizontal(|ui| {
                ui.set_min_height(CONTROL_H_SM);
                widgets::h2(ui, title);
                if let Some(h) = Self::panel_help(title) {
                    widgets::help(ui, h);
                }
                row(ui, Box::new(right))
            })
            .inner
        } else {
            ui.horizontal(|ui| {
                ui.set_min_height(CONTROL_H_SM);
                widgets::h2(ui, title);
                if let Some(h) = Self::panel_help(title) {
                    widgets::help(ui, h);
                }
            });
            // кнопки під назвою — від лівого краю, як і решта панелі
            let w = if right_w > 0.0 { right_w.min(ui.available_width()) } else { ui.available_width() };
            ui.allocate_ui_with_layout(vec2(w, CONTROL_H_SM), Layout::left_to_right(Align::Center), |ui| {
                row(ui, Box::new(right))
            })
            .inner
        };
        if (used - right_w).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_repaint();
        }
        ui.add_space(S3 - S2);
    }

    fn fixtures_panel(&mut self, ui: &mut Ui) -> Rect {
        let mut acts: Vec<Act> = vec![];
        self.tile_rects.clear();
        let h = self.v.hotkeys.clone();
        Self::panel_head(ui, "Прилади", |ui| {
            ui.spacing_mut().item_spacing.x = S1;
            if btn("Інверсія").small().key(short_key(&h, "selectInvert")).show(ui).clicked() {
                acts.push(Act::SelectInvert);
            }
            if btn("Жодного").small().key(short_key(&h, "selectNone")).show(ui).clicked() {
                acts.push(Act::SelectNone);
            }
            if btn("Усі").small().key(short_key(&h, "selectAll")).show(ui).clicked() {
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
            ui.add_space(S5);
            widgets::empty_state(
                ui,
                "bulb",
                "Приладів ще немає",
                "Додайте прожектори з адресами, як на самих приладах. Модель можна вибрати з бібліотеки.",
            );
            ui.add_space(S3);
            ui.vertical_centered(|ui| {
                if btn("Додати прилад").icon("plus").primary().show(ui).clicked() {
                    self.open_add();
                }
            });
            return Rect::NOTHING;
        }
        self.groups_row(ui);
        let desc = self.sel_description();
        ui.label(widgets::muted(if desc.is_empty() {
            "Нічого не вибрано".into()
        } else {
            format!("Вибрано {desc}")
        }));
        ui.add_space(S2);
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
        // плитка, що змінила місце (додали чи видалили прилад, змінилася кількість колонок), переїжджає
        // плавно; новий прилад виростає на своєму місці (крім першого кадру після запуску)
        let appear = ui.input(|i| i.time) > 1.0;
        for (f, (c, r, span)) in fixtures.iter().zip(positions) {
            let rect = Rect::from_min_size(
                pos2(area.left() + c as f32 * (cw + gap), area.top() + r as f32 * (th + gap)),
                vec2(cw * span as f32 + gap * (span as f32 - 1.0), th),
            );
            let slot = ((c as u64) << 32) | r as u64;
            let rect = anim::slot_rect(ui.ctx(), Id::new(("tile-slot", &f.id)), slot, rect, appear);
            self.tile(ui, rect, f, &eff_ids);
        }
        area
    }

    fn tile(&mut self, ui: &mut Ui, rect: Rect, f: &nitor_core::Fixture, eff_ids: &[String]) {
        let id = Id::new(("tile", &f.id));
        self.tile_rects.push((f.id.clone(), rect));
        let resp = ui.interact(rect, id, Sense::click()).on_hover_text(&f.profile);
        let selected = self.sel.contains(&f.id);
        let p = ui.painter().clone();
        // плитка піднімається на 2 px під мишею, рамка темнішає на крок
        let (h, _) = nitor_ui::anim::hover_press(ui.ctx(), id, resp.hovered(), false);
        let rect = nitor_ui::anim::lift_rect(rect, h, 0.0, 2.0);
        let (fill, stroke) =
            if selected { (ACCENT_SOFT, ACCENT) } else { (RAISED, LINE.lerp_to_gamma(LINE_STRONG, h)) };
        p.rect(rect, radius(R_LG), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
        if resp.has_focus() {
            p.rect_stroke(rect.expand(2.0), radius(R_LG + 2), Stroke::new(2.0_f32, ACCENT), StrokeKind::Outside);
        }
        let end_addr = self.v.end(f);
        let label = format!("{}, DMX {}–{}", f.name, f.address, end_addr);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label));
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
            p.rect_filled(bar, radius(R_SM), INK);
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
                let seg_sel = self.sel.contains(&sid);
                let seg_name = format!("{}, сегмент {}", f.name, k + 1);
                cresp.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, seg_sel, &seg_name)
                });
                widgets::focus_ring(ui, &cresp, cr, 3);
                if seg_sel {
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
        // «Пресети» є, щойно в шоу є прилад із вбудованими програмами (а не лише коли його вибрано)
        // або коли модель приладу має програми в іншому режимі: тоді «Пресети» підкажуть змінити режим
        let in_other_mode = || {
            self.v.show.fixtures.iter().filter_map(|f| f.model.as_ref()).any(|m| {
                nitor_core::library::find(&m.m, &m.n)
                    .is_some_and(|lm| lm.modes.iter().any(|md| nitor_core::fixtures::has_presets(md.channels())))
            })
        };
        let bars = !self.v.preset_fixtures.is_empty() || in_other_mode();
        let show_presets = bars && self.pane == Pane::Presets;
        ui.horizontal(|ui| {
            ui.set_min_height(CONTROL_H_SM);
            if bars {
                // перемикач «Пресети / Колір»
                let mut pane = if show_presets { Pane::Presets } else { Pane::Color };
                let opts = [(Pane::Presets, "Пресети"), (Pane::Color, "Колір")];
                if widgets::segmented(ui, Id::new("pane-seg"), &mut pane, &opts, None) {
                    self.pane = pane;
                }
                widgets::help(ui, Self::panel_help(if show_presets { "Пресети" } else { "Колір" }).unwrap_or_default());
            } else {
                widgets::h2(ui, "Колір");
                widgets::help(ui, Self::panel_help("Колір").unwrap_or_default());
            }
        });
        ui.add_space(S3 - S2);
        if show_presets && self.v.preset_fixtures.is_empty() {
            ui.add_space(S4);
            widgets::empty_state(
                ui,
                "grid",
                "Пресети недоступні в цьому режимі",
                "Вбудовані програми працюють лише в деяких режимах DMX (у WASH 243 — H001 і A001). Двічі клацніть прилад і натисніть «Змінити режим». Режим на самому приладі теж має бути таким самим.",
            );
        } else if show_presets {
            self.presets_pane(ui, &targets);
        } else {
            self.color_pane(ui);
        }
        Rect::NOTHING
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
                let c = wheel_hsv(h, d / r, 1.0);
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
        // вказівник — колір першого вибраного (відтінок і насиченість; чорний — без вказівника)
        let marker = self.first_sel_color().map(|(c, _)| c).filter(|c| *c != Rgb::BLACK);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = S4;
            let size = 2.0 * WHEEL_R;
            let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
            ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let resp = resp.on_hover_cursor(egui::CursorIcon::Crosshair);
            let at = |pos: Pos2| {
                let d = pos - rect.center();
                let s = (d.length() / WHEEL_R).min(1.0) as f64;
                let h = ((d.y as f64).atan2(d.x as f64) / (2.0 * std::f64::consts::PI) + 1.0) % 1.0;
                wheel_hsv(h, s, 1.0)
            };
            if resp.is_pointer_button_down_on() && (resp.dragged() || resp.drag_started() || resp.clicked()) {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let c = at(pos);
                    if c != self.last_color || resp.drag_started() || resp.clicked() {
                        self.pick_color(c);
                    }
                }
            } else if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    self.pick_color(at(pos));
                }
            }
            if let Some(c) = marker {
                let (h, s, _) = rgb_to_hsv(c.0 as f64, c.1 as f64, c.2 as f64);
                let a = (h * std::f64::consts::TAU) as f32;
                let m = rect.center() + vec2(a.cos(), a.sin()) * (s as f32).min(1.0) * WHEEL_R;
                let p = ui.painter();
                p.circle_filled(m, 8.0, c32(c));
                p.circle_stroke(m, 8.0, Stroke::new(2.0_f32, WHITE));
                p.circle_stroke(m, 10.0, Stroke::new(1.0_f32, rgba(0, 0, 0, 0.6)));
            }
            if resp.secondary_clicked() {
                let m = self.color_menu();
                self.want_menu(m);
            }
            self.palette_grid(ui);
        });
        ui.add_space(S3);
        self.hex_row(ui);
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
            if slider_row(ui, Id::new(("sl", key)), label, &mut v, 255.0, color, Shown::Raw, 84.0, key != "strobe") {
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
                    .on_hover_text(format!("{}, DMX {}–{}", p.name, p.lo, p.hi));
                let c = used.get(&n).copied().unwrap_or(0);
                let (fill, stroke, fg) = if c > 0 && c == fx.len() {
                    (ACCENT_STRONG, ACCENT_STRONG, WHITE)
                } else if c > 0 {
                    (RAISED, ACCENT, ACCENT)
                } else {
                    (RAISED, if resp.hovered() { LINE_STRONG } else { LINE }, TEXT)
                };
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, c > 0, &p.name));
                widgets::focus_ring(ui, &resp, r, R_MD);
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
        if slider_row(ui, Id::new("presetSpeed"), "Швидкість", &mut speed, 255.0, ACCENT, Shown::Pct, 84.0, false)
        {
            self.send(Command::Preset { ids: targets.to_vec(), n: None, speed: Some(speed as u8) });
        }
        ui.add_space(S3 - S2);
        let any = fx.iter().any(|f| f.preset.is_some());
        if btn("Вимкнути пресет").icon("stop").enabled(any).show(ui).clicked() {
            self.send(Command::Preset { ids: targets.to_vec(), n: Some(None), speed: None });
        }
    }

    /// Поле HEX під колом: показує колір першого вибраного, Enter чи клік поза полем — застосувати.
    fn hex_row(&mut self, ui: &mut Ui) {
        let cur = self.first_sel_color().map(|(c, _)| c);
        let id = Id::new("hex-edit");
        let err_id = Id::new("hex-err");
        let mut field = Rect::NOTHING;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S2;
            widgets::field_label(ui, "HEX");
            let (sw, _) = ui.allocate_exact_size(vec2(CONTROL_H, CONTROL_H), Sense::hover());
            match cur {
                Some(c) => {
                    ui.painter().rect(sw, radius(R_MD), c32(c), Stroke::new(1.0_f32, LINE_STRONG), StrokeKind::Inside);
                }
                None => {
                    ui.painter().rect_stroke(sw, radius(R_MD), Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
                }
            }
            let shown = cur.map(|c| c.hex().to_uppercase()).unwrap_or_default();
            let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| shown.clone());
            let r = widgets::input(ui, &mut text, "#RRGGBB", 110.0).on_hover_text(
                "Вписати колір, наприклад #FF8000. Діє на вибрані прилади або на всі, якщо нічого не вибрано",
            );
            field = r.rect;
            if r.changed() {
                ui.data_mut(|d| d.remove::<bool>(err_id));
            }
            if r.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
            } else if !ui.data(|d| d.get_temp::<bool>(err_id).unwrap_or(false)) {
                ui.data_mut(|d| d.remove::<String>(id));
            }
            if r.lost_focus() && text.trim() != shown {
                match nitor_core::Rgb::from_hex(text.trim()) {
                    Ok(c) => {
                        self.set_color(ColorValues { hex: Some(c), ..Default::default() });
                        ui.data_mut(|d| d.remove::<String>(id));
                    }
                    // неправильне значення лишається в полі, щоб його можна було виправити
                    Err(_) => ui.data_mut(|d| {
                        d.insert_temp(err_id, true);
                        d.insert_temp(id, text.clone());
                    }),
                }
            }
            if let Some(c) = cur {
                ui.label(widgets::muted(format!("R {}   G {}   B {}", c.0, c.1, c.2)));
            }
        });
        if ui.data(|d| d.get_temp::<bool>(err_id).unwrap_or(false)) {
            ui.painter().rect_stroke(field, radius(R_MD), Stroke::new(2.0_f32, DANGER), StrokeKind::Inside);
            ui.label(RichText::new("Потрібно шість знаків після #, наприклад #FF8000").size(FS_SM).color(DANGER));
        }
    }

    /// Смужка з 8 умовних приладів: вибраний ефект ще до запуску, з тією ж швидкістю, розкидом і палітрою.
    fn effect_preview(&mut self, ui: &mut Ui, w: f32) {
        let help = "Так ефект виглядатиме на приладах ще до запуску: з тією ж швидкістю, розкидом фази й палітрою. На прожектори його вмикає «Запустити».";
        if !self.fold(ui, "effPreview", "Перегляд", Some(help), true) {
            return;
        }
        let eff = nitor_core::Effect {
            kind: self.eff.kind,
            speed: speed_from_slider(self.eff.speed),
            spread: self.eff.spread as f64 / 100.0,
            colors: self.eff.colors.clone(),
            ..Default::default()
        };
        // без палітри ефект бере власний колір приладу — тут колір першого вибраного (або білий)
        let base = self.first_sel_color().map_or(Rgb(255, 255, 255), |(c, _)| c);
        let base = nitor_core::fixtures::Color::rgb(base.0 as f64, base.1 as f64, base.2 as f64);
        let t = ui.input(|i| i.time);
        let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        let p = ui.painter();
        p.rect(r, radius(R_MD), INK, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
        let n = 8;
        let d = (r.width() - 2.0 * S3) / n as f32;
        for i in 0..n {
            let c = nitor_core::effects::effect_color(&eff, i, n, t, 1.0, base);
            let ch = |v: f64| v.clamp(0.0, 255.0) as u8;
            let col = Color32::from_rgb(ch(c.r), ch(c.g), ch(c.b));
            let at = pos2(r.left() + S3 + d * (i as f32 + 0.5), r.center().y);
            p.circle_filled(at, 13.0, alpha(col, 0.25));
            p.circle_filled(at, 9.0, col);
        }
        // 20 кадрів/с досить для перегляду; не будимо програму, коли смужки не видно: прокручено за край,
        // відкрито вікно чи меню поверх або вікно згорнуто
        let seen = ui.is_rect_visible(r) && !self.any_dialog() && self.menu.is_none();
        if seen && !ui.input(|i| i.viewport().minimized.unwrap_or(false)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Частина панелі, що згортається; стан зберігається в налаштуваннях. Повертає, чи розгорнуто.
    fn fold(&mut self, ui: &mut Ui, key: &str, text: &str, help: Option<&str>, default: bool) -> bool {
        let open = self.v.prefs.folds.get(key).copied().unwrap_or(default);
        let now = widgets::fold(ui, key, text, help, open);
        if now != open {
            self.set_prefs(|p| {
                p.folds.insert(key.to_string(), now);
            });
        }
        now
    }

    /// Крива переходу між сценами: три плитки з графіком (налаштування зберігається).
    fn fade_curve_row(&mut self, ui: &mut Ui, w: f32) {
        use nitor_core::engine::FadeCurve;
        if !self.fold(ui, "fadeCurve", "Крива переходу", None, false) {
            return;
        }
        let cur = self.v.prefs.fade_curve;
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S1;
            let curves = [
                (FadeCurve::Linear, "Рівно", "Світло змінюється рівномірно"),
                (FadeCurve::EaseIn, "М'який старт", "Спершу повільно, наприкінці швидше"),
                (FadeCurve::EaseOut, "М'який кінець", "Спершу швидко, наприкінці плавно сповільнюється"),
            ];
            let cw = (w - 2.0 * S1) / 3.0;
            for (c, name, tip) in curves {
                let on = c == cur;
                let (r, resp) = ui.allocate_exact_size(vec2(cw, 58.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, name));
                let (fill, stroke) = if on {
                    (ACCENT_SOFT, ACCENT)
                } else if resp.hovered() {
                    (RAISED, LINE_STRONG)
                } else {
                    (RAISED, LINE)
                };
                widgets::focus_ring(ui, &resp, r, R_MD);
                let p = ui.painter();
                p.rect(r, radius(R_MD), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
                let g = Rect::from_min_size(r.min + vec2(S3, S2), vec2(r.width() - 2.0 * S3, 26.0));
                let pts: Vec<Pos2> = (0..=24)
                    .map(|k| {
                        let t = k as f64 / 24.0;
                        pos2(g.left() + t as f32 * g.width(), g.bottom() - c.apply(t) as f32 * g.height())
                    })
                    .collect();
                p.add(egui::Shape::line(pts, Stroke::new(2.0_f32, if on { ACCENT } else { MUTED })));
                p.text(
                    pos2(r.center().x, r.bottom() - S2),
                    Align2::CENTER_BOTTOM,
                    name,
                    font(FS_XS),
                    if on { TEXT } else { MUTED },
                );
                if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                    pick = Some(c);
                }
            }
        });
        if let Some(c) = pick {
            self.set_prefs(|p| p.fade_curve = c);
        }
    }

    // ------------------------------------------------------------------ сцени --
    fn scenes_block(&mut self, ui: &mut Ui) -> Rect {
        let start = ui.cursor().min;
        let first = self.v.prefs.scenes_first;
        let edit = self.layout_edit;
        let mut swap = false;
        Self::panel_head(ui, "Сцени", |ui| swap = edit && Self::swap_button(ui, first));
        if swap {
            self.act(ui.ctx(), Act::SwapSide);
        }
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
        widgets::number_f(ui, &mut self.scene_fade, 0.0, 60.0, 0.1, 1, w);
        ui.add_space(S3 - S2);
        self.fade_curve_row(ui, w);
        ui.add_space(S3 - S2);
        if btn("Відкрити з файлу").small().icon("upload").show(ui).clicked() {
            self.act(ui.ctx(), Act::OpenSceneFile);
        }
        ui.add_space(S3 - S2);
        let names: Vec<String> = self.v.show.scenes.keys().cloned().collect();
        if names.is_empty() {
            ui.add_space(S3);
            widgets::empty_state(
                ui,
                "layers",
                "Збережених сцен немає",
                "Налаштуйте світло, введіть назву вгорі й натисніть «Зберегти».",
            );
        }
        ui.spacing_mut().item_spacing.y = S1;
        for name in names {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = S1;
                // значок окремо від назви: вмикає й вимикає сцену
                let on = self.v.active_scene.as_deref() == Some(name.as_str());
                let (icon, tip, kind) = if on {
                    ("stop", "Вимкнути сцену", Kind::Toggled)
                } else {
                    ("play", "Увімкнути сцену", Kind::Normal)
                };
                if icon_btn(icon).round().kind(kind).tip(tip).show(ui).clicked() {
                    if on {
                        let ids: Vec<String> = self
                            .v
                            .show
                            .scenes
                            .get(&name)
                            .map(|s| s.fixtures.keys().cloned().collect())
                            .unwrap_or_default();
                        if !ids.is_empty() {
                            self.send(Command::Clear { ids });
                        }
                    } else {
                        let fade = self.scene_fade;
                        self.send(Command::RecallScene { name: name.clone(), fade });
                    }
                }
                let bw = ui.available_width() - 2.0 * (CONTROL_H_SM + S1);
                let r = btn(name.clone()).min_width(bw).left().tip(scene_tip(&name)).show(ui);
                self.scene_button(&r, &name);
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

    /// Кнопка в заголовку «Ефектів» і «Сцен»: перемістити блок вище чи нижче.
    fn swap_button(ui: &mut Ui, on_top: bool) -> bool {
        let (icon, tip) = if on_top {
            ("arrowDown", "Перемістити нижче")
        } else {
            ("arrowUp", "Перемістити вище")
        };
        icon_btn(icon).small().kind(Kind::Ghost).tip(tip).show(ui).clicked()
    }

    fn scene_button(&mut self, r: &egui::Response, name: &str) {
        if ui_alt(r) {
            if r.is_pointer_button_down_on() {
                self.flash_start(FlashSource::Scene(name.into()), FlashHold::Pointer);
            }
        } else if r.clicked() {
            let fade = self.scene_fade;
            self.send(Command::RecallScene { name: name.into(), fade });
        }
    }

    /// Групи — рядок кнопок над приладами.
    fn groups_row(&mut self, ui: &mut Ui) {
        let groups: Vec<(String, usize, bool)> = self
            .v
            .show
            .groups
            .iter()
            .map(|(n, ids)| (n.clone(), ids.len(), !ids.is_empty() && ids.iter().all(|i| self.sel.contains(i))))
            .collect();
        if groups.is_empty() && self.v.operator {
            return;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(S1, S1);
            for (name, n, on) in groups {
                let r = btn(name.clone())
                    .small()
                    .icon("users")
                    .kind(if on { Kind::Toggled } else { Kind::Normal })
                    .tip(format!("Приладів: {n}. Shift + клік — додати до вибору"))
                    .show(ui);
                if r.clicked() {
                    self.pick_group(&name, shift);
                }
                if r.secondary_clicked() {
                    let m = self.group_menu(&name);
                    self.want_menu(m);
                }
            }
            if !self.v.operator
                && btn("Група")
                    .small()
                    .icon("plus")
                    .enabled(!self.sel.is_empty())
                    .tip("Зберегти вибрані прилади як групу")
                    .show(ui)
                    .clicked()
            {
                self.act(ui.ctx(), Act::AskNewGroup);
            }
        });
        ui.add_space(S3 - S2);
    }

    /// Вихід з режиму оператора: з PIN — через вікно, без PIN — утриманням кнопки 3 с.
    fn exit_operator_button(&mut self, ui: &mut Ui) {
        const HOLD: f32 = 3.0;
        if self.v.has_pin {
            if btn("Вийти").icon("lock").tip("Вийти з режиму оператора").show(ui).clicked() {
                self.act(ui.ctx(), Act::AskExitOperator);
            }
            return;
        }
        let held = self.exit_hold.map_or(0.0, |t| t.elapsed().as_secs_f32());
        let r = btn("Вийти")
            .icon("lock")
            .progress(held / HOLD)
            .tip("Утримуйте 3 секунди, щоб вийти з режиму оператора")
            .show(ui);
        if !r.is_pointer_button_down_on() {
            self.exit_hold = None;
        } else if self.exit_hold.is_none() {
            self.exit_hold = Some(std::time::Instant::now());
        } else if held >= HOLD {
            self.exit_hold = None;
            self.send(Command::Operator { on: false, pin: String::new() });
        }
        if self.exit_hold.is_some() {
            ui.ctx().request_repaint();
        }
    }

    /// Екран оператора: групи, кольори палітри, сцени. Без патчу, налаштувань і редагування.
    pub fn operator_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            let cols = if full.width() > 900.0 { 3 } else { 1 };
            let w = full.width() / cols as f32;
            let col =
                |i: usize| Rect::from_min_size(pos2(full.left() + w * i as f32, full.top()), vec2(w, full.height()));
            for i in 1..cols {
                ui.painter().vline(col(i).left(), full.y_range(), Stroke::new(1.0_f32, LINE));
            }
            let groups = |app: &mut App, ui: &mut Ui| {
                let target =
                    if app.sel.is_empty() { "Для всіх приладів" } else { "Для вибраних" };
                Self::panel_head(ui, "Групи", |ui| {
                    ui.label(widgets::muted(target));
                });
                if app.v.show.groups.is_empty() {
                    ui.label(widgets::muted("Груп немає — кольори й сцени діють на всі прилади"));
                } else {
                    app.groups_row(ui);
                    if !app.sel.is_empty() && btn("Усі прилади").small().show(ui).clicked() {
                        app.sel.clear();
                    }
                }
            };
            let colors = |app: &mut App, ui: &mut Ui| {
                Self::panel_head(ui, "Кольори", |_| {});
                app.static_grid(ui);
            };
            let scenes = |app: &mut App, ui: &mut Ui| {
                Self::panel_head(ui, "Сцени", |_| {});
                let names: Vec<String> = app.v.show.scenes.keys().cloned().collect();
                if names.is_empty() {
                    ui.label(widgets::muted("Збережених сцен немає"));
                }
                let bw = ui.available_width();
                for name in names {
                    let r = btn(name.clone()).icon("play").min_width(bw).left().tip(scene_tip(&name)).show(ui);
                    app.scene_button(&r, &name);
                }
            };
            if cols == 3 {
                self.column(ui, col(0), "op-groups", |a, ui| {
                    groups(a, ui);
                    Rect::NOTHING
                });
                self.column(ui, col(1), "op-colors", |a, ui| {
                    colors(a, ui);
                    Rect::NOTHING
                });
                self.column(ui, col(2), "op-scenes", |a, ui| {
                    scenes(a, ui);
                    Rect::NOTHING
                });
            } else {
                self.column(ui, full, "op-all", |a, ui| {
                    groups(a, ui);
                    ui.add_space(S5);
                    colors(a, ui);
                    ui.add_space(S5);
                    scenes(a, ui);
                    Rect::NOTHING
                });
            }
        });
    }

    // ------------------------------------------------------- ефекти й сцени --
    /// Права колонка: «Ефекти» і «Сцени» в обраному порядку. Повертає прямокутник сцен (для меню).
    fn side_panel(&mut self, ui: &mut Ui) -> Rect {
        // кожен блок — окрема картка, щоб «Ефекти» і «Сцени» не зливалися в одне
        let card = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
            egui::Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(radius(R_LG))
                .inner_margin(egui::Margin::same(S4 as i8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .response
                .rect
        };
        // картки стоять одна під одною; після обміну плавно з'їжджають на нові місця за 0,25 с
        let top = ui.cursor().min;
        let w = ui.available_width();
        let hid = |n: &str| Id::new(("side-card-h", n));
        let (he, hs): (f32, f32) =
            ui.data(|d| (d.get_temp(hid("e")).unwrap_or(0.0), d.get_temp(hid("s")).unwrap_or(0.0)));
        let first = self.v.prefs.scenes_first;
        // зсув карток («Ефекти», «Сцени») згори для порядку «сцени першими» чи ні
        let offs = |scenes_first: bool| if scenes_first { (hs + S4, 0.0) } else { (0.0, he + S4) };
        let (mut oe, mut os) = offs(first);
        if let Some(t0) = self.side_anim {
            let t = anim::progress(ui.ctx(), t0, 0.25);
            let e = 1.0 - (1.0 - t).powi(3);
            let (fe, fs) = offs(!first);
            (oe, os) = (fe + (oe - fe) * e, fs + (os - fs) * e);
            if t < 1.0 {
                ui.ctx().request_repaint();
            } else {
                self.side_anim = None;
            }
        }
        let place = |ui: &mut Ui, y: f32, add: &mut dyn FnMut(&mut Ui)| {
            let max = Rect::from_min_size(pos2(top.x, top.y + y), vec2(w, f32::INFINITY));
            ui.scope_builder(egui::UiBuilder::new().max_rect(max), |ui| card(ui, add)).inner
        };
        let re = place(ui, oe, &mut |ui| self.effects_block(ui));
        let rs = place(ui, os, &mut |ui| {
            self.scenes_block(ui);
        });
        // висота змінилася — наступний кадр одразу, щоб картки не налазили одна на одну
        if (re.height() - he).abs() > 0.5 || (rs.height() - hs).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|d| {
            d.insert_temp(hid("e"), re.height());
            d.insert_temp(hid("s"), rs.height());
        });
        ui.allocate_rect(Rect::from_min_size(top, vec2(w, re.height() + rs.height() + S4)), Sense::hover());
        if first { rs } else { Rect::from_min_max(rs.min, pos2(rs.right(), rs.bottom() + 400.0)) }
    }

    /// Палітра поруч із колом: клік — колір на вибрані (або всі), Alt + утримання — спалах кольором.
    fn palette_grid(&mut self, ui: &mut Ui) {
        let colors = self.v.show.static_colors.clone();
        let h = self.v.hotkeys.clone();
        let (cell, gap) = (32.0, S2);
        let w = ui.available_width();
        let cols = (((w + gap) / (cell + gap)).floor() as usize).max(1);
        // остання клітинка — «+»: зберегти колір вибраного приладу
        let rows = (colors.len() + 1).div_ceil(cols);
        let (grid, _) = ui.allocate_exact_size(
            vec2(cols as f32 * (cell + gap) - gap, rows as f32 * (cell + gap) - gap),
            Sense::hover(),
        );
        // з клавіатури палітра — одна зупинка Tab: фокус на «поточному» кольорі, далі стрілками
        let (cur_id, go_id) = (Id::new("pal-cur"), Id::new("pal-go"));
        let n = colors.len();
        let cur = ui.data(|d| d.get_temp::<usize>(cur_id)).unwrap_or(0).min(n.saturating_sub(1));
        let go: Option<usize> = ui.data_mut(|d| d.remove_temp(go_id));
        let mut next = None;
        for (i, c) in colors.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(grid.left() + (i % cols) as f32 * (cell + gap), grid.top() + (i / cols) as f32 * (cell + gap)),
                vec2(cell, cell),
            );
            let key = short_key(&h, &format!("static{}", i + 1)).filter(|_| !self.v.prefs.hide_keys);
            let tip = match &key {
                Some(k) => format!("{} — {k}\nAlt + утримання — спалах цим кольором", c.name),
                None => format!("{}\nAlt + утримання — спалах цим кольором", c.name),
            };
            let sense = if i == cur { Sense::click() } else { Sense::CLICK };
            let resp = ui.interact(r, Id::new(("swatch", i)), sense).on_hover_text(tip);
            if go == Some(i) {
                resp.request_focus();
            }
            if resp.has_focus() {
                // стрілки — для палітри, а не для переходу egui до сусіднього елемента
                let filter = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() };
                ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, filter));
                let step = |k: egui::Key| ui.input_mut(|inp| inp.consume_key(egui::Modifiers::NONE, k));
                let to = if step(egui::Key::ArrowRight) {
                    Some(i + 1)
                } else if step(egui::Key::ArrowLeft) {
                    i.checked_sub(1)
                } else if step(egui::Key::ArrowDown) {
                    Some(i + cols)
                } else if step(egui::Key::ArrowUp) {
                    i.checked_sub(cols)
                } else {
                    None
                };
                next = to.filter(|t| *t < n);
            }
            if resp.clicked() {
                ui.data_mut(|d| d.insert_temp(cur_id, i));
            }
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &c.name));
            let rr = if resp.is_pointer_button_down_on() { r.shrink(1.0) } else { r };
            paint_static_chip(ui, rr, c.hex, c.w.is_some_and(|w| w > 0), R_SM);
            ui.painter().rect_stroke(rr, radius(R_SM), Stroke::new(1.0_f32, rgba(0, 0, 0, 0.4)), StrokeKind::Inside);
            if let Some(k) = key {
                // гаряча клавіша кольору — у куті плашки
                ui.painter().text(
                    rr.left_top() + vec2(4.0, 2.0),
                    Align2::LEFT_TOP,
                    k,
                    font(FS_XS),
                    readable_text(c.hex),
                );
            }
            if resp.hovered() || resp.has_focus() {
                ui.painter().rect_stroke(
                    r.expand(1.0),
                    radius(R_SM),
                    Stroke::new(2.0_f32, if resp.has_focus() { ACCENT } else { TEXT }),
                    StrokeKind::Outside,
                );
            }
            if ui.input(|i| i.modifiers.alt) {
                if resp.is_pointer_button_down_on() {
                    self.flash_color(StaticSource::Index(i), FlashHold::Pointer);
                }
            } else if resp.clicked() {
                let ids = self.sel.clone();
                self.act(ui.ctx(), Act::StaticTo { index: i, ids });
            }
            if resp.secondary_clicked() {
                let m = self.static_menu(i);
                self.want_menu(m);
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        // новий «поточний» колір стає фокусованим у наступному кадрі
        if let Some(t) = next {
            ui.data_mut(|d| {
                d.insert_temp(cur_id, t);
                d.insert_temp(go_id, t);
            });
            ui.ctx().request_repaint();
        }
        let i = colors.len();
        let r = Rect::from_min_size(
            pos2(grid.left() + (i % cols) as f32 * (cell + gap), grid.top() + (i / cols) as f32 * (cell + gap)),
            vec2(cell, cell),
        );
        let can = self.first_sel_color().is_some_and(|(c, w)| c != Rgb::BLACK || w > 0);
        let sense = if can { Sense::click() } else { Sense::hover() };
        let tip = if can {
            "Зберегти колір вибраного приладу в палітру"
        } else {
            "Виберіть прилад, щоб зберегти його колір"
        };
        let resp = ui.interact(r, Id::new("swatch-add"), sense).on_hover_text(tip);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, can, "Зберегти колір у палітру"));
        let a = if can { 1.0 } else { 0.45 };
        let hot = resp.hovered() && can;
        ui.painter().rect(
            r,
            radius(R_SM),
            if hot { RAISED } else { Color32::TRANSPARENT },
            Stroke::new(1.0_f32, if hot { LINE_STRONG } else { LINE_FIELD }.gamma_multiply(a)),
            StrokeKind::Inside,
        );
        // щойно збережено — галочка, що трохи підстрибує
        let saved = anim::anim_bool(ui.ctx(), Id::new("swatch-saved"), self.flashing("colorSave"), 0.2);
        let (icon, fg, s) = if saved > 0.0 {
            ("check", OK, 0.6 + 0.4 * saved + 0.35 * (saved * std::f32::consts::PI).sin())
        } else {
            ("plus", TEXT.gamma_multiply(a), 1.0)
        };
        nitor_ui::icons::image(icon, 16.0, fg).paint_at(ui, Rect::from_center_size(r.center(), vec2(16.0, 16.0) * s));
        if self.flashing("colorSave") {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if resp.clicked() {
            self.act(ui.ctx(), Act::SaveColor);
        }
        if can {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }

    fn static_grid(&mut self, ui: &mut Ui) {
        let colors = self.v.show.static_colors.clone();
        let w = ui.available_width();
        let gap = S2;
        // плитка не вужча за найдовше слово в назвах, щоб слова не рвалися посередині
        let word_w = colors
            .iter()
            .flat_map(|c| c.name.split_whitespace())
            .map(|word| widgets::text_width(ui, word, bold(FS_XS)))
            .fold(0.0_f32, f32::max);
        let min_w = (word_w + 2.0 * S1).max(92.0);
        let cols = (((w + gap) / (min_w + gap)).floor() as usize).max(1);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let rows = colors.len().div_ceil(cols);
        let (area, _) = ui.allocate_exact_size(vec2(w, (rows as f32 * (48.0 + gap) - gap).max(0.0)), Sense::hover());
        for (i, c) in colors.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(area.left() + (i % cols) as f32 * (cw + gap), area.top() + (i / cols) as f32 * (48.0 + gap)),
                vec2(cw, 48.0),
            );
            let resp = ui
                .interact(r, Id::new(("static", i)), Sense::click())
                .on_hover_text(format!("{}\nAlt + утримання — спалах цим кольором", c.name));
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &c.name));
            let with_w = c.w.is_some_and(|w| w > 0);
            let rr = if resp.is_pointer_button_down_on() { r.shrink(1.0) } else { r };
            paint_static_chip(ui, rr, c.hex, with_w, R_MD);
            ui.painter().rect_stroke(rr, radius(R_MD), Stroke::new(1.0_f32, rgba(0, 0, 0, 0.4)), StrokeKind::Inside);
            if resp.has_focus() {
                ui.painter().rect_stroke(
                    r.expand(2.0),
                    radius(R_MD + 2),
                    Stroke::new(2.0_f32, ACCENT),
                    StrokeKind::Outside,
                );
            }
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
            let long = widgets::text_width(ui, &c.name, bold(FS_SM)) > r.width() - 2.0 * S1;
            let mut job = egui::text::LayoutJob::simple(
                c.name.clone(),
                bold(if long { FS_XS } else { FS_SM }),
                fg,
                r.width() - 2.0 * S1,
            );
            job.wrap.max_rows = 2;
            job.halign = Align::Center;
            let g = ui.fonts(|f| f.layout_job(job));
            ui.painter().galley(pos2(rr.center().x, rr.bottom() - S1 - g.size().y), g, fg);
            if ui.input(|i| i.modifiers.alt) {
                // Alt + утримання — «Спалах» цим кольором
                if resp.is_pointer_button_down_on() {
                    self.flash_color(StaticSource::Index(i), FlashHold::Pointer);
                }
            } else if resp.clicked() {
                let ids = self.sel.clone();
                self.act(ui.ctx(), Act::StaticTo { index: i, ids });
            }
            if resp.secondary_clicked() {
                let m = self.static_menu(i);
                self.want_menu(m);
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }

    fn effects_block(&mut self, ui: &mut Ui) {
        let e = self.v.show.effect.clone();
        let state = if e.kind != EffectKind::None {
            format!("{}, {} прил.", e.kind.label(), e.ids.len())
        } else {
            String::new()
        };
        let on_top = !self.v.prefs.scenes_first;
        let edit = self.layout_edit;
        let mut swap = false;
        Self::panel_head(ui, "Ефекти", |ui| {
            swap = edit && Self::swap_button(ui, on_top);
            // ефект працює — мітка з назвою й кількістю приладів
            if !state.is_empty() {
                widgets::chip(ui, widgets::Tone::Info, &state);
            }
        });
        if swap {
            self.act(ui.ctx(), Act::SwapSide);
        }
        let w = ui.available_width();
        widgets::field_label(ui, "Ефект");
        let opts: Vec<(EffectKind, String)> =
            EffectKind::ALL.iter().skip(1).map(|k| (*k, k.label().to_string())).collect();
        widgets::select(ui, "effType", &mut self.eff.kind, &opts, w);
        ui.add_space(S3 - S2);
        let hz = speed_from_slider(self.eff.speed);
        let mut changed = false;
        // підпис і повзунок — щільно, рядок лише заввишки з повзунок
        let compact =
            |ui: &mut Ui, label: &str, id: &str, v: &mut f32, min: f32, tip: Option<String>, help: &str| -> bool {
                if help.is_empty() {
                    widgets::field_label(ui, label);
                } else {
                    widgets::field_label_help(ui, label, help);
                }
                ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                    let r = widgets::slider_ticked(ui, label, v, min, 100.0, ACCENT, w - 52.0 - S2);
                    let r = match tip {
                        Some(t) => r.on_hover_text(t),
                        None => r,
                    };
                    r.changed() | widgets::value_box(ui, Id::new(id), v, min, 100.0, Shown::Pct, 52.0)
                })
                .inner
            };
        changed |= compact(ui, "Швидкість", "effSpeed", &mut self.eff.speed, 1.0, Some(format!("{hz:.2} Гц")), "");
        ui.add_space(S2);
        let spread = "Зсуває прилади один відносно одного: на нулі всі світять однаково, з більшим значенням колір біжить від приладу до приладу.";
        changed |= compact(ui, "Розкид фази", "effSpread", &mut self.eff.spread, 0.0, None, spread);
        ui.add_space(S2);
        self.effect_preview(ui, w);
        ui.add_space(S2);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = S1;
            ui.label(widgets::muted("Палітра"));
            widgets::help(
                ui,
                "Кольори, якими грає ефект. Кнопка «+» додає поточний колір, клік по кольору прибирає його. Без палітри ефект бере власний колір кожного приладу.",
            );
            ui.add_space(S1);
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
                // прибирання у два кліки: перший позначає колір хрестиком, другий прибирає
                let armed_id = Id::new("palette-armed");
                let armed = ui.data(|d| d.get_temp::<usize>(armed_id)) == Some(i);
                let name = format!("Колір палітри {}", i + 1);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
                if resp.hovered() || armed {
                    ui.painter().rect_stroke(
                        r.expand(1.0),
                        radius(R_SM),
                        Stroke::new(2.0_f32, DANGER),
                        StrokeKind::Outside,
                    );
                }
                if armed {
                    ui.painter().rect_filled(r, radius(R_SM), alpha(INK, 0.6));
                    nitor_ui::icons::image("x", 14.0, WHITE).paint_at(ui, Rect::from_center_size(r.center(), vec2(14.0, 14.0)));
                }
                widgets::focus_ring(ui, &resp, r, R_SM);
                let focused = resp.has_focus();
                let tip = if armed { "Клацніть ще раз, щоб прибрати" } else { "Прибрати з палітри" };
                if resp.on_hover_text(tip).clicked() {
                    if armed {
                        remove = Some(i);
                        ui.data_mut(|d| d.remove::<usize>(armed_id));
                    } else {
                        ui.data_mut(|d| d.insert_temp(armed_id, i));
                    }
                } else if armed && !focused && !ui.rect_contains_pointer(r.expand(6.0)) {
                    ui.data_mut(|d| d.remove::<usize>(armed_id));
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
        ui.horizontal(|ui| {
            changed |= widgets::switch(ui, &mut self.eff.by_seg, "По сегментах").changed();
            widgets::help(
                ui,
                "Ефект біжить уздовж сегментів LED-бару (кожен сегмент — як окремий прилад), а не лише від приладу до приладу.",
            );
        });
        ui.add_space(S3 - S2);
        ui.horizontal(|ui| {
            changed |= widgets::switch(ui, &mut self.eff.beat, "У такт музиці").changed();
            widgets::help(
                ui,
                "Ефект крокує в такт музиці. Темп береться зі звуку комп'ютера, мікрофона, VirtualDJ або з тапу (клавіша T).",
            );
        });
        if self.eff.beat {
            ui.add_space(S3 - S2);
            changed |= widgets::switch(ui, &mut self.eff.beat_pulse, "Спалах яскравості на кожен удар").changed();
            ui.add_space(S3 - S2);
            ui.horizontal(|ui| {
                match self.v.bpm {
                    None => {
                        ui.label(RichText::new("Темп: темп ще невідомий").size(FS_MD));
                    }
                    Some(b) => {
                        // число BPM прокручується, коли темп змінюється
                        ui.label(RichText::new("Темп:").size(FS_MD));
                        let num = format!("{b:.0}");
                        let nw = widgets::text_width(ui, "000", font(FS_MD));
                        let (r, _) = ui.allocate_exact_size(vec2(nw, FS_MD * 1.3), Sense::hover());
                        anim::rolling_text(ui, Id::new("bpm-roll"), r.right_center(), &num, font(FS_MD), TEXT);
                        ui.label(RichText::new("BPM").size(FS_MD));
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let key = short_key(&self.v.hotkeys, "tap");
                    // кільце на кожен удар: видно, що програма тримає темп
                    let since = self.core.now() - self.v.beat_at;
                    let age = match self.v.bpm {
                        _ if self.v.beat_at <= 0.0 => None,
                        Some(b) if b > 0.0 => Some(since.rem_euclid(60.0 / b) as f32),
                        _ => Some(since as f32),
                    };
                    // між ударами — проснутися до наступного (поки кільце видно, кадри просить воно саме)
                    if let (Some(a), Some(b)) = (age, self.v.bpm.filter(|b| *b > 0.0)) {
                        let wait = (60.0 / b as f32 - a).max(0.01);
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(wait));
                    }
                    if btn("Тап").small().key(key).ring(age).tip("Натискайте в такт музиці").show(ui).clicked()
                    {
                        let now = self.core.now();
                        self.send(Command::Beat(now));
                    }
                });
            });
            ui.add_space(S3 - S2);
            widgets::field_label(ui, "Джерело ударів");
            let mut src = self.v.audio;
            let opts =
                [(AudioSource::Loopback, "Комп'ютер"), (AudioSource::Mic, "Мікрофон"), (AudioSource::Off, "Лише тап")];
            if widgets::segmented(ui, Id::new("audio-seg"), &mut src, &opts, Some(w)) {
                self.send(Command::SetAudio(src));
            }
            let err = self.audio.as_ref().map(|a| a.error.lock().unwrap().clone()).unwrap_or_default();
            if !err.is_empty() {
                ui.label(widgets::muted(cap(&err)));
            }
            if self.v.bpm.is_some() {
                ui.label(widgets::muted("Швидкість ефекту береться з темпу"));
            }
        }
        ui.add_space(S3 - S2);
        // «Запустити» — на всю ширину, «Зупинити» — значком праворуч, щоб обидві вміщалися в картці
        ui.allocate_ui_with_layout(vec2(w, CONTROL_H), Layout::right_to_left(Align::Center), |ui| {
            if icon_btn("stop").key(short_key(&self.v.hotkeys, "effectStop")).tip("Зупинити ефект").show(ui).clicked()
            {
                self.send(Command::SetEffect(EffectPatch { kind: Some(EffectKind::None), ..Default::default() }));
            }
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
            if btn(label).icon(icon).kind(kind).full_width().show(ui).clicked() {
                self.act(ui.ctx(), Act::RunEffectSelected);
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
            let open = self.monitor_open;
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, "DMX-монітор"));
            widgets::focus_ring(ui, &resp, r, R_SM);
            // трикутник розкриття малюємо самі: у шрифтах Windows може не бути гліфа «▸»
            let c = pos2(r.left() + 5.0, r.center().y);
            let tri = if self.monitor_open {
                vec![c + vec2(-4.5, -2.5), c + vec2(4.5, -2.5), c + vec2(0.0, 3.0)]
            } else {
                vec![c + vec2(-2.5, -4.5), c + vec2(3.0, 0.0), c + vec2(-2.5, 4.5)]
            };
            ui.painter().add(egui::Shape::convex_polygon(tri, TEXT, Stroke::NONE));
            ui.painter().text(
                pos2(r.left() + 16.0, r.center().y),
                Align2::LEFT_CENTER,
                "DMX-монітор",
                bold(FS_MD),
                TEXT,
            );
            let mut tw = 16.0 + widgets::text_width(ui, "DMX-монітор", bold(FS_MD));
            if widgets::help_shown() {
                let at = Rect::from_min_size(pos2(r.left() + tw + S2, r.center().y - 9.0), vec2(18.0, 18.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                    widgets::help(
                        ui,
                        "Показує, що програма зараз надсилає на кожен із 512 каналів DMX: чим вища смужка, тим більше значення (0–255). \
                         Зручно перевірити, чи доходить команда до потрібної адреси. Сховати монітор можна в «Налаштування → Загальні».",
                    )
                });
                tw += 18.0 + S1;
            }
            ui.painter().text(
                pos2(r.left() + tw + S2, r.center().y),
                Align2::LEFT_CENTER,
                format!("Відправлено кадрів: {frames}"),
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
                for (i, &v) in self.v.dmx.iter().enumerate() {
                    let r = Rect::from_min_size(
                        pos2(area.left() + (i % cols) as f32 * (cw + gap), area.top() + (i / cols) as f32 * (ch + gap)),
                        vec2(cw, ch),
                    );
                    if !clip.intersects(r) {
                        continue;
                    }
                    p.rect_filled(r, egui::CornerRadius::same(3), RAISED);
                    let fill_h = v as f32 / 255.0 * r.height();
                    p.rect_filled(
                        Rect::from_min_max(pos2(r.left(), r.bottom() - fill_h), r.max),
                        egui::CornerRadius::same(3),
                        alpha(ACCENT, 0.35),
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
    /// Повідомлення внизу по центру: нове виїжджає знизу, старіші піднімаються вище, кожне гасне наприкінці.
    pub fn show_toast(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        self.toasts.retain(|t| now < t.until);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.screen_rect();
        let mut bottom = screen.bottom() - S4;
        let mut moving = false;
        for t in self.toasts.iter().rev() {
            let id = Id::new(("toast", t.seq));
            let age = now.duration_since(t.born).as_secs_f32();
            let left = t.until.duration_since(now).as_secs_f32();
            // зі «Зменшити рух» повідомлення не виїжджає й не з'їжджає, лише проявляється
            let still = anim::reduced_motion();
            let k = anim::ease_out(age / 0.2);
            let fade = (left / 0.3).min(1.0) * k;
            moving |= age < 0.2 || left < 0.3;
            let y =
                if still { bottom } else { ctx.animate_value_with_time(id.with("y"), bottom, 0.18) + 16.0 * (1.0 - k) };
            let h = egui::Area::new(id)
                .order(egui::Order::Tooltip)
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(pos2(screen.center().x, y))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.multiply_opacity(fade);
                    egui::Frame::new()
                        .fill(if t.info { RAISED } else { TOAST_ERR_BG })
                        .stroke(Stroke::new(1.0_f32, if t.info { LINE_STRONG } else { DANGER }))
                        .corner_radius(radius(R_MD))
                        .inner_margin(egui::Margin::symmetric(S4 as i8, S2 as i8))
                        .show(ui, |ui| {
                            ui.set_max_width(screen.width() * 0.9);
                            let l = ui.label(RichText::new(&t.text).size(FS_MD));
                            // диктор зачитує повідомлення, щойно воно з'явилось
                            ui.ctx().accesskit_node_builder(l.id, |n| n.set_live(egui::accesskit::Live::Polite));
                        });
                })
                .response
                .rect
                .height();
            bottom -= h + S2;
        }
        if moving {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
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
fn scene_tip(name: &str) -> String {
    format!("{name}\nAlt + утримання — спалах цією сценою")
}

/// Alt затиснуто під час взаємодії з кнопкою.
fn ui_alt(r: &egui::Response) -> bool {
    r.ctx.input(|i| i.modifiers.alt)
}

/// Кнопка панелі вікна (як «Згорнути» в інсталяторі): значок малюється сам. `true` — натиснуто.
fn window_button(ui: &mut Ui, tip: &str, paint: impl FnOnce(&egui::Painter, Pos2)) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(CONTROL_H, CONTROL_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    if resp.hovered() {
        ui.painter().rect_filled(r, radius(R_MD), RAISED);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(r.expand(2.0), radius(R_MD + 2), Stroke::new(2.0_f32, ACCENT), StrokeKind::Outside);
    }
    paint(ui.painter(), r.center());
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Стовпці головного вікна: ідентифікатор, стандартна частка ширини, найменша ширина
/// (менше — вміст обрізався б) і назва для редагування вигляду.
const COLS: [(&str, f32, f32, &str); 3] =
    [("fixtures", 1.0, 410.0, "Прилади"), ("color", 1.35, 360.0, "Колір"), ("side", 1.0, 350.0, "Ефекти й сцени")];

/// Стандартна ширина «Кольору» не більша за цю: ширше коло й палітра лише розтягують порожнечу.
const COLOR_W_MAX: f32 = 500.0;

fn col_min(id: &str) -> f32 {
    COLS.iter().find(|c| c.0 == id).map_or(0.0, |c| c.2)
}

/// Порядок стовпців з налаштувань: невідомі й повтори відкидаються, пропущені — дописуються в кінець.
fn col_order(saved: &[String]) -> [&'static str; 3] {
    let mut out: Vec<&'static str> = vec![];
    for id in saved.iter().filter_map(|s| COLS.iter().find(|c| c.0 == s).map(|c| c.0)).chain(COLS.map(|c| c.0)) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    [out[0], out[1], out[2]]
}

/// Ширина стовпців у заданому порядку: збережена або стандартна частка, не менша за найменшу.
/// Зайве місце дістається останньому стовпцю; якщо не вміщається — стискаються з кінця.
fn col_widths(order: &[&str; 3], saved: &std::collections::BTreeMap<String, f32>, total: f32) -> [f32; 3] {
    let shares: f32 = COLS.iter().map(|c| c.1).sum();
    let color = (total * COLS[1].1 / shares).min(COLOR_W_MAX);
    let side = total * COLS[2].1 / shares;
    let mut w = order.map(|id| {
        let c = COLS.iter().find(|c| c.0 == id).copied().unwrap_or(COLS[0]);
        // що «Колір» не забрав — дістається «Приладам»; «Ефекти й сцени» — своєї частки
        let def = match id {
            "color" => color,
            "side" => side,
            _ => total - color - side,
        };
        saved.get(id).copied().unwrap_or(def).max(c.2)
    });
    w[2] = (total - w[0] - w[1]).max(col_min(order[2]));
    let mut over = w.iter().sum::<f32>() - total;
    for i in (0..3).rev() {
        let cut = over.min(w[i] - col_min(order[i])).max(0.0);
        w[i] -= cut;
        over -= cut;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_window() {
        let none = Default::default();
        assert_eq!(col_order(&["side".into(), "x".into(), "side".into()]), ["side", "fixtures", "color"]);
        let w = col_widths(&col_order(&[]), &none, 1340.0);
        assert!((w.iter().sum::<f32>() - 1340.0).abs() < 0.01 && (w[0] - 440.0).abs() < 0.01, "{w:?}");
        // збережена ширина, що не вміщається, стискається, але не менше за найменшу
        let big: std::collections::BTreeMap<String, f32> = [("fixtures".into(), 900.0)].into();
        let w = col_widths(&col_order(&[]), &big, 1200.0);
        assert!((w[0] - 490.0).abs() < 0.01 && w[1] == 360.0 && w[2] == 350.0, "{w:?}");
        // на широкому екрані «Колір» не розтягується
        let w = col_widths(&col_order(&[]), &none, 1920.0);
        assert!(w[1] == COLOR_W_MAX && (w[2] - 1920.0 / 3.35).abs() < 0.01, "{w:?}");
    }
}
