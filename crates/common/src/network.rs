/// Shared network constants for client and server.
///
/// Buffer sizing is derived from flow control parameters so the reliable channel
/// can hold `budget_per_tick * send_rate * timeout * safety_margin` bytes.

pub const PROTOCOL_ID: u64 = 7;

/// Renet default channel IDs. These map to the DefaultChannel enum order:
/// Unreliable=0, ReliableUnordered=1, ReliableOrdered=2.
pub const CH_UNRELIABLE: u8 = 0;
pub const CH_RELIABLE_UNORDERED: u8 = 1;
pub const CH_RELIABLE_ORDERED: u8 = 2;

/// Target bandwidth for ordered channel per client (bytes/sec).
/// Gameplay messages — small, latency-sensitive.
pub const BANDWIDTH_ORDERED: usize = 100_000;

/// Target bandwidth for unordered channel per client (bytes/sec).
/// Chunk data — large, throughput-sensitive.
pub const BANDWIDTH_UNORDERED: usize = 1_000_000;

/// Network drain rate (Hz) — how often queued messages are pushed to renet.
/// Higher = lower latency (max wait = 1/rate). I/O runs every frame regardless.
pub const SEND_RATE: f32 = 60.0;

/// Derived: bytes allowed per send tick (ordered).
pub const BUDGET_ORDERED: usize = (BANDWIDTH_ORDERED as f32 / SEND_RATE) as usize;

/// Derived: bytes allowed per send tick (unordered).
pub const BUDGET_UNORDERED: usize = (BANDWIDTH_UNORDERED as f32 / SEND_RATE) as usize;

/// Connection timeout (seconds).
pub const CONNECTION_TIMEOUT: f32 = 15.0;

/// Safety margin for buffer sizing.
pub const BUFFER_SAFETY_MARGIN: f32 = 2.0;

/// Derived: reliable ordered channel max memory per client.
/// `budget * send_rate * timeout * safety_margin` = 12500 * 8 * 15 * 2 = 3MB
pub const RELIABLE_ORDERED_MAX_MEMORY: usize =
    (BUDGET_ORDERED as f32 * SEND_RATE * CONNECTION_TIMEOUT * BUFFER_SAFETY_MARGIN) as usize;

/// Derived: reliable unordered channel max memory per client.
/// `budget * send_rate * timeout * safety_margin` = 125000 * 8 * 15 * 2 = 30MB
pub const RELIABLE_UNORDERED_MAX_MEMORY: usize =
    (BUDGET_UNORDERED as f32 * SEND_RATE * CONNECTION_TIMEOUT * BUFFER_SAFETY_MARGIN) as usize;

/// Health check interval (seconds). How often to poll buffer occupancy.
pub const HEALTH_CHECK_INTERVAL: f32 = 1.0;

/// Disconnect when available memory falls below this fraction of max.
pub const HEALTH_THRESHOLD: f32 = 0.2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_fit_typical_messages() {
        // Ordered must fit at least one gameplay message (~500B worst case)
        assert!(BUDGET_ORDERED >= 500,
            "ordered budget {}B too small for a gameplay message", BUDGET_ORDERED);
        // Unordered must fit at least one chunk (~7.5KB)
        assert!(BUDGET_UNORDERED >= 8_000,
            "unordered budget {}B too small for a chunk", BUDGET_UNORDERED);
    }

    #[test]
    fn buffers_hold_timeout_worth_of_budget() {
        let ticks = (SEND_RATE * CONNECTION_TIMEOUT) as usize;
        assert!(RELIABLE_ORDERED_MAX_MEMORY >= BUDGET_ORDERED * ticks,
            "ordered buffer {}B < {}s of budget", RELIABLE_ORDERED_MAX_MEMORY, CONNECTION_TIMEOUT);
        assert!(RELIABLE_UNORDERED_MAX_MEMORY >= BUDGET_UNORDERED * ticks,
            "unordered buffer {}B < {}s of budget", RELIABLE_UNORDERED_MAX_MEMORY, CONNECTION_TIMEOUT);
    }
}
