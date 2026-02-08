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
            position::Position,
            resources::*,
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

    // ADR-014: Static spawners disabled in favor of dynamic engagement system
    // ADR-009: Amp up combat - spawn Wild Dog spawners for isolated encounters
    // Player spawns at Qrz { q: 0, r: 0, z: 4 }
    // Place spawners 30+ hexes away on hex plane (q,r), at actual terrain elevation
    // Spread in different hex directions for isolated 1v1 encounters
    // let spawner_hex_positions = vec![
    //     (30, 0),      // ~30 hexes in +q direction
    //     (0, 30),      // ~30 hexes in +r direction
    //     (-30, 0),     // ~30 hexes in -q direction
    //     (0, -30),     // ~30 hexes in -r direction
    // ];

    // for (q, r) in spawner_hex_positions {
    //     // Convert hex position to world space to query terrain height
    //     let world_pos = map.convert(Qrz { q, r, z: 0 });
    //     let terrain_height = terrain.get(world_pos.x, world_pos.y);

    //     // Create spawner at terrain height
    //     let spawn_qrz = Qrz { q, r, z: terrain_height };

    //     let spawner = Spawner::new(
    //         NpcTemplate::random_mixed(),
    //         2,      // max_count: 2 NPCs per spawner
    //         4,      // spawn_radius: 4 hexes (tight group)
    //         35,     // player_activation_range: 35 hexes (wide activation)
    //         15,     // leash_distance: 15 hexes
    //         35,     // despawn_distance: 35 hexes (cleanup when far away)
    //         5000,   // respawn_timer_ms: 5 seconds (breathing room between waves)
    //     );

    //     commands.spawn((
    //         spawner,
    //         Loc::new(spawn_qrz),
    //         Name::new(format!("Mixed NPC Spawner {:?}", spawn_qrz)),
    //     ));
    // }
}

pub fn try_spawn(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
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
    mut reader: MessageReader<Do>,
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
                            Position::at_tile(qrz),
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
