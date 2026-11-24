//! Logistics: a camp whose line is raised sends its wagon to another hub
//! with what players loaded on it, by a map a player hands over. The map
//! says the way and is used up as the wagon leaves; the way is then the
//! trip's own, a leg to a chunk (`common_bevy::chart`).
//!
//! A trip is the calendar's daytime passing, as a camp's work is: each
//! leg takes the time its steps take a settler, counted whether or not
//! anyone is near. A leg is tried against the ground as it lies now as
//! the wagon comes to it, and one the ground no longer gives stops the
//! trip there until it does: a way blocked since the map was drawn is
//! still gone toward. Come to its hub, the wagon leaves what it carried
//! in that hub's depot and goes home by the way it came.
//!
//! Unbuilt: taking from a depot.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use common::Stack;
use common_bevy::{
    chart::{reversed, Chart, Leg},
    hubs::Hubs,
    systems::When,
};

/// A wagon on the road.
#[derive(Clone, Debug, PartialEq)]
pub struct Trip {
    /// The hub it carries to, by the tile it is known by, the haven by
    /// its own.
    pub to: (i32, i32),
    /// The way it is on: out, or home again once it has `turned`.
    pub way: Vec<Leg>,
    /// How many legs of the way are behind it.
    pub done: usize,
    /// The daytime it has put into the leg it is on.
    pub spare: Duration,
    /// Whether it has left its cargo and is on its way home.
    pub back: bool,
    /// The moment its travel is counted up to.
    pub counted: When,
    pub cargo: Vec<Stack>,
}

/// How a trip stands once its time is counted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Road {
    Going,
    /// The ground no longer gives the leg it has come to.
    Stalled,
    /// Its way is behind it.
    Arrived,
}

impl Trip {
    pub fn begun(to: (i32, i32), way: Vec<Leg>, cargo: Vec<Stack>, when: When) -> Trip {
        Trip { to, way, done: 0, spare: Duration::ZERO, back: false, counted: when, cargo }
    }

    /// Carries the trip on through `daytime`, a step of its way taking
    /// `step`, over the ground as `lies` holds it: each leg tried as it
    /// is come to. A stalled trip banks no time toward the leg it waits
    /// at.
    pub fn carry(&mut self, lies: &Chart, mut daytime: Duration, step: Duration) -> Road {
        loop {
            let Some(&leg) = self.way.get(self.done) else { return Road::Arrived };
            if !lies.open(leg) {
                self.spare = Duration::ZERO;
                return Road::Stalled;
            }
            let takes = step * leg.steps;
            if self.spare + daytime < takes {
                self.spare += daytime;
                return Road::Going;
            }
            daytime -= takes.saturating_sub(self.spare);
            self.spare = Duration::ZERO;
            self.done += 1;
        }
    }

    /// The tile the wagon has come to: where the leg it is on begins, or
    /// its way's end.
    pub fn at(&self) -> Option<(i32, i32)> {
        self.way.get(self.done).map(|leg| leg.from).or_else(|| self.way.last().map(|leg| leg.to))
    }

    /// The leg the wagon is on, none once its way is behind it.
    pub fn leg(&self) -> Option<Leg> {
        self.way.get(self.done).copied()
    }

    /// Turns the wagon for home: its way run the other way, from its end.
    pub fn turned(&mut self) {
        self.way = reversed(&self.way);
        (self.done, self.spare, self.back) = (0, Duration::ZERO, true);
    }
}

/// Where a wagon from the camp round `hearth` may be sent by the map
/// `chart`: each other hub, and the haven, that the map holds a way to,
/// by the tile it is known by, with that way; the haven first.
pub fn destinations(chart: &Chart, hubs: &Hubs, hearth: (i32, i32)) -> Vec<((i32, i32), Vec<Leg>)> {
    let Some(from) = chart.beside(hearth) else { return Vec::new() };
    let own = hubs.of_hearth(hearth);
    let mut others: Vec<(i32, i32)> = hubs.each().map(|(id, ..)| id).filter(|&id| Some(id) != own).map(|id| (id.0, id.1)).collect();
    others.sort();
    std::iter::once(hubs.haven())
        .chain(others)
        .filter_map(|hub| Some((hub, chart.way(from, chart.beside(hub)?)?)))
        .filter(|(_, way)| !way.is_empty())
        .collect()
}

/// What wagons have left at each hub, by the tile the hub was known by
/// when they came.
#[derive(Resource, Default)]
pub struct Depots {
    held: HashMap<(i32, i32), Vec<Stack>>,
    changed: Vec<(i32, i32)>,
}

impl Depots {
    /// What the depot at `hub` holds.
    pub fn on(&self, hub: (i32, i32)) -> &[Stack] {
        self.held.get(&hub).map_or(&[], Vec::as_slice)
    }

    /// Lays a depot kept from an earlier run.
    pub fn recall(&mut self, hub: (i32, i32), held: Vec<Stack>) {
        self.held.insert(hub, held);
    }

    /// Leaves `cargo` in the depot at `hub`, each stack on the one of its
    /// kind.
    pub fn leave(&mut self, hub: (i32, i32), cargo: &[Stack]) {
        let held = self.held.entry(hub).or_default();
        for stack in cargo {
            match held.iter_mut().find(|kept| kept.kind == stack.kind) {
                Some(kept) => kept.count += stack.count,
                None => held.push(*stack),
            }
        }
        self.changed.push(hub);
    }

    /// The hubs whose depot changed since the last call.
    pub fn take_changed(&mut self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Material, Stackable};
    use common_bevy::{
        chart::Leaf,
        chunk::{calculate_visible_chunks, chunk_tiles, ChunkId, CHUNK_RADIUS},
        components::entity_type::{decorator::Decorator, EntityType},
        resources::map::Map,
    };
    use qrz::Qrz;
    use std::sync::Arc;

    /// The chunks within `reach` of the origin's, charted flat and open
    /// but for the tiles `full` names.
    fn charted(reach: u8, full: impl Fn(i32, i32) -> bool) -> Chart {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        let filled = (0..common::TILE_SLOTS as usize).fold(common::Cover::NONE, |c, k| c.with_boulder(k));
        let mut chart = Chart::default();
        for chunk in calculate_visible_chunks(ChunkId(0, 0), reach) {
            for (q, r) in chunk_tiles(chunk) {
                let cover = if full(q, r) { filled } else { common::Cover::NONE };
                map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
            }
            chart.lay(Arc::new(Leaf::read(&map, chunk).unwrap()));
        }
        chart
    }

    fn centre(n: i32, m: i32) -> (i32, i32) {
        let centre = ChunkId(n, m).center();
        (centre.q, centre.r)
    }

    const STEP: Duration = Duration::from_secs(1);

    /// A trip takes the time its steps take, however the time is cut
    /// up, and comes home by the way it went.
    #[test]
    fn a_trip_takes_its_ways_time() {
        let open = charted(1, |_, _| false);
        let way = open.way(centre(-1, 0), centre(1, 0)).unwrap();
        let steps: u32 = way.iter().map(|leg| leg.steps).sum();
        let mut whole = Trip::begun(centre(1, 0), way.clone(), Vec::new(), When(0));
        assert_eq!(whole.carry(&open, STEP * (steps - 1), STEP), Road::Going);
        assert_eq!(whole.carry(&open, STEP, STEP), Road::Arrived);

        let mut cut = Trip::begun(centre(1, 0), way.clone(), Vec::new(), When(0));
        let mut ticks = 0;
        while cut.carry(&open, STEP / 2, STEP) != Road::Arrived {
            ticks += 1;
        }
        assert_eq!(ticks + 1, steps * 2);

        cut.turned();
        assert!(cut.back && cut.at() == Some(centre(1, 0)));
        assert_eq!(cut.carry(&open, STEP * steps, STEP), Road::Arrived);
        assert_eq!(cut.at(), Some(centre(-1, 0)));
    }

    /// A wall raised since the map was drawn stops the wagon at the leg
    /// it crosses, for as long as it stands, and no time waited there
    /// counts toward the leg.
    #[test]
    fn a_trip_stalls_where_the_ground_no_longer_gives_its_way() {
        let was = charted(1, |_, _| false);
        let rim = |q: i32, r: i32| qrz::hex_distance((q, r), (0, 0)) == CHUNK_RADIUS;
        let now = charted(1, rim);
        let way = was.way(centre(-1, 0), centre(1, 0)).unwrap();
        let mut trip = Trip::begun(centre(1, 0), way.clone(), Vec::new(), When(0));
        assert_eq!(trip.carry(&now, STEP * 10_000, STEP), Road::Stalled);
        let stopped = trip.done;
        assert!(stopped < way.len() && trip.spare.is_zero());
        assert_eq!(trip.carry(&now, STEP * 10_000, STEP), Road::Stalled);
        assert_eq!(trip.done, stopped);
        assert_eq!(trip.carry(&was, STEP * 10_000, STEP), Road::Arrived, "the way opened, it goes on");
    }

    /// A map offers the haven and every other hub it holds a way to, and
    /// none it does not reach.
    #[test]
    fn a_map_offers_the_hubs_it_reaches() {
        let chart = charted(4, |_, _| false);
        let mut hubs = Hubs::new(centre(4, 0));
        hubs.founded(centre(-4, 0));
        hubs.founded(centre(0, 4));
        hubs.founded(centre(20, 20));
        let offered: Vec<(i32, i32)> = destinations(&chart, &hubs, centre(-4, 0)).into_iter().map(|(hub, _)| hub).collect();
        assert_eq!(offered, vec![centre(4, 0), centre(0, 4)]);
        assert!(destinations(&Chart::default(), &hubs, centre(-4, 0)).is_empty());
    }

    /// What wagons leave at a hub lies there together.
    #[test]
    fn a_depot_holds_what_wagons_leave() {
        let of = |material, count| Stack { kind: Stackable::Material(material), count };
        let mut depots = Depots::default();
        depots.leave((3, 4), &[of(Material::Softwood, 2), of(Material::Hide, 1)]);
        depots.leave((3, 4), &[of(Material::Softwood, 5)]);
        assert_eq!(depots.on((3, 4)), &[of(Material::Softwood, 7), of(Material::Hide, 1)]);
        assert_eq!((depots.take_changed().len(), depots.on((0, 0)).len()), (2, 0));
    }
}
