//! DMX-виходи: uDMX, Enttec DMX USB Pro, Open DMX (FTDI/RS-485), Art-Net, sACN (E1.31), симулятор.
//!
//! Кожен вихід працює у власному потоці ([`Output`]) і безперервно (або за зміною + keepalive)
//! відправляє останній кадр із 512 каналів.

pub mod config;
pub mod drivers;
pub mod packets;
pub mod runner;

pub use config::{OUTPUT_KINDS, OutputConfig, kind_label};
pub use drivers::{SerialPortInfo, UdmxDevice, check_udmx, find_udmx_devices, list_serial_ports, start};
pub use runner::{Output, OutputInfo};
