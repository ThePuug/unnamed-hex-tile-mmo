//! The movement an ability makes. Every such move is a walk over the ground
//! to a standing tile ([`away`], [`toward`]) and one [`slide`] there: the
//! actor stands on the tile at once on the server and is drawn sliding to
//! it on every client. A Leap's is made with the cast.
//!
//! No such move carries an NPC past its [`Leash`]: the walk stops at the
//! last tile inside it, so a skill never sends its user home to heal. A
//! player has no leash.

use bevy::prelude::*;
use common_bevy::{
    components::{position::Position, Loc},
    message::{Component as MessageComponent, Do, Event as GameEvent},
    resources::map::Map,
};
use qrz::Qrz;

/// How long a leap's slide takes on screen.
pub const LEAP_MS: u16 = 250;

/// The leash an NPC's moves keep inside: within `reach` tiles of its `den`,
/// as its chase measures it (`behaviour::chase`).
#[derive(Clone, Copy, Debug)]
pub struct Leash {
    pub den: Qrz,
    pub reach: i32,
}

impl Leash {
    /// Whether an actor on `leash`, or on none, may stand on `tile`
    fn holds(leash: Option<Leash>, tile: Qrz) -> bool {
        leash.is_none_or(|leash| tile.flat_distance(&leash.den) <= leash.reach)
    }
}

/// The standing tile up to `tiles` steps away from `from`, walking the
/// ground from `at` along the line from `from` through it (`Qrz::beyond`),
/// and stopping where the ground or its `leash` does; `None` when not even
/// the first step can be taken. From `from`'s own tile, which no line
/// leads on from, the first step is any the ground allows and the line
/// runs on from it.
pub fn away(map: &Map, at: Qrz, from: Qrz, tiles: usize, leash: Option<Leash>) -> Option<Qrz> {
    let (mut ground, _) = map.get_by_qr(at.q, at.r)?;
    let line = match at.flat_distance(&from) {
        0 => map.neighbors(ground).first().map_or(Vec::new(), |(off, _)| {
            std::iter::once(*off).chain(off.beyond(at, tiles.saturating_sub(1))).collect()
        }),
        _ => at.beyond(from, tiles),
    };
    for next in line {
        let Some((floor, _)) = map.neighbors(ground).into_iter()
            .find(|(neighbor, _)| (neighbor.q, neighbor.r) == (next.q, next.r))
            .filter(|(neighbor, _)| Leash::holds(leash, *neighbor))
        else {
            break;
        };
        ground = floor;
    }
    let landing = ground + Qrz::Z;
    (landing != at).then_some(landing)
}

/// The standing tile up to `tiles` steps toward `to`, walking the ground
/// from `at`, each step the neighbour nearest `to`, stopping beside it;
/// `None` when not even the first step closes on it.
pub fn toward(map: &Map, at: Qrz, to: Qrz, tiles: usize, leash: Option<Leash>) -> Option<Qrz> {
    let (mut ground, _) = map.get_by_qr(at.q, at.r)?;
    for _ in 0..tiles {
        let Some((next, _)) = map.neighbors(ground).into_iter()
            .filter(|(neighbor, _)| neighbor.flat_distance(&to) >= 1)
            .min_by_key(|(neighbor, _)| neighbor.flat_distance(&to))
            .filter(|(neighbor, _)| neighbor.flat_distance(&to) < ground.flat_distance(&to))
            .filter(|(neighbor, _)| Leash::holds(leash, *neighbor))
        else {
            break;
        };
        ground = next;
    }
    let landing = ground + Qrz::Z;
    (landing != at).then_some(landing)
}

/// Moves `ent` to the standing tile `landing` at once, sliding there on
/// every client over `duration_ms`: round the tile `around` on its ring
/// when given, else the straight way.
pub fn slide(ent: Entity, landing: Qrz, duration_ms: u16, around: Option<Qrz>, commands: &mut Commands, writer: &mut MessageWriter<Do>) {
    let Ok(mut entity) = commands.get_entity(ent) else { return };
    entity.try_insert((Loc::new(landing), Position::at_tile(landing)));
    writer.write(Do { event: GameEvent::Displace { ent, destination: landing, duration_ms, around } });
    writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Loc(Loc::new(landing)) } });
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::components::entity_type::EntityType;

    fn flat() -> Map {
        let mut qrz_map = qrz::Map::<EntityType>::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
        for q in -12..=12 {
            for r in -12..=12 {
                qrz_map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(default()));
            }
        }
        Map::new(qrz_map)
    }

    #[test]
    fn a_leap_away_opens_the_distance_and_stops_where_the_ground_does() {
        let map = flat();
        let from = Qrz { q: 0, r: 0, z: 1 };
        let at = Qrz { q: 1, r: 0, z: 1 };
        let landing = away(&map, at, from, 4, None).unwrap();
        assert_eq!(landing.flat_distance(&from), 5, "four tiles further off");
        assert_eq!(landing.z, 1, "standing on the ground");
        let edge = away(&map, at, from, 40, None).unwrap();
        assert!(edge.flat_distance(&from) <= 24, "no further than the ground runs");
        assert_eq!(away(&map, edge, from, 3, None), None, "at the edge, nowhere further to go");

        // It follows the line from its target, whichever side that stands on
        for side in from.ring(1) {
            let landing = away(&map, from, side, 4, None).unwrap();
            assert_eq!(landing - from, (from - side) * 4, "straight on from {side:?}");
        }
        assert!(away(&map, from, Qrz { z: 5, ..from }, 4, None).is_some(), "off its target's own tile, some way");
    }

    #[test]
    fn a_move_stops_at_the_last_tile_inside_its_leash() {
        let map = flat();
        let den = Qrz { q: 0, r: 0, z: 1 };
        let leash = Some(Leash { den, reach: 5 });
        let at = Qrz { q: 3, r: 0, z: 1 };
        let within = Qrz { q: 1, r: 0, z: 1 };

        let landing = away(&map, at, within, 6, leash).unwrap();
        assert_eq!(landing.flat_distance(&den), 5, "a leap clear stops where its leash does");
        assert_eq!(away(&map, landing, within, 6, leash), None, "and from there leads nowhere further out");
        assert_eq!(away(&map, at, within, 6, None).unwrap().flat_distance(&den), 9, "with no leash it runs its length");

        let beyond = Qrz { q: 11, r: 0, z: 1 };
        assert_eq!(toward(&map, at, beyond, 6, leash).unwrap().flat_distance(&den), 5, "a dive at one outside it stops short");
        assert_eq!(toward(&map, landing, within, 6, leash).unwrap().flat_distance(&within), 1, "a dive back inside is whole");
    }

    #[test]
    fn a_leap_toward_closes_and_stops_beside() {
        let map = flat();
        let at = Qrz { q: 0, r: 0, z: 1 };
        let far = Qrz { q: 10, r: 0, z: 1 };
        let landing = toward(&map, at, far, 4, None).unwrap();
        assert_eq!(landing.flat_distance(&far), 6, "four tiles closer");
        let beside = toward(&map, at, far, 20, None).unwrap();
        assert_eq!(beside.flat_distance(&far), 1, "never onto it");
        assert_eq!(toward(&map, beside, far, 3, None), None, "beside it, no closer to go");
    }
}
