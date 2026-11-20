//! Потік відправки: тримає останній кадр і безперервно (або за зміною + keepalive) шле його
//! у драйвер. Порожній кадр шле ще трохи, щоб прилади згасли, а далі відправку призупиняє.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{DMX_CHANNELS, Result};

pub type Frame = [u8; DMX_CHANNELS];

/// Драйвер одного типу виходу.
pub trait Driver: Send {
    /// Відкрити пристрій або сокет. Помилка — текст для індикатора виходу.
    fn open(&mut self) -> Result<()>;
    fn send(&mut self, frame: &Frame) -> Result<()>;
    fn close(&mut self) {}
    /// Текст стану, коли вихід працює.
    fn status(&self) -> String {
        "Працює".into()
    }
}

/// Частота й режим відправки.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub refresh_hz: f64,
    /// `None` — слати кожен такт; інакше — за зміною кадру, але не рідше ніж раз на цей час.
    pub keepalive: Option<Duration>,
    /// Скільки ще слати порожній кадр, перш ніж призупинити відправку.
    pub idle_tail: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { refresh_hz: 40.0, keepalive: None, idle_tail: Duration::from_secs(1) }
    }
}

/// Стан виходу для індикатора у верхній панелі.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    pub kind: &'static str,
    pub ok: bool,
    pub status: String,
    pub frames: u64,
    /// Відправку призупинено: кадр порожній.
    pub paused: bool,
}

struct Shared {
    frame: Mutex<(Frame, bool)>, // кадр, змінився
    changed: Condvar,
    stop: AtomicBool,
    info: Mutex<OutputInfo>,
}

impl Shared {
    /// Чекає на зміну кадру або зупинку не довше за `timeout`.
    fn wait_changed(&self, timeout: Duration) {
        let guard = self.frame.lock().unwrap();
        let _ = self
            .changed
            .wait_timeout_while(guard, timeout, |(_, changed)| !*changed && !self.stop.load(Ordering::Relaxed));
    }

    /// Чекає лише на зупинку.
    fn wait_stop(&self, timeout: Duration) {
        let guard = self.frame.lock().unwrap();
        let _ = self.changed.wait_timeout_while(guard, timeout, |_| !self.stop.load(Ordering::Relaxed));
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// Запущений вихід. Зупиняється в `stop()` або при знищенні.
pub struct Output {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Output {
    pub fn start(kind: &'static str, driver: Box<dyn Driver>, timing: Timing) -> Output {
        let shared = Arc::new(Shared {
            frame: Mutex::new(([0; DMX_CHANNELS], false)),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            info: Mutex::new(OutputInfo {
                kind, ok: false, status: "Зупинено".into(), frames: 0, paused: false
            }),
        });
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name(format!("dmx-{kind}"))
            .spawn(move || run(&s, driver, timing))
            .expect("потік виходу");
        Output { shared, thread: Some(thread) }
    }

    pub fn set_frame(&self, frame: &Frame) {
        let mut f = self.shared.frame.lock().unwrap();
        if f.0 != *frame {
            f.0 = *frame;
            f.1 = true;
            self.shared.changed.notify_all();
        }
    }

    pub fn info(&self) -> OutputInfo {
        self.shared.info.lock().unwrap().clone()
    }

    /// Зупиняє потік і закриває пристрій.
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.changed.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let mut info = self.shared.info.lock().unwrap();
        info.ok = false;
        info.status = "Зупинено".into();
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(s: &Shared, mut driver: Box<dyn Driver>, timing: Timing) {
    let period = Duration::from_secs_f64(1.0 / timing.refresh_hz);
    let mut last_send = Instant::now() - period;
    let mut idle_since: Option<Instant> = None;
    let mut sent: Option<Frame> = None;
    let mut ok = false;
    while !s.stopped() {
        if !ok {
            match driver.open() {
                Ok(()) => {
                    ok = true;
                    let mut info = s.info.lock().unwrap();
                    info.ok = true;
                    info.status = driver.status();
                    log::info!("Вихід {} відкрито", info.kind);
                }
                Err(e) => {
                    let status = format!("Помилка: {e}");
                    let mut info = s.info.lock().unwrap();
                    if info.status != status {
                        // не засмічуємо журнал однаковими повідомленнями
                        log::warn!("Не вдалося відкрити {}: {e}", info.kind);
                    }
                    info.status = status;
                    drop(info);
                    s.wait_stop(Duration::from_secs(2));
                    continue;
                }
            }
        }
        match timing.keepalive {
            None => s.wait_stop(period),
            Some(keep) => {
                s.wait_changed(keep);
                let left = period.saturating_sub(last_send.elapsed());
                if !left.is_zero() {
                    std::thread::sleep(left);
                }
            }
        }
        if s.stopped() {
            break;
        }
        let frame = {
            let mut f = s.frame.lock().unwrap();
            f.1 = false;
            f.0
        };
        let now = Instant::now();
        if frame.iter().all(|&b| b == 0) && sent == Some(frame) {
            let since = *idle_since.get_or_insert(now);
            if now - since >= timing.idle_tail {
                s.info.lock().unwrap().paused = true;
                s.wait_changed(Duration::from_millis(500)); // нічого не шлемо, поки кадр не зміниться
                continue;
            }
        } else {
            idle_since = None;
        }
        s.info.lock().unwrap().paused = false;
        match driver.send(&frame) {
            Ok(()) => {
                s.info.lock().unwrap().frames += 1;
                sent = Some(frame);
                last_send = Instant::now();
            }
            Err(e) => {
                let mut info = s.info.lock().unwrap();
                log::warn!("Помилка відправки {}: {e}", info.kind);
                info.status = format!("Помилка: {e}");
                info.ok = false;
                ok = false;
                sent = None; // після перепідключення порожній кадр — знову кілька разів
                drop(info);
                driver.close();
            }
        }
    }
    driver.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder(Arc<Mutex<Vec<Frame>>>);

    impl Driver for Recorder {
        fn open(&mut self) -> Result<()> {
            Ok(())
        }
        fn send(&mut self, frame: &Frame) -> Result<()> {
            self.0.lock().unwrap().push(*frame);
            Ok(())
        }
    }

    #[test]
    fn streams_frames_continuously() {
        // кадр має йти безперервно, навіть якщо не змінюється (інакше LED-бари не реагують)
        let sent = Arc::new(Mutex::new(vec![]));
        let mut out =
            Output::start("test", Box::new(Recorder(sent.clone())), Timing { refresh_hz: 30.0, ..Default::default() });
        let mut f = [0u8; DMX_CHANNELS];
        f[..3].copy_from_slice(&[10, 20, 30]);
        out.set_frame(&f);
        std::thread::sleep(Duration::from_millis(500));
        out.stop();
        let same = sent.lock().unwrap().iter().filter(|x| x[..3] == [10, 20, 30]).count();
        assert!(same >= 8, "за 0.5 с надіслано лише {same} кадрів");
    }

    #[test]
    fn pauses_on_empty_frame() {
        let sent = Arc::new(Mutex::new(vec![]));
        let timing = Timing { refresh_hz: 200.0, keepalive: None, idle_tail: Duration::from_millis(100) };
        let mut out = Output::start("test", Box::new(Recorder(sent.clone())), timing);
        std::thread::sleep(Duration::from_millis(400));
        assert!(out.info().paused);
        let n = out.info().frames;
        assert!(n > 0, "порожній кадр усе ж пішов, щоб прилади згасли");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(out.info().frames, n, "далі — тиша");
        let mut f = [0u8; DMX_CHANNELS];
        f[0] = 255;
        out.set_frame(&f);
        std::thread::sleep(Duration::from_millis(100));
        let info = out.info();
        assert!(!info.paused && info.frames > n, "світло ввімкнули — кадри знову йдуть");
        let n = info.frames;
        std::thread::sleep(Duration::from_millis(100));
        assert!(out.info().frames > n);
        out.stop();
        assert_eq!(out.info().status, "Зупинено");
    }

    struct Failing;
    impl Driver for Failing {
        fn open(&mut self) -> Result<()> {
            Err(crate::Error("немає пристрою".into()))
        }
        fn send(&mut self, _: &Frame) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn open_error_is_reported() {
        let out = Output::start("test", Box::new(Failing), Timing::default());
        std::thread::sleep(Duration::from_millis(100));
        let info = out.info();
        assert!(!info.ok);
        assert_eq!(info.status, "Помилка: немає пристрою");
    }
}
