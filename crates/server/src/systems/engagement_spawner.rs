//! # Engagement Activation System
//!
//! Activates terrain-derived spawners when players approach.
//! Spawners live in the terrain SpawnerCache (lazily evaluated),
//! not as tiles in the Map.

use bevy::prelude::*;
use qrz::Qrz;
use rand::Rng;

use common_bevy::{
    components::{
        behaviour::Behaviour,
        engagement::{Engagement, EngagementMember, LastPlayerProximity, ZoneId},
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
use crate::resources::engagement_budget::EngagementBudget;

/// Minimum distance from player to activate a spawner (tiles)
const MIN_ACTIVATION_DISTANCE: i32 = 30;

/// Maximum distance from player to activate a spawner (tiles).
/// Must be within AOI_RADIUS (123) so the player can see the NPCs.
const MAX_ACTIVATION_DISTANCE: i32 = 100;

/// Minimum distance between active engagements (tiles)
const MIN_ENGAGEMENT_DISTANCE: i32 = 50;

/// Tracks which spawner tiles have active engagements (prevents double-activation).
/// Keyed by (q, r) of the spawner tile. Cleared when engagement is cleaned up.
#[derive(Resource, Default)]
pub struct ActiveSpawners(pub std::collections::HashSet<(i32, i32)>);

/// Activate spawners near players. Queries EventRegistry for spawner
/// placements within activation range, creates engagements at eligible ones.
pub fn activate_spawners(
    mut commands: Commands,
    mut budget: ResMut<EngagementBudget>,
    mut active: ResMut<ActiveSpawners>,
    mut registry: ResMut<crate::resources::event_registry::EventRegistry>,
    time: Res<Time>,
    terrain: Res<crate::resources::terrain::Terrain>,
    player_query: Query<&Loc, (With<Behaviour>, Changed<Loc>)>,
    engagement_query: Query<&Loc, With<Engagement>>,
) {
    use crate::resources::event_registry::EventTypeId;

    for player_loc in &player_query {
        let spawners = registry.spawners_near(&terrain, player_loc.q, player_loc.r);

        for placement in &spawners {
            let (q, r) = terrain::world_to_hex(placement.wx, placement.wy);
            let dist = player_loc.flat_distance(&Loc::from_qrz(q, r, 0));

            if dist < MIN_ACTIVATION_DISTANCE || dist > MAX_ACTIVATION_DISTANCE { continue; }

            registry.gate_metrics(EventTypeId::Spawner).candidate();

            if active.0.contains(&(q, r)) {
                registry.gate_metrics(EventTypeId::Spawner).reject("already_active");
                continue;
            }

            let location = Qrz { q, r, z: 0 };
            let zone_id = ZoneId::from_position(location);
            if !budget.can_spawn_in_zone(zone_id) {
                registry.gate_metrics(EventTypeId::Spawner).reject("budget_exhausted");
                continue;
            }

            let too_close = engagement_query.iter().any(|eloc| {
                location.flat_distance(&**eloc) < MIN_ENGAGEMENT_DISTANCE
            });
            if too_close {
                registry.gate_metrics(EventTypeId::Spawner).reject("too_close");
                continue;
            }

            let archetype = match placement.archetype {
                terrain::spawners::SpawnerArchetype::Berserker => EnemyArchetype::Berserker,
                terrain::spawners::SpawnerArchetype::Juggernaut => EnemyArchetype::Juggernaut,
                terrain::spawners::SpawnerArchetype::Kiter => EnemyArchetype::Kiter,
                terrain::spawners::SpawnerArchetype::Defender => EnemyArchetype::Defender,
            };

            registry.gate_metrics(EventTypeId::Spawner).accept();
            active.0.insert((q, r));
            let spawn_z = terrain.get(q, r);
            spawn_engagement(
                Qrz { q, r, z: spawn_z + 1 },
                archetype,
                &mut commands, &mut budget, &time, &terrain,
            );
            registry.gate_metrics(EventTypeId::Spawner).materialized();
        }
    }
}

/// Spawn an engagement at a spawner location with the given archetype.
fn spawn_engagement(
    location: Qrz,
    archetype: EnemyArchetype,
    commands: &mut Commands,
    budget: &mut EngagementBudget,
    time: &Time,
    terrain: &crate::resources::terrain::Terrain,
) {
    let level = calculate_enemy_level(location, HAVEN_LOCATION);
    let npc_count = rand::rng().random_range(1..=3u8);

    let mut engagement = Engagement::new(location, level, archetype, npc_count);
    let zone_id = engagement.zone_id;

    let engagement_entity = commands
        .spawn((
            engagement.clone(),
            Loc::new(location),
            LastPlayerProximity::new(time.elapsed()),
            HexAssignment::default(),
        ))
        .id();

    budget.register_engagement(zone_id);

    let attributes = calculate_enemy_attributes(level, archetype);

    for i in 0..npc_count {
        let offset = get_random_hex_offset(i as usize);
        let npc_location_base = location + offset;
        let npc_z = terrain.get(npc_location_base.q, npc_location_base.r);
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

/// Get a random hex offset for NPC placement within an engagement
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
