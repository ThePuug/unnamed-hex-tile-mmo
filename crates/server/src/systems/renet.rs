use bevy::prelude::*;
use ::renet::DefaultChannel;
use qrz::*;

use common_bevy::{
    chunk::PlayerDiscoveryState,
    components::{ *,
        behaviour::*,
        entity_type::{ *,
            actor::*,
        },
        equipment::{Equipment, Inventory},
        keybits::*,
        reaction_queue::*,
        resources::*,
        tier_lock::TierLock,
    },
    message::{ Event, * },
    plugins::nntree::*,
    resources::*,
    systems::combat::{
        resources as resource_calcs,
    },
};
use crate::*;


use crate::network::{ServerNet, NetServerEvent};

/// A connected client entering the world or leaving it. Connection is not
/// presence: a client connects to its character select, enters when it
/// asks to play, and may leave and enter again on the one connection. A
/// disconnect leaves the world too.
///
/// A client may instead view an actor: its lobby entry is then that actor,
/// which streams the world to it and sends it what an owner sees, while
/// nothing it sends controls it. Its character left the world to view, and
/// leaving the view leaves the actor as it was.
#[derive(Event, Debug)]
pub enum Presence {
    Enter { client_id: ::renet::ClientId },
    Leave { client_id: ::renet::ClientId },
    View { client_id: ::renet::ClientId, ent: Entity },
}

pub fn do_manage_connections(
    trigger: On<NetServerEvent>,
    mut commands: Commands,
) {
    match trigger.event() {
        NetServerEvent::ClientConnected { client_id } => {
            info!("Client {} connected", client_id);
        }
        NetServerEvent::ClientDisconnected { client_id, reason } => {
            info!("Client {} disconnected: {:?}", client_id, reason);
            commands.trigger(Presence::Leave { client_id: *client_id });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn do_presence(
    trigger: On<Presence>,
    mut commands: Commands,
    mut conn: ResMut<ServerNet>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    mut guards: ResMut<crate::systems::input::InputGuards>,
    mut loaded_by_query: Query<&mut common_bevy::components::loaded_by::LoadedBy>,
    mut writer: MessageWriter<Do>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    spawn_point: Res<common_bevy::components::resources::SpawnPoint>,
    characters: Query<(), With<PlayerControlled>>,
    actors: Query<(&EntityType, &Loc, Option<&ActorAttributes>), Without<RespawnTimer>>,
) {
    match trigger.event() {
        Presence::Enter { client_id } => {
            let client_id = *client_id;
                if lobby.contains_left(&client_id) {
                    return;
                }
                info!("Player {} entered the world", client_id);
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
                    max: max_health,
                };
                let stamina = Stamina {
                    state: max_stamina,
                    max: max_stamina,
                    regen_rate: stamina_regen,
                    last_update: time.elapsed(),
                };
                let mana = Mana {
                    state: max_mana,
                    max: max_mana,
                    regen_rate: mana_regen,
                    last_update: time.elapsed(),
                };
                let combat_state = CombatState {
                    in_combat: false,
                    last_action: time.elapsed(),
                };
                // Initialize reaction queue with the window its Awareness sees
                let queue_capacity = attrs.window_size();
                let reaction_queue = ReactionQueue::new(queue_capacity);
                let equipment = Equipment::starting_outfit();
                let bag = Inventory::wearing(&equipment);

                let ent = commands.spawn((
                    typ,
                    loc,
                    (PlayerControlled, common_bevy::components::behaviour::Side::PLAYERS),
                    attrs,
                    health,
                    stamina,
                    mana,
                    combat_state,
                    reaction_queue,
                    PlayerDiscoveryState::default(),
                    TierLock::new(),
                    common_bevy::components::target::Target::default(),
                )).id();
                commands.entity(ent).insert((
                    NearestNeighbor::new(ent, loc),
                    common_bevy::components::loaded_by::LoadedBy::default(),
                    common_bevy::components::AttackRange::default(),
                    equipment,
                    bag.clone(),
                ));

                // init input buffer for client
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 }].into(), ..default() }));

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Init { ent, dt }},
                    bincode::config::legacy()).unwrap();
                conn.send_reliable(client_id, DefaultChannel::ReliableOrdered, message);

                // Send own Spawn + component states directly to connecting client
                // AOI will handle discovering nearby entities via Changed<Loc>
                use crate::systems::world::generate_actor_spawn_events;
                let spawn_events = generate_actor_spawn_events(
                    ent,
                    typ,
                    qrz,
                    Some(attrs),
                    Some(&PlayerControlled),
                    Some(&common_bevy::components::behaviour::Side::PLAYERS),
                    None,
                    Some(&health),
                    Some(&stamina),
                    Some(&mana),
                    Some(&combat_state),
                    Some(&equipment),
                );

                for event in spawn_events {
                    let message = bincode::serde::encode_to_vec(event, bincode::config::legacy()).unwrap();
                    conn.send_reliable(client_id, DefaultChannel::ReliableOrdered, message);
                }

                // The bag goes to its owner only, after Init so the client has its entity
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Inventory { ent, bag }},
                    bincode::config::legacy()).unwrap();
                conn.send_reliable(client_id, DefaultChannel::ReliableOrdered, message);

                // Write Spawn to message bus so do_spawn_discover triggers initial chunk discovery
                writer.write(Do { event: Event::Spawn { ent, typ, qrz, attrs: Some(attrs) } });

                lobby.insert(client_id, ent);
            }
            Presence::Leave { client_id } => {
                let client_id = *client_id;
                let Some((_, ent)) = lobby.remove_by_left(&client_id) else { return };
                leave(client_id, ent, characters.contains(ent), &mut commands, &mut conn, &lobby, &mut buffers, &mut guards, &mut loaded_by_query);
            }
            Presence::View { client_id, ent: viewed } => {
                let (client_id, viewed) = (*client_id, *viewed);
                let Ok((&typ, &loc, attrs)) = actors.get(viewed) else { return };
                if let Some((_, ent)) = lobby.remove_by_left(&client_id) {
                    leave(client_id, ent, characters.contains(ent), &mut commands, &mut conn, &lobby, &mut buffers, &mut guards, &mut loaded_by_query);
                }
                info!("Client {} views {}", client_id, viewed);
                lobby.insert(client_id, viewed);
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::View { ent: viewed }},
                    bincode::config::legacy()).unwrap();
                conn.send_reliable(client_id, DefaultChannel::ReliableOrdered, message);
                // Streaming starts from the actor as it does from a character
                // entering: `do_spawn_discover` sends the chunks round it,
                // and AOI from its `Loc` changing, so what stands round it comes too
                commands.entity(viewed).insert((PlayerDiscoveryState::default(), loc));
                writer.write(Do { event: Event::Spawn { ent: viewed, typ, qrz: *loc, attrs: attrs.copied() } });
            }
        }
}


/// Takes `ent`, the lobby entry `client_id` just left, out of the client's
/// hands: a character leaves the world, despawned for everyone who saw it;
/// a viewed actor stays as it was, only no longer streaming to the client.
#[allow(clippy::too_many_arguments)]
fn leave(
    client_id: ::renet::ClientId,
    ent: Entity,
    character: bool,
    commands: &mut Commands,
    conn: &mut ServerNet,
    lobby: &Lobby,
    buffers: &mut InputQueues,
    guards: &mut crate::systems::input::InputGuards,
    loaded_by_query: &mut Query<&mut common_bevy::components::loaded_by::LoadedBy>,
) {
    info!("Player {} left the world", client_id);
    // What is queued for the character goes with it: a chunk
    // arriving after the client left would stand in its map
    // with nothing to evict it.
    conn.drop_queued(client_id);

    // Nothing is loaded on the client's behalf any more
    for mut loaded_by in loaded_by_query.iter_mut() {
        loaded_by.players.remove(&ent);
    }

    if !character {
        commands.entity(ent).try_remove::<(
            PlayerDiscoveryState,
            crate::systems::actor::VisibleChunkCache,
            crate::systems::summary::VisibleSummaryCache,
        )>();
        return;
    }
    buffers.remove(&ent);
    guards.0.remove(&ent);

    // Send Despawn to all players who had this entity loaded
    if let Ok(loaded_by) = loaded_by_query.get(ent) {
        let bytes = bincode::serde::encode_to_vec(
            Do { event: Event::Despawn { ent }},
            bincode::config::legacy()).unwrap();
        for &player_ent in &loaded_by.players {
            if let Some(player_client_id) = lobby.get_by_right(&player_ent) {
                conn.send_reliable(*player_client_id, DefaultChannel::ReliableOrdered, bytes.clone());
            }
        }
    }

    commands.entity(ent).despawn();
}

/// Reads what each client asks for onto the message bus.
///
/// What a client asks of its character is asked for the character the
/// lobby holds for it, whatever entity the client named, and for nothing
/// while it views an actor instead. Only the events listed here are taken
/// from a client; every other is the server's own.
pub fn write_try(
    mut commands: Commands,
    mut writer: MessageWriter<Try>,
    mut conn: ResMut<ServerNet>,
    lobby: Res<Lobby>,
    characters: Query<(), With<PlayerControlled>>,
) {
    for client_id in conn.clients_id() {
        // What the client controls: its character, never an actor it views
        let character = lobby.get_by_left(&client_id).copied().filter(|&ent| characters.contains(ent));
        while let Some(serialized) = conn.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let (Try { mut event }, _): (Try, _) = bincode::serde::borrow_decode_from_slice(&serialized, bincode::config::legacy()).unwrap();
            match event {
                Event::Ping { client_time } => {
                    // Immediately respond with Pong (echo client timestamp)
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Pong { client_time }},
                        bincode::config::legacy()).unwrap();
                    conn.send_reliable(client_id, DefaultChannel::ReliableOrdered, message);
                }
                Event::Play => commands.trigger(Presence::Enter { client_id }),
                Event::Leave => commands.trigger(Presence::Leave { client_id }),
                Event::View { ent } => commands.trigger(Presence::View { client_id, ent }),
                // An entity the client was told of and has not spawned, named by the server's id
                Event::Spawn { ent, .. } => {
                    writer.write(Try { event: Event::Spawn { ent, typ: EntityType::Unset, qrz: Qrz::default(), attrs: None }});
                }
                // An admin's party stands ahead of whichever actor it names
                Event::SpawnParty { .. } => {
                    writer.write(Try { event });
                }
                // An auto-attack is the server's to time, never a client's to ask for
                Event::UseAbility { ability: AbilityType::AutoAttack, .. } => {}
                Event::Input { .. }
                | Event::Gather { .. }
                | Event::Take { .. }
                | Event::CloseLoot { .. }
                | Event::Drop { .. }
                | Event::UseAbility { .. }
                | Event::Dismiss { .. }
                | Event::SetTierLock { .. }
                | Event::RespecAttributes { .. }
                | Event::Wear { .. }
                | Event::Teleport { .. } => {
                    let (Some(character), Some(ent)) = (character, event.ent_mut()) else { continue };
                    *ent = character;
                    writer.write(Try { event });
                }
                _ => {}
            }
        }
    }
}

/// Who a `Do` about an entity is sent to, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// Every client that has the entity loaded and the one whose lobby
    /// entry it is, in order
    Seen,
    /// The client whose lobby entry the entity is, in order
    Owner,
    /// That client, in no order: chunks and summaries are independent, and
    /// one held back would hold back the rest
    OwnerUnordered,
    /// Every client that simulates the entity, unreliably: the latest
    /// wins, and `Loc` repairs a loss
    Moving,
}

/// The entity a `Do` is about and the route it takes, or none for an event
/// that stays on the server. A `Spawn` is sent where it is made, to the
/// clients it is for.
fn route(event: &Event) -> Option<(Entity, Route)> {
    match event {
        Event::Incremental { ent, .. }
        | Event::Despawn { ent }
        | Event::Activity { ent, .. }
        | Event::InsertThreat { ent, .. }
        | Event::ApplyDamage { ent, .. }
        | Event::ClearQueue { ent, .. }
        | Event::UseAbility { ent, .. } => Some((*ent, Route::Seen)),
        Event::AbilityFailed { ent, .. }
        | Event::Confirm { ent, .. }
        | Event::RespecAttributes { ent, .. }
        | Event::Inventory { ent, .. }
        | Event::CoverChanged { ent, .. }
        | Event::Loot { ent, .. } => Some((*ent, Route::Owner)),
        Event::ChunkData { ent, .. }
        | Event::EvictChunks { ent, .. }
        | Event::SummaryBatch { ent, .. } => Some((*ent, Route::OwnerUnordered)),
        Event::MovementIntent { ent, .. } | Event::Displace { ent, .. } => Some((*ent, Route::Moving)),
        _ => None,
    }
}

/// Sends every `Do` on the bus to the clients its route names, encoded once.
pub fn send_do(
    mut conn: ResMut<ServerNet>,
    mut reader: MessageReader<Do>,
    loaded_by_query: Query<&common_bevy::components::loaded_by::LoadedBy>,
    lobby: Res<Lobby>,
    timings: Res<crate::plugins::metrics::SystemTimings>,
    characters: Query<(), With<PlayerControlled>>,
) {
    let mut _t = None;
    for message in reader.read() {
        let Some((ent, route)) = route(&message.event) else { continue };
        _t.get_or_insert_with(|| timings.scope("send_do"));
        let owner = lobby.get_by_right(&ent);
        let encode = || bincode::serde::encode_to_vec(message, bincode::config::legacy()).unwrap();
        match route {
            Route::Owner => {
                if let Some(client_id) = owner {
                    conn.send_reliable(*client_id, DefaultChannel::ReliableOrdered, encode());
                }
            }
            Route::OwnerUnordered => {
                if let Some(client_id) = owner {
                    conn.send_reliable(*client_id, DefaultChannel::ReliableUnordered, encode());
                }
            }
            Route::Seen => {
                let Ok(loaded_by) = loaded_by_query.get(ent) else {
                    if matches!(message.event, Event::Despawn { .. }) {
                        warn!("SERVER: Cannot send Despawn for entity {:?} - no LoadedBy component", ent);
                    }
                    continue;
                };
                // Its owner too: a client viewing the actor sees what befalls it
                let bytes = encode();
                let seeing = loaded_by.players.iter().filter(|&&player| player != ent).filter_map(|player| lobby.get_by_right(player));
                for client_id in owner.into_iter().chain(seeing) {
                    conn.send_reliable(*client_id, DefaultChannel::ReliableOrdered, bytes.clone());
                }
            }
            Route::Moving => {
                let Ok(loaded_by) = loaded_by_query.get(ent) else { continue };
                // A client viewing the actor simulates it as any other it sees;
                // a character's own client predicts it and wants none
                let bytes = encode();
                let viewer = owner.filter(|_| !characters.contains(ent));
                for client_id in loaded_by.players.iter().filter_map(|player| lobby.get_by_right(player)).chain(viewer) {
                    conn.send_unreliable(*client_id, bytes.clone());
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_only_its_owner_should_know_goes_to_its_owner_alone() {
        let ent = Entity::from_raw_u32(7).unwrap();
        let reason = AbilityFailReason::OnCooldown;
        assert_eq!(route(&Event::AbilityFailed { ent, reason }), Some((ent, Route::Owner)));
        assert_eq!(route(&Event::Loot { ent, entries: None }), Some((ent, Route::Owner)));
        assert_eq!(route(&Event::UseAbility { ent, ability: AbilityType::Lunge, target: None }), Some((ent, Route::Seen)));
        assert_eq!(route(&Event::Despawn { ent }), Some((ent, Route::Seen)));
        assert_eq!(route(&Event::Displace { ent, destination: Qrz::default(), duration_ms: 0, around: None }), Some((ent, Route::Moving)));
    }

    #[test]
    fn what_the_server_keeps_to_itself_is_sent_to_no_one() {
        let ent = Entity::from_raw_u32(7).unwrap();
        assert_eq!(route(&Event::Stumble { ent }), None);
        assert_eq!(route(&Event::DealDamage { source: ent, target: ent, base_damage: 1.0, ability: None, dot: 0.0 }), None);
        assert_eq!(route(&Event::Play), None);
    }
}
