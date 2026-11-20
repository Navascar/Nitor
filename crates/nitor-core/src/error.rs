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
        Error(e.to_string())
    }
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
