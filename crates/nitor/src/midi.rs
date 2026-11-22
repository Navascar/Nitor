//! MIDI-контролери: усі входи (повідомлення — у канал, вікно прокидається) і вихід APC mini mk2
//! для підсвічування падів. Лише Windows; порти відкриваються, тільки коли є прив'язки або навчання.

use std::sync::mpsc::{Receiver, channel};

use nitor_core::midi::MidiValue;

pub struct Midi {
    pub rx: Receiver<(String, MidiValue)>,
    /// Назви підключених входів.
    pub names: Vec<String>,
    #[cfg(windows)]
    _ins: Vec<midir::MidiInputConnection<()>>,
    #[cfg(windows)]
    out: Option<midir::MidiOutputConnection>,
}

impl Midi {
    /// Повідомлення на вихід APC mini mk2 (якщо він є).
    pub fn send(&mut self, msg: &[u8]) {
        #[cfg(windows)]
        if let Some(out) = self.out.as_mut() {
            let _ = out.send(msg);
        }
        #[cfg(not(windows))]
        let _ = msg;
    }

    pub fn has_apc(&self) -> bool {
        #[cfg(windows)]
        return self.out.is_some();
        #[cfg(not(windows))]
        false
    }
}

// shortcut: порти відкриваються один раз; контролер, підключений пізніше, — кнопкою «Перепідключити»
#[cfg(windows)]
pub fn open(ctx: egui::Context) -> Result<Midi, String> {
    let (tx, rx) = channel();
    let mut ins = vec![];
    let mut names = vec![];
    let probe = midir::MidiInput::new("Nitor").map_err(|e| e.to_string())?;
    for port in probe.ports() {
        let name = probe.port_name(&port).unwrap_or_default();
        let Ok(input) = midir::MidiInput::new("Nitor") else { continue };
        let (tx, ctx) = (tx.clone(), ctx.clone());
        let conn = input.connect(
            &port,
            "nitor-in",
            move |_, msg, _| {
                if let Some(m) = nitor_core::midi::parse(msg) {
                    let _ = tx.send(m);
                    ctx.request_repaint();
                }
            },
            (),
        );
        match conn {
            Ok(c) => {
                log::info!("MIDI-вхід: {name}");
                ins.push(c);
                names.push(name);
            }
            Err(e) => log::warn!("MIDI-вхід {name} недоступний: {e}"),
        }
    }
    let out = midir::MidiOutput::new("Nitor").ok().and_then(|o| {
        let port = o.ports().into_iter().find(|p| {
            o.port_name(p).is_ok_and(|n| n.to_lowercase().contains(&nitor_core::midi::APC_PORT.to_lowercase()))
        })?;
        o.connect(&port, "nitor-out").ok()
    });
    Ok(Midi { rx, names, _ins: ins, out })
}

#[cfg(not(windows))]
pub fn open(_ctx: egui::Context) -> Result<Midi, String> {
    let (_tx, rx) = channel();
    let _ = Midi { rx, names: vec![] };
    Err("MIDI-контролери працюють лише у Windows".into())
}
