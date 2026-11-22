//! Єдиний набір іконок: 24×24, лінія 2 px, заокруглені кінці (стиль Lucide, ISC).
//!
//! Іконка — біле SVG, яке фарбується (`tint`) під колір тексту кнопки.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use egui::{Color32, Image, ImageSource, Vec2};

pub const PATHS: &[(&str, &str)] = &[
    ("plus", "M12 5v14M5 12h14"),
    ("chevronDown", "M6 9l6 6 6-6"),
    ("x", "M6 6l12 12M18 6L6 18"),
    ("eraser", "M20 20H9L4 15a2 2 0 0 1 0-2.8L13.2 3a2 2 0 0 1 2.8 0l4.9 4.9a2 2 0 0 1 0 2.8L12 19.6M8 10.5l6.5 6.5"),
    (
        "bulbOff",
        "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.5 10.9c.6.5 1 1.2 1 2V16h5v-.1c0-.8.4-1.5 1-2A6 6 0 0 0 12 3zM3 3l18 18",
    ),
    (
        "settings",
        "M12.2 2h-.4a2 2 0 0 0-2 2v.2a2 2 0 0 1-1 1.7l-.4.3a2 2 0 0 1-2 0l-.2-.1a2 2 0 0 0-2.7.7l-.2.4a2 2 0 0 0 .7 2.7l.2.1a2 2 0 0 1 1 1.7v.5a2 2 0 0 1-1 1.7l-.2.1a2 2 0 0 0-.7 2.7l.2.4a2 2 0 0 0 2.7.7l.2-.1a2 2 0 0 1 2 0l.4.3a2 2 0 0 1 1 1.7v.2a2 2 0 0 0 2 2h.4a2 2 0 0 0 2-2v-.2a2 2 0 0 1 1-1.7l.4-.3a2 2 0 0 1 2 0l.2.1a2 2 0 0 0 2.7-.7l.2-.4a2 2 0 0 0-.7-2.7l-.2-.1a2 2 0 0 1-1-1.7v-.5a2 2 0 0 1 1-1.7l.2-.1a2 2 0 0 0 .7-2.7l-.2-.4a2 2 0 0 0-2.7-.7l-.2.1a2 2 0 0 1-2 0l-.4-.3a2 2 0 0 1-1-1.7V4a2 2 0 0 0-2-2zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    ),
    ("trash", "M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6"),
    ("copy", "M9 9h11v11H9zM5 15H4V4h11v1"),
    ("clipboard", "M9 3h6v4H9zM8 5H6v16h12V5h-2"),
    (
        "scissors",
        "M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM20 4L8.1 15.9M14.5 14.5L20 20M8.1 8.1L12 12",
    ),
    ("selectAll", "M4 4h16v16H4zM8 12l3 3 5-6"),
    ("play", "M8 5v14l11-7z"),
    ("stop", "M7 7h10v10H7z"),
    ("undo", "M9 14L4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3"),
    ("redo", "M15 14l5-5-5-5M20 9H9a5 5 0 0 0 0 10h3"),
    ("star", "M12 3l2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3l-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z"),
    ("edit", "M4 20h4L19 9l-4-4L4 16zM14 6l4 4"),
    ("reset", "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5"),
    (
        "sun",
        "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
    ),
    ("half", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 3v18"),
    ("grid", "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"),
    ("layers", "M12 3l9 5-9 5-9-5zM3 13l9 5 9-5"),
    ("list", "M9 6h12M9 12h12M9 18h12M4 6h.01M4 12h.01M4 18h.01"),
    ("circle", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z"),
    ("dot", "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"),
    ("swap", "M7 4L3 8l4 4M3 8h14M17 20l4-4-4-4M21 16H7"),
    ("refresh", "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5"),
    ("arrowUp", "M12 19V5M5 12l7-7 7 7"),
    ("arrowDown", "M12 5v14M19 12l-7 7-7-7"),
    ("download", "M12 3v12M7 10l5 5 5-5M5 21h14"),
    ("upload", "M12 15V3M7 8l5-5 5 5M5 21h14"),
    ("wrench", "M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.4-.6-.6-2.4z"),
    ("bolt", "M13 2L4 14h7l-1 8 9-12h-7z"),
    ("power", "M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0"),
    ("check", "M5 12l5 5 9-10"),
    ("lock", "M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4"),
    (
        "users",
        "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM22 21v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8",
    ),
    ("alert", "M12 9v4M12 17h.01M10.3 3.9L2 18a2 2 0 0 0 1.7 3h16.6a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"),
];

fn sources() -> &'static HashMap<&'static str, Arc<[u8]>> {
    static S: OnceLock<HashMap<&'static str, Arc<[u8]>>> = OnceLock::new();
    S.get_or_init(|| {
        PATHS
            .iter()
            .map(|(name, d)| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="48" height="48"><path d="{d}" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##
                );
                (*name, Arc::from(svg.into_bytes().into_boxed_slice()))
            })
            .collect()
    })
}

/// Картинка іконки заданого розміру й кольору.
pub fn image(name: &str, size: f32, color: Color32) -> Image<'static> {
    let bytes = sources().get(name).cloned().unwrap_or_else(|| sources()["x"].clone());
    Image::new(ImageSource::Bytes { uri: format!("bytes://icon/{name}.svg").into(), bytes: bytes.into() })
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color)
}

/// SVG-рядок як картинка (схеми приладів у бібліотеці).
pub fn svg(uri: String, svg: String) -> Image<'static> {
    Image::new(ImageSource::Bytes { uri: uri.into(), bytes: svg.into_bytes().into() })
}
