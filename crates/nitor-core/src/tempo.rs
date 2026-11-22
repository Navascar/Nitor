//! Темп для ефектів «у такт музиці»: удари з тапу чи зі звуку → BPM і момент останнього удару.

use serde::{Deserialize, Serialize};

/// Звідки брати удари для «у такт музиці», крім тапу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource {
    /// Звук, що грає на комп'ютері (без шуму залу й затримки).
    #[default]
    Loopback,
    Mic,
    /// Лише тап клавішею.
    Off,
}

/// Оцінка темпу за моментами ударів (тап клавішею `T`, удари зі звуку, біт OS2L).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tempo {
    /// Останні удари, секунди годинника рушія.
    beats: Vec<f64>,
    /// Ударів на хвилину; `None` — темп ще невідомий.
    pub bpm: Option<f64>,
    /// Момент останнього удару: від нього відлічуються такти.
    pub anchor: f64,
}

/// Пауза, після якої тапи починаються спочатку.
const RESET_S: f64 = 2.5;
const KEEP: usize = 8;

impl Tempo {
    pub fn beat(&mut self, at: f64) {
        if self.beats.last().is_some_and(|&l| at - l > RESET_S || at < l) {
            self.beats.clear();
        }
        self.beats.push(at);
        if self.beats.len() > KEEP {
            self.beats.remove(0);
        }
        self.anchor = at;
        // проміжки зводимо в 60..200 BPM (удар на вісімку чи пропущений удар не ламають темп)
        let mut gaps: Vec<f64> = self
            .beats
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|&g| g > 0.05)
            .map(|mut g| {
                while g < 0.3 {
                    g *= 2.0;
                }
                while g > 1.0 {
                    g /= 2.0;
                }
                g
            })
            .collect();
        if gaps.is_empty() {
            return;
        }
        gaps.sort_by(f64::total_cmp);
        self.bpm = Some((60.0 / gaps[gaps.len() / 2] * 10.0).round() / 10.0);
    }

    /// Скільки ударів минуло від останнього відомого (дробове), якщо темп відомий.
    pub fn beats_since(&self, now: f64) -> Option<f64> {
        self.bpm.map(|bpm| (now - self.anchor) * bpm / 60.0)
    }
}

/// Пошук ударів у звуці: енергія низьких частот (бочка) значно вища за середню за останню секунду.
pub struct BeatDetector {
    rate: f64,
    lp: f64,
    alpha: f64,
    block: Vec<f32>,
    history: Vec<f64>,
    /// Час від початку звуку, с.
    t: f64,
    last: f64,
}

const BLOCK: usize = 1024;
/// Удар — енергія блоку в стільки разів вища за середню.
const SENS: f64 = 1.5;
/// Тиша: удари не шукаємо.
const SILENCE: f64 = 1e-5;
/// Не частіше ніж 240 ударів на хвилину.
const MIN_GAP: f64 = 0.25;

impl BeatDetector {
    pub fn new(sample_rate: u32) -> BeatDetector {
        let rate = sample_rate.max(1) as f64;
        BeatDetector {
            rate,
            lp: 0.0,
            alpha: 1.0 - (-2.0 * std::f64::consts::PI * 150.0 / rate).exp(),
            block: Vec::with_capacity(BLOCK),
            history: vec![],
            t: 0.0,
            last: f64::NEG_INFINITY,
        }
    }

    /// Моно-відліки −1..1. Повертає зсуви ударів (с) від кінця переданих відліків — від’ємні або нуль.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<f64> {
        let mut out = vec![];
        let end = self.t + samples.len() as f64 / self.rate;
        for &x in samples {
            self.lp += self.alpha * (x as f64 - self.lp);
            self.block.push(self.lp as f32);
            self.t += 1.0 / self.rate;
            if self.block.len() < BLOCK {
                continue;
            }
            let e = self.block.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / BLOCK as f64;
            self.block.clear();
            let n = (self.rate / BLOCK as f64).round() as usize; // ~1 с історії
            if self.history.len() >= n / 2 {
                let avg = self.history.iter().sum::<f64>() / self.history.len() as f64;
                if e > SILENCE && e > avg * SENS && self.t - self.last >= MIN_GAP {
                    self.last = self.t;
                    out.push(self.t - end);
                }
            }
            self.history.push(e);
            if self.history.len() > n {
                self.history.remove(0);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_give_bpm() {
        let mut t = Tempo::default();
        assert_eq!(t.bpm, None);
        for i in 0..4 {
            t.beat(10.0 + i as f64 * 0.5);
        }
        assert_eq!(t.bpm, Some(120.0));
        assert_eq!(t.anchor, 11.5);
        assert_eq!(t.beats_since(12.0), Some(1.0));
        // удар на вісімку не подвоює темп
        t.beat(11.75);
        assert_eq!(t.bpm, Some(120.0));
        // довга пауза — тапи спочатку, темп лишається до наступної пари
        t.beat(20.0);
        t.beat(20.6);
        assert_eq!(t.bpm, Some(100.0));
    }

    #[test]
    fn detects_kick_drum() {
        let rate = 48_000;
        let mut d = BeatDetector::new(rate);
        // 4 с: тиха музика й «бочка» (60 Гц, 80 мс) двічі на секунду = 120 BPM
        let mut beats = vec![];
        let chunk = 480;
        let total = rate as usize * 4;
        let mut buf = vec![];
        for i in 0..total {
            let t = i as f64 / rate as f64;
            let ph = t % 0.5;
            let kick = if ph < 0.08 { (2.0 * std::f64::consts::PI * 60.0 * t).sin() * 0.8 } else { 0.0 };
            let noise = (t * 3000.0).sin() * 0.02;
            buf.push((kick + noise) as f32);
            if buf.len() == chunk {
                let end = (i + 1) as f64 / rate as f64;
                beats.extend(d.feed(&buf).into_iter().map(|off| end + off));
                buf.clear();
            }
        }
        let mut tempo = Tempo::default();
        for b in &beats {
            tempo.beat(*b);
        }
        assert!(beats.len() >= 6, "знайдено ударів: {}", beats.len());
        let bpm = tempo.bpm.unwrap();
        assert!((bpm - 120.0).abs() < 4.0, "BPM {bpm}");
        // у тиші ударів немає
        assert!(BeatDetector::new(rate).feed(&vec![0.0; 96_000]).is_empty());
    }
}
