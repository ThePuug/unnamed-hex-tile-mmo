use bevy::prelude::*;
use bevy_renet::netcode::ClientAuthentication;
use qrz::Qrz;
use ::renet::{DefaultChannel, RenetClient};

use crate::{
    client::{
        plugins::diagnostics::network_ui::NetworkMetrics,
        resources::{EntityMap, LoadedChunks},
    },
    common::{
        components::{behaviour::*, entity_type::*, reaction_queue::QueuedThreat},
        message::{Component, Event, *},
        resources::*
    }, *
};

// Helper function to get human-readable message type name
fn get_message_type_name(message: &Do) -> String {
    match &message.event {
        Event::Init { .. } => "Init".to_string(),
        Event::Spawn { .. } => "Spawn".to_string(),
        Event::Input { .. } => "Input".to_string(),
        Event::Despawn { .. } => "Despawn".to_string(),
        Event::Incremental { component, .. } => {
            match component {
                Component::Loc(_) => "Inc:Loc".to_string(),
                Component::Heading(_) => "Inc:Heading".to_string(),
                Component::Health(_) => "Inc:Health".to_string(),
                Component::Mana(_) => "Inc:Mana".to_string(),
                Component::Stamina(_) => "Inc:Stamina".to_string(),
                Component::TierLock(_) => "Inc:TierLock".to_string(),
                Component::CombatState(_) => "Inc:Combat".to_string(),
                Component::Behaviour(_) => "Inc:Behaviour".to_string(),
                Component::KeyBits(_) => "Inc:KeyBits".to_string(),
                Component::PlayerControlled(_) => "Inc:PlayerControlled".to_string(),
                Component::Returning(_) => "Inc:Returning".to_string(),
            }
        },
        Event::Gcd { .. } => "Gcd".to_string(),
        Event::ChunkData { .. } => "ChunkData".to_string(),
        Event::InsertThreat { .. } => "InsertThreat".to_string(),
        Event::ApplyDamage { .. } => "ApplyDamage".to_string(),
        Event::ClearQueue { .. } => "ClearQueue".to_string(),
        Event::AbilityFailed { .. } => "AbilityFailed".to_string(),
        Event::UseAbility { .. } => "UseAbility".to_string(),
        Event::Pong { .. } => "Pong".to_string(),
        Event::MovementIntent { .. } => "MovementIntent".to_string(),
        _ => "Other".to_string(),
    }
}

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
    mut do_writer: MessageWriter<Do>,
    mut try_writer: MessageWriter<Try>,
    mut conn: ResMut<RenetClient>,
    mut l2r: ResMut<EntityMap>,
    mut buffers: ResMut<InputQueues>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut network_metrics: ResMut<NetworkMetrics>,
    _locs: Query<&Loc>,
    time: Res<Time>,
) {
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        // Track network metrics (received from server)
        let message_type = get_message_type_name(&message);
        network_metrics.record_received(message_type, serialized.len());

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
                    crate::common::components::ally_target::AllyTarget::default(), // For ally targeting
                    crate::common::components::tier_lock::TierLock::default(), // For tier lock
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
                // Check if this is the local player (has InputQueue)
                let is_local_player = l2r.get_by_right(&ent)
                    .and_then(|&local_ent| buffers.get(&local_ent))
                    .is_some();

                if is_local_player {
                    // For local player: DON'T despawn the entity, just mark as dead
                    // The entity will be reused on respawn
                    // Entity stays alive but invisible/inactive (handled by update_dead_visibility)
                } else {
                    // For NPCs/other players: remove from EntityMap and delay despawn
                    // Entity stays alive for 3s in a death pose so damage numbers can render
                    let Some((local_ent, _)) = l2r.remove_by_right(&ent) else {
                        continue
                    };

                    commands.entity(local_ent).insert(
                        crate::client::components::DeathMarker {
                            death_time: time.elapsed(),
                        }
                    );
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
                    warn!("Client: InsertThreat target {:?} not in l2r map, requesting spawn", ent);
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                // Map threat source entity too
                let mapped_source = l2r.get_by_right(&threat.source).copied().unwrap_or(threat.source);
                let mapped_threat = QueuedThreat {
                    source: mapped_source,
                    ..threat
                };
                // Forward to Do writer for systems to handle
                do_writer.write(Do { event: Event::InsertThreat { ent, threat: mapped_threat } });
            }
            Do { event: Event::ApplyDamage { ent, damage, source } } => {
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    warn!("Client: ApplyDamage target {:?} not in l2r map, requesting spawn", ent);
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                // Map source entity too
                let source = l2r.get_by_right(&source).copied().unwrap_or(source);
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
            Do { event: Event::UseAbility { ent, ability, target_loc } } => {
                // Map entity ID and forward to ability_prediction system (ADR-012)
                let Some(&ent) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                do_writer.write(Do { event: Event::UseAbility { ent, ability, target_loc } });
            }
            Do { event: Event::Pong { client_time } } => {
                // Forward Pong to Do writer for handle_pong system
                do_writer.write(Do { event: Event::Pong { client_time } });
            }
            _ => {}
        }
    }

    // ADR-011: Listen for MovementIntent on Unreliable channel for bandwidth efficiency
    // Unreliable channel is used for frequent, self-correcting messages (latest wins)
    while let Some(serialized) = conn.receive_message(DefaultChannel::Unreliable) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        // Track network metrics (received from server)
        let message_type = get_message_type_name(&message);
        network_metrics.record_received(message_type, serialized.len());

        match message {
            Do { event: Event::MovementIntent { ent, destination, duration_ms } } => {
                // Map entity ID and forward for prediction system
                let Some(&local_ent) = l2r.get_by_right(&ent) else {
                    // Entity not yet spawned on client - ignore intent silently
                    continue
                };
                do_writer.write(Do { event: Event::MovementIntent { ent: local_ent, destination, duration_ms } });
            }
            _ => {
                // Only MovementIntent should be sent on Unreliable channel
                warn!("Unexpected message type on Unreliable channel: {:?}", message);
            }
        }
    }
}

pub fn send_try(
    mut conn: ResMut<RenetClient>,
    mut reader: MessageReader<Try>,
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
            Try { event: Event::SetTierLock { ent, tier } } => {
                conn.send_message(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event: Event::SetTierLock {
                    ent: *l2r.get_by_left(&ent).unwrap(),
                    tier
                }}, bincode::config::legacy()).unwrap());
            }
            _ => {}
        }
    }
}

/// Handle Pong response to refine time sync with measured network latency
pub fn handle_pong(
    mut reader: MessageReader<Do>,
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
    mut try_writer: MessageWriter<Try>,
    time: Res<Time>,
) {
    const PING_INTERVAL_MS: u128 = 5000; // Ping every 5 seconds

    let client_now = time.elapsed().as_millis();

    // Check if it's time to send another ping
    if client_now.saturating_sub(server.last_ping_time) >= PING_INTERVAL_MS {
        server.last_ping_time = client_now;
        try_writer.write(Try { event: Event::Ping { client_time: client_now } });
    }
}