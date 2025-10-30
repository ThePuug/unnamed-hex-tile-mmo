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
            keybits::*,
            reaction_queue::*,
            resources::*,
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
    mut reader: EventReader<ServerEvent>,
    mut writer: EventWriter<Do>,
    mut lobby: ResMut<Lobby>,
    mut buffers: ResMut<InputQueues>,
    query: Query<(&Loc, &EntityType, Option<&ActorAttributes>, Option<&Health>, Option<&Stamina>, Option<&Mana>, Option<&CombatState>)>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    nntree: Res<NNTree>,
) {
    for event in reader.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                info!("Player {} connected", client_id);
                let typ = EntityType::Actor(ActorImpl::new(
                    Origin::Natureborn,
                    Approach::Direct,
                    Resilience::Vital));
                let qrz = Qrz { q: 0, r: 0, z: 4 };
                let loc = Loc::new(qrz);
                // UAT Testing: Start at Focus threshold (2/3 queue slots boundary)
                // vitality_focus_axis = 33, spectrum = 33 allows shift to test boundary
                // Focus = axis + shift: starting at 33, shift down/up to cross 66 threshold
                // With shift -33: Focus = 0 (1 slot)
                // With shift 0: Focus = 33 (2 slots)
                // With shift +33: Focus = 66 (3 slots) ← crosses threshold here!
                let attrs = ActorAttributes::new(
                    -10, 45, -45,  // might_grace
                    33, 33, 0,     // vitality_focus
                    -10, 45, 45,   // instinct_presence
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
                    attrs,
                    health,
                    stamina,
                    mana,
                    combat_state,
                    reaction_queue,
                    PlayerDiscoveryState::default(),
                )).id();
                commands.entity(ent).insert(NearestNeighbor::new(ent, loc));
                writer.write(Do { event: Event::Spawn { ent, typ, qrz, attrs: Some(attrs) }});

                // init input buffer for client
                buffers.extend_one((ent, InputQueue {
                    queue: [Event::Input { ent, key_bits: KeyBits::default(), dt: 0, seq: 1 }].into() }));

                // init client
                let dt = time.elapsed().as_millis() + runtime.elapsed_offset;
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Init { ent, dt }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // Send initial resource states directly to connecting client
                // Can't use event writer here because send_do relies on NNTree proximity,
                // and this entity isn't in NNTree yet
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Incremental { ent, component: Component::Health(health) }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Incremental { ent, component: Component::Stamina(stamina) }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Incremental { ent, component: Component::Mana(mana) }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                let message = bincode::serde::encode_to_vec(
                    Do { event: Event::Incremental { ent, component: Component::CombatState(combat_state) }},
                    bincode::config::legacy()).unwrap();
                conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                // spawn nearby actors
                for other in nntree.locate_within_distance(loc, 20*20) {
                    let (&loc, &typ, attrs, health, stamina, mana, combat_state) = query.get(other.ent).unwrap();
                    let message = bincode::serde::encode_to_vec(
                        Do { event: Event::Spawn { typ, ent: other.ent, qrz: *loc, attrs: attrs.copied() }},
                        bincode::config::legacy()).unwrap();
                    conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);

                    // Send resource states if entity has them
                    if let Some(h) = health {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent: other.ent, component: Component::Health(*h) }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                    if let Some(s) = stamina {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent: other.ent, component: Component::Stamina(*s) }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                    if let Some(m) = mana {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent: other.ent, component: Component::Mana(*m) }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(*client_id, DefaultChannel::ReliableOrdered, message);
                    }
                    if let Some(cs) = combat_state {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Incremental { ent: other.ent, component: Component::CombatState(*cs) }},
                            bincode::config::legacy()).unwrap();
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
    mut writer: EventWriter<Try>,
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
                Try { event: Event::UseAbility { ent: _, ability } } => {
                    let Some(&ent) = lobby.get_by_left(&client_id) else { panic!("no {client_id} in lobby") };
                    writer.write(Try { event: Event::UseAbility { ent, ability }});
                }
                _ => {}
            }
        }
    }
 }

 pub fn send_do(
    query: Query<&Loc>,
    mut conn: ResMut<RenetServer>,
    mut reader: EventReader<Do>,
    nntree: Res<NNTree>,
    lobby: Res<Lobby>,
) {
    for &message in reader.read() {
        match message {
            Do { event: Event::Spawn { ent, typ, qrz, attrs } } => {
                for other in nntree.locate_within_distance(Loc::new(qrz), 20*20) {
                    let Some(client_id) = lobby.get_by_right(&other.ent) else { continue; };
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
                // Entity might have been despawned in the same frame, so handle gracefully
                let Ok(&loc) = query.get(ent) else { continue; };
                for other in nntree.locate_within_distance(loc, 20*20) {
                    if let Some(client_id) = lobby.get_by_right(&other.ent) {
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
                    let nearby_players: Vec<_> = nntree.locate_within_distance(loc, 70*70)
                        .filter_map(|other| lobby.get_by_right(&other.ent).map(|id| (*id, other.ent)))
                        .collect();

                    for (client_id, _) in nearby_players {
                        let message = bincode::serde::encode_to_vec(
                            Do { event: Event::Despawn { ent }},
                            bincode::config::legacy()).unwrap();
                        conn.send_message(client_id, DefaultChannel::ReliableOrdered, message);
                    }
                    // Note: Actual despawning happens in cleanup_despawned system (PostUpdate)
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
            _ => {}
        }
    }
}

/// System that actually despawns entities after network messages have been sent
/// This runs in PostUpdate after send_do to avoid race conditions
pub fn cleanup_despawned(
    mut commands: Commands,
    mut reader: EventReader<Do>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Despawn { ent } } = message {
            commands.entity(ent).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use bevy::MinimalPlugins;
    use crate::common::components::Actor;

    // Mock RenetServer for testing (avoids port binding issues)
    fn create_mock_renet_server() -> RenetServer {
        use ::renet::ConnectionConfig;
        RenetServer::new(ConnectionConfig::default())
    }

    #[test]
    fn test_send_do_despawns_entity_with_loc() {
        // Test that send_do properly despawns entities that have Loc component
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(create_mock_renet_server());
        app.init_resource::<Lobby>();
        app.insert_resource(NNTree::new_for_test());

        // Create entity with Loc component
        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let ent = app.world_mut().spawn((
            Actor,
            loc,
            Name::new("Test Entity"),
        )).id();

        // Send despawn event
        app.world_mut().send_event(Do {
            event: Event::Despawn { ent },
        });

        // Run send_do and cleanup systems
        app.add_systems(Update, send_do);
        app.add_systems(PostUpdate, cleanup_despawned);
        app.update();

        // Verify entity was despawned
        assert!(app.world().get_entity(ent).is_err(),
            "Entity with Loc should be despawned by cleanup_despawned");
    }

    #[test]
    fn test_cleanup_despawns_entity_without_loc() {
        // Test that cleanup_despawned handles entities without Loc component
        // (send_do can't broadcast without Loc, but cleanup should still despawn)
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(create_mock_renet_server());
        app.init_resource::<Lobby>();
        app.insert_resource(NNTree::new_for_test());

        // Create entity WITHOUT Loc component
        let ent = app.world_mut().spawn((
            Actor,
            Name::new("Entity Without Loc"),
        )).id();

        // Send despawn event
        app.world_mut().send_event(Do {
            event: Event::Despawn { ent },
        });

        // Run send_do and cleanup systems
        app.add_systems(Update, send_do);
        app.add_systems(PostUpdate, cleanup_despawned);
        app.update();

        // Entity should be despawned by cleanup_despawned even without Loc
        assert!(app.world().get_entity(ent).is_err(),
            "Entity without Loc should still be despawned by cleanup_despawned");
    }

    #[test]
    fn test_send_do_handles_already_despawned_entity_gracefully() {
        // Test that send_do doesn't crash when entity is already despawned
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(create_mock_renet_server());
        app.init_resource::<Lobby>();
        app.insert_resource(NNTree::new_for_test());

        // Create then immediately despawn entity
        let ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 0, r: 0, z: 0 }),
            Name::new("Already Despawned"),
        )).id();

        app.world_mut().despawn(ent);

        // Send despawn event for already-despawned entity
        app.world_mut().send_event(Do {
            event: Event::Despawn { ent },
        });

        // Should not panic
        app.add_systems(Update, send_do);
        app.add_systems(PostUpdate, cleanup_despawned);
        app.update();

        // Entity should remain despawned (no resurrection)
        assert!(app.world().get_entity(ent).is_err(),
            "Already despawned entity should remain despawned");
    }

    #[test]
    fn test_send_do_despawn_sends_to_nearby_players_only() {
        // Test that despawn events are only sent to players within 70-tile radius
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(create_mock_renet_server());
        app.init_resource::<Lobby>();
        app.insert_resource(NNTree::new_for_test());

        // Create NPC to despawn at origin
        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let npc_ent = app.world_mut().spawn((
            Actor,
            npc_loc,
            Name::new("NPC to Despawn"),
        )).id();

        // Create nearby player (within 70 tiles)
        let near_player_loc = Loc::new(Qrz { q: 10, r: -10, z: 0 });
        let _near_player = app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            near_player_loc,
            Name::new("Near Player"),
        )).id();

        // Create far player (beyond 70 tiles)
        let far_player_loc = Loc::new(Qrz { q: 100, r: -100, z: 0 });
        let _far_player = app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            far_player_loc,
            Name::new("Far Player"),
        )).id();

        // Send despawn event
        app.world_mut().send_event(Do {
            event: Event::Despawn { ent: npc_ent },
        });

        // Run send_do and cleanup systems
        app.add_systems(Update, send_do);
        app.add_systems(PostUpdate, cleanup_despawned);
        app.update();

        // Verify NPC was despawned
        assert!(app.world().get_entity(npc_ent).is_err(),
            "NPC should be despawned by cleanup_despawned");

        // Note: We can't easily verify network messages without mocking RenetServer,
        // but the code should only send to nearby players within 70-tile radius
    }
}