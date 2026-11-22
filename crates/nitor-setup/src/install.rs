//! Файлові операції встановлення й видалення (без вікон і без Windows API — перевіряються тестами).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use nitor_core::userdata::{self, Part};

pub const APP_EXE: &str = "nitor.exe";
pub const UNINSTALL_EXE: &str = "Uninstall.exe";
pub const LICENSES: &str = "THIRD_PARTY_LICENSES.md";
pub const LICENSES_TEXT: &str = include_str!("../../../THIRD_PARTY_LICENSES.md");

/// Стиснена програма, вкладена під час збирання.
static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

pub fn payload() -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(PAYLOAD).read_to_end(&mut out).map_err(|e| e.to_string())?;
    if out.is_empty() {
        return Err("це збірка інсталятора без програми".into());
    }
    Ok(out)
}

/// Розмір програми після встановлення, КБ (для «Параметри → Програми»).
pub fn installed_size_kb(app: &[u8]) -> u32 {
    ((app.len() + LICENSES_TEXT.len()) / 1024) as u32 + 1
}

/// Записує файл через тимчасовий і перейменування (запущений exe можна перейменувати, але не перезаписати).
fn write_replace(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    if path.exists() {
        let old = path.with_extension("old");
        let _ = std::fs::remove_file(&old);
        if std::fs::rename(path, &old).is_ok() {
            let _ = std::fs::remove_file(&old);
        }
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Копіює програму, деінсталятор (копію `setup`) і ліцензії в теку програми.
pub fn install_files(app_dir: &Path, app: &[u8], setup: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(app_dir).map_err(|e| format!("{}: {e}", app_dir.display()))?;
    write_replace(&app_dir.join(APP_EXE), app)?;
    write_replace(&app_dir.join(UNINSTALL_EXE), setup)?;
    write_replace(&app_dir.join(LICENSES), LICENSES_TEXT.as_bytes())?;
    Ok(())
}

/// Що видалити разом із програмою.
#[derive(Clone, Debug, PartialEq)]
pub enum Removal {
    /// Лише програму: сцени, прилади й налаштування лишаються.
    ProgramOnly,
    /// Програму й вибрані частини даних.
    WithData(BTreeSet<Part>),
}

/// Підсумок видалення.
#[derive(Debug, Default)]
pub struct Removed {
    /// Файл налаштувань лишився (з невидаленими частинами).
    pub data_kept: bool,
    pub failed: Vec<PathBuf>,
}

/// Видаляє програму з теки. Деінсталятор, що зараз працює (`except`), видаляється окремо.
pub fn uninstall_files(app_dir: &Path, removal: &Removal, except: Option<&Path>) -> Result<Removed, String> {
    // захист: видаляємо лише теку, схожу на теку Nitor
    let looks_like = app_dir.join(APP_EXE).exists() || app_dir.join(UNINSTALL_EXE).exists();
    if !looks_like {
        return Err(format!("Тека {} не схожа на теку Nitor — її не видалено.", app_dir.display()));
    }
    let config = app_dir.join("config.json");
    let mut out = Removed::default();
    match removal {
        Removal::ProgramOnly => out.data_kept = config.exists(),
        Removal::WithData(parts) => {
            if !parts.is_empty() {
                let removed = userdata::apply(&config, parts).map_err(|e| format!("Дані не видалено: {e}"))?;
                out.data_kept = !removed;
            } else {
                out.data_kept = config.exists();
            }
        }
    }
    let keep_data = |name: &str| out.data_kept && (name == "config.json");
    let Ok(entries) = std::fs::read_dir(app_dir) else { return Ok(out) };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if keep_data(&name) || except.is_some_and(|x| same_file(x, &p)) {
            continue;
        }
        // зіпсовані налаштування, відкладені вбік, — теж дані користувача
        if out.data_kept && name.starts_with("config.broken-") {
            continue;
        }
        let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        if r.is_err() {
            out.failed.push(p);
        }
    }
    if !out.data_kept && except.is_none() {
        let _ = std::fs::remove_dir(app_dir);
    }
    Ok(out)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Текст «Дані залишено: сцени, прилади…» для вікна видалення.
pub fn kept_parts(removal: &Removal) -> Vec<&'static str> {
    match removal {
        Removal::ProgramOnly => vec![],
        Removal::WithData(parts) => Part::ALL.iter().filter(|p| !parts.contains(p)).map(|p| p.label()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(p, b"x").unwrap();
    }

    #[test]
    fn install_writes_files_and_replaces() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("Nitor");
        install_files(&app, b"app-v1", b"setup").unwrap();
        install_files(&app, b"app-v2", b"setup2").unwrap();
        assert_eq!(std::fs::read(app.join(APP_EXE)).unwrap(), b"app-v2");
        assert_eq!(std::fs::read(app.join(UNINSTALL_EXE)).unwrap(), b"setup2");
        assert!(app.join(LICENSES).exists());
        assert!(!app.join("nitor.new").exists() && !app.join("nitor.old").exists());
    }

    #[test]
    fn uninstall_program_only_keeps_data() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("Nitor");
        install_files(&app, b"a", b"s").unwrap();
        touch(&app.join("config.json"));
        touch(&app.join("nitor.log"));
        let r = uninstall_files(&app, &Removal::ProgramOnly, None).unwrap();
        assert!(r.data_kept && r.failed.is_empty());
        let left: Vec<String> =
            std::fs::read_dir(&app).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
        assert_eq!(left, ["config.json"]);
    }

    #[test]
    fn uninstall_with_all_data_removes_folder() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("Nitor");
        install_files(&app, b"a", b"s").unwrap();
        touch(&app.join("config.json"));
        let parts: BTreeSet<Part> = Part::ALL.into_iter().collect();
        let r = uninstall_files(&app, &Removal::WithData(parts), None).unwrap();
        assert!(!r.data_kept);
        assert!(!app.exists());
    }

    #[test]
    fn uninstall_some_parts_keeps_rest() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("Nitor");
        install_files(&app, b"a", b"s").unwrap();
        std::fs::write(
            app.join("config.json"),
            r#"{"version":2,"scenes":{"S":{"fixtures":{}}},"fixtures":[{"id":"a","name":"A","address":1,"profile":"RGB (3 кан.)"}]}"#,
        )
        .unwrap();
        let parts: BTreeSet<Part> = [Part::Scenes].into_iter().collect();
        let r = uninstall_files(&app, &Removal::WithData(parts.clone()), None).unwrap();
        assert!(r.data_kept);
        let cfg = std::fs::read_to_string(app.join("config.json")).unwrap();
        assert!(!cfg.contains("\"scenes\"") && cfg.contains("fixtures"));
        assert_eq!(kept_parts(&Removal::WithData(parts)).len(), 4);
    }

    #[test]
    fn refuses_foreign_folder() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("important.docx"));
        assert!(uninstall_files(d.path(), &Removal::ProgramOnly, None).is_err());
        assert!(d.path().join("important.docx").exists());
    }

    #[test]
    fn running_uninstaller_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        let app = d.path().join("Nitor");
        install_files(&app, b"a", b"s").unwrap();
        let me = app.join(UNINSTALL_EXE);
        uninstall_files(&app, &Removal::WithData(Part::ALL.into_iter().collect()), Some(&me)).unwrap();
        assert!(me.exists() && !app.join(APP_EXE).exists());
    }
}
