use std::net::UdpSocket;
use std::time::{Duration, SystemTime};

use bevy::prelude::*;
use ::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use common::network::*;

/// The first retry waits this long, each after it twice the one before, up
/// to `BACKOFF_CAP`.
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(30);

// ── Wrapper resource ──

/// Owns the renet client and transport. Game systems access this, never renet
/// directly. It exists only while a connection is being made or held: a failed
/// or dropped connection removes it, and `Link` says when the next one is made.
#[derive(Resource)]
pub struct ClientNet {
    client: RenetClient,
    transport: NetcodeClientTransport,
    send_timer: f32,
}

/// Tells the server the client is leaving. Without it the server learns
/// of a closed window only from netcode's 15 s timeout, and the player
/// stands there for a client that reconnects in the meantime, at the
/// very tile it spawns on.
impl Drop for ClientNet {
    fn drop(&mut self) {
        self.transport.disconnect();
    }
}

impl ClientNet {
    fn new() -> Result<Self, String> {
        let server_addr = "127.0.0.1:5000".parse().unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let client_id = current_time.as_millis() as u64;
        let authentication = ClientAuthentication::Unsecure {
            client_id,
            protocol_id: PROTOCOL_ID,
            server_addr,
            user_data: None,
        };

        let transport = NetcodeClientTransport::new(current_time, authentication, socket).map_err(|e| e.to_string())?;

        let mut config = ConnectionConfig::default();
        config.server_channels_config[CH_RELIABLE_ORDERED as usize].max_memory_usage_bytes = RELIABLE_ORDERED_MAX_MEMORY;
        config.server_channels_config[CH_RELIABLE_UNORDERED as usize].max_memory_usage_bytes = RELIABLE_UNORDERED_MAX_MEMORY;
        let client = RenetClient::new(config);

        Ok(Self { client, transport, send_timer: 0.0 })
    }

    // ── Send ──

    /// Queue a reliable message to the server.
    pub fn send_reliable(&mut self, channel: DefaultChannel, message: Vec<u8>) {
        self.client.send_message(channel, message);
    }

    /// Send an unreliable message. Never budget-gated.
    pub fn send_unreliable(&mut self, message: Vec<u8>) {
        self.client.send_message(DefaultChannel::Unreliable, message);
    }

    // ── Receive ──

    pub fn receive_message(&mut self, channel: DefaultChannel) -> Option<::renet::Bytes> {
        self.client.receive_message(channel)
    }

    // ── Connection state ──

    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    /// The round trip to the server as renet measures it.
    pub fn rtt(&self) -> Duration {
        Duration::from_secs_f64(self.client.rtt())
    }
}

/// Where the client stands with the server.
#[derive(Resource, Clone, Debug, PartialEq)]
pub enum Link {
    /// An attempt is under way; `failures` came before it.
    Connecting { failures: u32 },
    /// Waiting out the back-off after `failures` in a row, until `retry_at`
    /// on the real clock. `reason` is why the last one failed.
    Waiting { failures: u32, retry_at: Duration, reason: String },
    Connected,
}

impl Default for Link {
    /// The first attempt is due at once.
    fn default() -> Self {
        Link::Waiting { failures: 0, retry_at: Duration::ZERO, reason: String::new() }
    }
}

impl Link {
    pub fn is_connected(&self) -> bool {
        matches!(self, Link::Connected)
    }

    /// Makes a waiting link's next attempt due now.
    pub fn retry_now(&mut self) {
        if let Link::Waiting { retry_at, .. } = self {
            *retry_at = Duration::ZERO;
        }
    }

    /// The link after an attempt or a connection failed for `reason` at
    /// `now`: a dropped connection counts as the first failure, so the
    /// client tries again after the shortest wait.
    fn failed(&self, reason: String, now: Duration) -> Link {
        let failures = match self {
            Link::Connecting { failures } | Link::Waiting { failures, .. } => failures + 1,
            Link::Connected => 1,
        };
        Link::Waiting { failures, retry_at: now + backoff(failures), reason }
    }
}

/// The wait after `failures` failed attempts in a row.
fn backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    (BACKOFF_FIRST * 2u32.pow(doublings)).min(BACKOFF_CAP)
}

// ── Plugin ──

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Link>();
        app.add_systems(PreUpdate, (connect, net_receive).chain());
        app.add_systems(PostUpdate, net_send);
    }
}

/// Opens a connection when the back-off is out.
fn connect(mut commands: Commands, mut link: ResMut<Link>, time: Res<Time<Real>>) {
    let Link::Waiting { failures, retry_at, .. } = *link else { return };
    if time.elapsed() < retry_at {
        return;
    }
    match ClientNet::new() {
        Ok(net) => {
            commands.insert_resource(net);
            *link = Link::Connecting { failures };
        }
        Err(reason) => *link = link.failed(reason, time.elapsed()),
    }
}

/// Process incoming packets, and notice a connection made or lost. An
/// unreachable server is an error from the socket, not a panic.
fn net_receive(
    mut commands: Commands,
    net: Option<ResMut<ClientNet>>,
    mut link: ResMut<Link>,
    time: Res<Time>,
    real: Res<Time<Real>>,
) {
    let Some(mut net) = net else { return };
    let ClientNet { ref mut client, ref mut transport, .. } = *net;
    let failure = match transport.update(time.delta(), client) {
        Err(e) => Some(e.to_string()),
        Ok(()) => {
            client.update(time.delta());
            client.disconnect_reason().map(|r| format!("{r:?}"))
        }
    };
    if let Some(reason) = failure {
        info!("Connection to server lost: {reason}");
        commands.remove_resource::<ClientNet>();
        *link = link.failed(reason, real.elapsed());
    } else if client.is_connected() && !link.is_connected() {
        info!("Connected to server");
        *link = Link::Connected;
    }
}

/// Rate-limited flush of outgoing packets.
fn net_send(net: Option<ResMut<ClientNet>>, time: Res<Time>) {
    let Some(mut net) = net else { return };
    let send_interval = 1.0 / SEND_RATE;
    net.send_timer += time.delta_secs();
    if net.send_timer < send_interval {
        return;
    }
    net.send_timer -= send_interval;
    let ClientNet { ref mut client, ref mut transport, .. } = *net;
    // A failed send is noticed by the next receive, which drops the link.
    let _ = transport.send_packets(client);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_its_cap() {
        let waits: Vec<Duration> = (1..=12).map(backoff).collect();
        assert_eq!(waits[0], BACKOFF_FIRST);
        assert!(waits.windows(2).all(|w| w[1] >= w[0]), "never shrinks");
        assert!(waits.windows(2).take(3).all(|w| w[1] == w[0] * 2), "doubles at first");
        assert_eq!(*waits.last().unwrap(), BACKOFF_CAP);
    }

    #[test]
    fn a_dropped_connection_retries_after_the_shortest_wait() {
        let now = Duration::from_secs(100);
        let Link::Waiting { failures, retry_at, .. } = Link::Connected.failed("lost".into(), now) else { panic!() };
        assert_eq!(failures, 1);
        assert_eq!(retry_at, now + BACKOFF_FIRST);
    }

    #[test]
    fn failures_in_a_row_accumulate() {
        let now = Duration::ZERO;
        let first = Link::Connecting { failures: 0 }.failed("a".into(), now);
        let Link::Waiting { failures, .. } = first else { panic!() };
        let second = Link::Connecting { failures }.failed("b".into(), now);
        let Link::Waiting { failures, retry_at, .. } = second else { panic!() };
        assert_eq!(failures, 2);
        assert_eq!(retry_at, BACKOFF_FIRST * 2);
    }

    #[test]
    fn retry_now_makes_the_attempt_due() {
        let mut link = Link::Waiting { failures: 3, retry_at: Duration::from_secs(60), reason: String::new() };
        link.retry_now();
        assert!(matches!(link, Link::Waiting { retry_at: Duration::ZERO, .. }));
    }
}
