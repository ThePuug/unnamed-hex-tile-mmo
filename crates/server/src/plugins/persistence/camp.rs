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
    #[serde(default)]
    post: Option<(i32, i32)>,
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
        post: camp.post,
        own: camp.own,
        spare: camp.spare,
        wagon: camp.wagon,
        claimed: camp.claimed,
        fortifying: camp.fortifying,
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

/// The camp `state` keeps, and what it holds that this build cannot play.
/// None where it cannot be read.
pub fn recalled(state: Value) -> Option<(Camp, Vec<Value>)> {
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
        post: state.post,
        own: state.own,
        spare: state.spare,
        wagon: state.wagon,
        claimed: state.claimed,
        fortifying: state.fortifying,
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
        camp.post = Some((1, 2));
        camp.own = Some((3, -1));
        camp.rising = Some((Step::Spare, (5, 2), Duration::from_secs(40)));
        camp.fortifying = true;
        camp.worked = When(1_500_000);
        camp.stock = vec![Stack { kind: Stackable::Material(Material::Hide), count: 4 }];
        camp.load = vec![Stack { kind: Stackable::Material(Material::Fibre), count: 2 }];
        let leg = Leg { from: (1, 2), by: (3, 4), to: (4, 4), steps: 5 };
        let mut trip = Trip::begun((90, -40), vec![leg, leg], vec![Stack { kind: Stackable::Material(Material::Softwood), count: 9 }], When(1_600_000));
        (trip.done, trip.spare, trip.back) = (1, Duration::from_millis(700), true);
        camp.trip = Some(trip);
        let coin = serde_json::json!({"coin": 1, "count": 2});
        let text = kept(&camp, &[coin.clone()]);
        let (back, dormant) = recalled(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!((back, dormant), (camp, vec![coin]));
        assert!(recalled(serde_json::json!("no camp")).is_none());
    }
}
