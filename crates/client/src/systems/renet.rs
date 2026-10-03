use bevy::prelude::*;
use ::renet::DefaultChannel;
use qrz::Qrz;

use crate::network::ClientNet;

use crate::{
    plugins::diagnostics::network_ui::NetworkMetrics,
    resources::{EntityMap, LoadedChunks},
};
use crate::*;
use common_bevy::{
    components::{behaviour::*, entity_type::*},
    message::{Component, Event, *},
    resources::*,
};

// Helper function to get human-readable message type name
fn get_message_type_name(message: &Do) -> &'static str {
    match &message.event {
        Event::Init { .. } => "Init",
        Event::Spawn { .. } => "Spawn",
        Event::Confirm { .. } => "Confirm",
        Event::Despawn { .. } => "Despawn",
        Event::Incremental { component, .. } => match component {
            Component::Loc(_) => "Inc:Loc",
            Component::Heading(_) => "Inc:Heading",
            Component::Health(_) => "Inc:Health",
            Component::Endurance(_) => "Inc:Endurance",
            Component::Mana(_) => "Inc:Mana",
            Component::Stamina(_) => "Inc:Stamina",
            Component::CombatState(_) => "Inc:Combat",
            Component::PlayerControlled(_) => "Inc:PlayerControlled",
            Component::Recovery(_) => "Inc:Recovery",
            Component::Returning(_) => "Inc:Returning",
            Component::Side(_) => "Inc:Side",
            Component::Status(_) => "Inc:Status",
            Component::Equipment(_) => "Inc:Equipment",
        },
        Event::ChunkData { .. } => "ChunkData",
        Event::InsertThreat { .. } => "InsertThreat",
        Event::ApplyDamage { .. } => "ApplyDamage",
        Event::ClearQueue { .. } => "ClearQueue",
        Event::UseAbility { .. } => "UseAbility",
        Event::Pong { .. } => "Pong",
        Event::MovementIntent { .. } => "MovementIntent",
        Event::Displace { .. } => "Displace",
        Event::EvictChunks { .. } => "EvictChunks",
        Event::SummaryBatch { .. } => "SummaryBatch",
        Event::Inventory { .. } => "Inventory",
        Event::Gather { .. } => "Gather",
        Event::CoverChanged { .. } => "CoverChanged",
        Event::Loot { .. } => "Loot",
        Event::Activity { .. } => "Activity",
        Event::View { .. } => "View",
        _ => "Other",
    }
}

pub fn write_do(
    mut commands: Commands,
    mut do_writer: MessageWriter<Do>,
    mut try_writer: MessageWriter<Try>,
    conn: Option<ResMut<ClientNet>>,
    entered: Res<crate::plugins::shell::Entered>,
    mut l2r: ResMut<EntityMap>,
    mut buffers: ResMut<InputQueues>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    summary_cache: Res<crate::resources::SummaryCache>,
    map: Res<common_bevy::resources::map::Map>,
    mut network_metrics: ResMut<NetworkMetrics>,
    _locs: Query<&Loc>,
    time: Res<Time>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let Some(mut conn) = conn else { return };
    let _t = client_timers.0.scope("write_do");
    // Out of the world every message is about a world the client has left,
    // or not yet entered: what was in flight when it left is drained unread.
    let in_world = entered.0;
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableOrdered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        // Track network metrics (received from server)
        let message_type = get_message_type_name(&message);
        network_metrics.record_received(message_type, serialized.len());
        if !in_world {
            continue;
        }

        match message {

            // insert l2r for player
            Do { event: Event::Init { ent: ent0, dt }} => {
                // Create local player entity with markers
                // Health/Stamina/Mana will be inserted by Incremental events from server
                let ent = commands.spawn((
                    Actor,
                    crate::components::Viewed,
                    PlayerControlled,
                    common_bevy::components::target::Target::default(), // For unified targeting system
                    common_bevy::components::ally_target::AllyTarget::default(), // For ally targeting
                )).id();
                info!("INIT: Spawned local player entity {:?} with Actor and PlayerControlled markers", ent);
                l2r.insert(ent, ent0);
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: default(), dt: 0, seq: 1 }].into(), ..default() }));
                do_writer.write(Do { event: Event::Init { ent, dt }});
            }

            // The client now sees the world as an actor it does not control:
            // its character left the world to view, and goes from here too,
            // for a server Despawn of the local player is kept for respawn
            Do { event: Event::View { ent } } => {
                let Some(&viewed) = l2r.get_by_right(&ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                let players: Vec<Entity> = buffers.entities().copied().collect();
                for player in players {
                    buffers.remove(&player);
                    l2r.remove_by_left(&player);
                    commands.entity(player).despawn();
                }
                do_writer.write(Do { event: Event::View { ent: viewed } });
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

            Do { event: Event::Despawn { ent } } => {
                // Check if this is the local player (has InputQueue)
                let is_local_player = l2r.get_by_right(&ent)
                    .and_then(|&local_ent| buffers.get(&local_ent))
                    .is_some();

                if is_local_player {
                    // The player falls as any actor does, but keeps its
                    // entity: the server respawns it, and it stands again
                    if let Some(&local_ent) = l2r.get_by_right(&ent) {
                        commands.entity(local_ent).try_insert(crate::components::DeathMarker { death_time: time.elapsed() });
                    }
                } else {
                    // For NPCs/other players: remove from EntityMap and delay despawn
                    // Entity stays alive for 3s in a death pose so damage numbers can render
                    let Some((local_ent, _)) = l2r.remove_by_right(&ent) else {
                        continue
                    };

                    if let Ok(mut cmd) = commands.get_entity(local_ent) {
                        cmd.insert(
                            crate::components::DeathMarker {
                                death_time: time.elapsed(),
                            }
                        );
                    } else {
                        warn!("Despawn for already-dead local entity {:?} (server {:?})", local_ent, ent);
                    }
                }
            }
            // A tile's cover is about no entity here
            Do { event: Event::CoverChanged { ent: _, q, r, cover } } => {
                do_writer.write(Do { event: Event::CoverChanged { ent: Entity::PLACEHOLDER, q, r, cover } });
            }
            Do { event: event @ Event::Pong { .. } } => {
                do_writer.write(Do { event });
            }
            // Every other is about an entity, known here by an id of its
            // own; one the client has not spawned is asked for. ChunkData
            // arrives on ReliableUnordered, handled below.
            Do { event: mut event @ (
                Event::Confirm { .. }
                | Event::Incremental { .. }
                | Event::Inventory { .. }
                | Event::Loot { .. }
                | Event::Activity { .. }
                | Event::InsertThreat { .. }
                | Event::ApplyDamage { .. }
                | Event::ClearQueue { .. }
                | Event::UseAbility { .. }
                | Event::RespecAttributes { .. }
            ) } => {
                let Some(ent) = event.ent_mut() else { continue };
                let Some(&local) = l2r.get_by_right(ent) else {
                    try_writer.write(Try { event: Event::Spawn { ent: *ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                    continue
                };
                *ent = local;
                // The others an event names: a blow's source keeps the server's
                // id where it is gone here, and a threat's always, since it
                // names the threat in the ClearQueue that ends it
                match &mut event {
                    Event::ApplyDamage { source, .. } => *source = l2r.get_by_right(source).copied().unwrap_or(*source),
                    Event::UseAbility { target, .. } => *target = target.and_then(|target| l2r.get_by_right(&target).copied()),
                    _ => {}
                }
                do_writer.write(Do { event });
            }
            _ => {}
        }
    }

    // Chunk data and eviction arrive on ReliableUnordered — no ordering needed between independent chunks
    while let Some(serialized) = conn.receive_message(DefaultChannel::ReliableUnordered) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        let message_type = get_message_type_name(&message);
        network_metrics.record_received(message_type, serialized.len());
        if !in_world {
            continue;
        }

        match message {
            Do { event: Event::ChunkData { ent: _, chunk_id, tiles } } => {
                // Reconstruct (q,r) from chunk_tiles iteration order. The
                // ground arrives as a spawn; the water goes straight to the
                // map, which holds it apart from the ground.
                for ((q, r), (z, typ, water)) in common_bevy::chunk::chunk_tiles(chunk_id).zip(tiles) {
                    let qrz = Qrz { q, r, z };
                    map.set_water(q, r, water);
                    do_writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None }});
                }
                loaded_chunks.insert(chunk_id);
            }
            Do { event: Event::EvictChunks { ent: _, chunks } } => {
                do_writer.write(Do { event: Event::EvictChunks { ent: Entity::PLACEHOLDER, chunks } });
            }
            Do { event: Event::SummaryBatch { ent: _, additions, removals: _ } } => {
                // Group additions by mesh region
                let region_lat = common_bevy::summary::mesh_region_lattice();
                let mut by_region: std::collections::HashMap<common_bevy::summary_mesh::MeshRegionKey, std::collections::HashMap<(i32,i32), common_bevy::summary::SummaryCell>> = std::collections::HashMap::new();
                for add in &additions {
                    let (mn, mm) = region_lat.cell_id(add.sq, add.sr);
                    let key = common_bevy::summary_mesh::MeshRegionKey { r: add.r, mn, mm };
                    by_region.entry(key).or_default().insert((add.sq, add.sr), add.cell);
                }
                for (key, cells) in by_region {
                    summary_cache.insert_region(key, crate::resources::RegionData { cells });
                }
            }
            _ => {
                warn!("Unexpected message type on ReliableUnordered channel");
            }
        }
    }

    // Listen for MovementIntent on Unreliable channel for bandwidth efficiency
    // Unreliable channel is used for frequent, self-correcting messages (latest wins)
    while let Some(serialized) = conn.receive_message(DefaultChannel::Unreliable) {
        let (message, _) = bincode::serde::decode_from_slice(&serialized, bincode::config::legacy()).unwrap();

        // Track network metrics (received from server)
        let message_type = get_message_type_name(&message);
        network_metrics.record_received(message_type, serialized.len());
        if !in_world {
            continue;
        }

        match message {
            // An intent for an entity not yet spawned is dropped: the next one repairs it.
            Do { event: mut event @ (Event::MovementIntent { .. } | Event::Displace { .. }) } => {
                let Some(ent) = event.ent_mut() else { continue };
                let Some(&local) = l2r.get_by_right(ent) else { continue };
                *ent = local;
                do_writer.write(Do { event });
            }
            _ => {
                panic!("Unexpected message on Unreliable channel: {:?}", message);
            }
        }
    }
}

/// Sends what the client asks of the server, each entity named by the
/// server's id for it. Only the events listed here cross the wire.
pub fn send_try(
    conn: Option<ResMut<ClientNet>>,
    mut reader: MessageReader<Try>,
    l2r: Res<EntityMap>,
) {
    let Some(mut conn) = conn else {
        reader.clear();
        return;
    };
    for message in reader.read() {
        let mut event = message.event.clone();
        match event {
            // A spawn is asked for by the server's own id, and these are about no entity
            Event::Spawn { .. } | Event::Play | Event::Leave | Event::Ping { .. } => {}
            Event::Input { .. }
            | Event::UseAbility { .. }
            | Event::View { .. }
            | Event::Gather { .. }
            | Event::Take { .. }
            | Event::CloseLoot { .. }
            | Event::Drop { .. }
            | Event::Wear { .. }
            | Event::Dismiss { .. }
            | Event::Teleport { .. }
            | Event::SpawnParty { .. }
            | Event::RespecAttributes { .. } => {
                // One the server never told the client of has nothing to ask
                let Some(ent) = event.ent_mut() else { continue };
                let Some(&remote) = l2r.get_by_left(ent) else { continue };
                *ent = remote;
                // An ability's target goes by the server's id too, or as none
                if let Event::UseAbility { target, .. } = &mut event {
                    *target = target.and_then(|target| l2r.get_by_left(&target).copied());
                }
            }
            _ => continue,
        }
        conn.send_reliable(DefaultChannel::ReliableOrdered, bincode::serde::encode_to_vec(Try { event }, bincode::config::legacy()).unwrap());
    }
}

/// Handle Pong response to refine time sync with measured network latency
pub fn handle_pong(
    mut reader: MessageReader<Do>,
    mut server: ResMut<crate::resources::Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Do { event: Event::Pong { client_time } } = message else { continue };
        let client_time = *client_time;
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
    mut server: ResMut<crate::resources::Server>,
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