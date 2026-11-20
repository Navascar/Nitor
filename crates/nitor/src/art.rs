//! Схематичне зображення приладу: форма за типом, світлодіоди — його кольори, у барів — сегменти.

use nitor_core::library::LibModel;

pub const LED: [(&str, &str, &str); 6] = [
    ("r", "#ff4a4a", "R"),
    ("g", "#3ddc6b", "G"),
    ("b", "#4f7dff", "B"),
    ("w", "#f4f1e8", "W"),
    ("a", "#ffb02e", "A"),
    ("uv", "#a066ff", "UV"),
];

/// Колірні канали моделі в порядку r g b w a uv.
pub fn colors(fx: &LibModel) -> Vec<(&'static str, &'static str, &'static str)> {
    LED.iter().copied().filter(|(k, _, _)| fx.modes.iter().any(|m| m.channels().iter().any(|c| c == k))).collect()
}

fn segments(fx: &LibModel) -> usize {
    fx.modes
        .iter()
        .map(|m| ["r", "g", "b", "w"].iter().map(|c| m.channels().iter().filter(|x| x == c).count()).max().unwrap_or(0))
        .max()
        .unwrap_or(1)
        .max(1)
}

pub fn svg(fx: &LibModel) -> String {
    let cols: Vec<&str> = colors(fx).iter().map(|c| c.1).collect();
    let pal: Vec<&str> = if cols.is_empty() { vec![LED[3].1] } else { cols };
    let body = r##"fill="#1b1f27" stroke="#4a5160" stroke-width="2""##;
    let led = |cx: f64, cy: f64, r: f64, i: usize| {
        format!(r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{r}" fill="{}" opacity=".9"/>"#, pal[i % pal.len()])
    };
    let pi = std::f64::consts::PI;
    let mut g = String::new();
    match fx.t.as_str() {
        "bar" => {
            let n = segments(fx).min(16);
            let (w, x0) = (200.0, 20.0);
            let cw = (w - 8.0) / n as f64;
            g += &format!(r#"<rect x="{x0}" y="34" width="{w}" height="28" rx="6" {body}/>"#);
            g += &format!(
                r##"<path d="M{} 62 l-8 14 h16 z M{} 62 l-8 14 h16 z" fill="#2a2f3a"/>"##,
                x0 + 20.0,
                x0 + w - 20.0
            );
            for i in 0..n {
                g += &format!(
                    r#"<rect x="{:.2}" y="38" width="{:.2}" height="20" rx="3" fill="{}" opacity=".9"/>"#,
                    x0 + 4.0 + i as f64 * cw + 1.5,
                    cw - 3.0,
                    pal[i % pal.len()]
                );
            }
        }
        "head" => {
            g += &format!(r#"<rect x="95" y="70" width="50" height="16" rx="4" {body}/>"#);
            g += r##"<path d="M100 70 V44 M140 70 V44" stroke="#4a5160" stroke-width="6" stroke-linecap="round"/>"##;
            g += &format!(r#"<circle cx="120" cy="40" r="26" {body}/>"#);
            for i in 0..6 {
                let a = i as f64 * pi / 3.0;
                g += &led(120.0 + 13.0 * a.cos(), 40.0 + 13.0 * a.sin(), 5.0, i);
            }
            g += &led(120.0, 40.0, 5.0, 6);
        }
        "strobe" => {
            g += &format!(r#"<rect x="45" y="22" width="150" height="54" rx="8" {body}/>"#);
            let c = if pal.contains(&LED[3].1) { LED[3].1 } else { pal[0] };
            g += &format!(r#"<rect x="57" y="40" width="126" height="18" rx="9" fill="{c}" opacity=".95"/>"#);
        }
        "matrix" => {
            g += &format!(r#"<rect x="70" y="8" width="100" height="80" rx="8" {body}/>"#);
            for y in 0..4 {
                for x in 0..5 {
                    g += &led(84.0 + x as f64 * 18.0, 22.0 + y as f64 * 18.0, 6.0, x + y);
                }
            }
        }
        "effect" => {
            g += &format!(r#"<circle cx="120" cy="48" r="34" {body}/>"#);
            for i in 0..8 {
                let a = i as f64 * pi / 4.0;
                g += &format!(
                    r#"<path d="M120 48 L{:.2} {:.2}" stroke="{}" stroke-width="3" stroke-linecap="round" opacity=".85"/>"#,
                    120.0 + 30.0 * a.cos(),
                    48.0 + 30.0 * a.sin(),
                    pal[i % pal.len()]
                );
            }
        }
        _ => {
            // PAR / заливка / інше: корпус-«банка» з LED-матрицею
            g += r##"<path d="M96 80 l-10 10 h68 l-10 -10" fill="#2a2f3a"/>"##;
            g += &format!(r#"<circle cx="120" cy="46" r="38" {body}/>"#);
            g += r##"<circle cx="120" cy="46" r="30" fill="#0f1217"/>"##;
            g += &led(120.0, 46.0, 6.0, 0);
            for i in 0..6 {
                let a = i as f64 * pi / 3.0;
                g += &led(120.0 + 16.0 * a.cos(), 46.0 + 16.0 * a.sin(), 6.0, i + 1);
            }
        }
    }
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 96" width="480" height="192">{g}</svg>"#)
}

/// «1 режим», «2 режими», «5 режимів».
pub fn modes_text(n: usize) -> String {
    let (d, h) = (n % 10, n % 100);
    let w = if d == 1 && h != 11 {
        "режим"
    } else if (2..=4).contains(&d) && !(12..=14).contains(&h) {
        "режими"
    } else {
        "режимів"
    };
    format!("{n} {w}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn plural() {
        assert_eq!(super::modes_text(1), "1 режим");
        assert_eq!(super::modes_text(3), "3 режими");
        assert_eq!(super::modes_text(11), "11 режимів");
        assert_eq!(super::modes_text(22), "22 режими");
    }
}
