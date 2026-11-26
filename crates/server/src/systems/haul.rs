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
//! still gone toward. The map says which chunks the wagon crosses and
//! where a hub stood; the last stretch is found on the ground as it
//! lies, to the hub standing there now ([`hub_at`]), so a hub that moved
//! is still come to. The wagon leaves what it carried in that hub's
//! storehouse and goes home by the way it came; where no hub stands there
//! any more it goes home with its load.
//!
//! What a storehouse holds is each player's own ([`Depots`]): what it
//! left there itself, and what wagons brought of what it loaded. A
//! player takes its own out and no other player's, to carry or to load
//! on a wagon again. What is no one's is anyone's to take, and its
//! taker's from then.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use common::{network::Identity, Stack};
use common_bevy::{
    chart::{reversed, Chart, Leg},
    hubs::{HubId, Hubs},
    picture::chunk_of,
    systems::When,
};

/// A wagon on the road.
#[derive(Clone, Debug, PartialEq)]
pub struct Trip {
    /// The hub it carries to, by the tile its hearth stood on as the map
    /// it was sent by charted it.
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
    pub cargo: Vec<Owned>,
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
    pub fn begun(to: (i32, i32), way: Vec<Leg>, cargo: Vec<Owned>, when: When) -> Trip {
        Trip { to, way, done: 0, spare: Duration::ZERO, back: false, counted: when, cargo }
    }

    /// [`Trip::carry_to`] its way's end.
    #[cfg(test)]
    pub fn carry(&mut self, lies: &Chart, mut daytime: Duration, step: Duration) -> Road {
        self.carry_to(lies, &mut daytime, step, usize::MAX)
    }

    /// Carries the trip on through `daytime`, a step of its way taking
    /// `step`, over the ground as `lies` holds it: each leg tried as it
    /// is come to, and none past the start of leg `bound`, where it is
    /// still going with what is left of `daytime`. A stalled trip banks
    /// no time toward the leg it waits at.
    pub fn carry_to(&mut self, lies: &Chart, daytime: &mut Duration, step: Duration, bound: usize) -> Road {
        loop {
            let Some(&leg) = self.way.get(self.done) else { return Road::Arrived };
            if self.done >= bound {
                return Road::Going;
            }
            if !lies.open(leg) {
                self.spare = Duration::ZERO;
                return Road::Stalled;
            }
            let takes = step * leg.steps;
            if self.spare + *daytime < takes {
                self.spare += std::mem::take(daytime);
                return Road::Going;
            }
            *daytime -= takes.saturating_sub(self.spare);
            self.spare = Duration::ZERO;
            self.done += 1;
        }
    }

    /// Whether the wagon is on its way out and has come to the last leg
    /// of it: where its end is found again ([`Trip::aim`]).
    pub fn nearly(&self) -> bool {
        !self.back && self.done + 1 == self.way.len()
    }

    /// Ends a trip come to its last leg out ([`Trip::nearly`]) beside
    /// `hearth`, where its hub's hearth is laid now, by a way found on
    /// the ground as `lies` holds it; or turns it for home with its load
    /// where no hub stands there any more. False where the ground gives
    /// no way to the hearth yet, and the wagon waits.
    pub fn aim(&mut self, lies: &Chart, hearth: Option<(i32, i32)>) -> bool {
        let leg = self.way[self.done];
        let Some(hearth) = hearth else {
            // Its way ends where it stands, as a way's last leg does
            self.way.truncate(self.done);
            self.way.push(Leg { from: leg.from, by: leg.from, to: leg.from, steps: 0 });
            self.turned();
            return true;
        };
        let Some(goal) = lies.beside(hearth) else { return false };
        if goal == leg.to {
            return true;
        }
        let Some(last) = lies.way(leg.from, goal) else { return false };
        self.way.truncate(self.done);
        self.way.extend(last);
        true
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

/// The hub standing where a map recorded one on `recorded`, with the
/// tile its hearth is laid on now: the hub whose ground holds that tile,
/// or else one whose hearth is laid in the same chunk. None where the
/// hub is gone.
pub fn hub_at(hubs: &Hubs, recorded: (i32, i32)) -> Option<(HubId, (i32, i32))> {
    let near = || hubs.each().filter(|(_, hearth, _)| chunk_of(*hearth) == chunk_of(recorded)).map(|(id, ..)| id).min();
    let id = hubs.holding(recorded).or_else(near)?;
    Some((id, hubs.hearth(id)?))
}

/// Where a wagon from the camp round `hearth` may be sent by the map
/// `chart`: each hub the map holds, as it stood when the map charted it,
/// that the map holds a way to, by its hearth's tile, with that way. A
/// map knows no hub founded since it passed, and still knows one razed
/// since. The camp's own hub is not offered, whichever of its hearths
/// the map holds.
pub fn destinations(chart: &Chart, hubs: &Hubs, hearth: (i32, i32)) -> Vec<((i32, i32), Vec<Leg>)> {
    let Some(from) = chart.beside(hearth) else { return Vec::new() };
    let own = hubs.of_hearth(hearth);
    let home = |tile: (i32, i32)| tile == hearth || own.is_some() && (hubs.of_hearth(tile) == own || hubs.holding(tile) == own);
    let mut others: Vec<(i32, i32)> = chart.hearths().filter(|&tile| !home(tile)).collect();
    others.sort();
    others.dedup();
    others.into_iter().filter_map(|hub| Some((hub, chart.way(from, chart.beside(hub)?)?))).filter(|(_, way)| !way.is_empty()).collect()
}

/// So many of a material and the player whose they are: the one who
/// loaded them on a wagon or left them in a storehouse. None where they
/// are no one's, loaded before a load said whose it was: anyone's to
/// take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Owned {
    pub who: Option<Identity>,
    pub stack: Stack,
}

/// Puts `owned` with what `held` has of its kind for its player.
pub fn put(held: &mut Vec<Owned>, owned: Owned) {
    match held.iter_mut().find(|kept| kept.who == owned.who && kept.stack.kind == owned.stack.kind) {
        Some(kept) => kept.stack.count += owned.stack.count,
        None => held.push(owned),
    }
}

/// How many `held` has of each kind, whoever's they are, in the order
/// it has them.
pub fn totals(held: &[Owned]) -> Vec<Stack> {
    let mut all: Vec<Stack> = Vec::new();
    for owned in held {
        match all.iter_mut().find(|stack| stack.kind == owned.stack.kind) {
            Some(stack) => stack.count += owned.stack.count,
            None => all.push(owned.stack),
        }
    }
    all
}

/// What each hub's storehouse holds, by the tile the hub is known by:
/// what players left there and what wagons brought, each stack its
/// player's own. It is the hub's whether or not its storehouse stands,
/// so what a wagon brings before one is raised waits for it.
#[derive(Resource, Default)]
pub struct Depots {
    held: HashMap<(i32, i32), Vec<Owned>>,
    changed: Vec<(i32, i32)>,
}

impl Depots {
    /// Everything the depot at `hub` holds, whoever's.
    pub fn on(&self, hub: (i32, i32)) -> &[Owned] {
        self.held.get(&hub).map_or(&[], Vec::as_slice)
    }

    /// What `who` may take from the depot at `hub`: its own, and what is
    /// no one's, a stack to a kind.
    pub fn of(&self, hub: (i32, i32), who: Identity) -> Vec<Stack> {
        let open: Vec<Owned> = self.on(hub).iter().copied().filter(|owned| owned.who.is_none_or(|whose| whose == who)).collect();
        totals(&open)
    }

    /// Lays a depot kept from an earlier run.
    pub fn recall(&mut self, hub: (i32, i32), held: Vec<Owned>) {
        self.held.insert(hub, held);
    }

    /// Leaves `brought` in the depot at `hub`, each stack with what its
    /// player has there of its kind.
    pub fn leave(&mut self, hub: (i32, i32), brought: &[Owned]) {
        let held = self.held.entry(hub).or_default();
        for owned in brought {
            put(held, *owned);
        }
        self.changed.push(hub);
    }

    /// Takes as many as `asked` of its kind out of what `who` may take
    /// from the depot at `hub` ([`Depots::of`]), its own before what is
    /// no one's and no more than there is: what was taken, none where
    /// there is none.
    pub fn take(&mut self, hub: (i32, i32), who: Identity, asked: Stack) -> Option<Stack> {
        let held = self.held.get_mut(&hub)?;
        let mut left = asked.count;
        for whose in [Some(who), None] {
            if let Some(owned) = held.iter_mut().find(|owned| owned.who == whose && owned.stack.kind == asked.kind) {
                let count = left.min(owned.stack.count);
                owned.stack.count -= count;
                left -= count;
            }
        }
        held.retain(|owned| owned.stack.count > 0);
        if left == asked.count {
            return None;
        }
        self.changed.push(hub);
        Some(Stack { kind: asked.kind, count: asked.count - left })
    }

    /// The hub known by `from` was taken in by the hub known by `into`:
    /// all its depot held is in that hub's, each stack still its
    /// player's. Returns what moved.
    pub fn join(&mut self, from: (i32, i32), into: (i32, i32)) -> Vec<Owned> {
        let moved = self.held.remove(&from).unwrap_or_default();
        if !moved.is_empty() {
            self.leave(into, &moved);
            self.changed.push(from);
        }
        moved
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

    /// A map offers the hubs that stood on it as it was charted and it
    /// holds a way to: one razed since still, one founded since not, and
    /// never the haven or the camp's own.
    #[test]
    fn a_map_offers_the_hubs_it_recorded() {
        let (own, stood, razed, since) = (centre(-4, 0), centre(4, 0), centre(0, 4), centre(2, -3));
        let mut chart = Chart::default();
        for leaf in charted(4, |_, _| false).leaves() {
            chart.lay(Arc::new(leaf.marked([own, stood, razed].into_iter())));
        }
        let mut hubs = Hubs::new(centre(1, 1));
        for hearth in [own, stood, since] {
            hubs.founded(hearth);
        }
        let offered: Vec<(i32, i32)> = destinations(&chart, &hubs, own).into_iter().map(|(hub, _)| hub).collect();
        let mut recorded = vec![stood, razed];
        recorded.sort();
        assert_eq!(offered, recorded);
        assert!(destinations(&Chart::default(), &hubs, own).is_empty());
    }

    /// A wagon come to its last leg ends beside its hub's hearth where
    /// that is laid now, not where the map recorded it; with no hub
    /// there it turns for home, its load still on it, by the way it
    /// came.
    #[test]
    fn a_wagon_finds_its_hub_where_it_now_stands() {
        let open = charted(1, |_, _| false);
        let (home, recorded) = (centre(-1, 0), centre(1, 0));
        let moved = (recorded.0 + 3, recorded.1 - 5);
        let way = open.way(home, recorded).unwrap();
        let softwood = Owned { who: None, stack: Stack { kind: Stackable::Material(Material::Softwood), count: 2 } };

        let mut trip = Trip::begun(recorded, way.clone(), vec![softwood], When(0));
        let (mut day, bound) = (STEP * 10_000, way.len() - 1);
        assert_eq!(trip.carry_to(&open, &mut day, STEP, bound), Road::Going);
        assert!(trip.nearly() && !day.is_zero(), "it stops at its last leg with the day left to it");
        assert!(trip.aim(&open, Some(moved)));
        assert_eq!(trip.carry_to(&open, &mut day, STEP, usize::MAX), Road::Arrived);
        assert_eq!(trip.at(), Some(moved));

        let mut gone = Trip::begun(recorded, way.clone(), vec![softwood], When(0));
        gone.carry_to(&open, &mut (STEP * 10_000), STEP, bound);
        let stopped = gone.at();
        assert!(gone.aim(&open, None));
        assert!(gone.back && gone.cargo == vec![softwood] && gone.at() == stopped);
        assert_eq!(gone.carry(&open, STEP * 10_000, STEP), Road::Arrived);
        assert_eq!(gone.at(), Some(home));

        let walled = charted(1, |q, r| qrz::hex_distance((q, r), moved) == 1);
        let mut waits = Trip::begun(recorded, way.clone(), Vec::new(), When(0));
        waits.carry_to(&open, &mut (STEP * 10_000), STEP, bound);
        assert!(!waits.aim(&walled, Some(moved)) && !waits.back, "walled in, the hub is waited for");
    }

    /// A hub stands where a map recorded it while its ground holds that
    /// tile or its hearth is laid in that chunk, and nowhere else.
    #[test]
    fn a_hub_stands_where_it_was_recorded() {
        let mut hubs = Hubs::new(centre(9, 9));
        let id = hubs.founded(centre(2, 0));
        let hearth = hubs.hearth(id).unwrap();
        assert_eq!(hub_at(&hubs, hearth), Some((id, hearth)));
        let aside = (centre(2, 0).0 + 7, centre(2, 0).1 - 7);
        assert_eq!(hub_at(&hubs, aside), Some((id, hearth)), "elsewhere in its chunk it is still found");
        assert_eq!(hub_at(&hubs, centre(-3, 1)), None);
    }

    /// A storehouse holds each player's own: what it left and what wagons
    /// brought of its loading together, to take out no more than it has
    /// and nothing of another's. What is no one's is anyone's, taken
    /// after its own. A hub taken in brings each player's with it.
    #[test]
    fn a_depot_holds_each_players_own() {
        let of = |material, count| Stack { kind: Stackable::Material(material), count };
        let (ann, ben) = (Identity::Key([1; 16]), Identity::Key([2; 16]));
        let mine = |who, stack| Owned { who: Some(who), stack };
        let mut depots = Depots::default();
        depots.leave((3, 4), &[mine(ann, of(Material::Softwood, 2)), mine(ben, of(Material::Softwood, 9)), Owned { who: None, stack: of(Material::Hide, 1) }]);
        depots.leave((3, 4), &[mine(ann, of(Material::Softwood, 5))]);
        assert_eq!((depots.of((3, 4), ann), depots.of((3, 4), ben)), (vec![of(Material::Softwood, 7), of(Material::Hide, 1)], vec![of(Material::Softwood, 9), of(Material::Hide, 1)]));
        assert_eq!(totals(depots.on((3, 4))), vec![of(Material::Softwood, 16), of(Material::Hide, 1)]);
        assert_eq!(depots.take((3, 4), ann, of(Material::Softwood, 4)), Some(of(Material::Softwood, 4)));
        assert_eq!(depots.take((3, 4), ann, of(Material::Softwood, 40)), Some(of(Material::Softwood, 3)), "no more than it has");
        assert_eq!((depots.take((3, 4), ann, of(Material::Softwood, 1)), depots.of((3, 4), ben)), (None, vec![of(Material::Softwood, 9), of(Material::Hide, 1)]));
        depots.leave((3, 4), &[Owned { who: None, stack: of(Material::Softwood, 2) }]);
        assert_eq!(depots.take((3, 4), ben, of(Material::Softwood, 10)), Some(of(Material::Softwood, 10)), "its own first, then what is no one's");
        assert_eq!((depots.of((3, 4), ann), depots.of((3, 4), ben)), (vec![of(Material::Hide, 1), of(Material::Softwood, 1)], vec![of(Material::Hide, 1), of(Material::Softwood, 1)]));
        depots.leave((3, 4), &[mine(ben, of(Material::Softwood, 9))]);
        assert_eq!(depots.take((3, 4), ann, of(Material::Softwood, 5)), Some(of(Material::Softwood, 1)), "and never another's");
        depots.leave((0, 0), &[mine(ben, of(Material::Softwood, 1))]);
        depots.take_changed();
        assert_eq!(depots.join((3, 4), (0, 0)).len(), 2);
        assert_eq!(depots.take((0, 0), ann, of(Material::Hide, 1)), Some(of(Material::Hide, 1)));
        assert_eq!((depots.of((0, 0), ben), depots.on((3, 4)).len()), (vec![of(Material::Softwood, 10)], 0));
        let mut changed = depots.take_changed();
        changed.sort();
        changed.dedup();
        assert_eq!((changed, depots.join((9, 9), (0, 0)).len()), (vec![(0, 0), (3, 4)], 0));
    }
}
