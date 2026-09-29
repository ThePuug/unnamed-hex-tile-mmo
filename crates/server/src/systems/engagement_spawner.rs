//! # Engagement Activation System
//!
//! Builds an engagement — a group of NPCs at a location — and tracks which
//! sites already have one.
//!
//! Nothing selects sites: the only den is one an admin asks for with
//! `Event::SpawnDen`, and `ActiveSpawners` is only ever cleared.

use bevy::prelude::*;
use qrz::Qrz;
use rand::Rng;

use common_bevy::{
    components::{
        behaviour::{Behaviour, PlayerControlled, Side},
        engagement::{Engagement, EngagementMember, LastPlayerProximity},
        equipment::{Equipment, Item, Piece},
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
        ActorAttributes, AirTime, LastAutoAttack, Physics, Loc,
    },
    message::{Event, Try},
    plugins::nntree::NearestNeighbor,
    spatial_difficulty::{calculate_enemy_attributes, EnemyArchetype},
    systems::combat::resources as resource_calcs,
};

/// Tracks which sites have active engagements.
/// Cleared when engagement is cleaned up (allows re-activation).
#[derive(Resource, Default)]
pub struct ActiveSpawners(pub std::collections::HashSet<(i32, i32)>);

/// Tiles between the edge of a den's acquisition range and the player who
/// asked for it, so the fight starts when the player walks in.
const DEN_CLEARANCE: i32 = 5;

/// How far an NPC of `archetype` reaches with its auto-attack: melee reach,
/// or for a Kiter the edge of its band, where it fires from.
fn attack_range(archetype: EnemyArchetype) -> i32 {
    match archetype {
        EnemyArchetype::Kiter => crate::systems::behaviour::kite::KITER_REACH,
        _ => common_bevy::components::AttackRange::default().0,
    }
}

/// The range an NPC of `archetype` acquires a target within.
fn acquisition_range(archetype: EnemyArchetype) -> u32 {
    match archetype {
        EnemyArchetype::Kiter => crate::systems::behaviour::kite::Kite::forest_sprite().acquisition_range,
        EnemyArchetype::Berserker | EnemyArchetype::Juggernaut | EnemyArchetype::Defender | EnemyArchetype::Skirmisher | EnemyArchetype::Ambusher => crate::systems::behaviour::ACQUISITION_RANGE,
    }
}

/// Levels below the player each of a den's pair stands: the level rule puts
/// a pair this far below about even with one of the player's level.
const PAIR_DEFICIT: u8 = 3;

/// A den one player can beat alone, `(npc_count, level)` for a player at
/// `player_level`: one NPC at its level, or with `pair` two `PAIR_DEFICIT`
/// levels below it.
fn den_for(player_level: u8, pair: bool) -> (u8, u8) {
    if pair { (2, player_level.saturating_sub(PAIR_DEFICIT)) } else { (1, player_level) }
}

/// Places the den a player asks for straight ahead of it, far enough that
/// none of the pack, standing a tile out from the den, has the player in
/// acquisition range. Acquisition measures `|Δz|` on top of the flat
/// distance, so a flat distance past the range is past it on any slope.
/// The den is one the player can beat alone (`den_for`), either kind alike.
pub fn try_spawn_den(
    mut reader: MessageReader<Try>,
    mut commands: Commands,
    query: Query<(&Loc, &Heading, &ActorAttributes), With<PlayerControlled>>,
    time: Res<Time>,
    registry: Res<crate::resources::event_registry::EventRegistry>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
) {
    for message in reader.read() {
        let Try { event: Event::SpawnDen { ent, archetype } } = message else { continue };
        let Ok((loc, heading, attrs)) = query.get(*ent) else { continue };
        let ahead = den_ahead(**loc, *heading, *archetype);
        let den = Qrz { q: ahead.q, r: ahead.r, z: registry.elevation_at(ahead.q, ahead.r) + 1 };
        info!("den: {archetype:?} at {den:?}, ahead of {ent} at {:?}", **loc);
        let player_level = attrs.total_level().min(u8::MAX as u32) as u8;
        let (npc_count, level) = den_for(player_level, rand::rng().random_bool(0.5));
        info!("den: {npc_count}x{archetype:?}@{level} for a level-{player_level} player");
        spawn_engagement(den, *archetype, Side::WILD, level, npc_count, |q, r| registry.elevation_at(q, r), &tuning, &mut commands, &time);
    }
}

/// How far ahead of the player a staged fight's middle stands: beyond the
/// range either team acquires a target from, so they pick each other.
const STAGE_AHEAD: i32 = crate::systems::behaviour::ACQUISITION_RANGE as i32 + 3;

/// How far either team of a staged fight stands from its middle, as the
/// balance arena sets its teams apart.
const STAGE_APART: i32 = 12;

/// Stages a fight ahead of the player who asks: two teams on sides of their
/// own, hostile to each other and to everyone else, set across the player's
/// line of sight so a camera behind the player sees both.
pub fn try_stage_fight(
    mut reader: MessageReader<Try>,
    mut commands: Commands,
    query: Query<(&Loc, &Heading), With<PlayerControlled>>,
    time: Res<Time>,
    registry: Res<crate::resources::event_registry::EventRegistry>,
    tuning: Res<crate::resources::tuning::ArchetypeTuning>,
) {
    for message in reader.read() {
        let Try { event: Event::StageFight { ent, west, east, level, b_level, size, b_size } } = message else { continue };
        let Ok((loc, heading)) = query.get(*ent) else { continue };
        let middle = **loc + heading.hex_dir() * STAGE_AHEAD;
        // Across the line of sight: a quarter turn from the way the player faces
        let across = heading.turned(common_bevy::components::heading::HEADING_SLOTS as i32 / 4).hex_dir() * STAGE_APART;
        for (archetype, side, level, size, at) in [
            (*west, Side(2), *level, *size, middle - across),
            (*east, Side(3), *b_level, *b_size, middle + across),
        ] {
            let den = Qrz { q: at.q, r: at.r, z: registry.elevation_at(at.q, at.r) + 1 };
            info!("stage: {size}x{archetype:?}@{level} at {den:?}, ahead of {ent} at {:?}", **loc);
            spawn_engagement(den, archetype, side, level, size, |q, r| registry.elevation_at(q, r), &tuning, &mut commands, &time);
        }
    }
}

/// The tile a den of `archetype` goes on, ahead of a player at `tile`
/// facing `heading`; its z is the player's.
fn den_ahead(tile: Qrz, heading: Heading, archetype: EnemyArchetype) -> Qrz {
    tile + heading.hex_dir() * (acquisition_range(archetype) as i32 + 1 + DEN_CLEARANCE)
}

/// Spawn an engagement of `npc_count` NPCs of `archetype` at `level`, on
/// `side`, round `location`; each stands a tile above the ground
/// `elevation` gives at its column.
pub fn spawn_engagement(
    location: Qrz,
    archetype: EnemyArchetype,
    side: Side,
    level: u8,
    npc_count: u8,
    elevation: impl Fn(i32, i32) -> i32,
    tuning: &crate::resources::tuning::ArchetypeTuning,
    commands: &mut Commands,
    time: &Time,
) {

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
        let npc_z = elevation(npc_location_base.q, npc_location_base.r);
        let npc_location = Qrz { q: npc_location_base.q, r: npc_location_base.r, z: npc_z + 1 };

        let actor_impl = ActorImpl {
            origin: Origin::Evolved,
            approach: archetype.approach(),
            resilience: archetype.resilience(),
            identity: ActorIdentity::Npc(archetype.npc_type()),
        };

        let max_health = attributes.constitution_from(tuning.base_health, tuning.health_per_vitality);
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
                (Behaviour::default(), side),
                EngagementMember(engagement_entity),
                common_bevy::components::loaded_by::LoadedBy::default(),
            ))
            .id();
        // An NPC wears what the server gives it, with no bag and no asking.
        let worn = kit(archetype);
        if worn.items().next().is_some() {
            commands.entity(npc_entity).insert(worn);
        }

        let (delay_min, delay_max) = tuning.delay(archetype);
        match archetype {
            EnemyArchetype::Berserker | EnemyArchetype::Juggernaut | EnemyArchetype::Defender | EnemyArchetype::Skirmisher | EnemyArchetype::Ambusher => {
                let chase = crate::systems::behaviour::chase::Chase {
                    acquisition_range: crate::systems::behaviour::ACQUISITION_RANGE,
                    leash_distance: crate::systems::behaviour::LEASH_DISTANCE,
                    attack_range: attack_range(archetype),
                };
                commands.entity(npc_entity).insert((
                    NearestNeighbor::new(npc_entity, npc_loc),
                    chase,
                    NpcRecovery::new(delay_min, delay_max),
                    common_bevy::components::AttackRange(attack_range(archetype)),
                    common_bevy::components::target::Target::default(),
                    Heading::default(),
                    common_bevy::components::Turn::default(),
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
                    common_bevy::components::Turn::default(),
                    Position::at_tile(npc_location),
                    AirTime::default(),
                    common_bevy::components::AttackRange(attack_range(archetype)),
                    LastAutoAttack::default(),
                    NpcRecovery::new(delay_min, delay_max),
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

/// What an NPC of `archetype` wears: the Defender its plate cuirass and
/// sword-breaker; the rest go bare.
fn kit(archetype: EnemyArchetype) -> Equipment {
    let mut worn = Equipment::default();
    if archetype == EnemyArchetype::Defender {
        worn.wear(Item { piece: Piece::PlateCuirass, style: 0 });
        worn.wear(Item { piece: Piece::SwordBreaker, style: 0 });
    }
    worn
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::components::heading::HEADING_SLOTS;

    #[test]
    fn a_den_is_one_at_the_players_level_or_a_pair_below_it() {
        for player in [1, 3, 10, 20] {
            assert_eq!(den_for(player, false), (1, player));
            let (count, level) = den_for(player, true);
            assert_eq!(count, 2);
            assert!(level < player || player == 0, "a pair stands below a level-{player} player");
        }
    }

    #[test]
    fn no_member_of_a_placed_den_starts_in_acquisition_range() {
        let player = Qrz { q: 104289, r: -4677, z: 0 };
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender, EnemyArchetype::Skirmisher] {
            for slot in 0..HEADING_SLOTS {
                let den = den_ahead(player, Heading::from_slot(slot), archetype);
                for i in 0..3 {
                    let member = den + get_random_hex_offset(i);
                    assert!(
                        player.flat_distance(&member) > acquisition_range(archetype) as i32,
                        "{archetype:?} member {i} at slot {slot} starts in range",
                    );
                }
            }
        }
    }
}
