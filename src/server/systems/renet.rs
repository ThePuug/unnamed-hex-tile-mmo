use bevy::prelude::*;
use bevy_renet::netcode::{ServerAuthentication, ServerConfig};
use qrz::*;
use ::renet::ServerEvent;

use crate::{ common::{
        chunk::PlayerDiscoveryState,
        components::{ *,
            behaviour::*,
            entity_type::{ *,
                actor::*,
            },
            heading::*,
            keybits::*,
            reaction_queue::*,
            resources::*,
            tier_lock::TierLock,
        },
        message::{ Component, Event, * },
        plugins::nntree::*,
        resources::*,
        systems::combat::{
            queue as queue_calcs,
            resources as resource_calcs,
        },
    }, *
};

// ADR-011 Phase 2: Relevance radius for movement intent broadcasting (30 hexes, larger than FOV)
// Note: Uses squared Euclidean distance as approximation for performance (R-Tree spatial index)
// 30 hex distance ≈ 30 tiles in practice given typical hex grid spacing
const INTENT_RELEVANCE_RADIUS_SQ: i32 = 30 * 30;

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

#[allow(clippy::too_many_arguments)]
pub fn do_manage_connections(
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut reader: MessageReader<ServerEvent>,
    mut writer: MessageWriter<Do>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    query: Query<(&Loc, &EntityType, Option<&ActorAttributes>, Option<&Health>, Option<&Stamina>, Option<&Mana>, Option<&CombatState>, Option<&PlayerControlled>, Option<&Heading>)>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    nntree: Res<NNTree>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let typ = EntityType::Actor(ActorImpl::new(
                    Origin::Evolved,
                    Approach::Direct,
                    Resilience::Vital,
                    ActorIdentity::Player));
                let qrz = Qrz { q: 0, r: 0, z: 4 };
                let loc = Loc::new(qrz);
                // Level 10: Full spectrum distribution (3+4+3=10 points)
                // Balanced axis (0) allows players to shift in any direction for skill balance testing
                // Focus spectrum = 4 provides 40% investment ratio → 2 queue slots
                // New system: raw investment counts (axis ×10, spectrum ×7 for reach)
                let attrs = ActorAttributes::new(
                    0, 3, 0,       // might_grace: 0 axis, 3 spectrum, 0 shift
                    0, 4, 0,       // vitality_focus: 0 axis, 4 spectrum, 0 shift
                    0, 3, 0,       // instinct_presence: 0 axis, 3 spectrum, 0 shift
                );
                // Calculate initial resources from attributes
                let max_health = attrs.max_health();
                let max_stamina = resource_calcs::calculate_max_stamina(&attrs);
                let max_mana = resource_calcs::calculate_max_mana(&attrs);
                let stamina_regen = resource_calcs::calculate_stamina_regen_rate(&attrs);
                let mana_regen = resource_calcs::calculate_mana_regen_rate(&attrs);

                let health = Health {
                    state: max_health,
                    step: max_health,
                    max: max_health,
                };
                let stamina = Stamina {
                    state: max_stamina,
                    step: max_stamina,
                    max: max_stamina,
                    regen_rate: stamina_regen,
                    last_update: time.elapsed(),
                };
                let mana = Mana {
                    state: max_mana,
                    step: max_mana,
                    max: max_mana,
                    regen_rate: mana_regen,
                    last_update: time.elapsed(),
                };
                let combat_state = CombatState {
                    in_combat: false,
                    last_action: time.elapsed(),
                };
                // Initialize reaction queue with capacity based on Focus attribute
                let queue_capacity = queue_calcs::calculate_queue_capacity(&attrs);
                let reaction_queue = ReactionQueue::new(queue_capacity);

                let ent = commands.spawn((
                    typ,
                    loc,
                    Behaviour::Controlled,
                    PlayerControlled,
                    attrs,
                    health,
                    stamina,
                    mana,
                    combat_state,
                    reaction_queue,
                    gcd::Gcd::new(),  // GCD component for cooldown tracking
                    LastAutoAttack::default(),  // ADR-009: Track auto-attack cooldown
                    PlayerDiscoveryState::default(),
                    TierLock::new(),  // ADR-010 Phase 1: Tier lock targeting
                    crate::common::components::target::Target::default(),  // For unified targeting system
                )).id();
                commands.entity(ent).insert(NearestNeighbor::new(ent, loc));

                // Broadcast Spawn to nearby players (goes through send_do system for proximity filtering)
                writer.write(Do { event: Event::Spawn { ent, typ, qrz, attrs: Some(attrs) }});
                // Broadcast PlayerControlled to nearby players so they recognize this as an ally
                writer.write(Do { event: Event::Incremental { ent, component: Component::PlayerControlled(PlayerControlled) }});

                // init input buffer for client
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 }].into() }));

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Init { ent, dt }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // Send Spawn + all component states directly to connecting client
                // Can't use event writer here because send_do relies on NNTree proximity,
                // and this entity isn't in NNTree yet
                // Use shared helper to ensure consistency with other player spawns
                use crate::server::systems::world::generate_actor_spawn_events;
                let spawn_events = generate_actor_spawn_events(
                    ent,
                    typ,
                    qrz,
                    Some(attrs),
                    Some(&PlayerControlled),
                    None,  // No heading initially
                    Some(&health),
                    Some(&stamina),
                    Some(&mana),
                    Some(&combat_state),
                );

                for event in spawn_events {
                    let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }

                // spawn nearby actors
                for other in nntree.locate_within_distance(loc, 20*20) {
                    let (&loc, &typ, attrs, health, stamina, mana, combat_state, player_controlled, heading) = query.get(other.ent).unwrap();

                    // Send Spawn + all actor components using shared helper to ensure consistency
                    let spawn_events = generate_actor_spawn_events(
                        other.ent,
                        typ,
                        *loc,
                        attrs.copied(),
                        player_controlled,
                        heading,
                        health,
                        stamina,
                        mana,
                        combat_state,
                    );

                    for event in spawn_events {
                        let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
                lobby.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.remove_by_left(&client_id).unwrap().1;
                buffers.remove(&ent);
                commands.entity(ent).despawn();
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Despawn { ent }}, 
                    bincode::config::legacy()).unwrap();
                conn.broadcast_message(DefaultChannel::ReliableOrdered, message);
            }
        }
    }
 }

pub fn write_try(
    mut writer: MessageWriter<Try>,
    mut conn: ResMut<RenetServer>,
    lobby: Res<Lobby>,
) {
    for client_id in conn.clients_id() {
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let (message, _) = bincode::serde::borrow_decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
            match message {
                Try { event: Event::Incremental { component: Component::KeyBits(keybits), .. } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::Incremental { ent, component: Component::KeyBits(keybits) }});
                }
                Try { event: Event::Gcd { typ, .. } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::Gcd { ent, typ }});
                }
                Try { event: Event::Spawn { ent, .. } } => {
                    writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                }
                Try { event: Event::UseAbility { ent: _, ability, target_loc } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::UseAbility { ent, ability, target_loc }});
                }
                Try { event: Event::Ping { client_time } } => {
                    // Immediately respond with Pong (echo client timestamp)
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Pong { client_time }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(client_id, DefaultChannel::ReliableOrdered, message);
                }
                Try { event: Event::Dismiss { ent: _ } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::Dismiss { ent }});
                }
                Try { event: Event::SetTierLock { ent: _, tier } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::SetTierLock { ent, tier }});
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    query: Query<&Loc>,
    mut conn: ResMut<RenetServer>,
    mut reader: MessageReader<Do>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { ent, typ, qrz, attrs } } => {
                // Always send Spawn to the owning client (for respawns, initial spawns, etc.)
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { ent, typ, qrz, attrs }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }

                // Also send to nearby players
                // Range must be at least 35 tiles to cover all chunks within FOV_CHUNK_RADIUS=2
                // (5x5 chunks, worst-case actor at corner chunk edge ≈ 34 tiles from center)
                // PLUS buffer for Z-level differences (NNTree uses 3D distance = hex_dist + |z_diff|)
                // Max terrain Z ~20, player spawns at Z=4, so max Z diff ~16 → use 55 tile radius
                let nearby = nntree.locate_within_distance(Loc::new(qrz), 55*55).collect::<Vec<_>>();
                for other in nearby {
                    let Some(client_id) = lobby.get_by_right(&other.ent) else {
                        continue;
                    };
                    // Skip if already sent above
                    if other.ent == ent { continue; }
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { ent, typ, qrz, attrs }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::Incremental { ent, component } } => {
                match component {
                    Component::KeyBits(_) => continue,
                    _ => {}
                }

                // Always send Incremental to owning client first (for respawns, etc.)
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Incremental { ent, component }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }

                // Also send to nearby players (skip owning client to avoid duplicate)
                // Entity might have been despawned in the same frame, so handle gracefully
                // Range must match Spawn events (55 tiles) to ensure component updates reach all discovering players
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 55*55) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        // Skip if we already sent to this client above
                        if other.ent == ent { continue; }
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent, component }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::Despawn { ent } } => {
                // Send despawn event to players who might have this entity rendered
                // Use a large radius (70 tiles) to ensure despawns reach all players within despawn_distance (60) with buffer
                if let Ok(&loc) = query.get(ent) {
                    for other in nntree.locate_within_distance(loc, 70*70) {
                        if let Some(client_id) = lobby.get_by_right(&other.ent) {
                            let message = bincode::serde::encode_to_vec(
                                Do { event: Event::Despawn { ent }},
                                bincode::config::legacy()).unwrap();
                            conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                        }
                    }
                    // Note: Actual despawning happens in cleanup_despawned system (PostUpdate)
                } else {
                    warn!("SERVER: Cannot send Despawn for entity {:?} - no Loc component", ent);
                }
            }
            Do { event: Event::ChunkData { ent, chunk_id, tiles } } => {
                // Send chunk data directly to the specific player
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::ChunkData { ent, chunk_id, tiles }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::InsertThreat { ent, threat } } => {
                // Send threat insertion to nearby players
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::InsertThreat { ent, threat }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::ApplyDamage { ent, damage, source } } => {
                // Send damage application to nearby players
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::ApplyDamage { ent, damage, source }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::ClearQueue { ent, clear_type } } => {
                // Send queue clear to nearby players
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::ClearQueue { ent, clear_type }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::Gcd { ent, typ } } => {
                // Send GCD to nearby players
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Gcd { ent, typ }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::AbilityFailed { ent, reason } } => {
                // Send ability failure only to the caster
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::AbilityFailed { ent, reason }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Do { event: Event::UseAbility { ent, ability, target_loc } } => {
                // Send ability success to nearby players (ADR-012: client applies recovery/synergies)
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::UseAbility { ent, ability, target_loc }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Do { event: Event::MovementIntent { ent, destination, duration_ms } } => {
                // ADR-011 Phase 2: Send movement intent to nearby players for client-side prediction
                // Uses Unreliable channel + relevance filtering for optimal bandwidth/latency
                // Dropped packets are acceptable - next intent supersedes previous (self-correcting)
                let Ok(&loc) = query.get(ent) else {
                    warn!("Cannot send MovementIntent for entity {:?} - no Loc component", ent);
                    continue;
                };

                for other in nntree.locate_within_distance(loc, INTENT_RELEVANCE_RADIUS_SQ) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::MovementIntent { ent, destination, duration_ms }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::Unreliable, message);
                    }
                }

            }
            _ => {}
        }
    }
}

/// System that actually despawns entities after network messages have been sent
/// This runs in PostUpdate after send_do to avoid race conditions
pub fn cleanup_despawned(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    respawn_query: Query<&RespawnTimer>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Despawn { ent } } = message {
            // Don't despawn entities with RespawnTimer (dead players waiting to respawn)
            if respawn_query.get(ent).is_ok() {
                continue;
            }
            commands.entity(ent).despawn();
        }
    }
}