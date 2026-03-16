use std::net::UdpSocket;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ConnectionConfig, netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig}};
use qrz::*;
use ::renet::{DefaultChannel, ServerEvent};

use common_bevy::{
    chunk::PlayerDiscoveryState,
    components::{ *,
        behaviour::*,
        entity_type::{ *,
            actor::*,
        },
        keybits::*,
        reaction_queue::*,
        resources::*,
        tier_lock::TierLock,
    },
    message::{ Component, Event, * },
    plugins::nntree::*,
    resources::*,
    systems::combat::{
        resources as resource_calcs,
    },
};
use crate::*;


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
    // Chunk data goes on ReliableUnordered — increase its buffer for the initial burst
    let mut config = ConnectionConfig::default();
    config.server_channels_config[1].max_memory_usage_bytes = 50 * 1024 * 1024;
    let server = RenetServer::new(config);

    (server, transport)
}

#[allow(clippy::too_many_arguments)]
pub fn do_manage_connections(
    trigger: On<RenetServerEvent>,
    mut commands: Commands,
    mut conn: ResMut<RenetServer>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    mut loaded_by_query: Query<&mut common_bevy::components::loaded_by::LoadedBy>,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    spawn_point: Res<common_bevy::components::resources::SpawnPoint>,
) {
    {
        let event = &trigger.event().0;
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let typ = EntityType::Actor(ActorImpl::new(
                    Origin::Evolved,
                    Approach::Direct,
                    Resilience::Vital,
                    ActorIdentity::Player));
                let qrz = spawn_point.0;
                let loc = Loc::new(qrz);
                let attrs = ActorAttributes::new(
                    -3, 4, 0,
                    1, 0, 0,
                    -3, 4, 0,
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
                let queue_capacity = attrs.window_size();
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
                    gcd::Gcd::new(),
                    LastAutoAttack::default(),
                    PlayerDiscoveryState::default(),
                    TierLock::new(),
                    common_bevy::components::target::Target::default(),
                )).id();
                commands.entity(ent).insert((
                    NearestNeighbor::new(ent, loc),
                    common_bevy::components::loaded_by::LoadedBy::default(),
                    common_bevy::components::AttackRange::default(),
                ));

                // init input buffer for client
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 }].into() }));

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Init { ent, dt }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // Send own Spawn + component states directly to connecting client
                // AOI will handle discovering nearby entities via Changed<Loc>
                use crate::systems::world::generate_actor_spawn_events;
                let spawn_events = generate_actor_spawn_events(
                    ent,
                    typ,
                    qrz,
                    Some(attrs),
                    Some(&PlayerControlled),
                    None,
                    Some(&health),
                    Some(&stamina),
                    Some(&mana),
                    Some(&combat_state),
                );

                for event in spawn_events {
                    let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }

                // Write Spawn to message bus so do_spawn_discover triggers initial chunk discovery
                writer.write(Do { event: Event::Spawn { ent, typ, qrz, attrs: Some(attrs) } });

                lobby.insert(*client_id, ent);
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                info!("Player {} disconnected: {}", client_id, reason);
                let ent = lobby.remove_by_left(client_id).unwrap().1;
                buffers.remove(&ent);

                // Send Despawn to all players who had this entity loaded
                if let Ok(loaded_by) = loaded_by_query.get(ent) {
                    for &player_ent in &loaded_by.players {
                        if let Some(player_client_id) = lobby.get_by_right(&player_ent) {
                            let message = bincode::serde::encode_to_vec(
                                Do { event: Event::Despawn { ent }},
                                bincode::config::legacy()).unwrap();
                            conn.send_message(*player_client_id, DefaultChannel::ReliableOrdered, message);
                        }
                    }
                }

                // Remove disconnected player from all LoadedBy sets
                for mut loaded_by in loaded_by_query.iter_mut() {
                    loaded_by.players.remove(&ent);
                }

                commands.entity(ent).despawn();
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
            let (message, _): (Try, _) = bincode::serde::borrow_decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
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
                Try { event: Event::UseAbility { ent: _, ability, target } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::UseAbility { ent, ability, target }});
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
                Try { event: Event::RespecAttributes { ent: _, might_grace_axis, might_grace_spectrum, might_grace_shift, vitality_focus_axis, vitality_focus_spectrum, vitality_focus_shift, instinct_presence_axis, instinct_presence_spectrum, instinct_presence_shift } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::RespecAttributes { ent, might_grace_axis, might_grace_spectrum, might_grace_shift, vitality_focus_axis, vitality_focus_spectrum, vitality_focus_shift, instinct_presence_axis, instinct_presence_spectrum, instinct_presence_shift }});
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    mut conn: ResMut<RenetServer>,
    mut reader: MessageReader<Do>,
    loaded_by_query: Query<&common_bevy::components::loaded_by::LoadedBy>,
    lobby: Res<Lobby>,
) {
    for message in reader.read() {
        match &message.event {
            // Spawn events are handled by the AOI system — skip here
            Event::Spawn { .. } => {}
            Event::Incremental { ent, component } => {
                let ent = *ent;
                let component = *component;
                if matches!(component, Component::KeyBits(_)) { continue; }

                // Send to owning client
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Incremental { ent, component }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }

                // Send to all players who have this entity loaded (skip owner to avoid duplicate)
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    let Some(client_id) = lobby.get_by_right(&player_ent) else { continue; };
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Incremental { ent, component }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Event::Despawn { ent } => {
                let ent = *ent;
                // Send despawn to all players who have this entity loaded
                let Ok(loaded_by) = loaded_by_query.get(ent) else {
                    warn!("SERVER: Cannot send Despawn for entity {:?} - no LoadedBy component", ent);
                    continue;
                };
                for &player_ent in &loaded_by.players {
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Despawn { ent }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::ChunkData { ent, .. } => {
                let ent = *ent;
                // Chunks are independent — ReliableUnordered avoids head-of-line blocking
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let serialized = bincode::serde::encode_to_vec(
                        message,
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableUnordered, serialized);
                }
            }
            Event::InsertThreat { ent, threat } => {
                let ent = *ent;
                let threat = *threat;
                // Send to owning client (player receiving threat needs to see it)
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::InsertThreat { ent, threat }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::InsertThreat { ent, threat }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::ApplyDamage { ent, damage, source } => {
                let ent = *ent;
                let damage = *damage;
                let source = *source;
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::ApplyDamage { ent, damage, source }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::ApplyDamage { ent, damage, source }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::ClearQueue { ent, clear_type } => {
                let ent = *ent;
                let clear_type = *clear_type;
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::ClearQueue { ent, clear_type }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::ClearQueue { ent, clear_type }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::Gcd { ent, typ } => {
                let ent = *ent;
                let typ = *typ;
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Gcd { ent, typ }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Gcd { ent, typ }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::AbilityFailed { ent, reason } => {
                let ent = *ent;
                let reason = *reason;
                // Send ability failure only to the caster
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::AbilityFailed { ent, reason }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
            }
            Event::UseAbility { ent, ability, target } => {
                let ent = *ent;
                let ability = *ability;
                let target = *target;
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::UseAbility { ent, ability, target }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                }
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if player_ent == ent { continue; }
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::UseAbility { ent, ability, target }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                }
            }
            Event::MovementIntent { ent, destination, duration_ms } => {
                let ent = *ent;
                let destination = *destination;
                let duration_ms = *duration_ms;
                // ADR-011: Movement intent via Unreliable channel for client-side prediction
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue; };
                for &player_ent in &loaded_by.players {
                    if let Some(client_id) = lobby.get_by_right(&player_ent) {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::MovementIntent { ent, destination, duration_ms }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::Unreliable, message);
                    }
                }
            }
            Event::RespecAttributes { ent, might_grace_axis, might_grace_spectrum, might_grace_shift, vitality_focus_axis, vitality_focus_spectrum, vitality_focus_shift, instinct_presence_axis, instinct_presence_spectrum, instinct_presence_shift } => {
                let ent = *ent;
                let might_grace_axis = *might_grace_axis;
                let might_grace_spectrum = *might_grace_spectrum;
                let might_grace_shift = *might_grace_shift;
                let vitality_focus_axis = *vitality_focus_axis;
                let vitality_focus_spectrum = *vitality_focus_spectrum;
                let vitality_focus_shift = *vitality_focus_shift;
                let instinct_presence_axis = *instinct_presence_axis;
                let instinct_presence_spectrum = *instinct_presence_spectrum;
                let instinct_presence_shift = *instinct_presence_shift;
                // Send respec confirmation only to the owning client
                if let Some(client_id) = lobby.get_by_right(&ent) {
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::RespecAttributes { ent, might_grace_axis, might_grace_spectrum, might_grace_shift, vitality_focus_axis, vitality_focus_spectrum, vitality_focus_shift, instinct_presence_axis, instinct_presence_spectrum, instinct_presence_shift }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
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
    for message in reader.read() {
        if let Do { event: Event::Despawn { ent } } = message {
            let ent = *ent;
            // Don't despawn entities with RespawnTimer (dead players waiting to respawn)
            if respawn_query.get(ent).is_ok() {
                continue;
            }
            commands.entity(ent).despawn();
        }
    }
}