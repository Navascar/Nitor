//! Налаштування DMX-виходу (зберігається в config.json).

use serde::{Deserialize, Serialize};

fn artnet_port() -> u16 {
    6454
}
fn sacn_universe() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputConfig {
    /// uDMX (Anyma та клони) через libusb; `index` — номер знайденого адаптера.
    Udmx {
        #[serde(default)]
        index: usize,
    },
    /// Enttec DMX USB Pro та сумісні.
    EnttecPro {
        #[serde(default)]
        port: String,
    },
    /// Enttec Open DMX / USB-RS485 на FTDI.
    OpenDmx {
        #[serde(default)]
        port: String,
    },
    /// Art-Net; порожній `host` — broadcast.
    Artnet {
        #[serde(default)]
        host: String,
        #[serde(default)]
        universe: u32,
        #[serde(default = "artnet_port")]
        port: u16,
    },
    /// sACN (E1.31); порожній `host` — multicast.
    Sacn {
        #[serde(default)]
        host: String,
        #[serde(default = "sacn_universe")]
        universe: u32,
    },
    /// Симулятор без обладнання.
    None,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig::Udmx { index: 0 }
    }
}

impl OutputConfig {
    /// Ключ типу, як у файлі налаштувань.
    pub fn kind(&self) -> &'static str {
        match self {
            OutputConfig::Udmx { .. } => "udmx",
            OutputConfig::EnttecPro { .. } => "enttec_pro",
            OutputConfig::OpenDmx { .. } => "open_dmx",
            OutputConfig::Artnet { .. } => "artnet",
            OutputConfig::Sacn { .. } => "sacn",
            OutputConfig::None => "none",
        }
    }

    pub fn label(&self) -> &'static str {
        kind_label(self.kind())
    }
}

/// Типи виходів у порядку списку «Інтерфейс».
pub const OUTPUT_KINDS: [&str; 6] = ["udmx", "enttec_pro", "open_dmx", "artnet", "sacn", "none"];

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "udmx" => "uDMX",
        "enttec_pro" => "Enttec DMX USB Pro",
        "open_dmx" => "Open DMX / USB-RS485",
        "artnet" => "Art-Net",
        "sacn" => "sACN / E1.31",
        _ => "Симулятор",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_tags() {
        let c: OutputConfig = serde_json::from_str(r#"{"type":"artnet","universe":3}"#).unwrap();
        assert_eq!(c, OutputConfig::Artnet { host: String::new(), universe: 3, port: 6454 });
        let c: OutputConfig = serde_json::from_str(r#"{"type":"none"}"#).unwrap();
        assert_eq!(c.kind(), "none");
        assert_eq!(serde_json::to_string(&OutputConfig::default()).unwrap(), r#"{"type":"udmx","index":0}"#);
    }
}
