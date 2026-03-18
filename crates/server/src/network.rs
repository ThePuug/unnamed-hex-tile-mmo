use std::collections::HashMap;
use std::net::UdpSocket;
use std::time::SystemTime;

use bevy::prelude::*;
use ::renet::{ClientId, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use common::network::*;

// ── Connection events ──

/// Triggered when a client connects or disconnects.
#[derive(Event, Debug)]
pub enum NetServerEvent {
    ClientConnected { client_id: ClientId },
    ClientDisconnected { client_id: ClientId, reason: ::renet::DisconnectReason },
}

// ── Per-client flow control state ──

#[derive(Default)]
pub(crate) struct ChannelState {
    pub(crate) queue: Vec<Vec<u8>>,
    budget: usize,
}

#[derive(Default)]
pub(crate) struct ClientState {
    pub(crate) ordered: ChannelState,
    pub(crate) unordered: ChannelState,
}

// ── Wrapper resource ──

/// Owns the renet server and transport. Game systems access this, never renet directly.
#[derive(Resource)]
pub struct ServerNet {
    server: RenetServer,
    transport: NetcodeServerTransport,
    clients: HashMap<ClientId, ClientState>,
    /// Accumulator for rate-limiting send_packets calls to SEND_RATE Hz.
    send_timer: f32,
    /// Accumulator for periodic health checks.
    health_timer: f32,
}

impl ServerNet {
    fn new() -> Self {
        let public_addr = "0.0.0.0:5000".parse().unwrap();
        let socket = UdpSocket::bind(public_addr).unwrap();
        let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let server_config = ServerConfig {
            current_time,
            max_clients: 64,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![public_addr],
            authentication: ServerAuthentication::Unsecure,
        };

        let transport = NetcodeServerTransport::new(server_config, socket).unwrap();

        let mut config = ConnectionConfig::default();
        config.server_channels_config[CH_RELIABLE_ORDERED as usize].max_memory_usage_bytes = RELIABLE_ORDERED_MAX_MEMORY;
        config.server_channels_config[CH_RELIABLE_UNORDERED as usize].max_memory_usage_bytes = RELIABLE_UNORDERED_MAX_MEMORY;
        let server = RenetServer::new(config);

        Self {
            server,
            transport,
            clients: HashMap::new(),
            send_timer: 0.0,
            health_timer: 0.0,
        }
    }

    // ── Send ──

    /// Queue a reliable message for a client. Deferred until the next send tick
    /// and subject to per-client per-tick byte budget.
    pub fn send_reliable(&mut self, client_id: ClientId, channel: DefaultChannel, message: Vec<u8>) {
        let state = self.clients.entry(client_id).or_default();
        match channel {
            DefaultChannel::ReliableOrdered => state.ordered.queue.push(message),
            DefaultChannel::ReliableUnordered => state.unordered.queue.push(message),
            DefaultChannel::Unreliable => unreachable!("use send_unreliable"),
        }
    }

    /// Send an unreliable message immediately. Never budget-gated (no ACK accumulation).
    pub fn send_unreliable(&mut self, client_id: ClientId, message: Vec<u8>) {
        self.server.send_message(client_id, DefaultChannel::Unreliable, message);
    }

    // ── Receive ──

    pub fn receive_message(&mut self, client_id: ClientId, channel: DefaultChannel) -> Option<::renet::Bytes> {
        self.server.receive_message(client_id, channel)
    }

    // ── Connection state ──

    pub fn clients_id(&self) -> Vec<ClientId> {
        self.server.clients_id()
    }

    // ── Metrics ──

    pub fn network_info(&self, client_id: ClientId) -> Result<::renet::NetworkInfo, ()> {
        self.server.network_info(client_id).map_err(|_| ())
    }

    /// Peak buffer occupancy for a specific channel across all clients (0.0–1.0).
    pub fn peak_buffer_occupancy(&self, channel: u8) -> f32 {
        let max_mem = match channel {
            CH_RELIABLE_ORDERED => RELIABLE_ORDERED_MAX_MEMORY,
            CH_RELIABLE_UNORDERED => RELIABLE_UNORDERED_MAX_MEMORY,
            _ => return 0.0,
        };
        let mut peak = 0.0f32;
        for &client_id in self.clients.keys() {
            let available = self.server.channel_available_memory(client_id, channel);
            let used = max_mem.saturating_sub(available);
            let occupancy = used as f32 / max_mem as f32;
            peak = peak.max(occupancy);
        }
        peak
    }

    /// P95 outbound queue depth in bytes for a specific channel across all clients.
    pub(crate) fn p95_queue_depth(&self, get_queue: impl Fn(&ClientState) -> &Vec<Vec<u8>>) -> usize {
        if self.clients.is_empty() { return 0; }
        let mut depths: Vec<usize> = self.clients.values()
            .map(|s| get_queue(s).iter().map(|m| m.len()).sum())
            .collect();
        depths.sort_unstable();
        let idx = ((depths.len() as f32 * 0.95) as usize).min(depths.len() - 1);
        depths[idx]
    }

    // ── Internal ──

    /// Drain each channel independently with its own budget.
    fn drain_queues(&mut self) {
        for (&client_id, state) in &mut self.clients {
            Self::drain_channel(&mut self.server, client_id, CH_RELIABLE_ORDERED, &mut state.ordered, BUDGET_ORDERED);
            Self::drain_channel(&mut self.server, client_id, CH_RELIABLE_UNORDERED, &mut state.unordered, BUDGET_UNORDERED);
        }
    }

    fn drain_channel(server: &mut RenetServer, client_id: ClientId, channel_id: u8, ch: &mut ChannelState, budget: usize) {
        ch.budget = budget;
        let mut first = true;
        while !ch.queue.is_empty() {
            let msg_size = ch.queue[0].len();
            if msg_size > ch.budget && !first {
                break;
            }
            first = false;
            ch.budget = ch.budget.saturating_sub(msg_size);
            let message = ch.queue.remove(0);
            server.send_message(client_id, channel_id, message);
        }
    }

    /// Check buffer health for all clients. Returns IDs of clients to disconnect.
    fn check_health(&self) -> Vec<ClientId> {
        let mut stale = Vec::new();
        for &client_id in self.clients.keys() {
            for (ch, max_mem) in [
                (CH_RELIABLE_ORDERED, RELIABLE_ORDERED_MAX_MEMORY),
                (CH_RELIABLE_UNORDERED, RELIABLE_UNORDERED_MAX_MEMORY),
            ] {
                let threshold_bytes = (max_mem as f32 * HEALTH_THRESHOLD) as usize;
                let available = self.server.channel_available_memory(client_id, ch);
                if available < threshold_bytes {
                    info!("Health check: client {} channel {} has {}B available (threshold {}B) — disconnecting",
                        client_id, ch, available, threshold_bytes);
                    stale.push(client_id);
                    break;
                }
            }
        }
        stale
    }
}

// ── Plugin ──

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ServerNet::new());
        app.add_systems(PreUpdate, net_receive);
        app.add_systems(PostUpdate, net_send);
    }
}

/// Process incoming packets and trigger connection events.
fn net_receive(mut net: ResMut<ServerNet>, mut commands: Commands, time: Res<Time>) {
    let ServerNet { ref mut server, ref mut transport, ref mut clients, .. } = *net;
    transport.update(time.delta(), server).unwrap();
    server.update(time.delta());

    while let Some(event) = server.get_event() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                clients.entry(client_id).or_default();
                commands.trigger(NetServerEvent::ClientConnected { client_id });
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                clients.remove(&client_id);
                commands.trigger(NetServerEvent::ClientDisconnected { client_id, reason });
            }
        }
    }
}

/// Rate-limited send: drain outbound queues up to budget, health check, flush packets.
fn net_send(mut net: ResMut<ServerNet>, time: Res<Time>) {
    let send_interval = 1.0 / SEND_RATE;
    net.send_timer += time.delta_secs();

    if net.send_timer < send_interval {
        return;
    }
    net.send_timer -= send_interval;
    net.drain_queues();

    // Periodic health check: disconnect clients whose buffer isn't draining
    net.health_timer += time.delta_secs();
    if net.health_timer >= HEALTH_CHECK_INTERVAL {
        net.health_timer -= HEALTH_CHECK_INTERVAL;
        let stale = net.check_health();
        for client_id in stale {
            net.clients.remove(&client_id);
            net.server.disconnect(client_id);
        }
    }

    // Flush to wire — only on drain ticks, not every frame
    let ServerNet { ref mut server, ref mut transport, .. } = *net;
    transport.send_packets(server);
}
