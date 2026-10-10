//! Shared network constants for client and server.
//!
//! Buffer sizing is derived from flow control parameters so the reliable channel
//! can hold `budget_per_tick * send_rate * timeout * safety_margin` bytes.

pub const PROTOCOL_ID: u64 = 7;

/// Renet default channel IDs. These map to the DefaultChannel enum order:
/// Unreliable=0, ReliableUnordered=1, ReliableOrdered=2.
pub const CH_UNRELIABLE: u8 = 0;
pub const CH_RELIABLE_UNORDERED: u8 = 1;
pub const CH_RELIABLE_ORDERED: u8 = 2;

/// Target bandwidth for ordered channel per client (bytes/sec).
/// Gameplay messages — small, latency-sensitive.
const BANDWIDTH_ORDERED: usize = 100_000;

/// Target bandwidth for unordered channel per client (bytes/sec).
/// Chunk data — large, throughput-sensitive.
const BANDWIDTH_UNORDERED: usize = 1_000_000;

/// Network drain rate (Hz) — how often queued messages are pushed to renet.
/// Higher = lower latency (max wait = 1/rate). I/O runs every frame regardless.
pub const SEND_RATE: f32 = 60.0;

/// Derived: bytes allowed per send tick (ordered).
pub const BUDGET_ORDERED: usize = (BANDWIDTH_ORDERED as f32 / SEND_RATE) as usize;

/// Derived: bytes allowed per send tick (unordered).
pub const BUDGET_UNORDERED: usize = (BANDWIDTH_UNORDERED as f32 / SEND_RATE) as usize;

/// Seconds of budget the reliable buffers are sized to hold. It mirrors
/// netcode's default connection timeout; nothing passes it to netcode.
const CONNECTION_TIMEOUT: f32 = 15.0;

/// Safety margin for buffer sizing.
const BUFFER_SAFETY_MARGIN: f32 = 2.0;

/// Derived: reliable ordered channel max memory per client.
/// `budget * send_rate * timeout * safety_margin`
pub const RELIABLE_ORDERED_MAX_MEMORY: usize =
    (BUDGET_ORDERED as f32 * SEND_RATE * CONNECTION_TIMEOUT * BUFFER_SAFETY_MARGIN) as usize;

/// Derived: reliable unordered channel max memory per client.
/// `budget * send_rate * timeout * safety_margin`
pub const RELIABLE_UNORDERED_MAX_MEMORY: usize =
    (BUDGET_UNORDERED as f32 * SEND_RATE * CONNECTION_TIMEOUT * BUFFER_SAFETY_MARGIN) as usize;

/// Health check interval (seconds). How often to poll buffer occupancy.
pub const HEALTH_CHECK_INTERVAL: f32 = 1.0;

/// Disconnect when available memory falls below this fraction of max.
pub const HEALTH_THRESHOLD: f32 = 0.2;
