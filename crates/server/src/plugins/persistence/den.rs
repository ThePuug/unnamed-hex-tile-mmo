//! A den as the store keeps it. The server's clock starts with the
//! server, so when a den came to be as it is is kept on the Unix clock
//! and read back onto this run's.

use std::time::Duration;

use common_bevy::{archetype::EnemyArchetype, moment::Moment};
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::systems::dens::{Den, DenState, DECAY};

#[derive(Serialize, Deserialize)]
struct State {
    archetype: u32,
    level: u8,
    size: u8,
    at: Qrz,
    cleared: bool,
    /// Milliseconds of the Unix clock.
    since: u64,
}

/// `den`, which has stood, as the store keeps it, at `now` on the server's
/// clock and `unix` milliseconds on the Unix clock; none for one that has
/// not.
pub fn kept(den: &Den, now: Moment, unix: u64) -> Option<String> {
    let state = State {
        archetype: den.archetype.id(),
        level: den.level,
        size: den.size,
        at: den.at?,
        cleared: den.state == DenState::Cleared,
        since: unix.saturating_sub(now.since(den.since).as_millis() as u64),
    };
    Some(serde_json::to_string(&state).expect("a den serializes"))
}

/// The den `state` keeps, with nothing of it out in the world, at `now`
/// on the server's clock and `unix` milliseconds on the Unix clock. None
/// where it cannot be read, its archetype is gone from the game, or it
/// was cleared long enough ago to have decayed.
pub fn recalled(state: serde_json::Value, now: Moment, unix: u64) -> Option<Den> {
    let state: State = serde_json::from_value(state).ok()?;
    let ago = Duration::from_millis(unix.saturating_sub(state.since));
    if state.cleared && ago >= DECAY {
        return None;
    }
    Some(Den {
        archetype: EnemyArchetype::kept_as(state.archetype)?,
        level: state.level,
        size: state.size,
        at: Some(state.at),
        state: if state.cleared { DenState::Cleared } else { DenState::UNWATCHED },
        // No earlier than this run's clock began
        since: now - ago.min(now.since(Moment::ZERO)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn den(state: DenState, since: Moment) -> Den {
        Den { archetype: EnemyArchetype::Flanker, level: 7, size: 3, at: Some(Qrz { q: 4, r: -9, z: 12 }), state, since }
    }

    #[test]
    fn a_den_is_recalled_where_and_as_it_stood_with_nothing_out() {
        let out = DenState::Standing { pack: Some(bevy::prelude::Entity::from_raw_u32(3).unwrap()), nest: None, refill: None };
        let now = Moment::from_millis(60_000);
        let text = kept(&den(out, Moment::from_millis(20_000)), now, 1_000_000).unwrap();
        let back = recalled(serde_json::from_str(&text).unwrap(), Moment::from_millis(5_000), 1_010_000).unwrap();
        assert_eq!(back, den(DenState::UNWATCHED, Moment::ZERO), "its pack is not kept, and it is no younger than this run");
        assert!(kept(&Den { at: None, ..den(out, now) }, now, 0).is_none(), "one that has not stood is not kept");
    }

    #[test]
    fn a_cleared_den_decays_on_the_clock_it_was_cleared_on() {
        let cleared_at = Moment::from_millis(30_000);
        let text = kept(&den(DenState::Cleared, cleared_at), Moment::from_millis(40_000), 2_000_000).unwrap();
        let state = || serde_json::from_str(&text).unwrap();
        let later = Moment::from_millis(500_000);
        let back = recalled(state(), later, 2_000_000 + 100_000).unwrap();
        assert_eq!(back.state, DenState::Cleared);
        assert_eq!(later.since(back.since), Duration::from_millis(110_000), "ten seconds before it was kept, a hundred since");
        assert!(recalled(state(), later, 2_000_000 + DECAY.as_millis() as u64).is_none(), "decayed while the server was down");
    }
}
