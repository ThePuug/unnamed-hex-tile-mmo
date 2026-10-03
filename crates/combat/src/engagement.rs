//! # Engagements
//!
//! Builds an engagement — a group of NPCs at a location — and finds where
//! a party stands to engage. Where a den or party goes in the world is the
//! installer's to say.

use bevy::prelude::*;
use qrz::Qrz;

use common_bevy::{
    components::{
        behaviour::Side,
        engagement::{Engagement, EngagementMember, LastPlayerProximity},
        equipment::{Equipment, Item, Piece},
        entity_type::{
            actor::{ActorIdentity, ActorImpl, Origin},
            EntityType,
        },
        heading::Heading,
        hex_assignment::HexAssignment,
        position::Position,
        AirTime, Loc,
    },
    plugins::nntree::NearestNeighbor,
    archetype::{calculate_enemy_attributes, EnemyArchetype},
    systems::combat::resources::Fighter,
};
use common_bevy::tuning::Tuning;

/// How far apart two parties staged to fight stand, as the balance arena
/// sets its teams apart: inside the range either acquires a target from.
pub const STAGE_GAP: i32 = 24;

/// The tile a party stands on to engage one at `from`, ahead along `dir`:
/// `STAGE_GAP` out, drawn in until it stands where the one there spots it,
/// by the measure of reach (`behaviour::spotted`). On flat ground it is the
/// arena's gap.
pub fn engaging_at(from: Qrz, dir: Qrz, elevation: impl Fn(i32, i32) -> i32) -> Qrz {
    let range = crate::behaviour::ACQUISITION_RANGE as i32;
    (1..=STAGE_GAP).rev()
        .map(|d| from + dir * d)
        .map(|at| Qrz { z: elevation(at.q, at.r) + 1, ..at })
        .find(|at| Loc::new(*at).distance(&Loc::new(from)) <= range)
        .unwrap_or(from + dir)
}

/// Spawn an engagement of `npc_count` NPCs of `archetype` at `level`, on
/// `side`, round `location`; each stands a tile above the ground
/// `elevation` gives at its column. Returns the engagement.
pub fn spawn_engagement(
    tuning: &Tuning,
    location: Qrz,
    archetype: EnemyArchetype,
    side: Side,
    level: u8,
    npc_count: u8,
    elevation: impl Fn(i32, i32) -> i32,
    commands: &mut Commands,
    time: &Time,
) -> Entity {
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
            approach: archetype.profile().approach,
            resilience: archetype.profile().resilience,
            identity: ActorIdentity::Npc(archetype),
        };

        let npc_loc = Loc::new(npc_location);
        let npc_entity = commands
            .spawn((
                EntityType::Actor(actor_impl),
                npc_loc,
                Fighter::new(tuning, attributes, time.elapsed()),
                side,
                EngagementMember(engagement_entity),
            ))
            .id();
        // An NPC wears what the server gives it, with no bag and no asking.
        let worn = kit(archetype);
        if worn.items().next().is_some() {
            commands.entity(npc_entity).insert(worn);
        }

        let chase = crate::behaviour::chase::Chase {
            acquisition_range: crate::behaviour::ACQUISITION_RANGE,
            leash_distance: crate::behaviour::LEASH_DISTANCE,
            attack_range: common_bevy::components::AttackRange::default().0,
        };
        commands.entity(npc_entity).insert((
            NearestNeighbor::new(npc_entity, npc_loc),
            chase,
            crate::behaviour::perception::Skill::default(),
            crate::behaviour::Bar::of(archetype),
            crate::behaviour::perception::Sight::default(),
            crate::behaviour::moves::Move::default(),
            common_bevy::components::AttackRange::default(),
            Heading::default(),
            common_bevy::components::Turn::default(),
            Position::at_tile(npc_location),
            AirTime::default(),
            common_bevy::components::movement_intent_state::MovementIntentState::default(),
        ));

        engagement.add_npc(npc_entity);
    }

    commands.entity(engagement_entity).insert(engagement);
    engagement_entity
}

pub fn get_random_hex_offset(index: usize) -> Qrz {
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

    #[test]
    fn on_the_flat_a_party_engages_at_the_stage_gap() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        assert_eq!(engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |_, _| 0), Qrz { q: STAGE_GAP, r: 0, z: 1 });
    }

    #[test]
    fn up_a_slope_a_party_draws_in_until_it_is_spotted() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        let at = engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |q, _| q);
        assert!(Loc::new(at).distance(&Loc::new(from)) <= crate::behaviour::ACQUISITION_RANGE as i32, "{at:?}");
        assert!(at.q < STAGE_GAP, "drawn in from the gap: {at:?}");
    }
}
