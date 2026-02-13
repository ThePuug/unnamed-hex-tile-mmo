//! # Engagement Spawning System (ADR-014)
//!
//! Dynamically spawns enemy encounters when players explore new chunks.
//! Replaces static spawners with exploration-driven content discovery.

use bevy::prelude::*;
use qrz::{Convert, Qrz};
use rand::Rng;

use crate::{
    common::{
        chunk::ChunkId,
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
        message::{Do, Event},
        plugins::nntree::NearestNeighbor,
        spatial_difficulty::{
            calculate_enemy_attributes, calculate_enemy_level, get_directional_zone,
            EnemyArchetype, HAVEN_LOCATION,
        },
        systems::combat::resources as resource_calcs,
    },
    server::resources::engagement_budget::EngagementBudget,
};

/// Spawn probability per chunk (50%)
const SPAWN_PROBABILITY: f64 = 0.5;

/// Minimum distance from any player to spawn engagement (tiles)
const MIN_DISTANCE_FROM_PLAYER: u32 = 30;

/// Minimum distance from other engagements (tiles)
const MIN_DISTANCE_FROM_ENGAGEMENT: u32 = 50;

/// System that listens for ChunkData events and attempts to spawn engagements
///
/// Multi-stage validation:
/// 1. Probability gate (50% chance)
/// 2. Budget check (max 8 per zone)
/// 3. Player proximity check (min 30 tiles)
/// 4. Engagement proximity check (min 50 tiles)
///
/// If all pass, sends Try::SpawnEngagement event
pub fn try_spawn_engagement(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<crate::common::message::Try>,
    budget: Res<EngagementBudget>,
    player_query: Query<(&Loc, &Behaviour)>,
    engagement_query: Query<&Loc, With<Engagement>>,
) {
    for message in reader.read() {
        // Only process ChunkData events
        let Do { event: Event::ChunkData { chunk_id, .. } } = message else {
            continue;
        };

        try_spawn_engagement_at_chunk(*chunk_id, &mut writer, &budget, &player_query, &engagement_query);
    }
}

/// System that processes Try::SpawnEngagement events and creates actual engagements
pub fn do_spawn_engagement(
    mut reader: MessageReader<crate::common::message::Try>,
    mut writer: MessageWriter<Do>,
    mut commands: Commands,
    mut budget: ResMut<EngagementBudget>,
    time: Res<Time>,
    terrain: Res<crate::server::resources::terrain::Terrain>,
    map: Res<crate::Map>,
) {
    for message in reader.read() {
        // Only process SpawnEngagement events
        let crate::common::message::Try { event: Event::SpawnEngagement { location } } = message else {
            continue;
        };

        // Double-check budget (prevents race condition when multiple chunks processed in same frame)
        let zone_id = ZoneId::from_position(*location);
        if !budget.can_spawn_in_zone(zone_id) {
            continue;
        }

        spawn_engagement_at(*location, &mut commands, &mut writer, &mut budget, &time, &terrain, &map);
    }
}

/// Helper function to attempt spawning engagement at a chunk
fn try_spawn_engagement_at_chunk(
    chunk_id: ChunkId,
    writer: &mut MessageWriter<crate::common::message::Try>,
    budget: &EngagementBudget,
    player_query: &Query<(&Loc, &Behaviour)>,
    engagement_query: &Query<&Loc, With<Engagement>>,
) {
    // Calculate chunk center position for spawning
    let chunk_center = chunk_id.center();

    // Stage 1: Probability gate (50% chance)
    if !rand::rng().random_bool(SPAWN_PROBABILITY) {
        return; // Silent skip
    }

    // Stage 2: Budget check (max 8 active per zone)
    let zone_id = ZoneId::from_position(chunk_center);
    if !budget.can_spawn_in_zone(zone_id) {
        return; // Zone full
    }

    // Stage 3: Player proximity check (min 30 tiles from ANY ACTUAL player, not NPCs)
    for (player_loc, behaviour) in player_query.iter() {
        // Only check actual players (Behaviour::Controlled), not NPCs
        if !matches!(behaviour, Behaviour::Controlled) {
            continue;
        }

        let distance = chunk_center.flat_distance(&**player_loc);
        if distance < MIN_DISTANCE_FROM_PLAYER as i16 {
            return; // Too close to a player
        }
    }

    // Stage 4: Engagement proximity check (min 50 tiles from other engagements)
    for engagement_loc in engagement_query.iter() {
        let distance = chunk_center.flat_distance(&**engagement_loc);
        if distance < MIN_DISTANCE_FROM_ENGAGEMENT as i16 {
            return; // Too close to another engagement
        }
    }
    writer.write(crate::common::message::Try {
        event: Event::SpawnEngagement {
            location: chunk_center,
        },
    });
}

/// Spawn an engagement at the given location
fn spawn_engagement_at(
    location: Qrz,
    commands: &mut Commands,
    _writer: &mut MessageWriter<Do>,
    budget: &mut EngagementBudget,
    time: &Time,
    terrain: &crate::server::resources::terrain::Terrain,
    map: &crate::Map,
) {
    // Calculate terrain height at spawn location (spawn on top of terrain, not in it)
    let px = map.convert(location).xz();
    let terrain_z = terrain.get(px.x, px.y);
    let location = Qrz { q: location.q, r: location.r, z: terrain_z + 1 };

    // Calculate level from distance to haven
    let level = calculate_enemy_level(location, HAVEN_LOCATION);

    // Determine archetype from directional zone
    let zone = get_directional_zone(location, HAVEN_LOCATION);
    let archetype = EnemyArchetype::from_zone(zone);

    // Random group size (1-3 NPCs)
    let npc_count = rand::rng().random_range(1..=3);

    // Create engagement parent entity
    let mut engagement = Engagement::new(location, level, archetype, npc_count);
    let zone_id = engagement.zone_id;

    // Spawn engagement entity
    let engagement_entity = commands
        .spawn((
            engagement.clone(),
            Loc::new(location),
            LastPlayerProximity::new(time.elapsed()),
            HexAssignment::default(),  // SOW-018: Hex assignment tracking
        ))
        .id();

    // Register in budget
    budget.register_engagement(zone_id);

    // Spawn NPCs as children
    let attributes = calculate_enemy_attributes(level, archetype);
    let _ability = archetype.ability();

    for i in 0..npc_count {
        // Spawn NPC slightly offset from engagement center (random hex neighbor)
        let offset = get_random_hex_offset(i as usize);
        let npc_location_base = location + offset;

        // Calculate terrain height at NPC location (spawn on top of terrain, not in it)
        let npc_px = map.convert(npc_location_base).xz();
        let npc_z = terrain.get(npc_px.x, npc_px.y);
        let npc_location = Qrz { q: npc_location_base.q, r: npc_location_base.r, z: npc_z + 1 };

        // Create NPC ActorImpl with triumvirate based on archetype (ADR-014)
        let actor_impl = ActorImpl {
            origin: Origin::Evolved,
            approach: archetype.approach(),
            resilience: archetype.resilience(),
            identity: ActorIdentity::Npc(archetype.npc_type()),
        };

        // Initialize resources from attributes (same as spawner.rs)
        let max_health = attributes.max_health();
        let max_stamina = resource_calcs::calculate_max_stamina(&attributes);
        let max_mana = resource_calcs::calculate_max_mana(&attributes);
        let stamina_regen = resource_calcs::calculate_stamina_regen_rate(&attributes);
        let mana_regen = resource_calcs::calculate_mana_regen_rate(&attributes);

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
        let queue_capacity = attributes.window_size();
        let reaction_queue = ReactionQueue::new(queue_capacity);

        // Spawn NPC entity (will be discovered when player gets near)
        let npc_loc = Loc::new(npc_location);
        let npc_entity = commands
            .spawn((
                EntityType::Actor(actor_impl),
                npc_loc,
                attributes,
                health,
                stamina,
                mana,
                combat_state,
                reaction_queue,
                Gcd::new(),                   // GCD tracking for abilities
                LastAutoAttack::default(),    // Auto-attack cooldown tracking
                Physics,
                Behaviour::default(),         // Will be set by AI system based on archetype
                EngagementMember(engagement_entity),
            ))
            .id();

        // Add AI behavior based on archetype (ADR-014 Phase 3)
        match archetype {
            EnemyArchetype::Berserker | EnemyArchetype::Juggernaut | EnemyArchetype::Defender => {
                // Melee aggressors - Chase behavior (aggressive pursuit)
                let chase = crate::server::systems::behaviour::chase::Chase {
                    acquisition_range: 15,  // 15 hex aggro range
                    leash_distance: 30,     // 30 hex leash distance
                    attack_range: 1,        // 1 hex melee range
                };
                commands.entity(npc_entity).insert((
                    NearestNeighbor::new(npc_entity, npc_loc),
                    chase,
                    NpcRecovery::for_archetype(archetype),  // SOW-018: Per-archetype recovery timer
                    crate::common::components::target::Target::default(),  // Target tracking for AI
                    Heading::default(),
                    Position::at_tile(npc_location),
                    AirTime::default(),
                ));
            }
            EnemyArchetype::Kiter => {
                // Ranged kiter - Kite behavior (Forest Sprite stats)
                let kite = crate::server::systems::behaviour::kite::Kite::forest_sprite();
                commands.entity(npc_entity).insert((
                    NearestNeighbor::new(npc_entity, npc_loc),
                    kite,
                    crate::common::components::target::Target::default(),  // Target tracking for AI
                    Heading::default(),
                    Position::at_tile(npc_location),
                    AirTime::default(),
                ));
            }
        }

        // Track NPC in engagement
        engagement.add_npc(npc_entity);

        // NOTE: NPCs are NOT broadcast immediately - they're sent when clients discover the chunk
        // See try_discover_chunk in actor.rs which sends all actors when chunk is discovered
        // This prevents "ghost NPCs" when engagements are abandoned but not yet cleaned up
    }

    // Update engagement with tracked NPCs
    commands.entity(engagement_entity).insert(engagement);
}

/// Get random hex offset for NPC positioning
/// Returns one of the 6 hex directions based on index
fn get_random_hex_offset(index: usize) -> Qrz {
    use qrz::DIRECTIONS;
    DIRECTIONS[index % 6]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_offset() {
        // Should cycle through 6 directions
        let offset0 = get_random_hex_offset(0);
        let offset1 = get_random_hex_offset(1);
        let offset6 = get_random_hex_offset(6);

        // Index 6 should wrap to index 0
        assert_eq!(offset0, offset6);

        // Offsets should be different
        assert_ne!(offset0, offset1);
    }
}
