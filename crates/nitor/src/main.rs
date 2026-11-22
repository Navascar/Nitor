//! Nitor — керування RGB-прожекторами через uDMX та інші DMX-інтерфейси.
//!
//! `nitor [--config шлях] [--check] [--reset-pin] [-v]`

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod already;
mod app;
mod art;
mod audio;
mod core;
mod dialogs;
mod hotkeys;

mod logging;
mod menus;
mod midi;
mod paths;
mod splash;

mod ui_main;
mod update;

mod winshell;

use std::path::PathBuf;
use std::time::Duration;

use app::{Act, App, TextOp};

struct Args {
    config: PathBuf,
    check: bool,
    reset_pin: bool,
    verbose: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { config: paths::config_path(), check: false, reset_pin: false, verbose: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = it.next().map(PathBuf::from).ok_or("--config потребує шляху")?,
            "--check" => a.check = true,
            "--reset-pin" => a.reset_pin = true,
            "-v" | "--verbose" => a.verbose = true,
            "-h" | "--help" => {
                return Err("Nitor — керування RGB-прожекторами через DMX\n\n\
                     nitor [--config шлях/до/config.json] [--check] [--reset-pin] [-v]\n\n  \
                     --config  файл налаштувань (типово %LOCALAPPDATA%\\Nitor\\config.json)\n  \
                     --check   перевірити підключення uDMX\n  \
                     --reset-pin  скинути PIN і вийти з режиму оператора\n  \
                     -v        докладний журнал"
                    .into());
            }
            other => return Err(format!("невідомий параметр {other}")),
        }
    }
    Ok(a)
}

#[cfg(windows)]
fn attach_console() {
    // програма без консолі; для --check і --help показуємо текст у консолі, з якої її запустили
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    unsafe {
        AttachConsole(u32::MAX);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(text) => {
            attach_console();
            println!("{text}");
            std::process::exit(2);
        }
    };
    if args.check {
        attach_console();
        let (lines, code) = nitor_core::outputs::check_udmx();
        for l in lines {
            println!("{l}");
        }
        std::process::exit(code);
    }
    if args.reset_pin {
        attach_console();
        // запущена програма перезаписала б файл своїми налаштуваннями
        let Some(_guard) = winshell::single_instance() else {
            println!("Спершу закрийте Nitor, потім повторіть команду.");
            std::process::exit(1);
        };
        let mut e = nitor_core::Engine::load(&args.config);
        e.pin = None;
        e.operator = false;
        match e.save(0.0) {
            Ok(()) => println!("PIN скинуто, режим оператора вимкнено."),
            Err(err) => {
                println!("Не вдалося зберегти налаштування: {err}");
                std::process::exit(1);
            }
        }
        return;
    }
    // попередження про другу копію — окремо, для знімка екрана в CI
    if std::env::var_os("NITOR_SCREENSHOT_ALREADY").is_some() {
        already::run();
        return;
    }
    // одна копія програми: друга показує попередження
    let Some(_guard) = winshell::single_instance() else {
        winshell::set_app_id();
        already::run();
        return;
    };
    winshell::set_app_id();
    let t0 = std::time::Instant::now();
    // перевірка оновлень — якнайраніше: від неї залежить, яким буде перше вікно
    let updater = update::Updater::new();
    // вікно запуску потрібне лише тоді, коли є оновлення; у режимі знімка екрана — одразу вікно програми
    let with_splash = (std::env::var_os("NITOR_SCREENSHOT").is_none()
        || std::env::var_os("NITOR_SCREENSHOT_SPLASH").is_some())
        && std::env::var_os("NITOR_NO_SPLASH").is_none();
    logging::init(paths::log_path(), args.verbose);
    log::info!("Nitor {} запускається, налаштування: {}", nitor_core::VERSION, args.config.display());
    std::panic::set_hook(Box::new(|info| log::error!("Збій: {info}")));
    updater.start();

    let engine = nitor_core::Engine::load(&args.config);
    let core = core::Core::start(engine);
    // Розмір вікна не змінюється після появи: Windows на мить розтягує старий кадр на новий
    // розмір. Тому вікно одразу створюється таким, яким воно буде: головним або вікном запуску
    // (якщо вже відомо, що є оновлення). Недовго чекаємо на перевірку тут; решту очікування
    // бере перший кадр, поки вікно ще приховане (див. `App::boot_gate`).
    let main_size = splash::main_size(winshell::work_area());
    let mut splash_now = false;
    if with_splash {
        while !updater.info().checked && t0.elapsed() < Duration::from_millis(600) {
            std::thread::sleep(Duration::from_millis(15));
        }
        let u = updater.info();
        splash_now = u.checked && update::decide(&u, f64::MAX, false) != update::SplashPhase::Open;
    }

    let icon = image::load_from_memory(include_bytes!("../../../assets/icon.ico"))
        .ok()
        .map(|i| i.into_rgba8())
        .map(|i| egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() });
    let mut viewport = egui::ViewportBuilder::default().with_title("Nitor").with_app_id("Nitor");
    let splash_h = splash::splash_height(&updater.info());
    viewport = if splash_now {
        viewport.with_inner_size([splash::WIDTH, splash_h]).with_decorations(false).with_resizable(false)
    } else {
        viewport
            .with_inner_size(main_size)
            .with_min_inner_size([900.0_f32.min(main_size.x), 600.0_f32.min(main_size.y)])
    };
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, persist_window: false, centered: true, ..Default::default() };
    set_renderer(&mut options);
    let core2 = core.clone();
    let updater2 = updater.clone();
    let result = eframe::run_native(
        "Nitor",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            nitor_ui::theme::install_fonts(&cc.egui_ctx);
            nitor_ui::theme::apply(&cc.egui_ctx);
            let mut app = App::new(core2, updater2);
            if splash_now {
                app.splash = Some(splash::Splash::new(t0, splash_h));
            } else if with_splash {
                app.boot = Some(t0);
            }
            winshell::listen_show(app.show_request.clone(), cc.egui_ctx.clone());
            winshell::listen_quit(app.quit_request.clone(), cc.egui_ctx.clone());
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = result {
        log::error!("Не вдалося відкрити вікно: {e}");
    }
    // програма завершується: світло гасне (крім перезапуску для оновлення)
    let updating = updater.info().status == update::Status::Restarting;
    core.shutdown(!updating);
    log::info!("Nitor завершено");
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh();
        self.flash_release(ctx);
        self.audio_tick();
        self.midi_tick(ctx);
        self.pending_menu = None;

        // інсталятор оновлення запущено — програма завершується, світло не гасне
        if self.updater.info().status == update::Status::Restarting && !self.quitting {
            log::info!("Оновлення: перезапуск");
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.quit_request.swap(false, std::sync::atomic::Ordering::Relaxed) {
            log::info!("Інсталятор просить завершити роботу");
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.show_request.swap(false, std::sync::atomic::Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if !self.boot_gate(ctx) {
            return;
        }
        if self.splash.is_some() {
            if !self.splash_frame(ctx) {
                self.screenshot(ctx);
                return;
            }
            self.splash = None;
            self.open_main_window(ctx);
        }
        if !self.main_window_ready(ctx) {
            return;
        }

        // закриття вікна (хрестик, Alt+F4) — лише після підтвердження
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close();
        }

        self.text_ops(ctx);
        self.topbar(ctx);
        if self.v.operator {
            self.operator_layout(ctx);
        } else {
            self.main_layout(ctx);
        }
        self.dialogs(ctx);
        self.hotkeys(ctx);
        if let Some(items) = self.pending_menu.take() {
            if !items.is_empty() {
                let pos = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();
                self.menu = Some(menus::MenuState { pos, items, opened: ctx.input(|i| i.time) });
            }
        }
        self.show_menu(ctx);
        self.show_toast(ctx);
        self.screenshot(ctx);
        // живі кольори приладів і DMX-монітор — ~15 разів на секунду; згорнуте вікно малювати
        // нема чого (світло рахує потік рушія), лише раз на секунду перевіряємо стан
        let minimized = ctx.input(|i| i.viewport().minimized == Some(true));
        ctx.request_repaint_after(Duration::from_millis(if minimized { 1000 } else { 66 }));
    }
}

impl App {
    /// Операції з текстом із контекстного меню поля вводу (через події egui).
    fn text_ops(&mut self, ctx: &egui::Context) {
        let Some((id, op, wait)) = self.pending_text_op.take() else { return };
        if wait > 0 {
            ctx.memory_mut(|m| m.request_focus(id));
            self.pending_text_op = Some((id, op, wait - 1));
            ctx.request_repaint();
            return;
        }
        let ev = match op {
            TextOp::Cut => egui::Event::Cut,
            TextOp::Copy => egui::Event::Copy,
            TextOp::Paste => match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(t) => egui::Event::Paste(t),
                Err(_) => {
                    self.toast("Немає доступу до буфера обміну", false);
                    return;
                }
            },
            TextOp::SelectAll => egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
        };
        ctx.input_mut(|i| i.events.push(ev));
    }

    fn hotkeys(&mut self, ctx: &egui::Context) {
        if self.settings.as_ref().is_some_and(|s| s.capturing.is_some()) {
            return;
        }
        // у полях введення — своє скасування й виділення
        if ctx.wants_keyboard_input() || self.any_dialog() || self.menu.is_some() {
            return;
        }
        let events: Vec<(egui::Key, egui::Modifiers, bool)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers } => {
                        Some((physical_key.unwrap_or(*key), *modifiers, *repeat))
                    }
                    _ => None,
                })
                .collect()
        });
        for (key, mods, repeat) in events {
            let Some(combo) = hotkeys::combo(key, mods) else { continue };
            let Some(action) = hotkeys::action_for(&self.v.hotkeys, &combo) else { continue };
            if repeat && action != "undo" && action != "redo" {
                continue;
            }
            // у режимі оператора — без скасування й налаштувань
            if self.v.operator && matches!(action.as_str(), "undo" | "redo" | "settings") {
                continue;
            }
            if action == "flash" {
                self.flash_color(app::FLASH_WHITE, app::FlashHold::Key(key));
                continue;
            }
            self.run_hotkey(ctx, &action);
        }
    }
}

impl App {
    /// Знімок вікна для перевірки (NITOR_SCREENSHOT=файл.png [NITOR_SCREENSHOT_ACTS=дії]): після кількох
    /// кадрів зберігає вікно у PNG і закривається. Використовується в CI на Windows.
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("NITOR_SCREENSHOT") else { return };
        let frame = ctx.cumulative_frame_nr();
        if frame == 5 {
            if let Ok(acts) = std::env::var("NITOR_SCREENSHOT_ACTS") {
                for a in acts.split(',') {
                    match a {
                        "selectAll" => self.act(ctx, Act::SelectAll),
                        "settings" => self.open_settings(None),
                        "patch" => self.open_settings(Some(dialogs::Tab::Patch)),
                        "keys" => self.open_settings(Some(dialogs::Tab::Keys)),
                        "midi" => self.open_settings(Some(dialogs::Tab::Midi)),
                        "add" => self.open_add(),
                        "library" => {
                            self.open_add();
                            self.lib = Some(dialogs::LibState { kind: String::new(), q: String::new(), shown: 60 });
                        }
                        "props" => {
                            if let Some(f) = self.v.show.fixtures.first() {
                                let id = f.id.clone();
                                self.open_props(&id);
                            }
                        }
                        "close" => self.request_close(),
                        "monitor" => self.monitor_open = true,
                        "menu" => {
                            let items = self.general_menu();
                            self.menu = Some(menus::MenuState { pos: egui::pos2(500.0, 300.0), items, opened: 0.0 });
                        }
                        _ => {}
                    }
                }
            }
        }
        // вікно запуску знімаємо, коли перевірка оновлень завершилась (або за 25 с)
        let waiting = self
            .splash
            .as_ref()
            .is_some_and(|s| !self.updater.info().checked && s.t0.elapsed() < std::time::Duration::from_secs(25));
        if waiting {
            self.shot_frame = None;
        } else if self.shot_frame.is_none() {
            self.shot_frame = Some((frame + 20).max(40));
        }
        if frame >= 40 && self.shot_frame == Some(frame) {
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
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

/// Рендер вікна. Запасний OpenGL у wgpu на старих драйверах (Intel без DX12) малює все, крім літер, тому
/// wgpu працює лише через DirectX 12 / Vulkan / Metal (зокрема програмний WARP, якщо відеокарти немає),
/// а якщо відеокарта є лише з OpenGL або DX12 / Vulkan немає зовсім — glow. `NITOR_RENDERER=glow|wgpu`.
pub(crate) fn set_renderer(o: &mut eframe::NativeOptions) {
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
