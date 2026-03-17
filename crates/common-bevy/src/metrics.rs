use serde::{Deserialize, Serialize};

pub const METRICS_MAGIC: [u8; 4] = *b"GMSV";
pub const METRICS_VERSION: u16 = 9;

/// How a snapshot field combines multiple `record()` calls between flushes.
#[derive(Clone, Copy, Debug)]
pub enum Aggregator {
    /// Most recent value wins. Not reset after flush.
    Last,
    /// Maximum of all recorded values. Reset to 0 after flush.
    Peak,
    /// Sum of all recorded values. Reset to 0 after flush.
    /// Console receives per-snapshot deltas, not cumulative totals.
    Sum,
}

/// Packet cadence — tells the console how to handle the data.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Cadence {
    /// Periodic snapshot (every 2s). Console updates gauge displays.
    Snapshot = 0,
    /// Per-event observation. Console accumulates into p95 windows.
    Event = 1,
}

/// Wire format shared between server and console.
/// Both MetricSnapshot::flush and MetricEvent::record produce these.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsPacket {
    pub group: String,
    pub cadence: Cadence,
    pub timestamp_secs: f64,
    pub fields: Vec<(String, f32)>,
}
