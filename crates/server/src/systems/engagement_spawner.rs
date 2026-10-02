//! # Engagement placement
//!
//! Places the engagements an admin asks for in the world. Nothing selects
//! sites: a den or a party is one an admin asks for (`Event::SpawnParty`).

use bevy::prelude::*;
use qrz::Qrz;

use combat::{behaviour::ACQUISITION_RANGE, engagement::{engaging_at, spawn_engagement}};
use common_bevy::{
    components::{behaviour::Side, heading::Heading, Loc},
    message::{Event, Try},
};
use common_bevy::tuning::Tuning;

/// Tiles between the edge of a den's acquisition range and the actor it
/// is placed ahead of, so the fight starts when that one walks in.
const DEN_CLEARANCE: i32 = 5;

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

/// Places the party an admin asks for ahead of the actor it names: a den
/// on the wild side, far enough that none of the pack, standing a tile out
/// from it, has the actor in acquisition range; or engaging it, on a side
/// of its own. Acquisition measures `|Δz|` on top of the flat distance, so
/// a flat distance past the range is past it on any slope.
pub fn try_spawn_party(
    tuning: Res<Tuning>,
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
        spawn_engagement(&tuning, at, *archetype, side, *level, *size, |q, r| registry.elevation_at(q, r), &mut commands, &time);
    }
}

/// The tile a den goes on, ahead of an actor at `tile` facing `heading`;
/// its z is the actor's.
fn den_ahead(tile: Qrz, heading: Heading) -> Qrz {
    tile + heading.hex_dir() * (ACQUISITION_RANGE as i32 + 1 + DEN_CLEARANCE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use combat::engagement::get_random_hex_offset;

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
                    player.flat_distance(&member) > ACQUISITION_RANGE as i32,
                    "member {i} at slot {slot} starts in range",
                );
            }
        }
    }
}
