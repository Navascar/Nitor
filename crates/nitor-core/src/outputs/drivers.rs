//! Драйвери виходів.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use super::config::OutputConfig;
use super::packets::{artnet_packet, enttec_pro_packet, sacn_multicast_address, sacn_packet};
use super::runner::{Driver, Frame, Output, Timing};
use crate::{DMX_CHANNELS, Result, bail, err};

/// Запускає вихід за налаштуваннями.
pub fn start(cfg: &OutputConfig) -> Output {
    let keep = Some(Duration::from_secs(1));
    let (driver, timing): (Box<dyn Driver>, Timing) = match cfg.clone() {
        OutputConfig::None => (Box::new(NullOutput), Timing::default()),
        OutputConfig::EnttecPro { port } => (Box::new(EnttecPro { port, ser: None }), Timing::default()),
        OutputConfig::OpenDmx { port } => {
            (Box::new(OpenDmx { port, ser: None }), Timing { refresh_hz: 30.0, ..Default::default() })
        }
        OutputConfig::Artnet { host, universe, port } => (
            Box::new(ArtNet { host, universe, port, sock: None, seq: 0 }),
            Timing { keepalive: keep, ..Default::default() },
        ),
        OutputConfig::Sacn { host, universe } => (
            Box::new(Sacn { host, universe, sock: None, seq: 0, cid: *uuid::Uuid::new_v4().as_bytes() }),
            Timing { keepalive: keep, ..Default::default() },
        ),
        // Шлемо кадр безперервно (~30/с), як світлові пульти. Багато клонів uDMX передають DMX
        // лише після запису по USB, а дешеві прилади (зокрема LED-бари) ігнорують одиночні кадри.
        OutputConfig::Udmx { index } => (
            Box::new(Udmx { index, dev: None, name: String::new() }),
            Timing { refresh_hz: 30.0, ..Default::default() },
        ),
    };
    Output::start(cfg.kind(), driver, timing)
}

/// Симулятор: нічого нікуди не шле, корисно для налаштування без обладнання.
pub struct NullOutput;

impl Driver for NullOutput {
    fn open(&mut self) -> Result<()> {
        Ok(())
    }
    fn send(&mut self, _: &Frame) -> Result<()> {
        Ok(())
    }
    fn status(&self) -> String {
        "Симуляція".into()
    }
}

// ------------------------------------------------------------- послідовні --
fn open_serial(port: &str, baud: u32, stop_bits: serialport::StopBits) -> Result<Box<dyn serialport::SerialPort>> {
    if port.trim().is_empty() {
        bail!("не вказано послідовний порт");
    }
    serialport::new(port.trim(), baud)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(stop_bits)
        .timeout(Duration::from_secs(1))
        .open()
        .map_err(|e| err!("{}: {}", port.trim(), serial_text(&e)))
}

/// Помилка послідовного порту українською.
fn serial_text(e: &serialport::Error) -> String {
    use serialport::ErrorKind as K;
    match e.kind() {
        K::NoDevice => "порт не знайдено або його зайняла інша програма".into(),
        K::InvalidInput => "неправильні параметри порту".into(),
        K::Io(std::io::ErrorKind::PermissionDenied) => "порт зайнятий іншою програмою".into(),
        K::Io(kind) => crate::error::io_text(&std::io::Error::from(kind)),
        K::Unknown => format!("невідома помилка порту ({})", e.description),
    }
}

/// Помилка USB українською.
fn usb_text(e: rusb::Error) -> &'static str {
    use rusb::Error as E;
    match e {
        E::Access => "немає доступу до пристрою (потрібен драйвер WinUSB)",
        E::NoDevice => "пристрій від'єднано",
        E::NotFound => "пристрій не знайдено",
        E::Busy => "пристрій зайнятий іншою програмою",
        E::Timeout => "пристрій не відповідає",
        E::Pipe => "пристрій відхилив запит (можливо, встановлено не той драйвер)",
        E::Io => "помилка обміну даними з пристроєм",
        E::NotSupported => "драйвер пристрою не підтримує цю операцію (потрібен WinUSB)",
        E::Overflow => "пристрій надіслав забагато даних",
        E::InvalidParam => "неправильний параметр запиту",
        E::Interrupted => "операцію перервано",
        E::NoMem => "недостатньо пам'яті",
        E::BadDescriptor => "пристрій повідомив пошкоджений опис",
        E::Other => "невідома помилка USB",
    }
}

pub struct EnttecPro {
    port: String,
    ser: Option<Box<dyn serialport::SerialPort>>,
}

impl Driver for EnttecPro {
    fn open(&mut self) -> Result<()> {
        self.ser = Some(open_serial(&self.port, 57600, serialport::StopBits::One)?);
        Ok(())
    }
    fn send(&mut self, frame: &Frame) -> Result<()> {
        let ser = self.ser.as_mut().ok_or_else(|| err!("порт закрито"))?;
        ser.write_all(&enttec_pro_packet(frame))?;
        Ok(())
    }
    fn close(&mut self) {
        self.ser = None;
    }
}

/// Enttec Open DMX / будь-який USB-RS485 адаптер (FTDI). Break формується програмно.
pub struct OpenDmx {
    port: String,
    ser: Option<Box<dyn serialport::SerialPort>>,
}

impl Driver for OpenDmx {
    fn open(&mut self) -> Result<()> {
        self.ser = Some(open_serial(&self.port, 250_000, serialport::StopBits::Two)?);
        Ok(())
    }
    fn send(&mut self, frame: &Frame) -> Result<()> {
        let ser = self.ser.as_mut().ok_or_else(|| err!("порт закрито"))?;
        ser.set_break().map_err(|e| err!("{}", serial_text(&e)))?;
        std::thread::sleep(Duration::from_micros(176)); // break ≥ 88 мкс
        ser.clear_break().map_err(|e| err!("{}", serial_text(&e)))?;
        std::thread::sleep(Duration::from_micros(12)); // MAB ≥ 8 мкс
        let mut data = Vec::with_capacity(DMX_CHANNELS + 1);
        data.push(0);
        data.extend_from_slice(frame);
        ser.write_all(&data)?;
        ser.flush()?;
        Ok(())
    }
    fn close(&mut self) {
        self.ser = None;
    }
}

/// Послідовний порт для списку «Послідовний порт».
#[derive(Debug, Clone, PartialEq)]
pub struct SerialPortInfo {
    pub device: String,
    pub description: String,
}

pub fn list_serial_ports() -> Vec<SerialPortInfo> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let description = match p.port_type {
                serialport::SerialPortType::UsbPort(u) => {
                    [u.manufacturer, u.product].into_iter().flatten().collect::<Vec<_>>().join(" ")
                }
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                _ => String::new(),
            };
            SerialPortInfo { device: p.port_name, description }
        })
        .collect()
}

// ------------------------------------------------------------------ мережа --
fn resolve(host: &str, port: u16) -> Result<SocketAddr> {
    (host, port).to_socket_addrs()?.find(SocketAddr::is_ipv4).ok_or_else(|| err!("не вдалося знайти вузол {host}"))
}

pub struct ArtNet {
    host: String,
    universe: u32,
    port: u16,
    sock: Option<(UdpSocket, SocketAddr)>,
    seq: u8,
}

impl Driver for ArtNet {
    fn open(&mut self) -> Result<()> {
        // Port-Address Art-Net — 15 біт; більше значення пішло б у чужий всесвіт
        if self.universe > 32767 {
            bail!("всесвіт Art-Net має бути 0..32767");
        }
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_broadcast(true)?;
        let host = if self.host.trim().is_empty() { "255.255.255.255" } else { self.host.trim() };
        self.sock = Some((sock, resolve(host, self.port)?));
        self.seq = 0;
        Ok(())
    }
    fn send(&mut self, frame: &Frame) -> Result<()> {
        let (sock, addr) = self.sock.as_ref().ok_or_else(|| err!("сокет закрито"))?;
        self.seq = self.seq % 255 + 1; // 0 вимикає перевірку послідовності, тому 1..255
        sock.send_to(&artnet_packet(frame, self.universe as u16, self.seq), addr)?;
        Ok(())
    }
    fn close(&mut self) {
        self.sock = None;
    }
}

pub struct Sacn {
    host: String,
    universe: u32,
    sock: Option<(UdpSocket, SocketAddr)>,
    seq: u8,
    cid: [u8; 16],
}

impl Driver for Sacn {
    fn open(&mut self) -> Result<()> {
        if !(1..=63999).contains(&self.universe) {
            bail!("всесвіт sACN має бути 1..63999");
        }
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_multicast_ttl_v4(8)?;
        let addr = if self.host.trim().is_empty() {
            SocketAddr::from((sacn_multicast_address(self.universe as u16), 5568))
        } else {
            resolve(self.host.trim(), 5568)?
        };
        self.sock = Some((sock, addr));
        Ok(())
    }
    fn send(&mut self, frame: &Frame) -> Result<()> {
        let (sock, addr) = self.sock.as_ref().ok_or_else(|| err!("сокет закрито"))?;
        self.seq = self.seq.wrapping_add(1);
        sock.send_to(&sacn_packet(frame, self.universe as u16, self.seq, &self.cid, "Nitor", 100), addr)?;
        Ok(())
    }
    fn close(&mut self) {
        self.sock = None;
    }
}

// -------------------------------------------------------------------- uDMX --
// Anyma uDMX та клони: USB-пристрій без COM-порту, керується vendor control transfer-ами
// через libusb (як в офіційній утиліті uDMX).
pub const UDMX_VID: u16 = 0x16C0;
pub const UDMX_PID: u16 = 0x05DC;
/// bRequest: wValue = к-сть каналів, wIndex = перший канал (з 0).
const UDMX_SET_CHANNEL_RANGE: u8 = 2;
/// vendor | device | host-to-device
const UDMX_REQUEST_TYPE: u8 = 0x40;
pub const USB_BACKEND: &str = "libusb-1.0 (вбудований)";

/// Знайдений uDMX.
#[derive(Debug, Clone, PartialEq)]
pub struct UdmxDevice {
    pub bus: u8,
    pub address: u8,
    pub product: String,
    pub serial: String,
    /// Пристрій відкривається (драйвер підходить). `false` у Windows — потрібен WinUSB.
    pub driver_ok: bool,
}

/// Контекст libusb (один на програму). Без доступу до USB — зрозуміла помилка, а не збій.
fn usb() -> Result<rusb::Context> {
    use rusb::UsbContext as _;
    static CTX: std::sync::OnceLock<std::result::Result<rusb::Context, rusb::Error>> = std::sync::OnceLock::new();
    let ctx = CTX.get_or_init(rusb::Context::new).clone();
    let ctx = ctx.map_err(|_| err!("не знайдено бібліотеку libusb"))?;
    let _ = ctx.devices(); // перевірка, що контекст працює
    Ok(ctx)
}

fn udmx_list() -> Result<Vec<(rusb::Device<rusb::Context>, UdmxDevice)>> {
    use rusb::UsbContext as _;
    let devices = usb()?.devices().map_err(|e| err!("не вдалося отримати список USB-пристроїв: {}", usb_text(e)))?;
    let mut out = vec![];
    for dev in devices.iter() {
        let Ok(desc) = dev.device_descriptor() else { continue };
        if desc.vendor_id() != UDMX_VID || desc.product_id() != UDMX_PID {
            continue;
        }
        // без прав/драйвера рядки не прочитати
        let (product, serial, driver_ok) = match dev.open() {
            Ok(h) => (
                h.read_product_string_ascii(&desc).unwrap_or_default(),
                h.read_serial_number_string_ascii(&desc).unwrap_or_default(),
                true,
            ),
            Err(_) => (String::new(), String::new(), false),
        };
        // VID/PID 16C0:05DC спільний для багатьох саморобних пристроїв V-USB
        if !product.is_empty() && !product.to_lowercase().contains("udmx") {
            continue;
        }
        let info = UdmxDevice {
            bus: dev.bus_number(),
            address: dev.address(),
            product: if product.is_empty() { "uDMX".into() } else { product },
            serial,
            driver_ok,
        };
        out.push((dev, info));
    }
    Ok(out)
}

/// Список знайдених uDMX (порожній, якщо libusb недоступна).
pub fn find_udmx_devices() -> Vec<UdmxDevice> {
    udmx_list().map(|l| l.into_iter().map(|(_, i)| i).collect()).unwrap_or_default()
}

pub struct Udmx {
    index: usize,
    dev: Option<rusb::DeviceHandle<rusb::Context>>,
    name: String,
}

impl Udmx {
    fn write(&self, frame: &Frame) -> Result<()> {
        let dev = self.dev.as_ref().ok_or_else(|| err!("uDMX закрито"))?;
        let written = dev
            .write_control(
                UDMX_REQUEST_TYPE,
                UDMX_SET_CHANNEL_RANGE,
                DMX_CHANNELS as u16,
                0,
                frame,
                Duration::from_millis(500),
            )
            .map_err(|e| err!("{}", usb_text(e)))?;
        if written != DMX_CHANNELS {
            bail!("uDMX прийняв {written} з {DMX_CHANNELS} байт");
        }
        Ok(())
    }
}

impl Driver for Udmx {
    fn open(&mut self) -> Result<()> {
        let devices = udmx_list()?;
        if devices.is_empty() {
            bail!("uDMX не знайдено");
        }
        let (dev, info) = &devices[self.index.min(devices.len() - 1)];
        let handle = dev.open().map_err(|e| err!("не вдалося відкрити uDMX: {}", usb_text(e)))?;
        let _ = handle.set_active_configuration(1); // на деяких драйверах вже налаштовано
        self.dev = Some(handle);
        // пробна відправка: якщо драйвер не той, помилка буде тут, а не в циклі
        self.write(&[0; DMX_CHANNELS])?;
        self.name = format!("{} (USB {}:{}, {USB_BACKEND})", info.product, info.bus, info.address);
        Ok(())
    }
    fn send(&mut self, frame: &Frame) -> Result<()> {
        self.write(frame)
    }
    fn close(&mut self) {
        self.dev = None;
    }
    fn status(&self) -> String {
        format!("Працює: {}", self.name)
    }
}

/// Діагностика uDMX для `nitor --check`: рядки звіту і код виходу.
pub fn check_udmx() -> (Vec<String>, i32) {
    let mut out = vec![];
    let list = match udmx_list() {
        Ok(l) => {
            out.push(format!("Бекенд libusb: {USB_BACKEND}"));
            l
        }
        Err(e) => {
            out.push(format!("Помилка: {e}"));
            return (out, 1);
        }
    };
    if list.is_empty() {
        out.push("uDMX не знайдено.".into());
        out.push("  * перевірте, що адаптер підключено (у Диспетчері пристроїв має бути «uDMX»);".into());
        out.push("  * Windows: встановіть драйвер WinUSB через Zadig (див. README).".into());
        return (out, 1);
    }
    for (i, (_, d)) in list.iter().enumerate() {
        let serial = if d.serial.is_empty() { "-" } else { &d.serial };
        out.push(format!("[{i}] {} serial={serial} USB {}:{}", d.product, d.bus, d.address));
    }
    let mut drv = Udmx { index: 0, dev: None, name: String::new() };
    let code = match drv.open() {
        Ok(()) => {
            out.push("Тестовий кадр відправлено, uDMX працює.".into());
            0
        }
        Err(e) => {
            out.push(format!("Пристрій знайдено, але відправка не вдалася: {e}"));
            out.push("  Найчастіше причина в драйвері. Встановіть WinUSB через Zadig.".into());
            1
        }
    };
    drv.close();
    (out, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artnet_rejects_universe_out_of_range() {
        for universe in [32768, 40000] {
            let mut d = ArtNet { host: "127.0.0.1".into(), universe, port: 6454, sock: None, seq: 0 };
            assert!(d.open().unwrap_err().0.contains("0..32767"));
        }
        let mut d = Sacn { host: String::new(), universe: 0, sock: None, seq: 0, cid: [0; 16] };
        assert!(d.open().unwrap_err().0.contains("1..63999"));
    }

    #[test]
    fn artnet_sends_to_host() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let port = rx.local_addr().unwrap().port();
        let mut d = ArtNet { host: "127.0.0.1".into(), universe: 3, port, sock: None, seq: 0 };
        d.open().unwrap();
        let mut f = [0u8; DMX_CHANNELS];
        f[0] = 42;
        d.send(&f).unwrap();
        let mut buf = [0u8; 1024];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(n, 18 + 512);
        assert_eq!((buf[12], buf[14], buf[18]), (1, 3, 42));
    }

    #[test]
    fn udmx_without_device_reports_clear_error() {
        let mut d = Udmx { index: 0, dev: None, name: String::new() };
        let e = d.open().unwrap_err();
        assert!(e.0.contains("uDMX") || e.0.contains("libusb"), "{e}");
    }

    #[test]
    fn serial_without_port_reports_error() {
        let mut d = EnttecPro { port: String::new(), ser: None };
        assert!(d.open().is_err());
    }
}
