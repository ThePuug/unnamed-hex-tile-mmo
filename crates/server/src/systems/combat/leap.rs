//! The caster's own movement as an ability makes it. Like any of an ability's
//! effects (`landing`), it takes one of two timings: with the cast, when the
//! move stands on its own — Disengage's leap clear of the blow it answers, or
//! onto the attacker it has already broken from —
//! or with the threat, when it is worth something only if the blow lands —
//! the Kiter's leap, which rides its Volley's slow, so a Kiter never leaps
//! from a target that can still follow it. Either timing moves through here.

use bevy::prelude::*;
use common_bevy::{
    components::{position::Position, Loc},
    message::{Component as MessageComponent, Do, Event as GameEvent},
    resources::map::Map,
};
use qrz::Qrz;

/// How long a leap's slide takes on screen.
const LEAP_MS: u16 = 250;

/// The standing tile `tiles` steps straight away from `from`, walking the
/// ground from `at`, each step the neighbour furthest from `from`; `None`
/// when not even the first step leads further away.
pub fn away(map: &Map, at: Qrz, from: Qrz, tiles: usize) -> Option<Qrz> {
    let (mut ground, _) = map.get_by_qr(at.q, at.r)?;
    for _ in 0..tiles {
        let Some((next, _)) = map.neighbors(ground).into_iter()
            .max_by_key(|(neighbor, _)| neighbor.flat_distance(&from))
            .filter(|(neighbor, _)| neighbor.flat_distance(&from) > ground.flat_distance(&from))
        else {
            break;
        };
        ground = next;
    }
    let landing = ground + Qrz::Z;
    (landing != at).then_some(landing)
}

/// The standing tile up to `tiles` steps toward `to`, walking the ground
/// from `at`, each step the neighbour nearest `to`, stopping beside it;
/// `None` when not even the first step closes on it.
pub fn toward(map: &Map, at: Qrz, to: Qrz, tiles: usize) -> Option<Qrz> {
    let (mut ground, _) = map.get_by_qr(at.q, at.r)?;
    for _ in 0..tiles {
        let Some((next, _)) = map.neighbors(ground).into_iter()
            .filter(|(neighbor, _)| neighbor.flat_distance(&to) >= 1)
            .min_by_key(|(neighbor, _)| neighbor.flat_distance(&to))
            .filter(|(neighbor, _)| neighbor.flat_distance(&to) < ground.flat_distance(&to))
        else {
            break;
        };
        ground = next;
    }
    let landing = ground + Qrz::Z;
    (landing != at).then_some(landing)
}

/// Moves `ent` to the standing tile `landing` at once, sliding there on
/// every client.
pub fn leap(ent: Entity, landing: Qrz, commands: &mut Commands, writer: &mut MessageWriter<Do>) {
    let Ok(mut entity) = commands.get_entity(ent) else { return };
    entity.try_insert((Loc::new(landing), Position::at_tile(landing)));
    writer.write(Do { event: GameEvent::Displace { ent, destination: landing + Qrz::Z, duration_ms: LEAP_MS, around: None } });
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
    fn a_leap_toward_closes_and_stops_beside() {
        let map = flat();
        let at = Qrz { q: 0, r: 0, z: 1 };
        let far = Qrz { q: 10, r: 0, z: 1 };
        let landing = toward(&map, at, far, 4).unwrap();
        assert_eq!(landing.flat_distance(&far), 6, "four tiles closer");
        let beside = toward(&map, at, far, 20).unwrap();
        assert_eq!(beside.flat_distance(&far), 1, "never onto it");
        assert_eq!(toward(&map, beside, far, 3), None, "beside it, no closer to go");
    }
}
