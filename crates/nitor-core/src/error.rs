use std::fmt;

/// Помилка дії користувача або даних. Текст — українською, його показує інтерфейс.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(io_text(&e))
    }
}

/// Текст системної помилки українською (англійський текст ОС — лише для невідомих випадків).
pub fn io_text(e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    let t = match e.kind() {
        K::NotFound => "не знайдено",
        K::PermissionDenied => "немає доступу",
        K::AlreadyExists => "вже існує",
        K::AddrInUse => "адресу вже зайнято іншою програмою",
        K::AddrNotAvailable => "адреса недоступна на цьому комп'ютері",
        K::ConnectionRefused => "з'єднання відхилено",
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe => "з'єднання розірвано",
        K::NotConnected => "немає з'єднання",
        K::NetworkUnreachable => "мережа недоступна",
        K::HostUnreachable => "вузол мережі недоступний",
        K::TimedOut => "пристрій не відповідає",
        K::InvalidInput => "неправильні параметри",
        K::InvalidData => "пошкоджені дані",
        K::UnexpectedEof => "файл обрізаний",
        K::StorageFull => "на диску немає місця",
        K::ReadOnlyFilesystem => "диск лише для читання",
        K::OutOfMemory => "недостатньо пам'яті",
        _ => return e.to_string(),
    };
    t.to_string()
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(e.to_string())
    }
}

/// `bail!("текст {}", x)` — повернути помилку з функції.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::Error(format!($($arg)*))) };
}

/// Створити помилку без виходу з функції.
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => { $crate::Error(format!($($arg)*)) };
}
