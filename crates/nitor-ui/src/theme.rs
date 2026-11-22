//! Дизайн-система: токени кольорів, шрифтів, відступів і радіусів.
//!
//! Один основний (синій) колір дії на екрані; червоний — лише небезпечні дії; контраст тексту ≥ 4.5:1.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x11, 0x15);
/// верхня панель, вікна, меню
pub const SURFACE: Color32 = Color32::from_rgb(0x17, 0x1a, 0x21);
/// плитки, поля, кнопки
pub const RAISED: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2c);
pub const LINE: Color32 = Color32::from_rgb(0x2b, 0x30, 0x3b);
pub const LINE_STRONG: Color32 = Color32::from_rgb(0x3d, 0x44, 0x52);
/// межа полів вводу, списків і чекбоксів: ≥ 3:1 на BG, SURFACE і RAISED (WCAG 1.4.11)
pub const LINE_FIELD: Color32 = Color32::from_rgb(0x70, 0x78, 0x88);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xee);
/// ≥ 4.5:1 на BG, SURFACE і RAISED
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa1, 0xae);
/// єдиний акцент: основні дії, вибір (рамки, текст, лінії)
pub const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x8c, 0xff);
/// заливка під білим текстом (контраст 4.7:1)
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(0x35, 0x70, 0xd9);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x2f, 0x63, 0xc4);
/// rgba(79, 140, 255, .14)
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(11, 20, 36, 36);
/// лише небезпечні дії
pub const DANGER: Color32 = Color32::from_rgb(0xef, 0x5b, 0x60);
/// заливка під білим текстом (контраст 4.6:1)
pub const DANGER_STRONG: Color32 = Color32::from_rgb(0xd2, 0x3f, 0x45);
/// лише попередження
pub const WARN: Color32 = Color32::from_rgb(0xf5, 0xa5, 0x24);
/// лише статус «працює»
pub const OK: Color32 = Color32::from_rgb(0x3e, 0xcf, 0x8e);
pub const WHITE: Color32 = Color32::WHITE;
pub const TOAST_ERR_BG: Color32 = Color32::from_rgb(0x3a, 0x1f, 0x22);
/// найтемніше: текст на світлих кольорах, «вимкнене» світло, тло монітора
pub const INK: Color32 = Color32::from_rgb(0x0b, 0x0d, 0x10);

// типографічна шкала
pub const FS_XS: f32 = 11.0;
pub const FS_SM: f32 = 12.0;
pub const FS_MD: f32 = 14.0;
pub const FS_LG: f32 = 16.0;
pub const FS_XL: f32 = 20.0;

// відступи
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
pub const S5: f32 = 24.0;

// радіуси
pub const R_SM: u8 = 6;
pub const R_MD: u8 = 10;
pub const R_LG: u8 = 14;

pub const CONTROL_H: f32 = 34.0;
pub const CONTROL_H_SM: f32 = 28.0;

/// Напівжирний шрифт для заголовків і назв.
pub const BOLD: &str = "bold";
/// Моноширинний шрифт для каналів і клавіш.
pub const MONO: &str = "mono";

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO.into()))
}

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn radius(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

/// Напівпрозорий колір із RGB 0..255 і альфою 0..1.
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

/// Колір токена з прозорістю 0..1.
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
}

fn read_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().filter_map(|p| std::fs::read(p).ok()).find(|d| draws_text(d))
}

/// Чи малює шрифт видимі літери (латиниця й українська кирилиця) так, як їх малюватиме egui.
/// Системний файл може розібратися без помилки, але давати порожні гліфи — тоді весь текст зникає.
fn draws_text(data: &[u8]) -> bool {
    use ab_glyph::{Font, FontRef};
    let Ok(f) = FontRef::try_from_slice(data) else { return false };
    "AaЖжЇїҐґ0".chars().all(|c| {
        let id = f.glyph_id(c);
        id.0 != 0
            && f.outline_glyph(id.with_scale(16.0)).is_some_and(|g| {
                let b = g.px_bounds();
                b.width() >= 1.0 && b.height() >= 1.0
            })
    })
}

/// Шрифти: системний Segoe UI на Windows (`system-ui`), інакше вбудований.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let f = |name: &str| format!(r"{windir}\Fonts\{name}");
    let regular = read_font(&[
        &f("segoeui.ttf"),
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]);
    let semibold = read_font(&[
        &f("seguisb.ttf"),
        &f("segoeuib.ttf"),
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    ]);
    let monospace = read_font(&[
        &f("CascadiaMono.ttf"),
        &f("consola.ttf"),
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    ]);
    let mut add = |key: &str, data: Option<Vec<u8>>, family: FontFamily| {
        let list_fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        let entry = fonts.families.entry(family).or_default();
        if let Some(d) = data {
            fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(d)));
            entry.insert(0, key.into());
        }
        // запасні гліфи (кирилиця, символи) — зі стандартних шрифтів egui
        for name in list_fallback {
            if !entry.contains(&name) {
                entry.push(name);
            }
        }
    };
    add("ui", regular.clone(), FontFamily::Proportional);
    add("ui-bold", semibold.or(regular), FontFamily::Name(BOLD.into()));
    add("ui-mono", monospace, FontFamily::Name(MONO.into()));
    ctx.set_fonts(fonts);
}

/// Стилі egui під токени дизайн-системи.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, font(FS_SM)),
            (TextStyle::Body, font(FS_MD)),
            (TextStyle::Button, font(FS_MD)),
            (TextStyle::Heading, bold(FS_LG)),
            (TextStyle::Monospace, mono(FS_SM)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(S2, S2);
        s.spacing.button_padding = egui::vec2(S3, 0.0);
        s.spacing.interact_size = egui::vec2(CONTROL_H, CONTROL_H);
        s.spacing.combo_height = 320.0;
        s.spacing.menu_margin = egui::Margin::same(S1 as i8);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.interaction.selectable_labels = false;
        s.animation_time = 0.12;

        let v: &mut Visuals = &mut s.visuals;
        v.dark_mode = true;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = RAISED;
        v.faint_bg_color = RAISED;
        v.code_bg_color = RAISED;
        v.window_stroke = Stroke::new(1.0_f32, LINE);
        v.window_corner_radius = radius(R_LG);
        v.menu_corner_radius = radius(R_LG);
        v.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: rgba(0, 0, 0, 0.45) };
        v.popup_shadow = v.window_shadow;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = ACCENT_STRONG;
        v.selection.stroke = Stroke::new(1.0_f32, WHITE);
        v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
        v.warn_fg_color = WARN;
        v.error_fg_color = DANGER;
        v.striped = false;
        let w = &mut v.widgets;
        for (st, fill, line) in [
            (&mut w.noninteractive, SURFACE, LINE),
            (&mut w.inactive, RAISED, LINE_FIELD),
            (&mut w.hovered, RAISED, MUTED),
            (&mut w.active, RAISED, ACCENT),
            (&mut w.open, RAISED, LINE_STRONG),
        ] {
            st.bg_fill = fill;
            st.weak_bg_fill = fill;
            st.bg_stroke = Stroke::new(1.0_f32, line);
            st.corner_radius = radius(R_MD);
            st.fg_stroke = Stroke::new(1.0_f32, TEXT);
            st.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_font_check() {
        let defaults = FontDefinitions::default();
        assert!(draws_text(defaults.font_data["Ubuntu-Light"].font.as_ref()));
        // emoji-шрифт не має латиниці й кирилиці — не годиться як основний
        assert!(!draws_text(defaults.font_data["NotoEmoji-Regular"].font.as_ref()));
        assert!(!draws_text(b"not a font"));
    }
}
