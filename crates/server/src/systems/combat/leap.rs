//! The caster's own movement as an ability makes it. Like any of an ability's
//! effects (`landing`), it takes one of two timings: with the cast, when the
//! move stands on its own — Disengage's leap clear of the blow it answers —
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

/// Moves `ent` to the standing tile `landing` at once, sliding there on
/// every client.
pub fn leap(ent: Entity, landing: Qrz, commands: &mut Commands, writer: &mut MessageWriter<Do>) {
    let Ok(mut entity) = commands.get_entity(ent) else { return };
    entity.try_insert((Loc::new(landing), Position::at_tile(landing)));
    writer.write(Do { event: GameEvent::Displace { ent, destination: landing + Qrz::Z, duration_ms: LEAP_MS, around: None } });
    writer.write(Do { event: GameEvent::Incremental { ent, component: MessageComponent::Loc(Loc::new(landing)) } });
}
