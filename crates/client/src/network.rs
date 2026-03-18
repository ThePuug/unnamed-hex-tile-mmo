use std::net::UdpSocket;
use std::time::SystemTime;

use bevy::prelude::*;
use ::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use common::network::*;

// ── Wrapper resource ──

/// Owns the renet client and transport. Game systems access this, never renet directly.
#[derive(Resource)]
pub struct ClientNet {
    client: RenetClient,
    transport: NetcodeClientTransport,
    send_timer: f32,
}

impl ClientNet {
    fn new() -> Self {
        let server_addr = "127.0.0.1:5000".parse().unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let client_id = current_time.as_millis() as u64;
        let authentication = ClientAuthentication::Unsecure {
            client_id,
            protocol_id: PROTOCOL_ID,
            server_addr,
            user_data: None,
        };

        let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();

        let mut config = ConnectionConfig::default();
        config.server_channels_config[CH_RELIABLE_ORDERED as usize].max_memory_usage_bytes = RELIABLE_ORDERED_MAX_MEMORY;
        config.server_channels_config[CH_RELIABLE_UNORDERED as usize].max_memory_usage_bytes = RELIABLE_UNORDERED_MAX_MEMORY;
        let client = RenetClient::new(config);

        Self { client, transport, send_timer: 0.0 }
    }

    // ── Send ──

    /// Queue a reliable message to the server. Phase 2 will add budget gating.
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
}

// ── Plugin ──

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClientNet::new());
        app.add_systems(PreUpdate, net_receive);
        app.add_systems(PostUpdate, net_send);
    }
}

/// Process incoming packets.
fn net_receive(mut net: ResMut<ClientNet>, time: Res<Time>) {
    let ClientNet { ref mut client, ref mut transport, .. } = *net;
    transport.update(time.delta(), client).unwrap();
    client.update(time.delta());
}

/// Rate-limited flush of outgoing packets.
fn net_send(mut net: ResMut<ClientNet>, time: Res<Time>) {
    let send_interval = 1.0 / SEND_RATE;
    net.send_timer += time.delta_secs();
    if net.send_timer < send_interval {
        return;
    }
    net.send_timer -= send_interval;
    let ClientNet { ref mut client, ref mut transport, .. } = *net;
    let _ = transport.send_packets(client);
}
