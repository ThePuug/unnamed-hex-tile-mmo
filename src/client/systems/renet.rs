use bevy::prelude::*;
use bevy_renet::netcode::ClientAuthentication;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::resources::*,
    common::{
        message::{Event, *}, 
        resources::*
    }, *
};

pub fn setup(
    mut commands: Commands,
) {
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
    let client = RenetClient::new(ConnectionConfig::default());

    commands.insert_resource(client);
    commands.insert_resource(transport);
}

pub fn send_do(
    mut commands: Commands,
    mut writer: EventWriter<Do>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut buffer: ResMut<InputQueue>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
        match message {

            // insert l2r for player
            Do { event: Event::Init { ent, dt }} => {
                debug!("Player {ent} connected, time offset: {dt}");
                let loc = commands.spawn(Actor).id();
                l2r.insert(loc, ent);
                writer.send(Do { event: Event::Init { ent: loc, dt }});
            }

            // insert l2r entry when spawning an Actor
            Do { event: Event::Spawn { ent, typ, qrz } } if typ == EntityType::Actor => {
                let ent = match typ {
                    EntityType::Actor => {
                        if let Some(&loc) = l2r.get_by_right(&ent) { loc }
                        else {
                            let loc = commands.spawn_empty().id();
                            l2r.insert(loc, ent);
                            loc        
                        } 
                    },
                    _ => { Entity::PLACEHOLDER }
                };
                writer.send(Do { event: Event::Spawn { ent, typ, qrz }});
            }

            // just pass through the other spawn events
            Do { event: Event::Spawn { ent, typ, qrz } } => {
                writer.send(Do { event: Event::Spawn { ent, typ, qrz }});
            }

            // manage the input queue before sending the incoming Input
            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let &ent = l2r.get_by_right(&ent).unwrap();
                if let Some(Event::Input { seq: seq0, dt: dt0, key_bits: key_bits0, .. }) = buffer.queue.pop_back() {
                    assert_eq!(seq0, seq);
                    assert_eq!(key_bits0, key_bits);
                    if (dt0 as i16 - dt as i16).abs() >= 100 { warn!("{dt0} !~ {dt}"); }
                    if buffer.queue.len() > 2 { warn!("long input queue, len: {}", buffer.queue.len()); }
                } else { unreachable!(); }
                writer.send(Do { event: Event::Input { ent, key_bits, dt, seq } });
            }
            Do { event: Event::Despawn { ent } } => {
                let (ent, _) = l2r.remove_by_right(&ent).unwrap();
                debug!("Player {ent} disconnected");
                commands.entity(ent).despawn();
            }
            Do { event: Event::Incremental { ent, attr } } => {
                let &ent = l2r.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Incremental { ent, attr } });
            }
            Do { event: Event::Gcd { ent, typ } } => {
                let &ent = l2r.get_by_right(&ent).unwrap();
                writer.send(Do { event: Event::Gcd { ent, typ } });
            }
            _ => {}
        }
    }
}

pub fn write_try(
    mut conn: ResMut<RenetClient>,
    mut reader: EventReader<Try>,
    l2r: Res<EntityMap>,
    mut buffer: ResMut<InputQueue>,
) {
    for &message in reader.read() {
        match message {
            // manage the queue before sending the outgoing Input
            Try { event: Event::Input { ent, key_bits, mut dt, mut seq } } => {
                // seq 0 is input this frame which we should handle
                // other received Input events are being replayed and should be ignored
                if seq != 0 { continue; }

                let input0 = buffer.queue.pop_front().unwrap();
                match input0 {
                    Event::Input { key_bits: key_bits0, dt: mut dt0, seq: seq0, .. } => {
                        seq = seq0;
                        if key_bits.key_bits != key_bits0.key_bits || dt0 > 1000 { 
                            buffer.queue.push_front(input0);
                            seq = if seq0 == 255 { 1 } else { seq0 + 1}; 
                            dt0 = 0;
                            conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Input { 
                                ent: *l2r.get_by_left(&ent).unwrap(), 
                                key_bits, dt: 0, seq,
                            }}, bincode::config::legacy()).unwrap());
                        }
                        dt += dt0;
                        buffer.queue.push_front(Event::Input { ent, key_bits, dt, seq });
                    }
                    _ => unreachable!()
                };
            }
            Try { event: Event::Gcd { ent, typ, .. } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Gcd { 
                    ent: *l2r.get_by_left(&ent).unwrap(), 
                    typ,
                }}, bincode::config::legacy()).unwrap());
            }
            _ => {}
        }
    }
}