//! Оновлення з GitHub Releases: перевірка під час запуску й далі щогодини (без мережі — кожні
//! 10 хвилин), завантаження `Nitor-Setup.exe` і тихе встановлення поверх. Інсталятор чекає, поки
//! програма завершиться, замінює файли й запускає нову версію. Світло під час оновлення не гасне.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub const ASSET_NAME: &str = "Nitor-Setup.exe";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(3600);
pub const OFFLINE_RETRY: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    Installing,
    /// інсталятор запущено — програма завершується
    Restarting,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateInfo {
    pub status: Status,
    /// Перевірка завершилась (будь-як): вікно запуску чекає саме на це.
    pub checked: bool,
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub url: Option<String>,
    pub page: String,
    pub error: String,
    pub progress: String,
    /// Чому автооновлення недоступне.
    pub blocked: Option<String>,
    pub offline: bool,
}

/// Перші три числа версії: «v2.10.1» → [2, 10, 1].
pub fn parse_version(v: &str) -> Vec<u64> {
    let nums: Vec<u64> = v
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(3)
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.is_empty() { vec![0] } else { nums }
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

/// Що нового — коротко, без markdown (для вікна запуску).
pub fn short_notes(notes: &str, limit: usize) -> String {
    let lines: Vec<String> = notes
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).or_else(|| l.strip_prefix('-')) {
            Some(rest) => format!("• {}", rest.trim_start()),
            None => l.to_string(),
        })
        .collect();
    let mut out = lines.iter().take(limit).cloned().collect::<Vec<_>>().join("\n");
    if lines.len() > limit {
        out.push_str("\n…");
    }
    out
}

/// Відсоток із тексту прогресу («Завантаження… 42%»).
pub fn progress_percent(text: &str) -> Option<u32> {
    let i = text.find('%')?;
    let digits: String = text[..i].trim_end().chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.chars().rev().collect::<String>().parse().ok()?;
    Some(n.min(100))
}

fn api_url() -> String {
    std::env::var("NITOR_UPDATE_API")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{}/releases/latest", nitor_core::GITHUB_REPO))
}

fn agent(timeout: Duration, url: &str) -> ureq::Agent {
    // локальний сервер (перевірка) — без проксі
    let local = url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(timeout)
        .try_proxy_from_env(!local)
        .user_agent(&format!("Nitor/{}", nitor_core::VERSION))
        .build()
}

struct Release {
    latest: String,
    notes: String,
    url: Option<String>,
    page: String,
}

enum CheckError {
    Offline(String),
    Other(String),
}

fn fetch_release() -> Result<Release, CheckError> {
    let resp =
        agent(Duration::from_secs(15), &api_url()).get(&api_url()).set("Accept", "application/vnd.github+json").call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Transport(t)) => return Err(CheckError::Offline(t.to_string())),
        Err(ureq::Error::Status(404, _)) => {
            // релізів ще немає
            return Ok(Release { latest: String::new(), notes: String::new(), url: None, page: String::new() });
        }
        Err(e) => return Err(CheckError::Other(e.to_string())),
    };
    let text = resp.into_string().map_err(|e| CheckError::Other(e.to_string()))?;
    let rel: serde_json::Value = serde_json::from_str(&text).map_err(|e| CheckError::Other(e.to_string()))?;
    let latest = rel["tag_name"].as_str().unwrap_or("").trim_start_matches(['v', 'V']).to_string();
    let url = rel["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == ASSET_NAME))
        .and_then(|x| x["browser_download_url"].as_str())
        .map(String::from);
    let notes: String = rel["body"].as_str().unwrap_or("").trim().chars().take(4000).collect();
    let page = rel["html_url"].as_str().unwrap_or("").to_string();
    Ok(Release { latest, notes, url, page })
}

/// Тека, куди встановлено програму (там, де лежить nitor.exe).
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from))
}

/// `None` — можна оновлюватись; інакше пояснення, чому ні.
pub fn can_self_update() -> Option<String> {
    // для перевірки вікна запуску й CI: вважати, що оновлення можна встановити
    if std::env::var_os("NITOR_UPDATE_ALLOW").is_some() {
        return None;
    }
    if !cfg!(windows) {
        return Some("автооновлення доступне лише у версії для Windows".into());
    }
    let dir = exe_dir()?;
    let installed = crate::paths::app_dir();
    let same = dir.canonicalize().ok() == installed.canonicalize().ok();
    if !same {
        return Some("програму запущено не з теки встановлення — оновіть її інсталятором".into());
    }
    let probe = dir.join(".write-test");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(probe);
            None
        }
        Err(_) => Some(format!("немає прав на запис у {}", dir.display())),
    }
}

#[derive(Default)]
struct Signals {
    check_now: bool,
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<UpdateInfo>>,
    wake: Arc<(Mutex<Signals>, Condvar)>,
}

impl Updater {
    pub fn new() -> Updater {
        let info = UpdateInfo {
            current: nitor_core::VERSION.into(),
            page: format!("https://github.com/{}/releases", nitor_core::GITHUB_REPO),
            ..Default::default()
        };
        Updater { state: Arc::new(Mutex::new(info)), wake: Arc::new((Mutex::new(Signals::default()), Condvar::new())) }
    }

    /// Фонова перевірка: одразу під час запуску, далі щогодини (без мережі — частіше).
    pub fn start(&self) {
        if std::env::var_os("NITOR_NO_UPDATE").is_some() {
            let mut s = self.state.lock().unwrap();
            s.checked = true;
            return;
        }
        let me = self.clone();
        let _ = std::thread::Builder::new().name("update".into()).spawn(move || {
            loop {
                me.check_blocking();
                let wait = if me.info().offline { OFFLINE_RETRY } else { CHECK_INTERVAL };
                let (lock, cv) = &*me.wake;
                let g = lock.lock().unwrap();
                let (mut g, _) = cv.wait_timeout_while(g, wait, |s| !s.check_now).unwrap();
                g.check_now = false;
            }
        });
    }

    pub fn info(&self) -> UpdateInfo {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, f: impl FnOnce(&mut UpdateInfo)) {
        f(&mut self.state.lock().unwrap());
    }

    /// Перевірити зараз (кнопка «Перевірити оновлення»).
    pub fn check(&self) {
        let (lock, cv) = &*self.wake;
        lock.lock().unwrap().check_now = true;
        cv.notify_all();
    }

    fn check_blocking(&self) {
        {
            let mut s = self.state.lock().unwrap();
            if matches!(s.status, Status::Checking | Status::Installing | Status::Restarting) {
                return;
            }
            s.status = Status::Checking;
            s.error.clear();
        }
        let result = fetch_release();
        let blocked = can_self_update();
        self.set(|s| {
            // поки йшла перевірка, могло початися встановлення — його стан не перезаписуємо
            if s.status == Status::Checking {
                s.status = Status::Idle;
            }
            s.checked = true;
            match result {
                Ok(r) => {
                    s.offline = false;
                    s.available = !r.latest.is_empty() && is_newer(&r.latest, &s.current) && r.url.is_some();
                    s.latest = r.latest;
                    s.notes = r.notes;
                    if !matches!(s.status, Status::Installing | Status::Restarting) {
                        s.url = r.url;
                    }
                    if !r.page.is_empty() {
                        s.page = r.page;
                    }
                    s.blocked = blocked;
                }
                Err(CheckError::Offline(e)) => {
                    log::info!("Немає підключення до інтернету — перевірку оновлень відкладено ({e})");
                    s.offline = true;
                    s.error = "Немає підключення до інтернету".into();
                }
                Err(CheckError::Other(e)) => {
                    log::warn!("Не вдалося перевірити оновлення: {e}");
                    s.error = format!("Не вдалося перевірити оновлення: {e}");
                }
            }
        });
    }

    /// Завантажити й запустити інсталятор нової версії.
    pub fn apply(&self) {
        let url = {
            let mut s = self.state.lock().unwrap();
            if !s.available || matches!(s.status, Status::Installing | Status::Restarting) {
                return;
            }
            let Some(url) = s.url.clone() else { return };
            s.status = Status::Installing;
            s.error.clear();
            s.progress = "Підготовка…".into();
            url
        };
        let me = self.clone();
        let _ = std::thread::Builder::new().name("update-install".into()).spawn(move || match me.install(&url) {
            Ok(()) => me.set(|s| {
                s.status = Status::Restarting;
                s.progress = "Перезапуск…".into();
            }),
            Err(e) => {
                log::error!("Оновлення не вдалося: {e}");
                me.set(|s| {
                    s.status = Status::Idle;
                    s.error = format!("Оновлення не вдалося: {e}");
                });
            }
        });
    }

    fn install(&self, url: &str) -> Result<(), String> {
        if let Some(reason) = can_self_update() {
            return Err(reason);
        }
        self.set(|s| s.progress = "Завантаження…".into());
        let resp = agent(Duration::from_secs(300), url).get(url).call().map_err(|e| e.to_string())?;
        let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let path = std::env::temp_dir().join(format!("Nitor-Setup-{}.exe", self.info().latest));
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut reader = resp.into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if total > 0 {
                let pct = done * 100 / total;
                self.set(|s| s.progress = format!("Завантаження… {pct}%"));
            }
        }
        drop(file);
        if total > 0 && done != total {
            return Err("файл оновлення завантажено не повністю".into());
        }
        self.set(|s| s.progress = "Встановлення…".into());
        // інсталятор дочекається завершення цієї програми, замінить файли й запустить нову версію
        std::process::Command::new(&path)
            .args(["/update", "--wait-pid", &std::process::id().to_string()])
            .spawn()
            .map_err(|e| format!("не вдалося запустити інсталятор: {e}"))?;
        Ok(())
    }
}

/// Що показувати вікну запуску.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplashPhase {
    Checking,
    Update,
    Installing,
    Failed,
    Open,
}

/// Довше за це на перевірку оновлень не чекаємо (повільний інтернет).
pub const CHECK_WAIT_S: f64 = 20.0;

/// Рішення вікна запуску: `waited_s` — скільки секунд вікно вже відкрите;
/// `requested` — користувач натиснув «Оновити».
pub fn decide(u: &UpdateInfo, waited_s: f64, requested: bool) -> SplashPhase {
    if matches!(u.status, Status::Installing | Status::Restarting) {
        return SplashPhase::Installing;
    }
    if requested {
        return if u.error.is_empty() { SplashPhase::Installing } else { SplashPhase::Failed };
    }
    if !u.checked && waited_s < CHECK_WAIT_S {
        return SplashPhase::Checking;
    }
    if u.available && u.blocked.is_none() {
        return SplashPhase::Update;
    }
    SplashPhase::Open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("2.0.1", "2.0.0"));
        assert!(is_newer("v2.10.0", "2.9.9"));
        assert!(!is_newer("2.0.0", "2.0.0"));
        assert!(!is_newer("1.26.9", "2.0.0"));
        assert!(!is_newer("", "2.0.0"));
        assert_eq!(parse_version("v1.2.3-beta"), vec![1, 2, 3]);
    }

    fn upd() -> UpdateInfo {
        UpdateInfo { checked: true, current: "2.0.0".into(), ..Default::default() }
    }

    #[test]
    fn startup_flow() {
        let mut u = upd();
        u.checked = false;
        assert_eq!(decide(&u, 1.0, false), SplashPhase::Checking);
        assert_eq!(decide(&u, CHECK_WAIT_S + 1.0, false), SplashPhase::Open, "повільний інтернет — не тримаємо");
        assert_eq!(decide(&upd(), 0.0, false), SplashPhase::Open);
        let offline = UpdateInfo { offline: true, error: "Немає підключення".into(), ..upd() };
        assert_eq!(decide(&offline, 0.0, false), SplashPhase::Open);
    }

    #[test]
    fn update_is_required() {
        let u = UpdateInfo { available: true, latest: "9.9.9".into(), ..upd() };
        assert_eq!(decide(&u, 0.0, false), SplashPhase::Update);
        let blocked = UpdateInfo { blocked: Some("немає прав".into()), ..u.clone() };
        assert_eq!(decide(&blocked, 0.0, false), SplashPhase::Open);
        assert_eq!(decide(&u, 0.0, true), SplashPhase::Installing);
        assert_eq!(decide(&UpdateInfo { status: Status::Installing, ..u.clone() }, 0.0, true), SplashPhase::Installing);
        let failed = UpdateInfo { error: "Оновлення не вдалося: мережа".into(), ..u };
        assert_eq!(decide(&failed, 0.0, true), SplashPhase::Failed);
    }

    #[test]
    fn text_helpers() {
        assert_eq!(progress_percent("Завантаження… 42%"), Some(42));
        assert_eq!(progress_percent("Підготовка…"), None);
        assert_eq!(short_notes("## 1.0\n- Перше\n- Друге\n* Третє", 6), "• Перше\n• Друге\n• Третє");
        let many: String = (0..10).map(|i| format!("- {i}\n")).collect();
        assert!(short_notes(&many, 2).ends_with('…'));
    }
}
