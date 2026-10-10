//! Generic system timing — transport-agnostic accumulator + RAII scope guard.

//! Usage:
//! ```ignore
//! let timers = SystemTimers::new();
//! let _t = timers.scope("my_system");
//! // ... work ...
//! // _t drops, records elapsed ms

//! // Periodically drain:
//! for (name, p95, count) in timers.drain() { ... }
//! ```

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

struct TimingBuffer {
    observations: Vec<f32>,
}

impl TimingBuffer {
    fn new() -> Self { Self { observations: Vec::with_capacity(64) } }

    fn record(&mut self, ms: f32) { self.observations.push(ms); }

    /// Compute p95 and sample count, then clear. Returns (p95_ms, count).
    fn drain(&mut self) -> (f32, f32) {
        self.observations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p95 = quantile(&self.observations, 0.95).unwrap_or(0.0);
        let count = self.observations.len() as f32;
        self.observations.clear();
        (p95, count)
    }
}

/// The value at quantile `q` (0 to 1) of `sorted`, nearest rank: the
/// element at rank `ceil(q·n)` counted from one, so `q = 1` is the last
/// and anything up to `1/n` the first; none of none.
pub fn quantile<T: Copy>(sorted: &[T], q: f64) -> Option<T> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

/// Transport-agnostic timing accumulator. Thread-safe via interior Mutex.
/// Each side (server, client) wraps this in a Bevy Resource and decides
/// how to report the drained data (UDP to console, local display, etc).
pub struct SystemTimers {
    buffers: Mutex<HashMap<&'static str, TimingBuffer>>,
}

impl SystemTimers {
    pub fn new() -> Self {
        Self { buffers: Mutex::new(HashMap::new()) }
    }

    /// Create a scoped timer. Records elapsed milliseconds on drop.
    pub fn scope(&self, name: &'static str) -> ScopeTimer<'_> {
        ScopeTimer { name, start: Instant::now(), timers: self }
    }

    /// Record an observation directly (ms).
    pub fn record(&self, name: &'static str, ms: f32) {
        self.buffers.lock().unwrap()
            .entry(name)
            .or_insert_with(TimingBuffer::new)
            .record(ms);
    }

    /// Drain all buffers. Returns (name, p95_ms, count) per system.
    /// Clears observations after draining.
    pub fn drain(&self) -> Vec<(&'static str, f32, f32)> {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.iter_mut()
            .map(|(&name, buf)| {
                let (p95, count) = buf.drain();
                (name, p95, count)
            })
            .collect()
    }
}

/// RAII timer guard. Records elapsed milliseconds into [`SystemTimers`] on drop.
pub struct ScopeTimer<'a> {
    name: &'static str,
    start: Instant,
    timers: &'a SystemTimers,
}

impl Drop for ScopeTimer<'_> {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f64() as f32 * 1000.0;
        self.timers.record(self.name, ms);
    }
}

#[cfg(test)]
mod tests {
    use super::quantile;

    fn sorted(v: &[u64]) -> Vec<u64> {
        let mut v = v.to_vec();
        v.sort_unstable();
        v
    }

    #[test]
    fn quantile_is_a_sample_at_its_rank() {
        let v = sorted(&[30, 10, 50, 20, 40]);
        assert_eq!(quantile(&v, 0.5), Some(30));
        assert_eq!(quantile(&v, 1.0), Some(50));
        assert_eq!(quantile(&v, 0.0), Some(10));
    }

    #[test]
    fn quantile_rises_with_q() {
        let v = sorted(&[7, 3, 9, 1, 12, 5, 30, 2]);
        let qs = [0.1, 0.5, 0.9, 0.95, 1.0].map(|q| quantile(&v, q).unwrap());
        assert!(qs.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn quantile_of_nothing_is_none() {
        assert_eq!(quantile::<u64>(&[], 0.95), None);
    }
}
