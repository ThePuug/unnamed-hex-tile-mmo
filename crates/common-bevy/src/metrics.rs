use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const METRICS_MAGIC: [u8; 4] = *b"GMSV";
pub const METRICS_VERSION: u16 = 7;

/// Server metrics collection.
///
/// Adding a metric:
/// 1. Add a field here (counter: u64, gauge: u32/f32)
/// 2. Increment/set it in the relevant system
/// 3. Bump METRICS_VERSION
///
/// Counters are cumulative from server start (collector computes rates).
/// Gauges are point-in-time values refreshed at snapshot time.
#[derive(Resource, Default, Clone, Serialize, Deserialize)]
pub struct ServerMetrics {
    // -- Timing --
    pub tick_count: u64,
    pub snapshot_time_secs: f64,

    // -- Critical gauges --
    pub loaded_hexes: u32,
    pub connected_players: u32,
    pub npc_count: u32,
    pub tick_duration_us: u64,
    pub tick_duration_max_us: u64,
    pub tick_overrun_count: u64,
    pub memory_bytes: u64,
    pub memory_map_bytes: u64,

    // -- Frame budget (all schedules) --
    pub frame_duration_us: u64,
    pub frame_duration_max_us: u64,
    pub frame_overrun_count: u64,
}
