use bevy::prelude::*;
use renet::ServerEvent;

use crate::{*,
    common::{
        message::{*, Event},
        components::{
            hx::*,
            keybits::*,
        },
        resources::*,
    },
};

pub fn new_renet_server() -> (RenetServer, NetcodeServerTransport) {
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
    let server = RenetServer::new(ConnectionConfig::default());

    (server, transport)
}

pub fn do_manage_connections(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<ServerEvent>,
    mut lobby: ResMut<Lobby>,
    mut queues: ResMut<InputQueues>,
    query: Query<(&Hx, &EntityType)>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let hx = Hx { q: 0, r: 0, z: 4 };
                let typ = EntityType::Actor;
                let ent = commands.spawn((
                    Transform::default(),
                    KeyBits::default(),
                    Heading::default(),
                    Offset::default(),
                    Actor::default(),
                    typ,
                    hx, 
                )).id();
                let message = bincode::serialize(&Do { event: Event::Spawn { ent, typ, hx }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
                for (_, &ent) in lobby.0.iter() {
                    let (&hx, &typ) = query.get(ent).unwrap();
                    let message = bincode::serialize(&Do { event: Event::Spawn { typ, ent, hx }}).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                lobby.0.insert(*client_id, ent);
                queues.0.insert(ent, InputQueue::default());
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.0.remove_by_left(&client_id).unwrap().1;
                queues.0.remove(&ent);
                commands.entity(ent).despawn();
                let message = bincode::serialize(&Do { event: Event::Despawn { ent }}).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
        }
    }
 }

pub fn write_try(
    mut writer: EventWriter<Try>,
    mut query: Query<&mut KeyBits>,
    mut conn: ResMut<RenetServer>,
    lobby: Res<Lobby>,
) {
    for client_id in conn.clients_id() {
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let message = bincode::deserialize(&serialized).unwrap();
            match message {
                Try { event: Event::Input { key_bits, .. } } => {
                    if let Some(&ent) = lobby.0.get_by_left(&client_id) {
                        let mut key_bits0 = query.get_mut(ent).unwrap();
                        if *key_bits0 != key_bits { *key_bits0 = key_bits; }
                    }
                }
                Try { event: Event::Discover { hx, .. } } => { 
                    if let Some(&ent) = lobby.0.get_by_left(&client_id) {
                        writer.send(Try { event: Event::Discover { ent, hx }});
                    }
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    mut map: ResMut<Map>,
    mut queues: ResMut<InputQueues>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { mut ent, typ, hx } } => {
                ent = commands.spawn((
                    hx,
                    Offset::default(),
                    typ,
                    Transform {
                        translation: (hx,Offset::default()).calculate(),
                        ..default()}, 
                )).id();
                map.insert(hx, ent);
                conn.broadcast_message(DefaultChannel::ReliableOrdered, 
                    bincode::serialize(&Do { event: Event::Spawn { ent, typ, hx }}).unwrap());
            }
            Do { event: Event::Move { ent, hx, heading } } => {
                conn.broadcast_message(DefaultChannel::ReliableOrdered, 
                    bincode::serialize(&Do { event: Event::Move { ent, hx, heading }}).unwrap());
            }
            Do { event: Event::Input { ent, key_bits, dt } } => {
                let mut key_bits_last = queues.0.get_mut(&ent).unwrap().0
                    .pop_front().unwrap_or(InputAccumulator { key_bits, dt: 0 });
                if key_bits.key_bits != key_bits_last.key_bits.key_bits || key_bits_last.dt > 1000 {
                    conn.send_message(*lobby.0.get_by_right(&ent).unwrap(), 
                        DefaultChannel::ReliableOrdered, 
                            bincode::serialize(&Do { event: Event::Input { 
                                ent,
                                key_bits: key_bits_last.key_bits, 
                                dt: key_bits_last.dt,
                    }}).unwrap());
                    trace!("Input: ent: {}, key_bits: {:?}, dt: {}", ent, key_bits_last.key_bits, key_bits_last.dt);
                    key_bits_last = InputAccumulator { key_bits, dt: 0 };
                }
                key_bits_last.dt += dt;
                queues.0.get_mut(&ent).unwrap().0.push_front(key_bits_last);
            }
            _ => {}
        }
    }
}