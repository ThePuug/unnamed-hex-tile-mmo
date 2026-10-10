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
        Event::Respawn { .. } => "Respawn",
        Event::Incremental { component, .. } => match component {
            Component::Loc(_) => "Inc:Loc",
            Component::Heading(_) => "Inc:Heading",
            Component::Health(_) => "Inc:Health",
            Component::Endurance(_) => "Inc:Endurance",
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
        Event::Den { .. } => "Den",
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
    time: Res<Time>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let Some(mut conn) = conn else { return };
    let _t = client_timers.scope("write_do");
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
                // Health/Endurance will be inserted by Incremental events from server
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
                // The local player has an input queue; it falls as any
                // actor does, but keeps its entity: the server respawns it,
                // and it stands again
                let local = l2r.get_by_right(&ent).copied().filter(|local| buffers.get(local).is_some());
                if let Some(local_ent) = local {
                    commands.entity(local_ent).try_insert(crate::components::DeathMarker { death_time: time.elapsed() });
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
            // A tile's cover, or a den, is about no entity here
            Do { event: Event::CoverChanged { ent: _, q, r, cover } } => {
                do_writer.write(Do { event: Event::CoverChanged { ent: Entity::PLACEHOLDER, q, r, cover } });
            }
            Do { event: Event::Den { ent: _, at, den } } => {
                do_writer.write(Do { event: Event::Den { ent: Entity::PLACEHOLDER, at, den } });
            }
            Do { event: event @ Event::Pong { .. } } => {
                do_writer.write(Do { event });
            }
            // Every other is about an entity, known here by an id of its
            // own; one the client has not spawned is asked for. ChunkData
            // arrives on ReliableUnordered, handled below.
            Do { event: mut event @ (
                Event::Confirm { .. }
                | Event::Respawn { .. }
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
            Event::Spawn { .. } | Event::Play | Event::Leave | Event::Ping => {}
            Event::Input { .. }
            | Event::UseAbility { .. }
            | Event::View { .. }
            | Event::Gather { .. }
            | Event::Take { .. }
            | Event::CloseLoot { .. }
            | Event::Drop { .. }
            | Event::Wear { .. }
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

/// Re-sets the clock from the server's game world time a Pong carries
pub fn handle_pong(
    mut reader: MessageReader<Do>,
    mut server: ResMut<crate::resources::Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Do { event: Event::Pong { dt } } = message else { continue };
        server.sync(*dt, time.elapsed().as_millis());
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
        try_writer.write(Try { event: Event::Ping });
    }
}

/// Takes the trip to the server, half the round trip renet measures on
/// every packet, once it has measured one
pub fn track_latency(conn: Option<Res<ClientNet>>, mut server: ResMut<crate::resources::Server>) {
    let Some(conn) = conn else { return };
    let rtt = conn.rtt();
    if !rtt.is_zero() {
        server.latency = rtt.as_millis() / 2;
    }
}

/// Holds the clock's lead by how far ahead of its moment the server says
/// each of the local player's presses arrived. An auto-attack is no press:
/// it comes due on the server's clock, which stamps it as it arrives
/// whatever the lead, and counted it would push the lead out without end.
pub fn handle_arrived(
    mut reader: MessageReader<Do>,
    mut server: ResMut<crate::resources::Server>,
    own: Query<(), With<common_bevy::components::Actor>>,
) {
    for message in reader.read() {
        let Do { event: Event::UseAbility { ent, ability, at, arrived, .. } } = message else { continue };
        if own.contains(*ent) && *ability != AbilityType::AutoAttack {
            server.arrived(at.as_millis() as i64 - arrived.as_millis() as i64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_the_players_own_presses_hold_the_lead() {
        let mut app = App::new();
        app.add_message::<Do>();
        app.insert_resource(crate::resources::Server::default());
        app.add_systems(Update, handle_arrived);
        let player = app.world_mut().spawn(common_bevy::components::Actor).id();
        let other = app.world_mut().spawn_empty().id();
        let used = |ent, ability, early: u64| Do { event: Event::UseAbility {
            ent, ability, target: None, at: Duration::from_millis(1_000 + early), arrived: Duration::from_millis(1_000),
        }};
        let margin = |app: &App| app.world().resource::<crate::resources::Server>().margin;

        let before = margin(&app);
        for _ in 0..20 {
            app.world_mut().write_message(used(player, AbilityType::AutoAttack, 0));
            app.world_mut().write_message(used(other, AbilityType::Parry, 0));
            app.update();
        }
        assert_eq!(margin(&app), before, "an auto-attack, or another's press, moves nothing");

        app.world_mut().write_message(used(player, AbilityType::Parry, 0));
        app.update();
        assert_ne!(margin(&app), before, "the player's own press does");
    }
}