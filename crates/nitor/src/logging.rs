//! Журнал `nitor.log` у теці програми (до 1 МБ, потім `nitor.log.1`).

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

struct FileLog {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
    level: log::LevelFilter,
}

const MAX: u64 = 1024 * 1024;

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.level && (m.target().starts_with("nitor") || m.level() <= log::Level::Warn)
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{} [{}] {}: {}\n", secs, r.level(), r.target(), r.args());
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if f.as_ref().and_then(|f| f.metadata().ok()).is_some_and(|m| m.len() > MAX) {
            *f = None;
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
        }
        if f.is_none() {
            *f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path).ok();
        }
        if let Some(file) = f.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

pub fn init(path: PathBuf, verbose: bool) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let level = if verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    let logger = Box::leak(Box::new(FileLog { path, file: Mutex::new(None), level }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}
