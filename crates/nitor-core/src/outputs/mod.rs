//! DMX-виходи: uDMX, Enttec DMX USB Pro, Open DMX (FTDI/RS-485), Art-Net, sACN (E1.31), симулятор.

pub mod config;

pub use config::{OUTPUT_KINDS, OutputConfig, kind_label};
