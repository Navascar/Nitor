//! Удари зі звуку для «у такт музиці»: звук самого комп'ютера (loopback) або мікрофон.
//! Захоплення — WASAPI, лише Windows; на інших системах звук недоступний (лишається тап).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nitor_core::Command;
use nitor_core::tempo::{AudioSource, BeatDetector};

use crate::core::Core;

/// Запущене захоплення звуку; зупиняється при знищенні.
pub struct Audio {
    pub source: AudioSource,
    stop: Arc<AtomicBool>,
    /// Помилка для підказки в інтерфейсі (порожньо — працює).
    pub error: Arc<Mutex<String>>,
}

impl Audio {
    pub fn start(core: Core, source: AudioSource) -> Audio {
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(String::new()));
        let (s, e) = (stop.clone(), error.clone());
        let spawned = std::thread::Builder::new().name("audio".into()).spawn(move || {
            if let Err(err) = run(&core, source, &s) {
                log::warn!("Звук для ударів недоступний: {err}");
                *e.lock().unwrap() = err;
            }
        });
        if spawned.is_err() {
            *error.lock().unwrap() = "не вдалося запустити потік звуку".into();
        }
        log::info!("Звук для ударів: {source:?}");
        Audio { source, stop, error }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Удари з відліків: зсув від кінця блоку → момент на годиннику рушія.
fn feed(core: &Core, det: &mut BeatDetector, mono: &[f32]) {
    let now = core.now();
    for off in det.feed(mono) {
        let _ = core.send(Command::Beat(now + off));
    }
}

#[cfg(windows)]
fn run(core: &Core, source: AudioSource, stop: &AtomicBool) -> Result<(), String> {
    use std::collections::VecDeque;
    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta};

    let e = |err: wasapi::WasapiError| err.to_string();
    let _ = initialize_mta();
    // звук комп'ютера — захоплення з пристрою відтворення (loopback)
    let dir = if source == AudioSource::Mic { Direction::Capture } else { Direction::Render };
    let device = DeviceEnumerator::new().map_err(e)?.get_default_device(dir).map_err(e)?;
    let mut client = device.get_iaudioclient().map_err(e)?;
    let rate = 48_000;
    let fmt = WaveFormat::new(32, 32, SampleType::Float, rate, 2, None);
    let (period, _) = client.get_device_period().map_err(e)?;
    // опитування замість подій: у loopback події не приходять, поки звук не грає
    let mode = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: period * 10 };
    client.initialize_client(&fmt, Direction::Capture, mode, None).map_err(e)?;
    let capture = client.get_audiocaptureclient().map_err(e)?;
    client.start_stream().map_err(e)?;
    let mut det = BeatDetector::new(rate as u32);
    let mut bytes: VecDeque<u8> = VecDeque::new();
    let mut mono = vec![];
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        capture.read_from_device_to_deque(&mut bytes).map_err(e)?;
        let (a, b) = bytes.as_slices();
        let all: Vec<u8> = a.iter().chain(b).copied().collect();
        let frames = all.len() / 8;
        mono.clear();
        mono.extend(all[..frames * 8].chunks_exact(8).map(|f| {
            let l = f32::from_le_bytes([f[0], f[1], f[2], f[3]]);
            let r = f32::from_le_bytes([f[4], f[5], f[6], f[7]]);
            (l + r) / 2.0
        }));
        bytes.drain(..frames * 8);
        if !mono.is_empty() {
            feed(core, &mut det, &mono);
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

#[cfg(not(windows))]
fn run(core: &Core, _source: AudioSource, _stop: &AtomicBool) -> Result<(), String> {
    let _ = feed; // викликається лише у Windows
    let _ = core;
    Err("звук доступний лише у Windows, користуйтеся тапом".into())
}
