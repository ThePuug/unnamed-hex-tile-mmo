use bevy::prelude::*;
use bevy_renet::netcode::ClientAuthentication;
use qrz::Qrz;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::resources::{EntityMap, LoadedChunks},
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
    mut loaded_chunks: ResMut<LoadedChunks>,
    locs: Query<&Loc>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        match message {

            // insert l2r for player
            Do { event: Event::Init { ent: ent0, dt }} => {
                // Create local player entity with markers
                // Health/Stamina/Mana will be inserted by Incremental events from server
                let ent = commands.spawn((
                    Actor,
                    Behaviour::Controlled,
                    PlayerControlled,
                    crate::common::components::target::Target::default(), // For unified targeting system
                )).id();
                info!("INIT: Spawned local player entity {:?} with Actor and PlayerControlled markers", ent);
                l2r.insert(ent, ent0);
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: default(), dt: 0, seq: 1 }].into() }));
                do_writer.write(Do { event: Event::Init { ent, dt }});
            }

            // insert l2r entry when spawning an Actor
            Do { event: Event::Spawn { ent, typ, qrz, attrs } } => {
                let ent = match typ {
                    EntityType::Actor(_) => {
                        if let Some(&loc) = l2r.get_by_right(&ent) {
                            loc
                        }
                        else {
                            let loc = commands.spawn(typ).id();
                            l2r.insert(loc, ent);
                            loc
                        }
                    },
                    EntityType::Projectile => {
                        // Spawn local entity for projectile visual
                        let loc = commands.spawn(typ).id();
                        l2r.insert(loc, ent);
                        loc
                    },
                    _ => { Entity::PLACEHOLDER }
                };
                do_writer.write(Do { event: Event::Spawn { ent, typ, qrz, attrs }});
            }

            Do { event: Event::Input { ent, key_bits, dt, seq } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                 };
                do_writer.write(Do { event: Event::Input { ent, key_bits, dt, seq } });
            }
            Do { event: Event::Despawn { ent } } => {
                debug!("[CLIENT DESPAWN] Received Despawn event for entity {:?}", ent);

                // Check if this is the local player (has InputQueue)
                let is_local_player = l2r.get_by_right(&ent)
                    .and_then(|&local_ent| buffers.get(&local_ent))
                    .is_some();

                if is_local_player {
                    // For local player: DON'T despawn the entity, just mark as dead
                    // The entity will be reused on respawn
                    // Entity stays alive but invisible/inactive (handled by update_dead_visibility)
                    info!("CLIENT: Received Despawn for LOCAL player {:?} - keeping entity alive (hidden)", ent);
                } else {
                    // For NPCs/other players: remove from EntityMap and despawn
                    let Some((local_ent, _)) = l2r.remove_by_right(&ent) else {
                        debug!("[CLIENT DESPAWN] Entity {:?} not in EntityMap - already despawned or never spawned", ent);
                        continue
                    };
                    debug!("[CLIENT DESPAWN] Despawning local entity {:?} (remote {:?})", local_ent, ent);

                    // Spawn hit flash effect for projectiles (capture Loc before despawning)
                    if let Ok(loc) = locs.get(local_ent) {
                        info!("[CLIENT DESPAWN] Writing SpawnHitFlash event at loc {:?}", **loc);
                        do_writer.write(Do { event: Event::SpawnHitFlash { loc: *loc } });
                    }

                    commands.entity(local_ent).despawn();
                }
            }
            Do { event: Event::Incremental { ent, component } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                do_writer.write(Do { event: Event::Incremental { ent, component } });
            }
            Do { event: Event::Gcd { ent, typ } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                do_writer.write(Do { event: Event::Gcd { ent, typ } });
            }
            Do { event: Event::ChunkData { ent: _, chunk_id, tiles } } => {
                // Unpack chunk into individual tile spawns
                for (qrz, typ) in tiles {
                    // Emit spawn events for world system to process
                    do_writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None }});
                }

                // Track that we received this chunk
                loaded_chunks.insert(chunk_id);
            }
            Do { event: Event::InsertThreat { ent, threat } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                // Forward to Do writer for systems to handle
                do_writer.write(Do { event: Event::InsertThreat { ent, threat } });
            }
            Do { event: Event::ApplyDamage { ent, damage, source } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                // Map source entity too
                let source = l2r.get_by_right(&source).copied().unwrap_or(source);
                // Forward to Do writer for systems to handle
                do_writer.write(Do { event: Event::ApplyDamage { ent, damage, source } });
            }
            Do { event: Event::ClearQueue { ent, clear_type } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                do_writer.write(Do { event: Event::ClearQueue { ent, clear_type } });
            }
            Do { event: Event::AbilityFailed { ent, reason } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                do_writer.write(Do { event: Event::AbilityFailed { ent, reason } });
            }
            Do { event: Event::Pong { client_time } } => {
                // Forward Pong to Do writer for handle_pong system
                do_writer.write(Do { event: Event::Pong { client_time } });
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
            Try { event: Event::Spawn { ent, typ, qrz, attrs } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Spawn {
                    ent, typ, qrz, attrs
                }}, bincode::config::legacy()).unwrap());
            }
            Try { event: Event::UseAbility { ent, ability, target_loc } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::UseAbility {
                    ent: *l2r.get_by_left(&ent).unwrap(),
                    ability,
                    target_loc
                }}, bincode::config::legacy()).unwrap());
            }
            Try { event: Event::Ping { client_time } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::Ping {
                    client_time
                }}, bincode::config::legacy()).unwrap());
            }
            _ => {}
        }
    }
}

/// Handle Pong response to refine time sync with measured network latency
pub fn handle_pong(
    mut reader: EventReader<Do>,
    mut server: ResMut<crate::client::resources::Server>,
    time: Res<Time>,
) {
    for &message in reader.read() {
        let Do { event: Event::Pong { client_time } } = message else { continue };
        let client_now = time.elapsed().as_millis();

        // Calculate round-trip time (RTT) and one-way latency
        let rtt = client_now.saturating_sub(client_time);
        let measured_latency = rtt / 2;

        let old_smoothed = server.smoothed_latency;

        // Update smoothed latency using exponential moving average
        // alpha = 0.2 means: 20% new measurement, 80% old average
        // This provides smoothing while still adapting to changes
        let alpha = 0.2;
        server.smoothed_latency = ((old_smoothed as f64 * (1.0 - alpha))
            + (measured_latency as f64 * alpha)) as u128;

        // Adjust server_time_at_init based on the change in latency estimate
        // This prevents time jumps - we gradually correct for latency changes
        let latency_delta = server.smoothed_latency as i128 - old_smoothed as i128;
        if latency_delta != 0 {
            // Positive delta = latency increased, we're behind, add time
            // Negative delta = latency decreased, we're ahead, subtract time
            server.server_time_at_init = if latency_delta > 0 {
                server.server_time_at_init.saturating_add(latency_delta as u128)
            } else {
                server.server_time_at_init.saturating_sub(latency_delta.unsigned_abs())
            };
        }
    }
}

/// Send periodic pings to keep latency estimate up-to-date
/// Sends a ping every 5 seconds to measure network conditions
pub fn periodic_ping(
    mut server: ResMut<crate::client::resources::Server>,
    mut try_writer: EventWriter<Try>,
    time: Res<Time>,
) {
    const PING_INTERVAL_MS: u128 = 5000; // Ping every 5 seconds

    let client_now = time.elapsed().as_millis();

    // Check if it's time to send another ping
    if client_now.saturating_sub(server.last_ping_time) >= PING_INTERVAL_MS {
        server.last_ping_time = client_now;
        try_writer.send(Try { event: Event::Ping { client_time: client_now } });
    }
}