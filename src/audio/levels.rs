use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const BARS: usize = 28;
const SMOOTH: f32 = 0.55;

pub struct AudioLevels {
    rms: AtomicU32,
    history: Mutex<VecDeque<f32>>,
    last_pushed: Mutex<f32>,
}

impl AudioLevels {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rms: AtomicU32::new(0),
            history: Mutex::new(VecDeque::from(vec![0.0; BARS])),
            last_pushed: Mutex::new(0.0),
        })
    }

    pub fn ingest(&self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }
        let mut sum_sq = 0f64;
        for s in samples {
            let v = *s as f64 / i16::MAX as f64;
            sum_sq += v * v;
        }
        let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
        let normalized = (rms * 3.2).clamp(0.0, 1.0);

        self.rms
            .store(normalized.to_bits(), Ordering::Relaxed);

        let smoothed = {
            let mut last = self.last_pushed.lock();
            *last = *last * (1.0 - SMOOTH) + normalized * SMOOTH;
            *last
        };

        let mut h = self.history.lock();
        h.pop_front();
        h.push_back(smoothed);
    }

    #[allow(dead_code)]
    pub fn rms(&self) -> f32 {
        f32::from_bits(self.rms.load(Ordering::Relaxed))
    }

    pub fn snapshot(&self) -> Vec<f32> {
        self.history.lock().iter().copied().collect()
    }

    pub fn reset(&self) {
        self.rms.store(0, Ordering::Relaxed);
        *self.last_pushed.lock() = 0.0;
        let mut h = self.history.lock();
        for v in h.iter_mut() {
            *v = 0.0;
        }
    }
}
