use bevy::prelude::*;
use bevy_renet::netcode::ClientAuthentication;
use qrz::Qrz;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::resources::EntityMap, 
    common::{
        components::{behaviour::*, entity_type::*},
        message::{Component, Event, *}, 
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

pub fn write_do(
    mut commands: Commands,
    mut do_writer: EventWriter<Do>,
    mut try_writer: EventWriter<Try>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut buffers: ResMut<InputQueues>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
        match message {

            // insert l2r for player
            Do { event: Event::Init { ent: ent0, dt }} => {
                let ent = commands.spawn((Actor,Behaviour::Controlled)).id();
                debug!("Player {ent0} connected as {ent}, time offset: {dt}");
                l2r.insert(ent, ent0);
                buffers.extend_one((ent, InputQueue { 
                    queue: [Event::Input { ent, key_bits: default(), dt: 0, seq: 1 }].into() }));
                do_writer.write(Do { event: Event::Init { ent, dt }});
            }

            // insert l2r entry when spawning an Actor
            Do { event: Event::Spawn { ent, typ, qrz } } => {
                let ent = match typ {
                    EntityType::Actor(_) => {
                        if let Some(&loc) = l2r.get_by_right(&ent) { loc }
                        else {
                            let loc = commands.spawn(typ).id();
                            l2r.insert(loc, ent);
                            loc
                        } 
                    },
                    _ => { Entity::PLACEHOLDER }
                };
                do_writer.write(Do { event: Event::Spawn { ent, typ, qrz }});
            }

            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else { 
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default() }});
                    continue
                 };
                do_writer.write(Do { event: Event::Input { ent, key_bits, dt, seq } });
            }
            Do { event: Event::Despawn { ent } } => {
                let Some((ent, _)) = l2r.remove_by_right(&ent) else {
                    // Entity not in our map - likely another client disconnecting or already despawned
                    continue
                };
                commands.entity(ent).despawn();
            }
            Do { event: Event::Incremental { ent, component } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else { 
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default() }});
                    continue
                };
                do_writer.write(Do { event: Event::Incremental { ent, component } });
            }
            Do { event: Event::Gcd { ent, typ } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else { 
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default() }});
                    continue
                };
                do_writer.write(Do { event: Event::Gcd { ent, typ } });
            }
            _ => {}
        }
    }
}

pub fn send_try(
    mut conn: ResMut<RenetClient>,
    mut reader: EventReader<Try>,
    l2r: Res<EntityMap>,
) {
    for &message in reader.read() {
        match message {
            Try { event: Event::Incremental { ent, component: Component::KeyBits(keybits) } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Incremental { 
                    ent: *l2r.get_by_left(&ent).unwrap(),
                    component: Component::KeyBits(keybits) 
                }}, bincode::config::legacy()).unwrap());
            }
            Try { event: Event::Gcd { ent, typ, .. } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Gcd { 
                    ent: *l2r.get_by_left(&ent).unwrap(), 
                    typ,
                }}, bincode::config::legacy()).unwrap());
            }
            Try { event: Event::Spawn { ent, typ, qrz } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Spawn { 
                    ent, typ, qrz
                }}, bincode::config::legacy()).unwrap());
            } 
            _ => {}
        }
    }
}