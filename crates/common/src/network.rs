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

/// The UDP port the server listens on and the client connects to: 5000,
/// or `PORT` where set, so a second server can run beside one already up.
pub fn port() -> u16 {
    std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5000)
}

/// Who a connection says it is: the provider vouching for it and that
/// provider's name for the player. A client's own key is the one provider:
/// whoever holds the key is the player.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Identity {
    Key([u8; 16]),
}

impl Identity {
    const KEY: u8 = 1;

    /// The identity as netcode's connect user data carries it: the
    /// provider's byte, then its name.
    pub fn user_data(self) -> [u8; 256] {
        let mut data = [0; 256];
        match self {
            Identity::Key(key) => {
                data[0] = Self::KEY;
                data[1..17].copy_from_slice(&key);
            }
        }
        data
    }

    /// The identity `data` carries, or none where it names no provider.
    pub fn from_user_data(data: &[u8; 256]) -> Option<Self> {
        match data[0] {
            Self::KEY => Some(Identity::Key(data[1..17].try_into().expect("16 bytes"))),
            _ => None,
        }
    }

    /// The provider, as the store names it.
    pub fn provider(self) -> &'static str {
        match self {
            Identity::Key(_) => "key",
        }
    }

    /// The provider's name for the player.
    pub fn name(self) -> String {
        match self {
            Identity::Key(key) => key.iter().map(|byte| format!("{byte:02x}")).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_crosses_the_connect_data_whole() {
        let identity = Identity::Key(*b"0123456789abcdef");
        assert_eq!(Identity::from_user_data(&identity.user_data()), Some(identity));
        assert_eq!(Identity::from_user_data(&[0; 256]), None);
        assert_eq!(identity.name().len(), 32);
    }
}
