//! Інтеграція з Windows: одна копія програми, ідентифікатор для панелі задач.
//!
//! Друга копія не запускається: вона лише просить уже відкриту показати своє вікно
//! (подія `Local\Nitor.Show`) і завершується.

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
                    ctx.request_repaint();
                } else {
                    break;
                }
            }
        });
    }

    pub fn set_app_id() {
        let id = wide(super::APP_ID);
        unsafe {
            SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
        }
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

    pub fn set_app_id() {}
}

pub use imp::*;
