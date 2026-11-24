//! A camp as the store keeps it. Its times are moments of the calendar,
//! kept as they are; what it holds is kept as a pile's is, each stack by
//! its number ([`stored`]).

use std::time::Duration;

use common_bevy::systems::When;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::stored;
use crate::systems::settlers::{Camp, Step};

#[derive(Serialize, Deserialize)]
struct State {
    arrives: When,
    arrived: bool,
    own: Option<(i32, i32)>,
    spare: Option<(i32, i32)>,
    wagon: Option<(i32, i32)>,
    claimed: bool,
    fortifying: bool,
    stock: Vec<Value>,
    worked: When,
    /// What it has begun and not yet raised: the step of its line, by its
    /// number, the tile, and the milliseconds of work its stage still
    /// wants.
    #[serde(default)]
    rising: Option<(u32, (i32, i32), u64)>,
}

/// `camp`, holding `dormant` besides, as the store keeps it.
pub fn kept(camp: &Camp, dormant: &[Value]) -> String {
    let state = State {
        arrives: camp.arrives,
        arrived: camp.arrived,
        rising: camp.rising.map(|(step, at, left)| (step.id(), at, left.as_millis() as u64)),
        own: camp.own,
        spare: camp.spare,
        wagon: camp.wagon,
        claimed: camp.claimed,
        fortifying: camp.fortifying,
        stock: camp.stock.iter().copied().map(stored::stack).chain(dormant.iter().cloned()).collect(),
        worked: camp.worked,
    };
    serde_json::to_string(&state).expect("a camp serializes")
}

/// The camp `state` keeps, and what it holds that this build cannot play.
/// None where it cannot be read.
pub fn recalled(state: Value) -> Option<(Camp, Vec<Value>)> {
    let state: State = serde_json::from_value(state).ok()?;
    let (stock, dormant) = stored::stacks(state.stock);
    let camp = Camp {
        arrives: state.arrives,
        arrived: state.arrived,
        rising: state.rising.and_then(|(step, at, left)| Some((Step::kept_as(step)?, at, Duration::from_millis(left)))),
        own: state.own,
        spare: state.spare,
        wagon: state.wagon,
        claimed: state.claimed,
        fortifying: state.fortifying,
        stock,
        worked: state.worked,
    };
    Some((camp, dormant))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Material, Stack, Stackable};

    /// A camp comes back as it was kept, its times the calendar's own;
    /// what this build cannot play of its materials stays kept.
    #[test]
    fn a_camp_is_recalled_as_it_was_kept() {
        let mut camp = Camp::founded(When(1_000_000));
        camp.own = Some((3, -1));
        camp.rising = Some((Step::Spare, (5, 2), Duration::from_secs(40)));
        camp.fortifying = true;
        camp.worked = When(1_500_000);
        camp.stock = vec![Stack { kind: Stackable::Material(Material::Hide), count: 4 }];
        let coin = serde_json::json!({"coin": 1, "count": 2});
        let text = kept(&camp, &[coin.clone()]);
        let (back, dormant) = recalled(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!((back, dormant), (camp, vec![coin]));
        assert!(recalled(serde_json::json!("no camp")).is_none());
    }
}
