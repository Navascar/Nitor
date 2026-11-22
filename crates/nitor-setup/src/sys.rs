//! Windows: теки, ярлики, «Параметри → Програми», закриття запущеного Nitor, запуск програми.
//! На інших системах — заглушки (інсталятор потрібен лише на Windows).

use std::path::{Path, PathBuf};

pub const APP_ID: &str = "Navascar.Nitor";
pub const UNINST_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Nitor";

/// Тека програми: `%LOCALAPPDATA%\Nitor`.
pub fn app_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NITOR_HOME") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Nitor")
}

pub struct RegInfo<'a> {
    pub version: &'a str,
    pub size_kb: u32,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile,
    };
    use windows::Win32::System::Threading::{
        EVENT_MODIFY_STATE, OpenEventW, OpenMutexW, OpenProcess, PROCESS_SYNCHRONIZE, SYNCHRONIZATION_SYNCHRONIZE,
        SetEvent, WaitForSingleObject,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHCNE_ASSOCCHANGED, SHCNF_IDLIST,
        SHChangeNotify, SHGetKnownFolderPath, ShellLink,
    };
    use windows::core::{GUID, HSTRING, Interface, PROPVARIANT};

    fn known(id: &GUID) -> Option<PathBuf> {
        unsafe {
            let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as _));
            s.map(PathBuf::from)
        }
    }

    pub fn desktop_link() -> Option<PathBuf> {
        known(&FOLDERID_Desktop).map(|d| d.join("Nitor.lnk"))
    }

    pub fn start_menu_link() -> Option<PathBuf> {
        known(&FOLDERID_Programs).map(|d| d.join("Nitor.lnk"))
    }

    /// Ярлик на nitor.exe з ідентифікатором програми (щоб закріплення на панелі задач працювало).
    pub fn write_shortcut(link: &Path, exe: &Path) -> Result<(), String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let sl: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
            sl.SetPath(&HSTRING::from(exe.as_os_str())).map_err(|e| e.to_string())?;
            if let Some(dir) = exe.parent() {
                sl.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(|e| e.to_string())?;
            }
            sl.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0).map_err(|e| e.to_string())?;
            sl.SetDescription(&HSTRING::from("Nitor — керування RGB-прожекторами")).map_err(|e| e.to_string())?;
            let store: IPropertyStore = sl.cast().map_err(|e| e.to_string())?;
            store.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(APP_ID)).map_err(|e| e.to_string())?;
            store.Commit().map_err(|e| e.to_string())?;
            if let Some(dir) = link.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let pf: IPersistFile = sl.cast().map_err(|e| e.to_string())?;
            pf.Save(&HSTRING::from(link.as_os_str()), true).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn notify_shell() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }

    pub fn register(app_dir: &Path, info: &RegInfo) -> Result<(), String> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINST_KEY).map_err(|e| e.to_string())?;
        let dir = app_dir.display().to_string();
        let uninst = format!("\"{}\"", app_dir.join(crate::install::UNINSTALL_EXE).display());
        let set = |n: &str, v: &str| k.set_value(n, &v.to_string()).map_err(|e| e.to_string());
        set("DisplayName", "Nitor")?;
        set("DisplayVersion", info.version)?;
        set("Publisher", "Nitor")?;
        set("DisplayIcon", &app_dir.join(crate::install::APP_EXE).display().to_string())?;
        set("InstallLocation", &dir)?;
        set("UninstallString", &uninst)?;
        set("QuietUninstallString", &format!("{uninst} /S"))?;
        set("URLInfoAbout", &format!("https://github.com/{}", nitor_core::GITHUB_REPO))?;
        k.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
        k.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
        k.set_value("EstimatedSize", &info.size_kb).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn unregister() {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINST_KEY);
    }

    fn wide(s: &str) -> HSTRING {
        HSTRING::from(s)
    }

    /// Nitor зараз працює.
    pub fn app_running() -> bool {
        unsafe {
            match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &wide(r"Local\Nitor.App")) {
                Ok(h) => {
                    let _ = CloseHandle(h);
                    true
                }
                Err(_) => false,
            }
        }
    }

    /// Просить Nitor завершитися (подія Local\Nitor.Quit).
    pub fn request_quit() {
        unsafe {
            if let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, &wide(r"Local\Nitor.Quit")) {
                let _ = SetEvent(h);
                let _ = CloseHandle(h);
            }
        }
    }

    /// Чекає, поки процес завершиться (не довше `secs`).
    pub fn wait_pid(pid: u32, secs: u32) {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                let _ = WaitForSingleObject(h, secs * 1000) == WAIT_OBJECT_0;
                let _ = CloseHandle(h);
            }
        }
    }

    pub fn launch(exe: &Path) -> Result<(), String> {
        std::process::Command::new(exe)
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn desktop_link() -> Option<PathBuf> {
        None
    }
    pub fn start_menu_link() -> Option<PathBuf> {
        None
    }
    pub fn write_shortcut(_link: &Path, _exe: &Path) -> Result<(), String> {
        Ok(())
    }
    pub fn notify_shell() {}
    pub fn register(_app_dir: &Path, info: &RegInfo) -> Result<(), String> {
        log::debug!("{UNINST_KEY}: {} ({} КБ), {APP_ID}", info.version, info.size_kb);
        Ok(())
    }
    pub fn unregister() {}
    pub fn app_running() -> bool {
        false
    }
    pub fn request_quit() {}
    pub fn wait_pid(_pid: u32, _secs: u32) {}
    pub fn launch(exe: &Path) -> Result<(), String> {
        std::process::Command::new(exe).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

pub use imp::*;

/// Закриває запущений Nitor і чекає, поки він завершиться. `false` — не закрився.
pub fn close_app(timeout_s: u32) -> bool {
    if !app_running() {
        return true;
    }
    request_quit();
    for _ in 0..timeout_s * 10 {
        if !app_running() {
            // процес відпустив м'ютекс — даємо час закрити файли
            std::thread::sleep(std::time::Duration::from_millis(500));
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    !app_running()
}
