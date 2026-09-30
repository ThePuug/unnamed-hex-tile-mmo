//! # Engagement Activation System
//!
//! Builds an engagement — a group of NPCs at a location. Nothing selects
//! sites: a den or a party is one an admin asks for (`Event::SpawnParty`).

use std::ops::RangeInclusive;

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
        npc_recovery::NpcRecovery,
        position::Position,
        AirTime, Loc,
    },
    message::{Event, Try},
    plugins::nntree::NearestNeighbor,
    spatial_difficulty::{calculate_enemy_attributes, EnemyArchetype},
    systems::combat::resources::Fighter,
};

/// How long a live NPC waits, in milliseconds, once it can afford its
/// signature and is out of recovery, before it uses it, drawn afresh each
/// use: play's pace, spreading a pack apart. It sits outside the balance,
/// which the arena measures with no wait at all.
pub const SIGNATURE_WAIT_MS: RangeInclusive<u64> = 3000..=6000;

/// Tiles between the edge of a den's acquisition range and the actor it
/// is placed ahead of, so the fight starts when that one walks in.
const DEN_CLEARANCE: i32 = 5;

/// How far an NPC of `archetype` reaches with its auto-attack: melee reach,
/// or for a Kiter the edge of its band, where it fires from.
fn attack_range(archetype: EnemyArchetype) -> i32 {
    match archetype {
        EnemyArchetype::Kiter => crate::systems::behaviour::KITER_REACH,
        _ => common_bevy::components::AttackRange::default().0,
    }
}

/// How far apart two parties staged to fight stand, as the balance arena
/// sets its teams apart: inside the range either acquires a target from.
pub const STAGE_GAP: i32 = 24;

/// Sides of their own for parties staged to engage, handed out in turn
/// past the players' and the wild's: every such party hostile to every
/// other and to everyone else.
#[derive(Resource)]
pub struct Parties(u8);

impl Default for Parties {
    fn default() -> Self {
        Self(2)
    }
}

impl Parties {
    fn next(&mut self) -> Side {
        let side = Side(self.0);
        self.0 = self.0.checked_add(1).unwrap_or(2);
        side
    }
}

/// The tile a party stands on to engage one at `from`, ahead along `dir`:
/// `STAGE_GAP` out, drawn in until it stands where the one there spots it,
/// by the measure of reach (`behaviour::spotted`). On flat ground it is the
/// arena's gap.
pub fn engaging_at(from: Qrz, dir: Qrz, elevation: impl Fn(i32, i32) -> i32) -> Qrz {
    let range = crate::systems::behaviour::ACQUISITION_RANGE as i32;
    (1..=STAGE_GAP).rev()
        .map(|d| from + dir * d)
        .map(|at| Qrz { z: elevation(at.q, at.r) + 1, ..at })
        .find(|at| Loc::new(*at).distance(&Loc::new(from)) <= range)
        .unwrap_or(from + dir)
}

/// Places the party an admin asks for ahead of the actor it names: a den
/// on the wild side, far enough that none of the pack, standing a tile out
/// from it, has the actor in acquisition range; or engaging it, on a side
/// of its own. Acquisition measures `|Δz|` on top of the flat distance, so
/// a flat distance past the range is past it on any slope.
pub fn try_spawn_party(
    mut reader: MessageReader<Try>,
    mut commands: Commands,
    query: Query<(&Loc, &Heading)>,
    time: Res<Time>,
    registry: Res<crate::resources::event_registry::EventRegistry>,
    mut parties: ResMut<Parties>,
) {
    for message in reader.read() {
        let Try { event: Event::SpawnParty { ent, archetype, level, size, engage } } = message else { continue };
        let Ok((loc, heading)) = query.get(*ent) else { continue };
        let at = if *engage {
            engaging_at(**loc, heading.hex_dir(), |q, r| registry.elevation_at(q, r))
        } else {
            let ahead = den_ahead(**loc, *heading);
            Qrz { z: registry.elevation_at(ahead.q, ahead.r) + 1, ..ahead }
        };
        let side = if *engage { parties.next() } else { Side::WILD };
        info!("party: {size}x{archetype:?}@{level} on {side:?} at {at:?}, {} {ent} at {:?}", if *engage { "engaging" } else { "ahead of" }, **loc);
        spawn_engagement(at, *archetype, side, *level, *size, |q, r| registry.elevation_at(q, r), SIGNATURE_WAIT_MS, &mut commands, &time);
    }
}

/// The tile a den goes on, ahead of an actor at `tile` facing `heading`;
/// its z is the actor's.
fn den_ahead(tile: Qrz, heading: Heading) -> Qrz {
    tile + heading.hex_dir() * (crate::systems::behaviour::ACQUISITION_RANGE as i32 + 1 + DEN_CLEARANCE)
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
    wait: RangeInclusive<u64>,
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
            approach: archetype.profile().approach,
            resilience: archetype.profile().resilience,
            identity: ActorIdentity::Npc(archetype),
        };

        let npc_loc = Loc::new(npc_location);
        let npc_entity = commands
            .spawn((
                EntityType::Actor(actor_impl),
                npc_loc,
                Fighter::new(attributes, time.elapsed()),
                side,
                EngagementMember(engagement_entity),
            ))
            .id();
        // An NPC wears what the server gives it, with no bag and no asking.
        let worn = kit(archetype);
        if worn.items().next().is_some() {
            commands.entity(npc_entity).insert(worn);
        }

        let chase = crate::systems::behaviour::chase::Chase {
            acquisition_range: crate::systems::behaviour::ACQUISITION_RANGE,
            leash_distance: crate::systems::behaviour::LEASH_DISTANCE,
            attack_range: attack_range(archetype),
        };
        commands.entity(npc_entity).insert((
            NearestNeighbor::new(npc_entity, npc_loc),
            chase,
            NpcRecovery::new(*wait.start(), *wait.end()),
            common_bevy::components::AttackRange(attack_range(archetype)),
            Heading::default(),
            common_bevy::components::Turn::default(),
            Position::at_tile(npc_location),
            AirTime::default(),
            common_bevy::components::movement_intent_state::MovementIntentState::default(),
        ));

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

    #[test]
    fn on_the_flat_a_party_engages_at_the_stage_gap() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        assert_eq!(engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |_, _| 0), Qrz { q: STAGE_GAP, r: 0, z: 1 });
    }

    #[test]
    fn up_a_slope_a_party_draws_in_until_it_is_spotted() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        let at = engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |q, _| q);
        assert!(Loc::new(at).distance(&Loc::new(from)) <= crate::systems::behaviour::ACQUISITION_RANGE as i32, "{at:?}");
        assert!(at.q < STAGE_GAP, "drawn in from the gap: {at:?}");
    }

    #[test]
    fn every_party_is_on_a_side_of_its_own() {
        let mut parties = Parties::default();
        let (a, b) = (parties.next(), parties.next());
        assert!(a.is_hostile_to(b));
        assert!(a.is_hostile_to(Side::PLAYERS) && a.is_hostile_to(Side::WILD));
    }
    use common_bevy::components::heading::HEADING_SLOTS;

    #[test]
    fn no_member_of_a_placed_den_starts_in_acquisition_range() {
        let player = Qrz { q: 104289, r: -4677, z: 0 };
        for slot in 0..HEADING_SLOTS {
            let den = den_ahead(player, Heading::from_slot(slot));
            for i in 0..3 {
                let member = den + get_random_hex_offset(i);
                assert!(
                    player.flat_distance(&member) > crate::systems::behaviour::ACQUISITION_RANGE as i32,
                    "member {i} at slot {slot} starts in range",
                );
            }
        }
    }
}
