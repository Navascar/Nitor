//! Інтеграція з Windows: одна копія програми, ідентифікатор для панелі задач, трей, автозапуск,
//! місцевий час для розкладу.
//!
//! Друга копія не запускається: вона показує попередження й на прохання користувача просить
//! уже відкриту показати своє вікно (подія `Local\Nitor.Show`).

/// Ідентифікатор програми для панелі задач (той самий записує інсталятор у ярлики).
pub const APP_ID: &str = "Navascar.Nitor";

#[cfg(windows)]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub struct Guard(HANDLE);

    unsafe impl Send for Guard {}

    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// `Some` — ця копія перша; `None` — програма вже працює.
    pub fn single_instance() -> Option<Guard> {
        let name = wide(r"Local\Nitor.App");
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if h.is_null() {
                return Some(Guard(h));
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                return None;
            }
            Some(Guard(h))
        }
    }

    /// Просить уже відкриту копію показати своє вікно.
    pub fn signal_existing() {
        let name = wide(r"Local\Nitor.Show");
        unsafe {
            let h = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
            if !h.is_null() {
                SetEvent(h);
                CloseHandle(h);
            }
        }
    }

    /// Слухає запити «покажи вікно» від інших копій.
    pub fn listen_show(flag: Arc<AtomicBool>, ctx: egui::Context) {
        listen(r"Local\Nitor.Show", flag, ctx);
    }

    /// Слухає запит інсталятора «заверши роботу» (встановлення або видалення поверх запущеної програми).
    pub fn listen_quit(flag: Arc<AtomicBool>, ctx: egui::Context) {
        listen(r"Local\Nitor.Quit", flag, ctx);
    }

    fn listen(event: &str, flag: Arc<AtomicBool>, ctx: egui::Context) {
        let name = wide(event);
        let h = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
        if h.is_null() {
            return;
        }
        let h = h as usize;
        let _ = std::thread::Builder::new().name("event".into()).spawn(move || {
            loop {
                if unsafe { WaitForSingleObject(h as HANDLE, INFINITE) } == WAIT_OBJECT_0 {
                    flag.store(true, Ordering::Relaxed);
                    // сховане в трей вікно не перемальовується — показуємо, щоб запит виконався
                    show_window();
                    ctx.request_repaint();
                } else {
                    break;
                }
            }
        });
    }

    /// Робоча область основного монітора (без панелі задач) у точках інтерфейсу.
    pub fn work_area() -> Option<egui::Vec2> {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut r as *mut RECT).cast(), 0) };
        if ok == 0 {
            return None;
        }
        // поки winit не ввімкнув DPI-обізнаність, і розміри, і DPI — «віртуальні» (96); потім — справжні
        let scale = unsafe { GetDpiForSystem() }.max(96) as f32 / 96.0;
        let size = egui::vec2((r.right - r.left) as f32, (r.bottom - r.top) as f32) / scale;
        (size.x > 0.0 && size.y > 0.0).then_some(size)
    }

    pub fn set_app_id() {
        let id = wide(super::APP_ID);
        unsafe {
            SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
        }
    }

    static HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    static HIDDEN: AtomicBool = AtomicBool::new(false);

    pub fn set_hwnd(h: isize) {
        HWND.store(h, Ordering::Relaxed);
    }

    /// Ховає вікно (програма лишається в треї).
    pub fn hide_window() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let h = HWND.load(Ordering::Relaxed);
        if h != 0 {
            HIDDEN.store(true, Ordering::Relaxed);
            unsafe { ShowWindow(h as _, SW_HIDE) };
        }
    }

    /// Показує сховане вікно.
    pub fn show_window() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOW, SetForegroundWindow, ShowWindow};
        let h = HWND.load(Ordering::Relaxed);
        if h != 0 && HIDDEN.swap(false, Ordering::Relaxed) {
            unsafe {
                ShowWindow(h as _, SW_SHOW);
                SetForegroundWindow(h as _);
            }
        }
    }

    /// Зсув місцевого часу від UTC, хвилини (з урахуванням літнього часу).
    pub fn local_offset_min() -> i32 {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        let (mut l, mut u): (SYSTEMTIME, SYSTEMTIME) = unsafe { (std::mem::zeroed(), std::mem::zeroed()) };
        unsafe {
            GetLocalTime(&mut l);
            GetSystemTime(&mut u);
        }
        let min = |t: &SYSTEMTIME| {
            nitor_core::schedule::days_from_civil(t.wYear as i64, t.wMonth as u32, t.wDay as u32) * 1440
                + t.wHour as i64 * 60
                + t.wMinute as i64
        };
        (min(&l) - min(&u)) as i32
    }

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    /// Nitor запускається разом із Windows.
    pub fn autostart() -> bool {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).and_then(|k| k.get_value::<String, _>("Nitor")).is_ok()
    }

    pub fn set_autostart(on: bool) -> Result<(), String> {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            k.set_value("Nitor", &format!("\"{}\"", exe.display())).map_err(|e| e.to_string())
        } else {
            match k.delete_value("Nitor") {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }

    pub struct Tray(#[allow(dead_code)] tray_icon::TrayIcon);

    /// Значок у треї: клік або «Показати Nitor» — вікно, «Закрити Nitor» — вікно й запит на закриття.
    pub fn tray(ask_quit: Arc<AtomicBool>, ctx: egui::Context) -> Result<Tray, String> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};
        use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
        let img = image::load_from_memory(include_bytes!("../../../assets/icon.ico"))
            .map_err(|e| e.to_string())?
            .resize(32, 32, image::imageops::FilterType::Triangle)
            .into_rgba8();
        let (w, h) = img.dimensions();
        let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h).map_err(|e| e.to_string())?;
        let show = MenuItem::new("Показати Nitor", true, None);
        let quit = MenuItem::new("Закрити Nitor", true, None);
        let menu = Menu::new();
        menu.append_items(&[&show, &quit]).map_err(|e| e.to_string())?;
        let (show_id, quit_id) = (show.id().clone(), quit.id().clone());
        let c = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == quit_id {
                ask_quit.store(true, Ordering::Relaxed);
            }
            if e.id == quit_id || e.id == show_id {
                show_window();
                c.request_repaint();
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_window();
                ctx.request_repaint();
            }
        }));
        let t = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nitor — світло працює")
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray(t))
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    pub struct Guard;

    pub fn single_instance() -> Option<Guard> {
        Some(Guard)
    }

    pub fn signal_existing() {}

    pub fn listen_show(_flag: Arc<AtomicBool>, _ctx: egui::Context) {}

    pub fn listen_quit(_flag: Arc<AtomicBool>, _ctx: egui::Context) {}

    pub fn work_area() -> Option<egui::Vec2> {
        None
    }

    pub fn set_app_id() {
        let _ = super::APP_ID;
    }

    pub fn hide_window() {}

    // shortcut: без Windows вважаємо місцевий час UTC (програма — для Windows), для розробки на Linux цього досить
    pub fn local_offset_min() -> i32 {
        0
    }

    pub fn autostart() -> bool {
        false
    }

    pub fn set_autostart(_on: bool) -> Result<(), String> {
        Err("автозапуск доступний лише у Windows".into())
    }

    pub struct Tray;

    pub fn tray(_ask_quit: Arc<AtomicBool>, _ctx: egui::Context) -> Result<Tray, String> {
        Err("трей доступний лише у Windows".into())
    }
}

pub use imp::*;
