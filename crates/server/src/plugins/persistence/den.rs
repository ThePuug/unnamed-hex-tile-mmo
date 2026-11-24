//! A den as the store keeps it: when it came to be as it is is a moment
//! of the calendar, kept as it is.

use common_bevy::{archetype::EnemyArchetype, systems::When};
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::systems::dens::{Den, DenState};

#[derive(Serialize, Deserialize)]
struct State {
    archetype: u32,
    level: u8,
    size: u8,
    at: Qrz,
    cleared: bool,
    since: When,
}

/// `den`, which has stood, as the store keeps it; none for one that has
/// not.
pub fn kept(den: &Den) -> Option<String> {
    let state = State {
        archetype: den.archetype.id(),
        level: den.level,
        size: den.size,
        at: den.at?,
        cleared: den.state == DenState::Cleared,
        since: den.since,
    };
    Some(serde_json::to_string(&state).expect("a den serializes"))
}

/// The den `state` keeps, with nothing of it out in the world, as the
/// calendar reads `now`. None where it cannot be read, its archetype is
/// gone from the game, or it was cleared long enough ago to have decayed.
pub fn recalled(state: serde_json::Value, now: When) -> Option<Den> {
    let state: State = serde_json::from_value(state).ok()?;
    let den = Den {
        archetype: EnemyArchetype::kept_as(state.archetype)?,
        level: state.level,
        size: state.size,
        at: Some(state.at),
        state: if state.cleared { DenState::Cleared } else { DenState::UNWATCHED },
        since: state.since,
    };
    (!den.decayed(now)).then_some(den)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::dens::DECAY;

    fn den(state: DenState, since: When) -> Den {
        Den { archetype: EnemyArchetype::Flanker, level: 7, size: 3, at: Some(Qrz { q: 4, r: -9, z: 12 }), state, since }
    }

    #[test]
    fn a_den_is_recalled_where_and_as_it_stood_with_nothing_out() {
        let out = DenState::Standing { pack: Some(bevy::prelude::Entity::from_raw_u32(3).unwrap()), nest: None, refill: None };
        let text = kept(&den(out, When(20_000))).unwrap();
        let back = recalled(serde_json::from_str(&text).unwrap(), When(900_000_000)).unwrap();
        assert_eq!(back, den(DenState::UNWATCHED, When(20_000)), "its pack is not kept");
        assert!(kept(&Den { at: None, ..den(out, When(0)) }).is_none(), "one that has not stood is not kept");
    }

    #[test]
    fn a_cleared_den_decays_on_the_calendar_it_was_cleared_on() {
        let cleared = When(2_000_000);
        let text = kept(&den(DenState::Cleared, cleared)).unwrap();
        let state = || serde_json::from_str(&text).unwrap();
        let back = recalled(state(), cleared + DECAY - std::time::Duration::from_secs(1)).unwrap();
        assert_eq!((back.state, back.since), (DenState::Cleared, cleared));
        assert!(recalled(state(), cleared + DECAY).is_none(), "decayed while the server was down");
        assert!(recalled(state(), When(0)).is_some(), "a calendar moved back before it was cleared leaves it cleared");
    }
}
