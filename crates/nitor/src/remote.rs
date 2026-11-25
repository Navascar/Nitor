//! Пульт з телефона: невелика вебсторінка в локальній мережі. Вхід — за PIN режиму оператора;
//! на телефоні те саме, що в режимі оператора: сцени, кольори палітри, групи, майстер, «Очистити»,
//! Blackout і «Спалах». Сторінка генерується тут; JavaScript — лише утримання «Спалаху» й повзунок.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nitor_core::engine::{FlashSource, StaticSource};
use nitor_core::{Command, Rgb};

use crate::core::Core;

/// Порт пульта за замовчуванням.
pub const PORT: u16 = 8820;
/// Невдалих спроб PIN до паузи.
const TRIES: u32 = 5;
/// Якщо телефон загубив зв'язок посеред «Спалаху», світло повертається саме.
const FLASH_MAX: Duration = Duration::from_secs(20);

pub struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn start(core: Core, port: u16) -> Result<Server, String> {
        let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => "зайнятий іншою програмою — виберіть інший".to_string(),
            _ => nitor_core::error::io_text(&e),
        })?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        std::thread::Builder::new()
            .name("remote".into())
            .spawn(move || serve(&listener, &core, &s))
            .map_err(|e| e.to_string())?;
        log::info!("Пульт з телефона: порт {port}");
        Ok(Server { port, stop })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Стан пульта: сесії телефонів, захист від підбору PIN, вибрані прилади, «Спалах».
#[derive(Default)]
struct State {
    sessions: Vec<String>,
    fails: u32,
    locked_until: Option<Instant>,
    sel: Vec<String>,
    flash_until: Option<Instant>,
}

fn serve(listener: &TcpListener, core: &Core, stop: &AtomicBool) {
    let mut st = State::default();
    while !stop.load(Ordering::Relaxed) {
        if st.flash_until.is_some_and(|t| Instant::now() > t) {
            st.flash_until = None;
            let _ = core.send(Command::Flash(None));
        }
        match listener.accept() {
            Ok((conn, _)) => {
                if let Err(e) = handle(conn, core, &mut st) {
                    log::debug!("Пульт: {e}");
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    if st.flash_until.is_some() {
        let _ = core.send(Command::Flash(None));
    }
}

/// Запит HTTP: метод, шлях, cookie сесії й тіло форми.
#[derive(Debug, PartialEq)]
struct Req {
    method: String,
    path: String,
    token: Option<String>,
    fetch: bool,
    body: String,
}

/// Розбирає запит; `None` — ще не весь (або зламаний).
fn parse(buf: &[u8]) -> Option<Req> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let (mut len, mut token, mut fetch) = (0usize, None, false);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => len = v.parse().ok()?,
            "x-fetch" => fetch = true,
            "cookie" => {
                token = v.split(';').filter_map(|c| c.trim().strip_prefix("nitor=")).next().map(String::from);
            }
            _ => {}
        }
    }
    let body = buf.get(end + 4..end + 4 + len)?;
    Some(Req { method, path, token, fetch, body: String::from_utf8_lossy(body).into() })
}

/// Значення поля форми `application/x-www-form-urlencoded`.
fn field(body: &str, name: &str) -> Option<String> {
    body.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into()
}

fn handle(mut conn: TcpStream, core: &Core, st: &mut State) -> std::io::Result<()> {
    conn.set_nonblocking(false)?;
    conn.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut buf = vec![];
    let mut chunk = [0u8; 4096];
    let req = loop {
        let n = conn.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(r) = parse(&buf) {
            break r;
        }
        if buf.len() > 16 * 1024 {
            return respond(&mut conn, "413 Payload Too Large", "", "");
        }
    };
    let authed = req.token.as_ref().is_some_and(|t| st.sessions.contains(t));
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") if authed => respond(&mut conn, "200 OK", "", &page(core, st)),
        ("GET", "/") => respond(&mut conn, "200 OK", "", &login(None)),
        ("POST", "/login") => {
            if st.locked_until.is_some_and(|t| Instant::now() < t) {
                return respond(&mut conn, "200 OK", "", &login(Some("Забагато спроб. Зачекайте хвилину.")));
            }
            let pin = field(&req.body, "pin").unwrap_or_default();
            let ok = {
                let s = core.lock();
                s.engine.pin.is_some() && s.engine.pin_ok(&pin)
            };
            if !ok {
                st.fails += 1;
                if st.fails >= TRIES {
                    st.fails = 0;
                    st.locked_until = Some(Instant::now() + Duration::from_secs(60));
                }
                return respond(&mut conn, "200 OK", "", &login(Some("Неправильний PIN.")));
            }
            st.fails = 0;
            let token = uuid::Uuid::new_v4().simple().to_string();
            st.sessions.push(token.clone());
            if st.sessions.len() > 16 {
                st.sessions.remove(0);
            }
            let cookie = format!("Set-Cookie: nitor={token}; HttpOnly; SameSite=Strict; Path=/\r\nLocation: /\r\n");
            respond(&mut conn, "303 See Other", &cookie, "")
        }
        ("POST", "/act") if authed => {
            act(core, st, &req.body);
            if req.fetch {
                respond(&mut conn, "204 No Content", "", "")
            } else {
                respond(&mut conn, "303 See Other", "Location: /\r\n", "")
            }
        }
        ("POST", _) => respond(&mut conn, "303 See Other", "Location: /\r\n", ""),
        _ => respond(&mut conn, "404 Not Found", "", "Не знайдено"),
    }
}

fn respond(conn: &mut TcpStream, status: &str, extra: &str, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\n{extra}Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    conn.write_all(head.as_bytes())?;
    conn.write_all(body.as_bytes())
}

/// Дія з телефона.
fn act(core: &Core, st: &mut State, body: &str) {
    let a = field(body, "a").unwrap_or_default();
    let v = field(body, "v").unwrap_or_default();
    let ids = st.sel.clone();
    let cmd = if let Some(n) = a.strip_prefix("static:").and_then(|n| n.parse().ok()) {
        Some(Command::StaticColor { ids, source: StaticSource::Index(n) })
    } else if let Some(name) = a.strip_prefix("scene:") {
        Some(Command::RecallScene { name: name.into(), fade: 1.0 })
    } else if let Some(name) = a.strip_prefix("group:") {
        let g = core.lock().engine.show.groups.get(name).cloned().unwrap_or_default();
        st.sel = if st.sel == g { vec![] } else { g };
        None
    } else {
        match a.as_str() {
            "all" => {
                st.sel.clear();
                None
            }
            "clear" => Some(Command::Clear { ids }),
            "blackout" => Some(Command::Blackout(None)),
            "master" => v.parse().ok().map(Command::SetMaster),
            "flash" if v == "1" => {
                st.flash_until = Some(Instant::now() + FLASH_MAX);
                Some(Command::Flash(Some(FlashSource::Color { ids, source: crate::app::FLASH_WHITE })))
            }
            "flash" => {
                st.flash_until = None;
                Some(Command::Flash(None))
            }
            _ => None,
        }
    };
    if let Some(c) = cmd {
        if let Err(e) = core.send(c) {
            log::warn!("Пульт: {e}");
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// Кольори сторінки — з токенів теми програми.
fn css() -> String {
    use nitor_ui::theme::*;
    let h = |c: egui::Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
    format!(
        "*{{box-sizing:border-box}}:root{{color-scheme:dark}}\
         body{{margin:0;padding:16px max(16px,env(safe-area-inset-right)) 16px max(16px,env(safe-area-inset-left));\
         background:{bg};color:{text};font:16px system-ui,sans-serif;-webkit-tap-highlight-color:transparent}}h2{{font-size:15px;color:{muted};margin:20px 0 8px;font-weight:600}}\
         form{{margin:0}}.row{{display:flex;flex-wrap:wrap;gap:8px}}\
         button{{font:inherit;min-height:48px;padding:0 16px;border-radius:10px;border:1px solid {line};\
         background:{raised};color:{text};flex:1 1 140px;touch-action:manipulation;overflow-wrap:break-word;hyphens:auto;\
         transition:transform 90ms ease-out}}button:active{{transform:translateY(1px) scale(.985)}}\
         button:focus-visible,input:focus-visible{{outline:2px solid {accent};outline-offset:2px}}\
         button.on{{border-color:{accent};color:{accent}}}\
         button.danger{{border-color:{danger};color:{danger}}}button.danger.on{{background:{danger_fill};\
         border-color:{danger_fill};color:#fff}}#flash{{user-select:none;-webkit-user-select:none;\
         -webkit-touch-callout:none}}label{{display:flex;justify-content:space-between}}\
         @media (prefers-reduced-motion:reduce){{button{{transition:none}}button:active{{transform:none}}}}.colors button{{flex:1 1 30%;font-weight:600;padding:0 6px;font-size:15px}}\
         button.primary{{background:{accent_fill};border-color:{accent_fill};color:#fff;width:100%}}\
         input[type=range]{{width:100%;accent-color:{accent}}}input[type=password]{{font:inherit;width:100%;\
         min-height:48px;border-radius:10px;border:1px solid {line};background:{raised};color:{text};padding:0 12px}}\
         .err{{color:{danger}}}.muted{{color:{muted};font-size:14px}}",
        bg = h(BG),
        text = h(TEXT),
        muted = h(MUTED),
        line = h(LINE_STRONG),
        raised = h(RAISED),
        accent = h(ACCENT),
        danger = h(DANGER),
        danger_fill = h(DANGER_STRONG),
        accent_fill = h(ACCENT_STRONG),
    )
}

fn html(body: &str) -> String {
    format!(
        "<!doctype html><html lang=uk><head><meta charset=utf-8><meta name=viewport \
         content=\"width=device-width,initial-scale=1,viewport-fit=cover\"><meta name=theme-color content=\"{}\">\
         <title>Nitor</title><style>{}</style></head>\
         <body>{body}</body></html>",
        {
            let c = nitor_ui::theme::BG;
            format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
        },
        css()
    )
}

fn login(err: Option<&str>) -> String {
    let err = err.map_or(String::new(), |e| format!("<p class=err>{e}</p>"));
    html(&format!(
        "<h1>Nitor</h1>{err}<form method=post action=/login>\
         <p><label for=pin class=muted>PIN режиму оператора</label></p>\
         <input type=password id=pin name=pin inputmode=numeric autofocus autocomplete=off><p></p>\
         <button class=primary>Увійти</button></form>"
    ))
}

fn page(core: &Core, st: &State) -> String {
    let (master, blackout, colors, scenes, groups, active) = {
        let s = core.lock();
        let sh = &s.engine.show;
        (
            sh.master,
            sh.blackout,
            sh.static_colors.clone(),
            sh.scenes.keys().cloned().collect::<Vec<_>>(),
            sh.groups.clone(),
            s.engine.active_scene.clone(),
        )
    };
    let pct = (master as f32 / 2.55).round();
    let btn = |a: &str, label: &str, class: &str, style: &str| {
        // стан «увімкнено» чує й екранний диктор
        let pressed = if class.split(' ').any(|c| c == "on") { " aria-pressed=true" } else { "" };
        format!(
            "<button name=a value=\"{}\" class=\"{class}\" style=\"{style}\"{pressed}>{}</button>",
            esc(a),
            esc(label)
        )
    };
    let mut b = String::from("<h1>Nitor</h1>");
    b += &format!(
        "<form method=post action=/act><input type=hidden name=a value=master><h2>Майстер {pct}\u{a0}%</h2>\
         <input type=range name=v min=0 max=255 value={master} aria-label=Майстер \
         aria-valuetext=\"{pct} %\" onchange=\"this.form.submit()\"></form>"
    );
    b += "<h2>Світло</h2><form method=post action=/act class=row>";
    b += "<button type=button id=flash>Спалах</button>";
    b += &btn("clear", "Очистити", "", "");
    b += &btn("blackout", "Blackout", if blackout { "danger on" } else { "danger" }, "");
    b += "</form>";
    if !groups.is_empty() {
        b += "<h2>Групи</h2><form method=post action=/act class=row>";
        b += &btn("all", "Усі прилади", if st.sel.is_empty() { "on" } else { "" }, "");
        for (name, ids) in &groups {
            b += &btn(&format!("group:{name}"), name, if !ids.is_empty() && st.sel == *ids { "on" } else { "" }, "");
        }
        b += "</form>";
    }
    b += "<h2>Кольори</h2><form method=post action=/act class=\"row colors\">";
    for (i, c) in colors.iter().enumerate() {
        let fg = crate::app::readable_text(c.hex);
        let style = format!("background:{};color:{}", hex(c.hex), hex(Rgb(fg.r(), fg.g(), fg.b())));
        b += &btn(&format!("static:{i}"), &c.name, "", &style);
    }
    b += "</form>";
    if !scenes.is_empty() {
        b += "<h2>Сцени</h2><form method=post action=/act class=row>";
        for s in &scenes {
            b += &btn(&format!("scene:{s}"), s, if active.as_deref() == Some(s.as_str()) { "on" } else { "" }, "");
        }
        b += "</form>";
    }
    // «Спалах» світить, поки кнопку тримають
    b += "<script>const f=document.getElementById('flash');const s=v=>fetch('/act',{method:'POST',\
          headers:{'Content-Type':'application/x-www-form-urlencoded','X-Fetch':'1'},body:'a=flash&v='+v});\
          f.addEventListener('pointerdown',e=>{e.preventDefault();s(1)});\
          ['pointerup','pointercancel','pointerleave'].forEach(n=>f.addEventListener(n,()=>s(0)));</script>";
    html(&b)
}

/// Адреса комп'ютера в локальній мережі (без жодного запиту назовні: UDP лише вибирає маршрут).
pub fn lan_ip() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    s.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests_and_forms() {
        let raw = b"POST /act HTTP/1.1\r\nHost: x\r\nCookie: a=1; nitor=abc\r\nContent-Length: 28\r\nX-Fetch: 1\r\n\r\na=scene%3A%D0%A5%D0%BE%D1%80";
        assert_eq!(parse(&raw[..raw.len() - 5]), None, "тіло ще не дійшло");
        let r = parse(raw).unwrap();
        assert_eq!(
            (r.method.as_str(), r.path.as_str(), r.token.as_deref(), r.fetch),
            ("POST", "/act", Some("abc"), true)
        );
        assert_eq!(field(&r.body, "a").as_deref(), Some("scene:Хор"));
        assert_eq!(
            field("a=group%3A%D0%9B%D1%96%D0%B2%D0%B0+%D1%81%D1%82%D0%BE%D1%80%D0%BE%D0%BD%D0%B0", "a").as_deref(),
            Some("group:Ліва сторона")
        );
        assert_eq!(field("pin=12%", "pin").as_deref(), Some("12%"), "зламане кодування не падає");
        assert_eq!(esc("<b \"x\">&"), "&lt;b &quot;x&quot;&gt;&amp;");
    }
}
