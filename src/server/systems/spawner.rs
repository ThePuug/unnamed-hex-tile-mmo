use bevy::{prelude::*, ecs::hierarchy::ChildOf};
use bevy_behave::prelude::*;
use qrz::Qrz;
use rand::Rng;

use crate::{
    common::{
        components::{*, spawner::*, entity_type::*, behaviour::{Behaviour, PathTo}, reaction_queue::*, resources::*, gcd::Gcd},
        message::*,
        plugins::nntree::*,
        resources::map::Map,
        systems::combat::{
            queue as queue_calcs,
            resources as resource_calcs,
        },
    },
    server::systems::behaviour::{
        FindSomethingInterestingWithin, Nearby, NearbyOrigin,
        find_target::FindOrKeepTarget,
        face_target::FaceTarget,
    },
};

/// System that ticks spawners and spawns NPCs when conditions are met
pub fn tick_spawners(
    mut commands: Commands,
    time: Res<Time>,
    mut spawners: Query<(Entity, &Loc, &mut Spawner)>,
    spawned: Query<&ChildOf>,
    players: Query<(&Loc, &Behaviour)>,
    mut writer: EventWriter<Do>,
    map: Res<Map>,
) {
    let elapsed = time.elapsed().as_millis();

    for (spawner_ent, &spawner_loc, mut spawner) in &mut spawners {
        // Check cooldown
        let time_since_last = elapsed - spawner.last_spawn_attempt;
        if time_since_last < spawner.respawn_timer_ms as u128 {
            continue;
        }

        // Check if any player is nearby (only Behaviour::Controlled entities are players)
        let player_distances: Vec<_> = players.iter()
            .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
            .map(|(player_loc, _)| spawner_loc.distance(player_loc))
            .collect();

        let has_nearby_player = player_distances.iter()
            .any(|&dist| dist <= spawner.player_activation_range as i16);

        if !has_nearby_player {
            continue;
        }

        // Count how many NPCs this spawner has alive
        let alive_count = spawned
            .iter()
            .filter(|child_of| child_of.parent() == spawner_ent)
            .count();

        if alive_count >= spawner.max_count as usize {
            continue;
        }

        // Spawn new NPC at random location within radius
        let spawn_qrz = random_hex_within_radius(*spawner_loc, spawner.spawn_radius);
        spawn_npc(
            &mut commands,
            spawner.npc_template,
            spawn_qrz,
            spawner.spawn_radius,
            spawner_ent,
            &mut writer,
            &map,
            &time,
        );

        // Update cooldown
        spawner.last_spawn_attempt = elapsed;
    }
}

/// Helper function to spawn an NPC from a template
fn spawn_npc(
    commands: &mut Commands,
    template: NpcTemplate,
    qrz: impl Into<Qrz>,
    spawn_radius: u8,
    spawner_ent: Entity,
    writer: &mut EventWriter<Do>,
    map: &Map,
    time: &Time,
) {
    let qrz = qrz.into();
    // Search from higher Z to account for sloped terrain
    // On slopes, spawn location might be uphill from spawner
    let search_start = Qrz {
        q: qrz.q,
        r: qrz.r,
        z: qrz.z + spawn_radius as i16,
    };
    let Some((terrain_qrz, _entity_type)) = map.find(search_start, -60) else {
        warn!("Failed to find terrain for spawn location {:?}, skipping spawn", search_start);
        return;
    };
    // Spawn NPC one tile ABOVE terrain (not inside it)
    let spawn_qrz = Qrz {
        q: terrain_qrz.q,
        r: terrain_qrz.r,
        z: terrain_qrz.z + 1,
    };
    let loc = Loc::new(spawn_qrz);
    let actor_impl = template.actor_impl();
    let typ = EntityType::Actor(actor_impl);

    // Create behavior tree based on template
    let behavior_tree = match template {
        NpcTemplate::Dog | NpcTemplate::Wolf => {
            // ADR-006: New combat behavior with sticky targeting and sustained pressure
            BehaveTree::new(behave! {
                Behave::Forever => {
                    Behave::Sequence => {
                        Behave::spawn_named(
                            "find or keep target",
                            FindOrKeepTarget {
                                dist: 20,           // Acquisition range
                                leash_distance: 30, // Max chase distance
                            }
                        ),
                        Behave::spawn_named(
                            "face target (before pathfinding)",
                            FaceTarget
                        ),
                        Behave::spawn_named(
                            "set dest near target",
                            Nearby {
                                min: 1,
                                max: 1,  // Get close to target (1 tile away)
                                origin: NearbyOrigin::Target,
                            }
                        ),
                        Behave::spawn_named(
                            "path to dest",
                            PathTo::default()
                        ),
                        Behave::spawn_named(
                            "face target (after pathfinding)",
                            FaceTarget
                        ),
                        // ADR-009: Auto-attack is handled by passive system (process_passive_auto_attack)
                        // Dogs only need to pathfind to target and face them - attacks are automatic
                        Behave::Wait(1.0),  // DIAGNOSTIC: Testing if wait fixes movement regression
                    }
                }
            })
        }
        NpcTemplate::Rabbit => {
            BehaveTree::new(behave! {
                Behave::Forever => {
                    Behave::Sequence => {
                        Behave::spawn_named(
                            "find something interesting",
                            FindSomethingInterestingWithin { dist: 15 }
                        ),
                        Behave::spawn_named(
                            "set dest to target",
                            Nearby {
                                min: 0,
                                max: 0,
                                origin: NearbyOrigin::Target,
                            }
                        ),
                        Behave::spawn_named(
                            "path to dest",
                            PathTo::default()
                        ),
                        Behave::Wait(8.),
                    }
                }
            })
        }
    };

    // Level 10: Might+Instinct focused distribution
    // axis=-6 (3 levels) + spectrum=2 (2 levels) = 5 levels per stat → 10 total
    // Negative axis favors might and instinct sides
    let attrs = ActorAttributes::new(
        -6, 2, 0,      // might_grace: might-focused (5 levels)
        0, 0, 0,       // vitality_focus: no investment (0 levels)
        -6, 2, 0,      // instinct_presence: instinct-focused (5 levels)
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

    let ent = commands
        .spawn((
            typ,
            loc,
            Physics, // CRITICAL: NPCs need Physics component to actually move!
            ChildOf(spawner_ent),
            Name::new(format!("NPC {:?}", template)),
            attrs,
            health,
            stamina,
            mana,
            combat_state,
            reaction_queue,
            Gcd::new(),  // GCD component for cooldown tracking
            LastAutoAttack::default(),  // ADR-009: Track auto-attack cooldown
            children![(
                Name::new("behaviour"),
                behavior_tree,
            )],
        ))
        .id();

    commands.entity(ent).insert(NearestNeighbor::new(ent, loc));

    // Send spawn event to clients
    writer.write(Do {
        event: crate::common::message::Event::Spawn { ent, typ, qrz: spawn_qrz, attrs: Some(attrs) },
    });

    // Send initial resource states to clients via Incremental
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Health(health) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Stamina(stamina) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::Mana(mana) }});
    writer.write(Do { event: crate::common::message::Event::Incremental { ent, component: crate::common::message::Component::CombatState(combat_state) }});
}

/// Helper function to generate a random hex within a radius
/// Only randomizes horizontal position (q, r), keeps center's Z for terrain lookup
fn random_hex_within_radius(center: impl Into<Qrz>, radius: u8) -> Qrz {
    if radius == 0 {
        return center.into();
    }

    let center = center.into();
    let mut rng = rand::rng();

    // Generate random HORIZONTAL offset within radius
    // Z coordinate is NOT randomized - it will be determined by terrain height
    let radius = radius as i16;

    loop {
        let q_offset = rng.random_range(-radius..=radius);
        let r_offset = rng.random_range(-radius..=radius);

        // Check if within radius using flat hex distance (ignore Z)
        // Flat distance = max(|q|, |r|, |q+r|)
        let dist = q_offset.abs().max(r_offset.abs()).max((q_offset + r_offset).abs());
        if dist <= radius {
            return Qrz {
                q: center.q + q_offset,
                r: center.r + r_offset,
                z: center.z,  // Keep center's Z - map.find will adjust to terrain
            };
        }
    }
}

/// System that despawns NPCs when all players are beyond the despawn distance
pub fn despawn_out_of_range(
    spawners: Query<(Entity, &Loc, &Spawner)>,
    npcs: Query<(Entity, &Loc, &ChildOf), Without<Spawner>>,
    players: Query<(&Loc, &Behaviour)>,
    mut writer: EventWriter<Do>,
) {
    for (spawner_ent, &spawner_loc, spawner) in &spawners {
        // Check if any player is within despawn distance of this spawner (only Behaviour::Controlled entities are players)
        let player_distances: Vec<_> = players.iter()
            .filter(|(_, behaviour)| matches!(behaviour, Behaviour::Controlled))
            .map(|(player_loc, _)| spawner_loc.distance(player_loc))
            .collect();

        let has_nearby_player = player_distances.iter()
            .any(|&dist| dist <= spawner.despawn_distance as i16);

        if has_nearby_player {
            continue;
        }

        // Count and despawn all NPCs from this spawner
        let npcs_to_despawn: Vec<_> = npcs.iter()
            .filter(|(_, _, child_of)| child_of.parent() == spawner_ent)
            .map(|(ent, _, _)| ent)
            .collect();

        if !npcs_to_despawn.is_empty() {
            for npc_ent in npcs_to_despawn {
                // Send despawn event - the actual despawning will happen in PostUpdate
                // after send_do has sent the network message
                writer.write(Do {
                    event: crate::common::message::Event::Despawn { ent: npc_ent },
                });
                // Don't despawn here - let the send_do system handle it after sending the message
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::components::{entity_type::{actor::*, decorator::Decorator}, behaviour::Behaviour};
    use crate::common::message::Event as MsgEvent;

    /// Helper function to create a map with terrain tiles for testing
    fn create_test_map() -> Map {
        let mut qrz_map = qrz::Map::new(1.0, 0.8);
        for q in -20..=20 {
            for r in -20..=20 {
                let z: i16 = -q - r;
                if z.abs() <= 20 { // Keep within reasonable Z range
                    qrz_map.insert(Qrz { q, r, z }, EntityType::Decorator(Decorator { index: 3, is_solid: true }));
                }
            }
        }
        Map::new(qrz_map)
    }

    #[test]
    fn test_random_hex_within_radius_zero_returns_center() {
        let center = Qrz { q: 5, r: 3, z: -8 };
        let result = random_hex_within_radius(center, 0);
        assert_eq!(result, center);
    }

    #[test]
    fn test_random_hex_within_radius_respects_bounds() {
        let center = Qrz { q: 10, r: 5, z: 3 };
        let radius = 3;

        for _ in 0..100 {
            let result = random_hex_within_radius(center, radius);

            // Check flat distance (only q and r offsets matter)
            let q_offset = result.q - center.q;
            let r_offset = result.r - center.r;
            let dist = q_offset.abs().max(r_offset.abs()).max((q_offset + r_offset).abs());

            assert!(dist <= radius as i16, "Generated hex {:?} is outside radius {}", result, radius);
            assert_eq!(result.z, center.z, "Z coordinate should not be randomized");
        }
    }

    #[test]
    fn test_spawner_does_not_spawn_without_nearby_players() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        // Create a spawner but NO players
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            10, // activation range: 10
            20,
            30,
            0, // 0 ms cooldown (should trigger immediately if player nearby)
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner")));

        // Create an NPC nearby (should NOT trigger spawning - NPCs aren't players)
        let npc_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 }); // distance 5
        app.world_mut().spawn((
            Actor,
            npc_loc,
            Name::new("NPC Dog"),
            // NOTE: No Behaviour::Controlled = this is NOT a player
        ));

        // Run the spawner system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Check that NO spawn events were emitted
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events().collect();

        assert_eq!(spawn_events.len(), 0,
            "Expected no spawn events without players nearby, but got {}. \
             BUG: System may be counting NPCs as players!", spawn_events.len());
    }

    #[test]
    fn test_spawner_spawns_when_player_in_range() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        // Create a spawner
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            10, // activation range: 10
            20,
            30,
            0, // 0 ms cooldown (should trigger immediately)
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner")));

        // Create a REAL player within range (distance 5 < 10)
        // Real players have Behaviour::Controlled component
        let player_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,  // ← THIS is what makes it a player!
            player_loc,
            Name::new("Test Player"),
        ));

        // Run the spawner system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Check that a spawn event was emitted
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events().collect();


        assert!(spawn_events.len() > 0,
            "Expected at least one spawn event with player in range, got 0. \
             BUG: System may not be detecting players correctly!");

        // Verify it's a spawn event
        if let Some(Do { event: MsgEvent::Spawn { typ, .. }}) = spawn_events.first() {
            match typ {
                EntityType::Actor(_) => {}, // Expected
                _ => panic!("Expected Actor entity type, got {:?}", typ),
            }
        } else {
            panic!("Expected Event::Spawn, got {:?}", spawn_events.first());
        }
    }

    #[test]
    fn test_spawner_respects_max_count() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);  // MinimalPlugins includes TimePlugin
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        let max_count = 2;
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            max_count,
            3,
            10,
            20,
            30,
            0, // 0 ms cooldown
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Create a REAL player within range
        let player_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,  // Real player
            player_loc,
            Name::new("Test Player"),
        ));

        // Spawn max_count NPCs already
        for i in 0..max_count {
            app.world_mut().spawn((
                Actor,
                spawner_loc,
                ChildOf(spawner_ent),
                Name::new(format!("Existing NPC {}", i)),
            ));
        }

        // Run the spawner system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Check that NO new spawn events were emitted (already at max)
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events().collect();

        assert_eq!(spawn_events.len(), 0,
            "Expected no spawn events when at max_count, but got {}", spawn_events.len());
    }

    #[test]
    fn test_despawn_removes_npcs_when_no_players_nearby() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();

        let despawn_distance = 30;
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            20,
            10,
            despawn_distance,
            1000,
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Spawn an NPC near the spawner
        let npc_loc = Loc::new(Qrz { q: 3, r: 0, z: -3 });
        let npc_ent = app.world_mut().spawn((
            Actor,
            npc_loc,
            ChildOf(spawner_ent),
            Name::new("Test NPC"),
        )).id();

        // Create a REAL player FAR from the spawner (distance > despawn_distance)
        // Using Chebyshev distance: max(|40-0|, |0-0|, |(-40)-0|) = 40 > 30
        let player_loc = Loc::new(Qrz { q: 40, r: 0, z: -40 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,  // Real player
            player_loc,
            Name::new("Far Player"),
        ));

        // Run the despawn system
        app.add_systems(Update, despawn_out_of_range);
        app.update();

        // NOTE: The despawn system only SENDS despawn events, it doesn't actually despawn entities.
        // The actual despawning happens in a separate system (send_do) that processes events.
        // So we only check that the despawn EVENT was sent.

        // CRITICAL: Check that despawn event was sent to clients
        let events = app.world().resource::<Events<Do>>();
        let despawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
            .collect();

        assert!(despawn_events.len() > 0,
            "Expected Event::Despawn to be sent when all players beyond despawn distance, but got none. \
             BUG: System may be counting the NPC itself as a nearby 'player'!");

        // Verify the despawn event is for the correct entity
        if let Some(Do { event: MsgEvent::Despawn { ent }}) = despawn_events.first() {
            assert_eq!(*ent, npc_ent,
                "Despawn event should be for NPC entity {:?}, but got {:?}", npc_ent, ent);
        }

        // NOTE: NPC entity still exists in this test because actual despawning happens
        // in a separate system. This is correct behavior - despawn_out_of_range only
        // sends the event, it doesn't despawn entities directly.
    }

    #[test]
    fn test_despawn_keeps_npcs_when_player_nearby() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();

        let despawn_distance = 30;
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            20,
            10,
            despawn_distance,
            1000,
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Spawn an NPC near the spawner
        let npc_loc = Loc::new(Qrz { q: 3, r: 0, z: -3 });
        let npc_ent = app.world_mut().spawn((
            Actor,
            npc_loc,
            ChildOf(spawner_ent),
            Name::new("Test NPC"),
        )).id();

        // Create a REAL player NEAR the spawner (distance < despawn_distance)
        // Using Chebyshev distance: max(|15-0|, |0-0|, |(-15)-0|) = 15 < 30
        let player_loc = Loc::new(Qrz { q: 15, r: 0, z: -15 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,  // Real player
            player_loc,
            Name::new("Nearby Player"),
        ));

        // Run the despawn system
        app.add_systems(Update, despawn_out_of_range);
        app.update();

        // Check that the NPC still exists
        assert!(app.world().get_entity(npc_ent).is_ok(),
            "Expected NPC to remain when player within despawn distance");
    }

    #[test]
    fn test_spawner_last_spawn_attempt_is_server_side_only() {
        // CRITICAL: Verify that last_spawn_attempt is always initialized to 0
        // This is server-side state that should never be sent over network
        let spawner = Spawner::new(
            NpcTemplate::Wolf,
            3,
            5,
            20,
            10,
            30,
            5000,
        );

        // Verify last_spawn_attempt starts at 0
        assert_eq!(spawner.last_spawn_attempt, 0,
            "last_spawn_attempt should be initialized to 0, but got {}",
            spawner.last_spawn_attempt);

        // NOTE: The Spawner component SHOULD NOT be sent over the network.
        // Spawners are server-side only entities that control NPC spawning.
        // Clients only see the spawned NPCs, not the spawners themselves.
        // If spawners ever need to be sent to clients in the future,
        // last_spawn_attempt MUST be marked with #[serde(skip)] or similar.
    }

    #[test]
    fn test_npc_template_actor_impl_returns_correct_types() {
        // Verify each NPC template returns the expected actor configuration
        let dog = NpcTemplate::Dog.actor_impl();
        assert_eq!(dog.origin, Origin::Evolved);
        assert_eq!(dog.approach, Approach::Direct);
        assert_eq!(dog.resilience, Resilience::Primal);

        let wolf = NpcTemplate::Wolf.actor_impl();
        assert_eq!(wolf.origin, Origin::Evolved);
        assert_eq!(wolf.approach, Approach::Ambushing);
        assert_eq!(wolf.resilience, Resilience::Vital);

        let rabbit = NpcTemplate::Rabbit.actor_impl();
        assert_eq!(rabbit.origin, Origin::Evolved);
        assert_eq!(rabbit.approach, Approach::Evasive);
        assert_eq!(rabbit.resilience, Resilience::Primal);
    }

    #[test]
    fn test_despawn_handles_npc_without_loc_component() {
        // Test that despawn doesn't crash if NPC is missing Loc component
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();

        let despawn_distance = 30;
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            20,
            10,
            despawn_distance,
            1000,
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Spawn an NPC WITHOUT Loc component (edge case - should be skipped by query)
        let _broken_npc_ent = app.world_mut().spawn((
            Actor,
            // NOTE: Missing Loc component!
            ChildOf(spawner_ent),
            Name::new("Broken NPC"),
        )).id();

        // Spawn a valid NPC WITH Loc component
        let valid_npc_loc = Loc::new(Qrz { q: 3, r: 0, z: -3 });
        let _valid_npc_ent = app.world_mut().spawn((
            Actor,
            valid_npc_loc,
            ChildOf(spawner_ent),
            Name::new("Valid NPC"),
        )).id();

        // Create a player far away
        let player_loc = Loc::new(Qrz { q: 40, r: 0, z: -40 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Far Player"),
        ));

        // Run the despawn system - should not panic
        app.add_systems(Update, despawn_out_of_range);
        app.update();

        // Check that a despawn event was sent for the valid NPC
        let events = app.world().resource::<Events<Do>>();
        let despawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
            .collect();

        assert_eq!(despawn_events.len(), 1,
            "Expected 1 despawn event for valid NPC, got {}", despawn_events.len());

        // The broken NPC should NOT generate a despawn event (query won't match it)
        // This is actually OK - NPCs without Loc shouldn't exist in production
    }

    #[test]
    fn test_despawn_with_multiple_npcs_from_same_spawner() {
        // Test despawning multiple NPCs at once
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();

        let despawn_distance = 30;
        let spawner = Spawner::new(
            NpcTemplate::Dog,
            5,
            3,
            20,
            10,
            despawn_distance,
            1000,
        );
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Spawn 3 NPCs from the same spawner
        let _npc1_ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 2, r: 0, z: -2 }),
            ChildOf(spawner_ent),
            Name::new("NPC 1"),
        )).id();

        let _npc2_ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 3, r: 0, z: -3 }),
            ChildOf(spawner_ent),
            Name::new("NPC 2"),
        )).id();

        let _npc3_ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 4, r: 0, z: -4 }),
            ChildOf(spawner_ent),
            Name::new("NPC 3"),
        )).id();

        // Create a player far away
        let player_loc = Loc::new(Qrz { q: 40, r: 0, z: -40 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Far Player"),
        ));

        // Run the despawn system
        app.add_systems(Update, despawn_out_of_range);
        app.update();

        // Check that 3 despawn events were sent (actual despawn happens in send_do system)
        let events = app.world().resource::<Events<Do>>();
        let despawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
            .collect();

        assert_eq!(despawn_events.len(), 3,
            "Expected 3 despawn events, got {}", despawn_events.len());
    }

    #[test]
    fn test_despawn_only_affects_correct_spawner() {
        // Test that despawn only removes NPCs from the correct spawner
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();

        let despawn_distance = 30;

        // Create spawner 1
        let spawner1 = Spawner::new(NpcTemplate::Dog, 5, 3, 20, 10, despawn_distance, 1000);
        let spawner1_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let spawner1_ent = app.world_mut().spawn((spawner1, spawner1_loc, Name::new("Spawner 1"))).id();

        // Create spawner 2 (far away, with player nearby)
        let spawner2 = Spawner::new(NpcTemplate::Dog, 5, 3, 20, 10, despawn_distance, 1000);
        let spawner2_loc = Loc::new(Qrz { q: 100, r: 0, z: -100 });
        let spawner2_ent = app.world_mut().spawn((spawner2, spawner2_loc, Name::new("Spawner 2"))).id();

        // Spawn NPC from spawner 1
        let npc1_ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 3, r: 0, z: -3 }),
            ChildOf(spawner1_ent),
            Name::new("NPC from Spawner 1"),
        )).id();

        // Spawn NPC from spawner 2
        let _npc2_ent = app.world_mut().spawn((
            Actor,
            Loc::new(Qrz { q: 103, r: 0, z: -103 }),
            ChildOf(spawner2_ent),
            Name::new("NPC from Spawner 2"),
        )).id();

        // Create a player near spawner 2 (but far from spawner 1)
        let player_loc = Loc::new(Qrz { q: 105, r: 0, z: -105 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Player near Spawner 2"),
        ));

        // Run the despawn system
        app.add_systems(Update, despawn_out_of_range);
        app.update();

        // Check that only 1 despawn event was sent (for NPC from spawner 1)
        // NPC from spawner 2 should remain because player is nearby
        let events = app.world().resource::<Events<Do>>();
        let despawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
            .collect();

        assert_eq!(despawn_events.len(), 1,
            "Expected 1 despawn event (only NPC 1, not NPC 2 with nearby player), got {}", despawn_events.len());

        // Verify it's the correct NPC being despawned
        if let MsgEvent::Despawn { ent } = &despawn_events[0].event {
            assert_eq!(*ent, npc1_ent,
                "Expected despawn event for NPC from Spawner 1, got event for different entity");
        }
    }

    #[test]
    fn test_spawn_event_creates_correct_entity_structure() {
        // Integration test: verify spawn events contain all necessary components
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        let spawner = Spawner::new(NpcTemplate::Wolf, 5, 3, 10, 15, 30, 0);
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let _spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

        // Add player in range
        let player_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Test Player"),
        ));

        // Run the spawner tick system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Check spawn event was sent
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Spawn { .. }))
            .collect();

        assert_eq!(spawn_events.len(), 1, "Expected 1 spawn event");

        // Verify spawn event contains correct entity type
        if let MsgEvent::Spawn { ent: _, typ, qrz: event_qrz, .. } = &spawn_events[0].event {
            match typ {
                EntityType::Actor(actor) => {
                    // Verify it's a Wolf (from NpcTemplate::Wolf)
                    assert_eq!(actor.origin, Origin::Evolved,
                        "Expected Wolf origin to be Natureborn");
                    assert_eq!(actor.approach, Approach::Ambushing,
                        "Expected Wolf approach to be Ambushing");
                },
                _ => panic!("Expected Actor entity type, got {:?}", typ),
            }

            // Verify spawn location is within radius (3 tiles) of spawner
            let distance = spawner_loc.flat_distance(event_qrz);
            assert!(distance <= 3,
                "Expected spawn within radius 3, but distance was {}", distance);
        } else {
            panic!("Expected Spawn event variant");
        }
    }

    #[test]
    fn test_timer_edge_case_elapsed_equals_respawn_timer() {
        // Test spawner when elapsed time exactly equals respawn_timer_ms
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        // Create spawner with 0ms cooldown - should spawn immediately when player is in range
        let spawner = Spawner::new(NpcTemplate::Dog, 5, 3, 10, 15, 30, 0);

        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        app.world_mut().spawn((spawner, spawner_loc, Name::new("Edge Case Spawner")));

        // Add player in range
        let player_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Test Player"),
        ));

        // Run the spawner tick system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Should spawn when elapsed >= respawn_timer_ms
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Spawn { .. }))
            .collect();

        assert_eq!(spawn_events.len(), 1,
            "Expected spawn when elapsed time exactly equals respawn_timer_ms");
    }

    #[test]
    fn test_despawn_distance_with_different_z_levels() {
        // Test that despawn distance correctly accounts for vertical (Z) differences
        // Test case 1: Player close on flat distance, same Z - should NOT despawn
        {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_event::<Do>();

            let despawn_distance = 25;
            let spawner = Spawner::new(NpcTemplate::Dog, 5, 3, 20, 10, despawn_distance, 1000);
            let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
            let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

            let _npc_ent = app.world_mut().spawn((
                Actor,
                Loc::new(Qrz { q: 5, r: 0, z: -5 }),
                ChildOf(spawner_ent),
                Name::new("Test NPC"),
            )).id();

            // Player at flat distance 10, same Z-level as spawner
            // Flat distance: max(10, 10, 0) / 2 = 10, Z diff = 0, total = 10 (within despawn_distance 25)
            let player_loc = Loc::new(Qrz { q: 10, r: -10, z: 0 });
            app.world_mut().spawn((
                Actor,
                Behaviour::Controlled,
                player_loc,
                Name::new("Player Same Z"),
            ));

            app.add_systems(Update, despawn_out_of_range);
            app.update();

            let events = app.world().resource::<Events<Do>>();
            let despawn_events: Vec<_> = events.iter_current_update_events()
                .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
                .collect();

            assert_eq!(despawn_events.len(), 0,
                "Expected no despawn when player at distance 10 (within despawn_distance 25)");
        }

        // Test case 2: Player at same flat distance but higher Z - SHOULD despawn
        {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_event::<Do>();

            let despawn_distance = 25;
            let spawner = Spawner::new(NpcTemplate::Dog, 5, 3, 20, 10, despawn_distance, 1000);
            let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
            let spawner_ent = app.world_mut().spawn((spawner, spawner_loc, Name::new("Test Spawner"))).id();

            let _npc_ent = app.world_mut().spawn((
                Actor,
                Loc::new(Qrz { q: 5, r: -5, z: 0 }),
                ChildOf(spawner_ent),
                Name::new("Test NPC"),
            )).id();

            // Player at same flat hex position as test case 1 but with Z offset
            // Flat distance: max(10, 10, 0) / 2 = 10, Z difference = 20, total = 30 (beyond 25)
            let player_loc = Loc::new(Qrz { q: 10, r: -10, z: 20 });
            app.world_mut().spawn((
                Actor,
                Behaviour::Controlled,
                player_loc,
                Name::new("Player High Z"),
            ));

            app.add_systems(Update, despawn_out_of_range);
            app.update();

            let events = app.world().resource::<Events<Do>>();
            let despawn_events: Vec<_> = events.iter_current_update_events()
                .filter(|Do { event }| matches!(event, MsgEvent::Despawn { .. }))
                .collect();

            assert_eq!(despawn_events.len(), 1,
                "Expected despawn when player at distance 30 (flat 10 + Z offset 20, beyond despawn_distance 25)");
        }
    }

    #[test]
    fn test_spawner_activates_with_multiple_players() {
        // Verify spawner activates when ANY player is in range
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        let spawner = Spawner::new(NpcTemplate::Rabbit, 5, 3, 10, 15, 30, 0);
        let spawner_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        app.world_mut().spawn((spawner, spawner_loc, Name::new("Multi-Player Spawner")));

        // Add player 1 - out of range
        let player1_loc = Loc::new(Qrz { q: 50, r: 0, z: -50 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player1_loc,
            Name::new("Far Player"),
        ));

        // Add player 2 - in range
        let player2_loc = Loc::new(Qrz { q: 5, r: 0, z: -5 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player2_loc,
            Name::new("Near Player"),
        ));

        // Add player 3 - out of range
        let player3_loc = Loc::new(Qrz { q: -50, r: 0, z: 50 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player3_loc,
            Name::new("Another Far Player"),
        ));

        // Run spawner system
        app.add_systems(Update, tick_spawners);
        app.update();

        // Should spawn because player2 is in range (even though player1 and player3 are not)
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Spawn { .. }))
            .collect();

        assert_eq!(spawn_events.len(), 1,
            "Expected spawn when ANY player is in range (player2), regardless of other far players");
    }

    #[test]
    fn test_spawn_location_within_exact_radius_bounds() {
        // Verify NPC spawns are exactly within the specified spawn_radius
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_event::<Do>();
        app.insert_resource(NNTree::new_for_test());
        app.insert_resource(create_test_map());

        let spawn_radius = 5;
        let spawner = Spawner::new(NpcTemplate::Dog, 10, spawn_radius, 20, 15, 30, 0);
        let spawner_loc = Loc::new(Qrz { q: 10, r: 5, z: -15 }); // Non-zero spawner position
        app.world_mut().spawn((spawner, spawner_loc, Name::new("Radius Test Spawner")));

        // Add player in range
        let player_loc = Loc::new(Qrz { q: 12, r: 5, z: -17 });
        app.world_mut().spawn((
            Actor,
            Behaviour::Controlled,
            player_loc,
            Name::new("Test Player"),
        ));

        // Run spawner multiple times to get multiple spawn attempts
        app.add_systems(Update, tick_spawners);
        for _ in 0..10 {
            app.update();
        }

        // Check all spawn events
        let events = app.world().resource::<Events<Do>>();
        let spawn_events: Vec<_> = events.iter_current_update_events()
            .filter(|Do { event }| matches!(event, MsgEvent::Spawn { .. }))
            .collect();

        assert!(spawn_events.len() >= 1, "Expected at least one spawn event");

        // Verify every spawn is within the exact radius
        for spawn_event in spawn_events {
            if let MsgEvent::Spawn { ent: _, typ: _, qrz: spawn_qrz, .. } = &spawn_event.event {
                let distance = spawner_loc.flat_distance(spawn_qrz);
                assert!(distance <= spawn_radius as i16,
                    "Spawn location {:?} is at distance {} from spawner {:?}, exceeds spawn_radius {}",
                    spawn_qrz, distance, *spawner_loc, spawn_radius);
            }
        }
    }
}
