//! A den as the store keeps it: where it stands and what lives in it.
//! Whether its nest stands is its colony's to keep
//! (`systems::dens::Colony`, kept as its own JSON).

use common_bevy::archetype::EnemyArchetype;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::systems::dens::{Den, GROUPS};

#[derive(Serialize, Deserialize)]
struct State {
    archetype: u32,
    /// Kept as `size` while a den's pack was one group.
    #[serde(alias = "size")]
    groups: u8,
    at: Qrz,
}

/// `den`, which has stood, as the store keeps it; none for one that has
/// not.
pub fn kept(den: &Den) -> Option<String> {
    let state = State { archetype: den.archetype.id(), groups: den.groups, at: den.at? };
    Some(serde_json::to_string(&state).expect("a den serializes"))
}

/// The den `state` keeps, with nothing of it out in the world. None where
/// it cannot be read or its archetype is gone from the game.
pub fn recalled(state: serde_json::Value) -> Option<Den> {
    let state: State = serde_json::from_value(state).ok()?;
    Some(Den {
        archetype: EnemyArchetype::kept_as(state.archetype)?,
        groups: state.groups.clamp(GROUPS.0, GROUPS.1),
        at: Some(state.at),
        out: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::dens::Out;

    #[test]
    fn a_den_is_recalled_where_it_stood_with_nothing_out() {
        let nest = bevy::prelude::Entity::from_raw_u32(3).unwrap();
        let den = Den {
            archetype: EnemyArchetype::Flanker,
            groups: 3,
            at: Some(Qrz { q: 4, r: -9, z: 12 }),
            out: Some(Out { nest, groups: [Some(nest), None, None, None], refill: None }),
        };
        let text = kept(&den).unwrap();
        let back = recalled(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(back, Den { out: None, ..den }, "its pack is not kept");
        assert!(kept(&Den { at: None, ..den }).is_none(), "one that has not stood is not kept");
    }

    #[test]
    fn a_den_kept_as_one_pack_is_recalled_in_groups() {
        let old = serde_json::json!({"archetype": EnemyArchetype::Flanker.id(), "level": 4, "size": 3, "at": {"q": 1, "r": 2, "z": 3}, "cleared": true, "since": 9});
        assert_eq!(recalled(old).map(|den| den.groups), Some(3));
    }
}
