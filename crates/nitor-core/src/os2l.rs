//! OS2L (VirtualDJ): JSON-об'єкти, що йдуть потоком TCP один за одним.
//! Беремо удари (`beat`) і кнопки (`btn`), решту пропускаємо.

use serde_json::Value;

/// Порт сервера за замовчуванням.
pub const PORT: u16 = 8818;

#[derive(Debug, Clone, PartialEq)]
pub enum Os2l {
    Beat,
    /// Кнопка VirtualDJ: назва й натиснуто (`true`) чи відпущено.
    Button(String, bool),
}

/// Розбирає накопичений потік; розібране прибирає з `buf`, недочитаний кінець лишає.
pub fn feed(buf: &mut String) -> Vec<Os2l> {
    let mut out = vec![];
    let mut used = 0;
    let mut it = serde_json::Deserializer::from_str(buf).into_iter::<Value>();
    loop {
        match it.next() {
            Some(Ok(v)) => {
                used = it.byte_offset();
                match v.get("evt").and_then(Value::as_str) {
                    Some("beat") => out.push(Os2l::Beat),
                    Some("btn") => {
                        if let Some(name) = v.get("name").and_then(Value::as_str) {
                            let on = v.get("state").and_then(Value::as_str) != Some("off");
                            out.push(Os2l::Button(name.to_string(), on));
                        }
                    }
                    _ => {}
                }
            }
            Some(Err(e)) if e.is_eof() => break,
            // сміття — відкидаємо все прочитане, щоб не застрягнути
            Some(Err(_)) => {
                used = buf.len();
                break;
            }
            None => break,
        }
    }
    buf.drain(..used);
    // shortcut: без кінця об'єкта буфер росте; 64 КБ — явно зіпсований потік
    if buf.len() > 64 * 1024 {
        buf.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stream_in_pieces() {
        let mut buf = String::from(r#"{"evt":"beat","change":false,"pos":1,"bpm":128}{"evt":"btn","name":"Хор","pa"#);
        assert_eq!(feed(&mut buf), vec![Os2l::Beat]);
        buf.push_str(r#"ge":"","state":"on"}{"evt":"cmd","id":1}{"evt":"btn","name":"Хор","state":"off"}"#);
        assert_eq!(feed(&mut buf), vec![Os2l::Button("Хор".into(), true), Os2l::Button("Хор".into(), false)]);
        assert!(buf.trim().is_empty());
        let mut junk = String::from("не json");
        assert!(feed(&mut junk).is_empty());
        assert!(junk.is_empty(), "сміття не накопичується");
    }
}
