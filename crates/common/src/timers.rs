//! Generic system timing — transport-agnostic accumulator + RAII scope guard.
//!
//! Usage:
//! ```ignore
//! let timers = SystemTimers::new();
//! let _t = timers.scope("my_system");
//! // ... work ...
//! // _t drops, records elapsed ms
//!
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
        if self.observations.is_empty() { return (0.0, 0.0); }
        self.observations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = self.observations.len();
        let p95_idx = ((n as f64 * 0.95).ceil() as usize).saturating_sub(1).min(n - 1);
        let p95 = self.observations[p95_idx];
        let count = n as f32;
        self.observations.clear();
        (p95, count)
    }
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
