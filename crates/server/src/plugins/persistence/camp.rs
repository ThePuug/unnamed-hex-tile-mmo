//! A camp as the store keeps it. Its times are moments of the calendar,
//! kept as they are; what it holds is kept as a pile's is, each stack by
//! its number ([`stored`]).

use std::time::Duration;

use common_bevy::systems::When;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use common_bevy::chart::Leg;

use super::stored;
use crate::systems::{
    haul::Trip,
    settlers::{Camp, Step},
};

/// A wagon's trip as kept: its times the calendar's, what it carries as
/// a pile's is.
#[derive(Serialize, Deserialize)]
struct Road {
    to: (i32, i32),
    way: Vec<Leg>,
    done: usize,
    /// The milliseconds of daytime put into the leg it is on.
    spare: u64,
    back: bool,
    counted: When,
    cargo: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
struct State {
    arrives: When,
    arrived: bool,
    /// The tiles a camp kept before hubs had sites stood its things on.
    #[serde(default, skip_serializing)]
    post: Option<(i32, i32)>,
    #[serde(default, skip_serializing)]
    own: Option<(i32, i32)>,
    #[serde(default, skip_serializing)]
    spare: Option<(i32, i32)>,
    #[serde(default, skip_serializing)]
    wagon: Option<(i32, i32)>,
    /// Kept only by a camp of before hubs had sites.
    #[serde(default, skip_serializing)]
    claimed: Option<bool>,
    /// What it has of the things it took down: hide, wood, stone.
    #[serde(default)]
    credit: (u32, u32, u32),
    #[serde(default)]
    clearing: Vec<(i32, i32)>,
    stock: Vec<Value>,
    worked: When,
    /// What it has begun and not yet raised: the step of its line, by its
    /// number, the tile, and the milliseconds of work its stage still
    /// wants.
    #[serde(default)]
    rising: Option<(u32, (i32, i32), u64)>,
    /// What is loaded on its wagon.
    #[serde(default)]
    load: Vec<Value>,
    #[serde(default)]
    trip: Option<Road>,
}

/// `camp`, holding `dormant` besides, as the store keeps it.
pub fn kept(camp: &Camp, dormant: &[Value]) -> String {
    let state = State {
        arrives: camp.arrives,
        arrived: camp.arrived,
        rising: camp.rising.map(|(step, at, left)| (step.id(), at, left.as_millis() as u64)),
        post: None,
        own: None,
        spare: None,
        wagon: None,
        claimed: None,
        credit: (camp.credit.hide, camp.credit.wood, camp.credit.stone),
        clearing: camp.clearing.clone(),
        stock: camp.stock.iter().copied().map(stored::stack).chain(dormant.iter().cloned()).collect(),
        worked: camp.worked,
        load: camp.load.iter().copied().map(stored::stack).collect(),
        trip: camp.trip.as_ref().map(|trip| Road {
            to: trip.to,
            way: trip.way.clone(),
            done: trip.done,
            spare: trip.spare.as_millis() as u64,
            back: trip.back,
            counted: trip.counted,
            cargo: trip.cargo.iter().copied().map(stored::stack).collect(),
        }),
    };
    serde_json::to_string(&state).expect("a camp serializes")
}

/// The camp `state` keeps, known by `key`, and what it holds that this
/// build cannot play. None where it cannot be read. One kept before hubs
/// had sites is to take down what it stood off them, its hearth where
/// that is on no site, and build with that again.
pub fn recalled(key: (i32, i32), state: Value) -> Option<(Camp, Vec<Value>)> {
    let state: State = serde_json::from_value(state).ok()?;
    let (stock, mut dormant) = stored::stacks(state.stock);
    // What the wagon holds that this build cannot play stays kept, with
    // the camp's own
    let (load, unloaded) = stored::stacks(state.load);
    dormant.extend(unloaded);
    let trip = state.trip.map(|road| {
        let (cargo, lost) = stored::stacks(road.cargo);
        dormant.extend(lost);
        Trip { to: road.to, way: road.way, done: road.done, spare: Duration::from_millis(road.spare), back: road.back, counted: road.counted, cargo }
    });
    let camp = Camp {
        arrives: state.arrives,
        arrived: state.arrived,
        rising: state.rising.and_then(|(step, at, left)| Some((Step::kept_as(step)?, at, Duration::from_millis(left)))),
        credit: common::building::Needs { hide: state.credit.0, wood: state.credit.1, stone: state.credit.2 },
        clearing: state
            .clearing
            .into_iter()
            .chain([state.post, state.own, state.spare, state.wagon].into_iter().flatten())
            .chain(state.claimed.is_some().then_some(key).filter(|&key| common_bevy::sites::site_near(key) != key))
            .collect(),
        stock,
        worked: state.worked,
        load,
        trip,
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
        camp.rising = Some((Step::Clear, (5, 2), Duration::from_secs(40)));
        camp.credit = common::building::Needs { hide: 1, wood: 8, stone: 2 };
        camp.clearing = vec![(5, 2), (6, 2)];
        camp.worked = When(1_500_000);
        camp.stock = vec![Stack { kind: Stackable::Material(Material::Hide), count: 4 }];
        camp.load = vec![Stack { kind: Stackable::Material(Material::Fibre), count: 2 }];
        let leg = Leg { from: (1, 2), by: (3, 4), to: (4, 4), steps: 5 };
        let mut trip = Trip::begun((90, -40), vec![leg, leg], vec![Stack { kind: Stackable::Material(Material::Softwood), count: 9 }], When(1_600_000));
        (trip.done, trip.spare, trip.back) = (1, Duration::from_millis(700), true);
        camp.trip = Some(trip);
        let coin = serde_json::json!({"coin": 1, "count": 2});
        let text = kept(&camp, &[coin.clone()]);
        let (back, dormant) = recalled((6, 3), serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!((back, dormant), (camp, vec![coin]));
        assert!(recalled((6, 3), serde_json::json!("no camp")).is_none());
        let old = serde_json::json!({"arrives": 5, "arrived": true, "post": [1, 2], "own": [3, -1], "spare": null, "wagon": null, "claimed": true, "fortifying": false, "stock": [], "worked": 9, "rising": [2, [4, 4], 100]});
        let (on, _) = recalled((6, 3), old.clone()).unwrap();
        assert_eq!((on.clearing, on.rising.map(|(step, ..)| step)), (vec![(1, 2), (3, -1)], Some(Step::Home)), "what an old camp stood off its sites comes down");
        let (off, _) = recalled((7, 3), old).unwrap();
        assert_eq!(off.clearing, vec![(1, 2), (3, -1), (7, 3)], "and its hearth, where that is on no site");
    }
}
