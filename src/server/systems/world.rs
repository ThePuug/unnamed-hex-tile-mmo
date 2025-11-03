use bevy::{ecs::entity::Entities, prelude::*};
use chrono::{
    offset::Local, Datelike, Timelike
};
use qrz::*;

use crate::{
    common::{
        components::{ *,
            behaviour::PlayerControlled,
            entity_type::*,
            heading::Heading,
            keybits::KeyBits,
            offset::Offset,
            resources::*,
            spawner::*,
        },
        message::{ Component, Event, * },
        systems::*
    },
    server::resources::*
};

/// Helper function to generate Spawn event + component sync events for an actor entity.
/// This ensures consistent syncing of actor state across different scenarios:
/// - Initial player connection (via do_manage_connections)
/// - Remote player discovery (via try_spawn)
/// - Respawning (future use)
///
/// Returns a Vec of Do events with Spawn first, followed by Incremental events for all available components.
/// This ordering is critical - the entity must be spawned before components can be attached.
pub fn generate_actor_spawn_events(
    ent: Entity,
    typ: EntityType,
    qrz: Qrz,
    attrs: Option<ActorAttributes>,
    player_controlled: Option<&PlayerControlled>,
    heading: Option<&Heading>,
    health: Option<&Health>,
    stamina: Option<&Stamina>,
    mana: Option<&Mana>,
    combat_state: Option<&CombatState>,
) -> Vec<Do> {
    let mut events = Vec::new();

    // Spawn event MUST come first to ensure entity exists before components arrive
    events.push(Do { event: Event::Spawn { ent, typ, qrz, attrs }});

    if let Some(pc) = player_controlled {
        events.push(Do { event: Event::Incremental { ent, component: Component::PlayerControlled(*pc) }});
    }

    if let Some(h) = heading {
        events.push(Do { event: Event::Incremental { ent, component: Component::Heading(*h) }});
    }

    if let Some(h) = health {
        events.push(Do { event: Event::Incremental { ent, component: Component::Health(*h) }});
    }

    if let Some(s) = stamina {
        events.push(Do { event: Event::Incremental { ent, component: Component::Stamina(*s) }});
    }

    if let Some(m) = mana {
        events.push(Do { event: Event::Incremental { ent, component: Component::Mana(*m) }});
    }

    if let Some(cs) = combat_state {
        events.push(Do { event: Event::Incremental { ent, component: Component::CombatState(*cs) }});
    }

    events
}

pub fn setup(
    mut commands: Commands,
    mut runtime: ResMut<RunTime>,
    time: Res<Time>,
    terrain: Res<crate::server::resources::terrain::Terrain>,
    map: Res<crate::Map>,
) {
    let elapsed = time.elapsed().as_millis();
    let secs_since_midnight = Local::now().time().num_seconds_from_midnight();
    let days_since_monday = Local::now().weekday().number_from_monday() - 1;
    let weeks_since_year = Local::now().iso_week().week();
    runtime.elapsed_offset = weeks_since_year as u128 * SEASON_MS
        + days_since_monday as u128 * WEEK_MS
        + secs_since_midnight as u128 * 1000
        - elapsed;

    // ADR-009: Amp up combat - spawn Wild Dog spawners for isolated encounters
    // Player spawns at Qrz { q: 0, r: 0, z: 4 }
    // Place spawners 30+ hexes away on hex plane (q,r), at actual terrain elevation
    // Spread in different hex directions for isolated 1v1 encounters
    let spawner_hex_positions = vec![
        (30, 0),      // ~30 hexes in +q direction
        (0, 30),      // ~30 hexes in +r direction
        (-30, 0),     // ~30 hexes in -q direction
        (0, -30),     // ~30 hexes in -r direction
    ];

    for (q, r) in spawner_hex_positions {
        // Convert hex position to world space to query terrain height
        let world_pos = map.convert(Qrz { q, r, z: 0 });
        let terrain_height = terrain.get(world_pos.x, world_pos.y);

        // Create spawner at terrain height
        let spawn_qrz = Qrz { q, r, z: terrain_height };

        let spawner = Spawner::new(
            NpcTemplate::Dog,
            3,      // max_count: 3 dogs per spawner (manageable group)
            4,      // spawn_radius: 4 hexes (tight group)
            12,     // player_activation_range: 12 hexes (must approach deliberately)
            20,     // leash_distance: 20 hexes (won't chase forever)
            35,     // despawn_distance: 35 hexes (cleanup when far away)
            5000,   // respawn_timer_ms: 5 seconds (breathing room between waves)
        );

        commands.spawn((
            spawner,
            Loc::new(spawn_qrz),
            Name::new(format!("Wild Dog Spawner {:?}", spawn_qrz)),
        ));
    }
}

pub fn try_spawn(
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
    query: Query<(
        &Loc,
        &EntityType,
        Option<&ActorAttributes>,
        Option<&PlayerControlled>,
        Option<&Heading>,
        Option<&Health>,
        Option<&Stamina>,
        Option<&Mana>,
        Option<&CombatState>,
    ), Without<RespawnTimer>>,
) {
    for &message in reader.read() {
        let Try { event: Event::Spawn { ent, .. }} = message else { continue };
        // Skip dead players (those with RespawnTimer) - they shouldn't be discovered/spawned
        // until process_respawn sends an official Spawn event after the 5-second timer
        let Ok((loc, typ, attrs, player_controlled, heading, health, stamina, mana, combat_state)) = query.get(ent) else { continue; };

        // Send Spawn + all available actor components using shared helper
        // This ensures remote players are immediately visible and targetable
        let spawn_events = generate_actor_spawn_events(
            ent,
            *typ,
            **loc,
            attrs.copied(),
            player_controlled,
            heading,
            health,
            stamina,
            mana,
            combat_state,
        );

        for event in spawn_events {
            writer.write(event);
        }
    }
}

pub fn do_spawn(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut map: ResMut<crate::Map>,
    entities: &Entities,
    existing_actors: Query<(), With<Actor>>,
) {
    for &message in reader.read() {
        if let Do { event: Event::Spawn { qrz, typ, ent, .. } } = message {
            match typ {
                EntityType::Decorator(_) => {
                    if map.get(qrz).is_none() { map.insert(qrz, typ) }
                },
                EntityType::Actor(_) => {
                    // Only insert components if:
                    // 1. The entity exists (it was created by player connection or NPC spawner)
                    // 2. The entity doesn't already have the Actor component
                    // This handles the case where try_discover_chunk sends spawn events for
                    // existing players/NPCs - we skip those because they're already set up
                    if entities.contains(ent) && existing_actors.get(ent).is_err() {
                        commands.entity(ent).insert((
                            Actor,
                            AirTime { state: Some(125), step: None },
                            KeyBits::default(),
                            Heading::default(),
                            Offset::default(),
                            LastAutoAttack::default(), // ADR-009: Track auto-attack cooldown
                            Transform {
                                translation: map.convert(qrz),
                                ..default()},
                        ));
                    }
                },
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::{App, Update};
    use crate::common::{
        components::{behaviour::*, entity_type::actor::*, resources::*},
        message::Component,
    };
    use qrz::Qrz;

    #[test]
    fn test_try_spawn_sends_full_player_state() {
        // When a client requests a remote player's entity (Try::Spawn),
        // the server should respond with:
        // 1. Event::Spawn (basic entity data)
        // 2. All critical components (PlayerControlled, Heading, Health, etc.)
        //
        // This ensures remote players are visible and targetable immediately,
        // not just after they move.

        let mut app = App::new();
        app.add_event::<Try>();
        app.add_event::<Do>();
        app.insert_resource(crate::Map::new(qrz::Map::new(1.0, 0.8)));

        app.add_systems(Update, try_spawn);

        // Create a remote player entity with full state
        let remote_player_loc = Loc::new(Qrz { q: 5, r: -5, z: 0 });
        let remote_player_heading = Heading::new(Qrz { q: 1, r: 0, z: 0 });
        let remote_player_attrs = ActorAttributes::new(0, 4, 0, 0, 3, 0, 0, 3, 0);
        let remote_player_health = Health { state: 100.0, step: 100.0, max: 100.0 };
        let remote_player_stamina = Stamina {
            state: 50.0,
            step: 50.0,
            max: 50.0,
            regen_rate: 1.0,
            last_update: std::time::Duration::ZERO,
        };
        let remote_player_mana = Mana {
            state: 30.0,
            step: 30.0,
            max: 30.0,
            regen_rate: 1.0,
            last_update: std::time::Duration::ZERO,
        };
        let remote_player_combat_state = CombatState {
            in_combat: false,
            last_action: std::time::Duration::ZERO,
        };

        let remote_player = app.world_mut().spawn((
            EntityType::Actor(ActorImpl::new(
                Origin::Evolved,
                Approach::Direct,
                Resilience::Vital,
                ActorIdentity::Player,
            )),
            remote_player_loc,
            remote_player_heading,
            remote_player_attrs,
            remote_player_health,
            remote_player_stamina,
            remote_player_mana,
            remote_player_combat_state,
            Behaviour::Controlled,
            PlayerControlled,
        )).id();

        // Simulate client requesting this entity
        app.world_mut().send_event(Try {
            event: Event::Spawn {
                ent: remote_player,
                typ: EntityType::Unset,
                qrz: Qrz::default(),
                attrs: None,
            },
        });

        app.update();

        // Collect all Do events
        let do_events: Vec<_> = {
            let mut reader = app.world_mut().resource_mut::<Events<Do>>().get_cursor();
            let events = app.world().resource::<Events<Do>>();
            reader.read(events).cloned().collect()
        };

        // Assert: Should have Event::Spawn
        let spawn_events: Vec<_> = do_events
            .iter()
            .filter_map(|d| {
                if let Do { event: Event::Spawn { ent, typ, qrz, attrs } } = d {
                    Some((*ent, *typ, *qrz, *attrs))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            spawn_events.len(),
            1,
            "Should send exactly one Spawn event"
        );
        assert_eq!(spawn_events[0].0, remote_player, "Spawn should be for the remote player");
        assert_eq!(spawn_events[0].2, *remote_player_loc, "Spawn should have correct location");

        // Assert: Should have Event::Incremental with PlayerControlled
        let player_controlled_events: Vec<_> = do_events
            .iter()
            .filter_map(|d| {
                if let Do {
                    event: Event::Incremental {
                        ent,
                        component: Component::PlayerControlled(_),
                    },
                } = d
                {
                    Some(*ent)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            player_controlled_events.len(),
            1,
            "Should send PlayerControlled component so remote player can be targeted as ally"
        );
        assert_eq!(
            player_controlled_events[0], remote_player,
            "PlayerControlled should be for the remote player"
        );

        // Assert: Should have Event::Incremental with Heading
        let heading_events: Vec<_> = do_events
            .iter()
            .filter_map(|d| {
                if let Do {
                    event: Event::Incremental {
                        ent,
                        component: Component::Heading(heading),
                    },
                } = d
                {
                    Some((*ent, *heading))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            heading_events.len(),
            1,
            "Should send Heading component so remote player is positioned correctly"
        );
        assert_eq!(
            heading_events[0].0, remote_player,
            "Heading should be for the remote player"
        );
        assert_eq!(
            heading_events[0].1, remote_player_heading,
            "Heading should match the remote player's actual heading"
        );

        // Assert: Should have Event::Incremental with Health
        let health_events: Vec<_> = do_events
            .iter()
            .filter_map(|d| {
                if let Do {
                    event: Event::Incremental {
                        ent,
                        component: Component::Health(health),
                    },
                } = d
                {
                    Some((*ent, *health))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            health_events.len(),
            1,
            "Should send Health component for health bar display"
        );
        assert_eq!(health_events[0].0, remote_player);
        // Verify health values match (can't use assert_eq! as Health doesn't derive PartialEq)
        assert_eq!(health_events[0].1.state, remote_player_health.state);
        assert_eq!(health_events[0].1.step, remote_player_health.step);
        assert_eq!(health_events[0].1.max, remote_player_health.max);
    }
}
