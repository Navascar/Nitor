//! Фоновий потік рушія: 40 кадрів/с рендер → вихід DMX, збереження налаштувань.
//!
//! Працює незалежно від вікна (згорнуте вікно не перемальовується, а світло має йти далі).

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nitor_core::engine::FPS;
use nitor_core::outputs::{self, Output, OutputInfo};
use nitor_core::{Command, DMX_CHANNELS, Engine};

pub struct State {
    pub engine: Engine,
    pub output: Output,
}

#[derive(Clone)]
pub struct Core {
    state: Arc<Mutex<State>>,
    epoch: Instant,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Core {
    pub fn start(engine: Engine) -> Core {
        let epoch = Instant::now();
        let output = outputs::start(&engine.output_cfg);
        let mut engine = engine;
        engine.set_epoch(0.0);
        let core = Core {
            state: Arc::new(Mutex::new(State { engine, output })),
            epoch,
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            thread: Arc::new(Mutex::new(None)),
        };
        let c = core.clone();
        let t = std::thread::Builder::new().name("engine".into()).spawn(move || c.run()).expect("потік рушія");
        *core.thread.lock().unwrap() = Some(t);
        core
    }

    /// Секунди від запуску програми (монотонний годинник рушія).
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Виконує команду; помилка — текст для сповіщення.
    pub fn send(&self, cmd: Command) -> Result<Option<String>, String> {
        let now = self.now();
        let mut s = self.lock();
        s.engine.handle(cmd, now).map_err(|e| e.0)?;
        Ok(s.engine.pop_notice())
    }

    pub fn output_info(&self) -> OutputInfo {
        self.lock().output.info()
    }

    fn run(&self) {
        let period = Duration::from_secs_f64(1.0 / FPS);
        while !self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            let started = Instant::now();
            let now = self.now();
            {
                let mut s = self.lock();
                if s.engine.output_changed {
                    s.engine.output_changed = false;
                    s.output.stop();
                    s.output = outputs::start(&s.engine.output_cfg);
                }
                let frame = s.engine.render(now);
                s.output.set_frame(&frame);
                if s.engine.save_due(now) {
                    if let Err(e) = s.engine.save(now) {
                        log::error!("Не вдалося зберегти налаштування: {e}");
                    }
                }
            }
            std::thread::sleep(period.saturating_sub(started.elapsed()));
        }
    }

    /// Завершення: зберегти налаштування, погасити світло, закрити вихід.
    pub fn shutdown(&self, blackout: bool) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
        let now = self.now();
        let mut s = self.lock();
        if s.engine.is_dirty() {
            if let Err(e) = s.engine.save(now) {
                log::error!("Не вдалося зберегти налаштування: {e}");
            }
        }
        if blackout {
            s.output.set_frame(&[0; DMX_CHANNELS]);
            std::thread::sleep(Duration::from_millis(150));
        }
        s.output.stop();
    }
}
