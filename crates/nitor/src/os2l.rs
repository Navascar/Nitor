//! Сервер OS2L для VirtualDJ: VirtualDJ знаходить його в мережі (mDNS, `_os2l._tcp`) і шле удари та кнопки.
//! Удар — у темп «у такт музиці», натиснута кнопка з назвою сцени — виклик сцени.

use std::io::Read;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nitor_core::Command;
use nitor_core::os2l::{Os2l, feed};

use crate::core::Core;

pub struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
    mdns: Option<mdns_sd::ServiceDaemon>,
}

impl Server {
    pub fn start(core: Core, port: u16) -> Result<Server, String> {
        let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| format!("порт {port} зайнятий: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        std::thread::Builder::new()
            .name("os2l".into())
            .spawn(move || accept(&listener, &core, &s))
            .map_err(|e| e.to_string())?;
        let mdns = announce(port).map_err(|e| log::warn!("OS2L: не вдалося оголосити в мережі: {e}")).ok();
        log::info!("OS2L: сервер на порту {port}");
        Ok(Server { port, stop, mdns })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(d) = self.mdns.take() {
            let _ = d.shutdown();
        }
    }
}

fn announce(port: u16) -> Result<mdns_sd::ServiceDaemon, mdns_sd::Error> {
    let d = mdns_sd::ServiceDaemon::new()?;
    let host = format!("nitor-{port}.local.");
    let info = mdns_sd::ServiceInfo::new(
        "_os2l._tcp.local.",
        "Nitor",
        &host,
        "",
        port,
        None::<std::collections::HashMap<String, String>>,
    )?
    .enable_addr_auto();
    d.register(info)?;
    Ok(d)
}

fn accept(listener: &TcpListener, core: &Core, stop: &Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut conn, addr)) => {
                log::info!("OS2L: підключився {addr}");
                let (core, stop) = (core.clone(), stop.clone());
                let _ = std::thread::Builder::new().name("os2l-conn".into()).spawn(move || {
                    // у Windows прийняте з'єднання успадковує неблокувальний режим слухача
                    let _ = conn.set_nonblocking(false);
                    let _ = conn.set_read_timeout(Some(Duration::from_millis(500)));
                    let mut buf = String::new();
                    let mut raw: Vec<u8> = vec![];
                    let mut chunk = [0u8; 4096];
                    while !stop.load(Ordering::Relaxed) {
                        match conn.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                // літера UTF-8 може розірватися між шматками — недочитаний кінець чекає наступного
                                raw.extend_from_slice(&chunk[..n]);
                                let ok = match std::str::from_utf8(&raw) {
                                    Ok(_) => raw.len(),
                                    Err(e) if e.error_len().is_none() => e.valid_up_to(),
                                    Err(_) => raw.len(),
                                };
                                buf.push_str(&String::from_utf8_lossy(&raw[..ok]));
                                raw.drain(..ok);
                                for m in feed(&mut buf) {
                                    handle(&core, m);
                                }
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(_) => break,
                        }
                    }
                    log::info!("OS2L: {addr} від'єднався");
                });
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn handle(core: &Core, m: Os2l) {
    let r = match m {
        Os2l::Beat => core.send(Command::Beat(core.now())),
        // кнопка VirtualDJ з назвою сцени — сцена; інші кнопки пропускаємо
        Os2l::Button(name, true) if core.lock().engine.show.scenes.contains_key(&name) => {
            core.send(Command::RecallScene { name, fade: 0.0 })
        }
        Os2l::Button(..) => Ok(None),
    };
    if let Err(e) = r {
        log::warn!("OS2L: {e}");
    }
}
