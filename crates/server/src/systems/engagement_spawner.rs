//! # Engagement Activation System
//!
//! Activates terrain-derived spawners when players approach.
//! Spawners are placed by SpawnerEvent in the world event composite.
//! The activation system reads SpawnerPlacementIndex for nearby placements.

use bevy::prelude::*;
use qrz::Qrz;
use rand::Rng;

use common_bevy::{
    components::{
        behaviour::{Behaviour, PlayerControlled},
        engagement::{Engagement, EngagementMember, LastPlayerProximity},
        entity_type::{
            actor::{ActorIdentity, ActorImpl, Origin},
            EntityType,
        },
        gcd::Gcd,
        heading::Heading,
        hex_assignment::HexAssignment,
        npc_recovery::NpcRecovery,
        position::Position,
        reaction_queue::ReactionQueue,
        resources::{CombatState, Health, Mana, Stamina},
        AirTime, LastAutoAttack, Physics, Loc,
    },
    plugins::nntree::NearestNeighbor,
    spatial_difficulty::{
        calculate_enemy_attributes, calculate_enemy_level,
        EnemyArchetype, HAVEN_LOCATION,
    },
    systems::combat::resources as resource_calcs,
};

/// Minimum distance from player to activate a spawner (tiles)
const MIN_ACTIVATION_DISTANCE: i32 = 30;

/// Maximum distance from player to activate a spawner (tiles).
/// Must be within AOI_RADIUS (123) so the player can see the NPCs.
const MAX_ACTIVATION_DISTANCE: i32 = 100;



/// Tracks which spawner tiles have active engagements.
/// Cleared when engagement is cleaned up (allows re-activation).
#[derive(Resource, Default)]
pub struct ActiveSpawners(pub std::collections::HashSet<(i32, i32)>);

/// Activate spawners near players. Spacing enforced at survey time (min_spacing).
pub fn activate_spawners(
    mut commands: Commands,
    mut active: ResMut<ActiveSpawners>,
    registry: Res<crate::resources::event_registry::EventRegistry>,
    time: Res<Time>,
    timings: Res<crate::plugins::metrics::SystemTimings>,
    player_query: Query<&Loc, (With<PlayerControlled>, Changed<Loc>)>,
) {
    if player_query.is_empty() { return; }
    let _t = timings.scope("spawner");

    for player_loc in &player_query {
        let spawners = registry.spawners_near(player_loc.q, player_loc.r);

        for placement in &spawners {
            let (q, r) = (placement.q, placement.r);
            let dist = player_loc.flat_distance(&Loc::from_qrz(q, r, 0));

            if dist < MIN_ACTIVATION_DISTANCE || dist > MAX_ACTIVATION_DISTANCE { continue; }
            if active.0.contains(&(q, r)) { continue; }

            let archetype = match placement.archetype {
                world::events::spawner::SpawnerArchetype::Berserker => EnemyArchetype::Berserker,
                world::events::spawner::SpawnerArchetype::Juggernaut => EnemyArchetype::Juggernaut,
                world::events::spawner::SpawnerArchetype::Kiter => EnemyArchetype::Kiter,
                world::events::spawner::SpawnerArchetype::Defender => EnemyArchetype::Defender,
            };

            active.0.insert((q, r));
            let spawn_z = registry.elevation_at(q, r);
            spawn_engagement(
                Qrz { q, r, z: spawn_z + 1 },
                archetype,
                &mut commands, &time, &registry,
            );
        }
    }
}

/// Spawn an engagement at a spawner location with the given archetype.
fn spawn_engagement(
    location: Qrz,
    archetype: EnemyArchetype,
    commands: &mut Commands,
    time: &Time,
    registry: &crate::resources::event_registry::EventRegistry,
) {
    let level = calculate_enemy_level(location, HAVEN_LOCATION);
    let npc_count = rand::rng().random_range(1..=3u8);

    let mut engagement = Engagement::new(location, level, archetype, npc_count);

    let engagement_entity = commands
        .spawn((
            engagement.clone(),
            Loc::new(location),
            LastPlayerProximity::new(time.elapsed()),
            HexAssignment::default(),
        ))
        .id();

    let attributes = calculate_enemy_attributes(level, archetype);

    for i in 0..npc_count {
        let offset = get_random_hex_offset(i as usize);
        let npc_location_base = location + offset;
        let npc_z = registry.elevation_at(npc_location_base.q, npc_location_base.r);
        let npc_location = Qrz { q: npc_location_base.q, r: npc_location_base.r, z: npc_z + 1 };

        let actor_impl = ActorImpl {
            origin: Origin::Evolved,
            approach: archetype.approach(),
            resilience: archetype.resilience(),
            identity: ActorIdentity::Npc(archetype.npc_type()),
        };

        let max_health = attributes.max_health();
        let max_stamina = resource_calcs::calculate_max_stamina(&attributes);
        let max_mana = resource_calcs::calculate_max_mana(&attributes);
        let stamina_regen = resource_calcs::calculate_stamina_regen_rate(&attributes);
        let mana_regen = resource_calcs::calculate_mana_regen_rate(&attributes);

        let health = Health { state: max_health, step: max_health, max: max_health };
        let stamina = Stamina { state: max_stamina, step: max_stamina, max: max_stamina, regen_rate: stamina_regen, last_update: time.elapsed() };
        let mana = Mana { state: max_mana, step: max_mana, max: max_mana, regen_rate: mana_regen, last_update: time.elapsed() };
        let combat_state = CombatState { in_combat: false, last_action: time.elapsed() };
        let queue_capacity = attributes.window_size();
        let reaction_queue = ReactionQueue::new(queue_capacity);

        let npc_loc = Loc::new(npc_location);
        let npc_entity = commands
            .spawn((
                EntityType::Actor(actor_impl),
                npc_loc,
                attributes,
                health, stamina, mana,
                combat_state,
                reaction_queue,
                Gcd::new(),
                LastAutoAttack::default(),
                Physics,
                Behaviour::default(),
                EngagementMember(engagement_entity),
                common_bevy::components::loaded_by::LoadedBy::default(),
            ))
            .id();

        match archetype {
            EnemyArchetype::Berserker | EnemyArchetype::Juggernaut | EnemyArchetype::Defender => {
                let chase = crate::systems::behaviour::chase::Chase {
                    acquisition_range: 15,
                    leash_distance: 30,
                    attack_range: 1,
                };
                commands.entity(npc_entity).insert((
                    NearestNeighbor::new(npc_entity, npc_loc),
                    chase,
                    NpcRecovery::for_archetype(archetype),
                    common_bevy::components::target::Target::default(),
                    Heading::default(),
                    Position::at_tile(npc_location),
                    AirTime::default(),
                    common_bevy::components::movement_intent_state::MovementIntentState::default(),
                ));
            }
            EnemyArchetype::Kiter => {
                let kite = crate::systems::behaviour::kite::Kite::forest_sprite();
                commands.entity(npc_entity).insert((
                    NearestNeighbor::new(npc_entity, npc_loc),
                    kite,
                    common_bevy::components::target::Target::default(),
                    Heading::default(),
                    Position::at_tile(npc_location),
                    AirTime::default(),
                    common_bevy::components::AttackRange(6),
                    LastAutoAttack::default(),
                    NpcRecovery::for_archetype(archetype),
                    common_bevy::components::movement_intent_state::MovementIntentState::default(),
                ));
            }
        }

        engagement.add_npc(npc_entity);
    }

    commands.entity(engagement_entity).insert(engagement);
}

fn get_random_hex_offset(index: usize) -> Qrz {
    let directions = [
        Qrz { q: 1, r: 0, z: 0 },
        Qrz { q: -1, r: 0, z: 0 },
        Qrz { q: 0, r: 1, z: 0 },
        Qrz { q: 0, r: -1, z: 0 },
        Qrz { q: 1, r: -1, z: 0 },
        Qrz { q: -1, r: 1, z: 0 },
    ];
    directions[index % directions.len()]
}
