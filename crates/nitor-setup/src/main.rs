//! Nitor-Setup.exe — встановлення й видалення Nitor (права адміністратора не потрібні).
//!
//! * без параметрів — вікно встановлення (ярлики на вибір);
//! * `/S` — тихе встановлення (обидва ярлики; `--no-desktop`, `--no-start-menu`);
//! * `/update --wait-pid N` — оновлення з програми: дочекатися її завершення, замінити файли, запустити;
//! * `Uninstall.exe` (копія інсталятора в теці програми) або `/uninstall` — вікно видалення;
//!   `/S` — повне видалення без запитань.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod install;
mod sys;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{Align, Layout, RichText, Sense, Stroke, vec2};
use install::{APP_EXE, Removal, UNINSTALL_EXE};
use nitor_core::userdata::Part;
use nitor_ui::theme::*;
use nitor_ui::widgets::{self, Kind, btn, icon_btn};

const VERSION: &str = nitor_core::VERSION;

#[derive(Default, Clone)]
struct Args {
    silent: bool,
    update: bool,
    uninstall: bool,
    /// запущено з тимчасової копії деінсталятора
    temp_copy: bool,
    wait_pid: Option<u32>,
    app_dir: Option<PathBuf>,
    no_desktop: bool,
    no_start_menu: bool,
    /// тихе видалення: які дані видалити (`scenes,fixtures,profiles,colors,settings`; порожньо — лише програму)
    parts: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.to_ascii_lowercase().as_str() {
            "/s" | "--silent" => a.silent = true,
            "/update" | "--update" => a.update = true,
            "/uninstall" | "--uninstall" => a.uninstall = true,
            "/uninstall-temp" => {
                a.uninstall = true;
                a.temp_copy = true;
            }
            "--wait-pid" => a.wait_pid = it.next().and_then(|p| p.parse().ok()),
            "--app-dir" => a.app_dir = it.next().map(PathBuf::from),
            "--no-desktop" => a.no_desktop = true,
            "--no-start-menu" => a.no_start_menu = true,
            "--parts" => a.parts = Some(it.next().unwrap_or_default()),
            _ => {}
        }
    }
    // копія інсталятора в теці програми — деінсталятор
    let me = std::env::current_exe().ok();
    if me.as_ref().and_then(|p| p.file_name()).is_some_and(|n| n.eq_ignore_ascii_case(UNINSTALL_EXE)) {
        a.uninstall = true;
    }
    a
}

// --------------------------------------------------------------------- журнал --
fn log_path() -> PathBuf {
    std::env::temp_dir().join("Nitor-install.log")
}

#[derive(Default)]
struct Job {
    heading: String,
    status: String,
    pct: f32,
    log: Vec<String>,
    /// `Some(Ok(текст))` — готово; `Some(Err(текст))` — помилка
    done: Option<Result<String, String>>,
}

type Shared = Arc<Mutex<Job>>;

fn note(job: &Shared, line: impl Into<String>) {
    let line = line.into();
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()));
    job.lock().unwrap().log.push(line);
}

fn step(job: &Shared, status: &str, pct: f32) {
    note(job, status);
    let mut j = job.lock().unwrap();
    j.status = status.into();
    j.pct = pct;
}

// ---------------------------------------------------------------- встановлення --
#[derive(Clone, Copy)]
struct Shortcuts {
    desktop: bool,
    start_menu: bool,
}

fn run_install(job: &Shared, sc: Shortcuts) -> Result<String, String> {
    let app_dir = sys::app_dir();
    note(job, format!("Nitor {VERSION} — встановлення в {}", app_dir.display()));
    let app = install::payload()?;
    let setup = std::fs::read(std::env::current_exe().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    step(job, "Закриття Nitor", 0.1);
    if !sys::close_app(20) {
        return Err("Не вдалося закрити Nitor. Закрийте програму й спробуйте ще раз.".into());
    }
    step(job, "Копіювання файлів", 0.5);
    install::install_files(&app_dir, &app, &setup)?;
    let exe = app_dir.join(APP_EXE);
    step(job, "Ярлики", 0.75);
    for (want, link) in [(sc.desktop, sys::desktop_link()), (sc.start_menu, sys::start_menu_link())] {
        let Some(link) = link else { continue };
        if want {
            if let Err(e) = sys::write_shortcut(&link, &exe) {
                note(job, format!("Ярлик {}: {e}", link.display()));
            }
        } else if link.exists() {
            let _ = std::fs::remove_file(&link);
        }
    }
    step(job, "Реєстрація в «Параметри → Програми»", 0.9);
    sys::register(&app_dir, &sys::RegInfo { version: VERSION, size_kb: install::installed_size_kb(&app) })?;
    sys::notify_shell();
    step(job, " ", 1.0);
    Ok("Nitor встановлено".into())
}

/// Оновлення з програми: тихо, без вікна; світло не гасне (програма вже завершилась без блекауту).
fn run_update(job: &Shared, pid: Option<u32>) -> Result<(), String> {
    let app_dir = sys::app_dir();
    note(job, format!("Nitor {VERSION} — оновлення в {}", app_dir.display()));
    if let Some(pid) = pid {
        sys::wait_pid(pid, 60);
    }
    if !sys::close_app(20) {
        return Err("Nitor не закрився".into());
    }
    let app = install::payload()?;
    let setup = std::fs::read(std::env::current_exe().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    install::install_files(&app_dir, &app, &setup)?;
    sys::register(&app_dir, &sys::RegInfo { version: VERSION, size_kb: install::installed_size_kb(&app) })?;
    sys::launch(&app_dir.join(APP_EXE))?;
    note(job, "Оновлено, Nitor запущено");
    Ok(())
}

// ------------------------------------------------------------------- видалення --
fn run_uninstall(job: &Shared, app_dir: &Path, removal: &Removal) -> Result<String, String> {
    note(job, format!("Nitor {VERSION} — видалення з {}", app_dir.display()));
    step(job, "Закриття Nitor", 0.1);
    if !sys::close_app(20) {
        return Err("Не вдалося закрити Nitor. Закрийте програму й спробуйте ще раз.".into());
    }
    step(job, "Ярлики на робочому столі та в меню «Пуск»", 0.3);
    for link in [sys::desktop_link(), sys::start_menu_link()].into_iter().flatten() {
        if link.exists() {
            match std::fs::remove_file(&link) {
                Ok(()) => note(job, format!("Видалено {}", link.display())),
                Err(e) => note(job, format!("Не вдалося видалити {}: {e}", link.display())),
            }
        }
    }
    step(job, "config.json", 0.5);
    let me = std::env::current_exe().ok();
    let r = install::uninstall_files(app_dir, removal, me.as_deref())?;
    step(job, "Реєстрація", 0.9);
    sys::unregister();
    sys::notify_shell();
    step(job, " ", 1.0);
    if !r.failed.is_empty() {
        for p in &r.failed {
            note(job, format!("Не вдалося видалити {}", p.display()));
        }
        return Err(format!("Деякі файли не вдалося видалити. Видаліть теку вручну: {}", app_dir.display()));
    }
    Ok(if !r.data_kept {
        String::new()
    } else if removal == &Removal::ProgramOnly {
        format!(
            "Збережені дані залишилися в теці {} — після повторного встановлення вони з'являться знову.",
            app_dir.display()
        )
    } else {
        format!("Готово. Дані залишено: {}.", install::kept_parts(removal).join(", ").to_lowercase())
    })
}

fn part_key(s: &str) -> Option<Part> {
    Some(match s {
        "scenes" => Part::Scenes,
        "fixtures" => Part::Fixtures,
        "profiles" => Part::Profiles,
        "colors" => Part::Colors,
        "settings" => Part::Settings,
        _ => return None,
    })
}

/// Запущений деінсталятор не може видалити сам себе: працюємо з тимчасової копії.
fn relaunch_from_temp(args: &Args, app_dir: &Path) -> bool {
    let Ok(me) = std::env::current_exe() else { return false };
    let tmp = std::env::temp_dir().join(format!("Nitor-Uninstall-{}.exe", std::process::id()));
    if std::fs::copy(&me, &tmp).is_err() {
        return false;
    }
    let mut cmd = std::process::Command::new(&tmp);
    cmd.arg("/uninstall-temp").arg("--app-dir").arg(app_dir);
    if args.silent {
        cmd.arg("/S");
    }
    if let Some(p) = &args.parts {
        cmd.arg("--parts").arg(p);
    }
    cmd.spawn().is_ok()
}

/// Тимчасова копія деінсталятора видаляє себе після завершення.
fn schedule_self_delete() {
    #[cfg(windows)]
    if let Ok(me) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        let cmd = format!("ping 127.0.0.1 -n 3 > nul & del /f /q \"{}\"", me.display());
        let _ = std::process::Command::new("cmd").args(["/C", &cmd]).creation_flags(0x0800_0000).spawn();
    }
}

fn main() {
    let args = parse_args();
    if args.uninstall {
        let app_dir = args.app_dir.clone().unwrap_or_else(|| {
            std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(sys::app_dir)
        });
        if !args.temp_copy && cfg!(windows) && relaunch_from_temp(&args, &app_dir) {
            return;
        }
        if args.silent {
            let job: Shared = Default::default();
            let removal = match &args.parts {
                None => Removal::WithData(Part::ALL.into_iter().collect()),
                Some(list) if list.trim().is_empty() => Removal::ProgramOnly,
                Some(list) => Removal::WithData(list.split(',').filter_map(|s| part_key(s.trim())).collect()),
            };
            let code = match run_uninstall(&job, &app_dir, &removal) {
                Ok(_) => 0,
                Err(e) => {
                    note(&job, e);
                    1
                }
            };
            let _ = std::fs::remove_dir(&app_dir);
            schedule_self_delete();
            std::process::exit(code);
        }
        let code = gui(Mode::Uninstall { app_dir: app_dir.clone() });
        let _ = std::fs::remove_dir(&app_dir);
        schedule_self_delete();
        std::process::exit(code);
    }
    if args.update {
        let job: Shared = Default::default();
        let code = match run_update(&job, args.wait_pid) {
            Ok(()) => 0,
            Err(e) => {
                note(&job, format!("Оновлення не вдалося: {e}"));
                // програма вже закрилася заради оновлення — запускаємо ту версію, що є
                let exe = sys::app_dir().join(APP_EXE);
                if exe.exists() {
                    if let Err(e) = sys::launch(&exe) {
                        note(&job, format!("Не вдалося запустити Nitor: {e}"));
                    }
                }
                1
            }
        };
        std::process::exit(code);
    }
    if args.silent {
        let job: Shared = Default::default();
        let sc = Shortcuts { desktop: !args.no_desktop, start_menu: !args.no_start_menu };
        let code = match run_install(&job, sc) {
            Ok(_) => 0,
            Err(e) => {
                note(&job, e);
                1
            }
        };
        std::process::exit(code);
    }
    std::process::exit(gui(Mode::Install));
}

// ------------------------------------------------------------------------ вікно --
enum Mode {
    Install,
    Uninstall { app_dir: PathBuf },
}

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Ask,
    Working,
    Done,
    Failed,
}

struct Gui {
    mode: Mode,
    phase: Phase,
    job: Shared,
    desktop: bool,
    start_menu: bool,
    with_data: bool,
    parts: BTreeSet<Part>,
    show_log: bool,
    code: Arc<Mutex<i32>>,
    height: f32,
    running: bool,
}

fn gui(mode: Mode) -> i32 {
    let code = Arc::new(Mutex::new(1));
    let title = match &mode {
        Mode::Install => format!("Nitor {VERSION} — встановлення"),
        Mode::Uninstall { .. } => format!("Nitor {VERSION} — видалення"),
    };
    let icon = image::load_from_memory(include_bytes!("../../../assets/icon.ico"))
        .ok()
        .map(|i| i.into_rgba8())
        .map(|i| egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(&title)
        .with_inner_size([600.0, 300.0])
        .with_decorations(false)
        .with_resizable(false);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, centered: true, persist_window: false, ..Default::default() };
    set_renderer(&mut options);
    let c = code.clone();
    let r = eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            nitor_ui::theme::install_fonts(&cc.egui_ctx);
            nitor_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Gui {
                mode,
                phase: Phase::Ask,
                job: Default::default(),
                desktop: true,
                start_menu: true,
                with_data: false,
                parts: Part::ALL.into_iter().collect(),
                show_log: false,
                code: c,
                height: 0.0,
                running: sys::app_running(),
            }))
        }),
    );
    if let Err(e) = r {
        let _ = std::fs::write(log_path(), format!("Не вдалося створити вікно: {e}\n"));
        return 3;
    }
    *code.lock().unwrap()
}

/// Робота, що виконується у фоновому потоці вікна.
type Task = Box<dyn FnOnce(&Shared) -> Result<String, String> + Send>;

impl Gui {
    fn start(&mut self, ctx: &egui::Context) {
        self.phase = Phase::Working;
        let job = self.job.clone();
        let code = self.code.clone();
        let ctx = ctx.clone();
        let task: Task = match &self.mode {
            Mode::Install => {
                let sc = Shortcuts { desktop: self.desktop, start_menu: self.start_menu };
                job.lock().unwrap().heading = "Встановлення…".into();
                Box::new(move |j| run_install(j, sc))
            }
            Mode::Uninstall { app_dir } => {
                let removal = if self.with_data { Removal::WithData(self.parts.clone()) } else { Removal::ProgramOnly };
                let dir = app_dir.clone();
                job.lock().unwrap().heading = "Видалення…".into();
                Box::new(move |j| run_uninstall(j, &dir, &removal))
            }
        };
        std::thread::spawn(move || {
            let r = task(&job);
            if let Err(e) = &r {
                note(&job, e.clone());
            }
            *code.lock().unwrap() = if r.is_ok() { 0 } else { 1 };
            job.lock().unwrap().done = Some(r);
            ctx.request_repaint();
        });
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.phase == Phase::Working {
            if let Some(r) = self.job.lock().unwrap().done.clone() {
                self.phase = if r.is_ok() { Phase::Done } else { Phase::Failed };
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.phase == Phase::Ask {
            self.running = sys::app_running();
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        }
        // закриття під час роботи — не перериваємо
        if ctx.input(|i| i.viewport().close_requested()) && self.phase == Phase::Working {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let install = matches!(self.mode, Mode::Install);
        let job = self.job.lock().unwrap();
        let (heading, status) = match self.phase {
            Phase::Ask => {
                if install {
                    ("Встановлення Nitor".to_string(), "Права адміністратора не потрібні.".to_string())
                } else {
                    ("Видалення Nitor".to_string(), "Оберіть, що видалити з комп'ютера.".to_string())
                }
            }
            Phase::Working => (job.heading.clone(), job.status.clone()),
            Phase::Done => match job.done.clone() {
                Some(Ok(text)) if install => (text, " ".into()),
                Some(Ok(text)) => ("Nitor видалено".into(), text),
                _ => (String::new(), String::new()),
            },
            Phase::Failed => (
                if install {
                    "Не вдалося встановити"
                } else {
                    "Не вдалося видалити Nitor"
                }
                .into(),
                job.done.clone().and_then(|r| r.err()).unwrap_or_default(),
            ),
        };
        let pct = job.pct;
        let log = job.log.clone();
        drop(job);

        let mut main_clicked = false;
        let mut second_clicked = false;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(SURFACE).stroke(Stroke::new(1.0_f32, LINE)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let w = ui.available_width();
                // заголовок вікна: логотип, «Nitor», «Згорнути», «Закрити» — за нього вікно перетягується
                let head = ui.allocate_ui_with_layout(vec2(w, 56.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(20.0);
                    ui.add(
                        egui::Image::new(egui::include_image!("../../../assets/icon.svg"))
                            .fit_to_exact_size(vec2(28.0, 28.0)),
                    );
                    ui.add_space(10.0);
                    ui.label(RichText::new("Nitor").font(bold(FS_LG)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let can_close = self.phase != Phase::Working;
                        if icon_btn("x").kind(Kind::Ghost).enabled(can_close).tip("Закрити").show(ui).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let (r, resp) = ui.allocate_exact_size(vec2(CONTROL_H, CONTROL_H), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, radius(R_MD), RAISED);
                        }
                        ui.painter().hline(
                            r.center().x - 6.0..=r.center().x + 6.0,
                            r.center().y,
                            Stroke::new(2.0_f32, TEXT),
                        );
                        if resp.on_hover_text("Згорнути").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
                let drag = ui.interact(head.response.rect, egui::Id::new("drag"), Sense::drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 8, bottom: 20 }).show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new(&heading).font(bold(FS_XL)));
                        ui.add_space(6.0);
                        let status_col = if self.phase == Phase::Failed { DANGER } else { MUTED };
                        ui.label(RichText::new(&status).size(FS_SM).color(status_col));
                        if self.phase == Phase::Ask {
                            ui.add_space(18.0);
                            match &self.mode {
                                Mode::Install => {
                                    widgets::check(ui, &mut self.desktop, "Створити ярлик на робочому столі");
                                    ui.add_space(12.0);
                                    widgets::check(ui, &mut self.start_menu, "Додати ярлик у меню «Пуск»");
                                }
                                Mode::Uninstall { .. } => self.uninstall_options(ui),
                            }
                        }
                        if self.phase == Phase::Working || (self.phase != Phase::Ask && pct > 0.0) {
                            ui.add_space(14.0);
                            let bw = ui.available_width();
                            let (r, _) = ui.allocate_exact_size(vec2(bw, 6.0), Sense::hover());
                            ui.painter().rect_filled(r, egui::CornerRadius::same(3), LINE);
                            let f = egui::Rect::from_min_size(r.min, vec2(bw * pct.clamp(0.0, 1.0), 6.0));
                            ui.painter().rect_filled(
                                f,
                                egui::CornerRadius::same(3),
                                if self.phase == Phase::Failed { DANGER } else { ACCENT },
                            );
                        }
                        if self.show_log {
                            ui.add_space(12.0);
                            egui::Frame::new()
                                .fill(RAISED)
                                .corner_radius(radius(R_MD))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    egui::ScrollArea::vertical().max_height(160.0).stick_to_bottom(true).show(
                                        ui,
                                        |ui| {
                                            for l in &log {
                                                ui.label(RichText::new(l).font(mono(FS_XS)).color(MUTED));
                                            }
                                        },
                                    );
                                });
                        }
                    },
                );
                let (line, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                ui.painter().rect_filled(line, 0.0, LINE);
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 14)).show(ui, |ui| {
                    ui.allocate_ui_with_layout(vec2(w - 48.0, CONTROL_H), Layout::left_to_right(Align::Center), |ui| {
                        if !log.is_empty() && btn("Журнал").show(ui).clicked() {
                            self.show_log = !self.show_log;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let (main, second) = self.buttons(install);
                            if let Some((text, kind)) = main {
                                main_clicked =
                                    btn(text).kind(kind).enabled(self.phase != Phase::Working).show(ui).clicked();
                            }
                            if let Some(text) = second {
                                second_clicked = btn(text).enabled(self.phase != Phase::Working).show(ui).clicked();
                            }
                        });
                    });
                });
                ui.min_rect().height()
            });
        let h = panel.inner.ceil();
        if (h - self.height).abs() > 0.5 {
            self.height = h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(600.0, h)));
        }
        if second_clicked {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.screenshot(ctx);
        if main_clicked {
            match self.phase {
                Phase::Ask | Phase::Failed => self.start(ctx),
                Phase::Done => {
                    if install {
                        let _ = sys::launch(&sys::app_dir().join(APP_EXE));
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Phase::Working => {}
            }
        }
    }
}

impl Gui {
    /// Знімок вікна для перевірки (NITOR_SCREENSHOT=файл.png): зберігає вікно у PNG і закривається.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("NITOR_SCREENSHOT") else { return };
        let frame = ctx.cumulative_frame_nr();
        if frame == 2 && std::env::var_os("NITOR_SCREENSHOT_DATA").is_some() {
            self.with_data = true;
        }
        if frame == 30 {
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
                let _ = buf.save(PathBuf::from(&path));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }

    /// Основна й друга кнопки внизу вікна.
    fn buttons(&self, install: bool) -> (Option<(String, Kind)>, Option<String>) {
        match self.phase {
            Phase::Ask | Phase::Working => {
                let main = match (install, self.running) {
                    (true, true) => ("Закрити Nitor і встановити", Kind::Primary),
                    (true, false) => ("Встановити", Kind::Primary),
                    (false, true) => ("Закрити Nitor і видалити", Kind::DangerOn),
                    (false, false) => ("Видалити", Kind::DangerOn),
                };
                (Some((main.0.into(), main.1)), Some("Скасувати".into()))
            }
            Phase::Done if install => (Some(("Запустити Nitor".into(), Kind::Primary)), Some("Закрити".into())),
            Phase::Done => (Some(("Закрити".into(), Kind::Primary)), None),
            Phase::Failed => (Some(("Спробувати ще раз".into(), Kind::Primary)), Some("Закрити".into())),
        }
    }

    fn uninstall_options(&mut self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let card = |ui: &mut egui::Ui, on: bool, title: &str, text: &str| -> bool {
            let galley = ui.fonts(|f| f.layout(text.to_string(), font(FS_SM), MUTED, w - 2.0 * S3 - 26.0));
            let h = S3 * 2.0 + 20.0 + galley.size().y + 2.0;
            let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            let stroke = if on {
                ACCENT
            } else if resp.hovered() {
                LINE_STRONG
            } else {
                LINE
            };
            ui.painter().rect(
                r,
                radius(R_MD),
                if on { ACCENT_SOFT } else { egui::Color32::TRANSPARENT },
                Stroke::new(1.0_f32, stroke),
                egui::StrokeKind::Inside,
            );
            let c = egui::pos2(r.left() + S3 + 8.0, r.top() + S3 + 10.0);
            ui.painter().circle_stroke(c, 8.0, Stroke::new(1.5_f32, if on { ACCENT } else { LINE_FIELD }));
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, title));
            if resp.has_focus() {
                ui.painter().rect_stroke(
                    r.expand(2.0),
                    radius(R_MD + 2),
                    Stroke::new(2.0_f32, ACCENT),
                    egui::StrokeKind::Outside,
                );
            }
            if on {
                ui.painter().circle_filled(c, 4.5, ACCENT);
            }
            ui.painter().text(
                egui::pos2(r.left() + S3 + 26.0, r.top() + S3),
                egui::Align2::LEFT_TOP,
                title,
                bold(FS_MD),
                TEXT,
            );
            ui.painter().galley(egui::pos2(r.left() + S3 + 26.0, r.top() + S3 + 22.0), galley, MUTED);
            resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        };
        if card(
            ui,
            !self.with_data,
            "Лише програму",
            "Сцени, прилади й налаштування залишаться. Після повторного встановлення все буде на місці.",
        ) {
            self.with_data = false;
        }
        ui.add_space(8.0);
        if card(
            ui,
            self.with_data,
            "Програму разом із даними",
            "Видалити також збережені дані. Можна вибрати, які саме.",
        ) {
            self.with_data = true;
        }
        if self.with_data {
            ui.add_space(12.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.vertical(|ui| {
                for (part, label) in [
                    (Part::Scenes, "Сцени"),
                    (Part::Fixtures, "Прилади та профілі їхніх моделей"),
                    (Part::Profiles, "Власні профілі каналів"),
                    (Part::Colors, "Статичні кольори"),
                    (Part::Settings, "Налаштування: DMX-інтерфейс, яскравість і гарячі клавіші"),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_space(26.0);
                        let mut on = self.parts.contains(&part);
                        if widgets::check(ui, &mut on, label).changed() {
                            if on {
                                self.parts.insert(part);
                            } else {
                                self.parts.remove(&part);
                            }
                        }
                    });
                }
                ui.label(widgets::muted(
                    "Щоб зберегти окрему сцену, спершу збережіть її у файл: меню сцени → «Зберегти у файл…».",
                ));
            });
        }
    }
}

/// Рендер вікна. Запасний OpenGL у wgpu на старих драйверах (Intel без DX12) малює все, крім літер, тому
/// wgpu працює лише через DirectX 12 / Vulkan / Metal (зокрема програмний WARP, якщо відеокарти немає),
/// а якщо відеокарта є лише з OpenGL або DX12 / Vulkan немає зовсім — glow. `NITOR_RENDERER=glow|wgpu`.
fn set_renderer(o: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    let all = wgpu::Backends::PRIMARY | wgpu::Backends::GL;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: all, ..Default::default() });
    let gpus: Vec<_> = instance.enumerate_adapters(all).into_iter().map(|a| a.get_info()).collect();
    for i in &gpus {
        log::info!("Відеоадаптер: {} ({:?}, {:?})", i.name, i.backend, i.device_type);
    }
    let hw = |i: &&wgpu::AdapterInfo| i.device_type != wgpu::DeviceType::Cpu;
    let primary = |i: &&wgpu::AdapterInfo| i.backend != wgpu::Backend::Gl;
    let primary_hw = gpus.iter().filter(hw).any(|i| primary(&i));
    let primary_any = gpus.iter().any(|i| primary(&i));
    let gl_hw = gpus.iter().filter(hw).any(|i| i.backend == wgpu::Backend::Gl);
    o.renderer = match std::env::var("NITOR_RENDERER").as_deref() {
        Ok("glow") => eframe::Renderer::Glow,
        Ok("wgpu") => eframe::Renderer::Wgpu,
        _ if !primary_hw && (gl_hw || !primary_any) => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let mut setup = WgpuSetupCreateNew::default();
    setup.instance_descriptor.backends = wgpu::Backends::PRIMARY;
    o.wgpu_options = WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() };
    log::info!("Рендер вікна: {:?}", o.renderer);
}
