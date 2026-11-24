//! Settlers: the NPCs that build a hub. One comes to every hearth a
//! player builds, a while after it is built, and raises what the camp
//! round it lacks, one thing at a time and in one order: a tent of its
//! own, a tent for a player to claim, stakes along the edge of the hub's
//! ground, and a wagon. It raises only what it has been handed the
//! materials for, by a player who hails it, and what it raises is the
//! hub's and never taken down. A fortification is paid for whole: handed
//! its materials once, the settler raises every stake the edge wants,
//! one by one.
//!
//! A camp is what is kept: when its settler comes, what stands of its
//! line, the materials it holds and when its settler last worked, each
//! time a moment of the calendar. All its work is the calendar's daytime
//! passing, so an admin who moves the calendar on moves the camp's work
//! on with it, and one who holds it holds the work. The
//! settler itself is not; it is in the world while a player is near its
//! hearth. The world goes on without one: a camp found again is given
//! at once what its settler would have raised in the daytime between, of
//! what it held the materials for. At night a settler sleeps, out of the
//! world in its tent, or on the ground by its hearth until it has one:
//! it raises nothing and no one hails it.
//!
//! A thing is raised by stages: begun when the settler reaches its tile
//! and its materials are spent, carried a stage further each time the
//! settler has worked on it a while, and raised from the last. The tile
//! holds the stage, so what is part raised stays so, whoever comes and
//! goes.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use bevy::prelude::*;
use combat::behaviour::Body;
use common::{
    building::{self, Fixture, Needs, Wants},
    Stack, Stackable,
};
use common_bevy::{
    components::{
        behaviour::{PlayerControlled, Side},
        entity_type::{
            actor::{ActorIdentity, ActorImpl, Approach, Origin, Resilience},
            EntityType,
        },
        equipment::{Equipment, Inventory, Item, Piece},
        heading::Heading,
        loaded_by::LoadedBy,
        movement_intent_state::MovementIntentState,
        position::Position,
        AirTime, Loc, Turn,
    },
    hubs::{HubId, Hubs},
    message::{Do, Event, Try},
    moment::Moment,
    path,
    systems::When,
    plugins::nntree::NearestNeighbor,
    resources::map::Map,
    systems::movement::MOVEMENT_SPEED,
};
use qrz::Qrz;

use crate::systems::{
    building::{occupied, Built, Owner, Site},
    characters::Character,
    economy::{Cause, Holder, Moved},
};

/// How fast settlers come and work: what a hub's growth may one day
/// quicken.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Settling {
    /// How long after a hearth is built its settler comes.
    pub arrives: Duration,
    /// How long a settler works on a tent, on one tile of stakes and on
    /// a wagon, from begun to raised: long enough that a camp is earned.
    pub tent: Duration,
    pub stakes: Duration,
    pub wagon: Duration,
}

impl Default for Settling {
    fn default() -> Self {
        Self { arrives: Duration::from_secs(120), tent: Duration::from_secs(600), stakes: Duration::from_secs(45), wagon: Duration::from_secs(1200) }
    }
}

impl Settling {
    fn work(&self, step: Step) -> Duration {
        match step {
            Step::Own | Step::Spare => self.tent,
            Step::Stakes => self.stakes,
            Step::Wagon => self.wagon,
        }
    }

    /// How long it works to carry a step one stage on.
    fn stage(&self, step: Step) -> Duration {
        self.work(step) / (building::STAGES as u32 + 1)
    }

    /// Everything `times` as fast.
    pub fn hastened(self, times: u32) -> Self {
        Self { arrives: self.arrives / times, tent: self.tent / times, stakes: self.stakes / times, wagon: self.wagon / times }
    }
}

/// How near its hearth a player must be for a camp's settler to be in
/// the world, in tiles, and how far the nearest may be before it leaves.
const WATCHED: i32 = 48;
const UNWATCHED: i32 = 80;

/// How often a settler with nothing in hand looks for work.
const THINK: Duration = Duration::from_secs(1);

/// How often a camp whose settler is at work is kept anew, so a server
/// that stops without warning credits its settler little time twice.
const KEEP: Duration = Duration::from_secs(30);

/// How much daytime uncounted is more than a settler's own steps
/// account for: it was out of the world, or the calendar was moved on,
/// and its camp is given the work of the time between.
const AWAY: Duration = Duration::from_secs(30);

/// How many tiles a settler walks in a second.
fn walked() -> f32 {
    MOVEMENT_SPEED * 1000.0 * PACE / (common::grid::HEX_RADIUS * 3f32.sqrt())
}

/// How long a settler may stand on one tile of its way before it gives
/// the way up and looks again.
const STUCK: Duration = Duration::from_secs(8);

/// The share of a run a settler walks at.
const PACE: f32 = 0.5;

/// How near a player hails a settler from, in tiles.
const HAIL: i32 = 2;

/// How far from its hearth a camp's tents and wagon stand, in tiles, at
/// least and at most.
const YARD: (i32, i32) = (2, 4);

/// What a camp's settler raises, in the order it raises it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The settler's own tent.
    Own,
    /// The tent a player claims.
    Spare,
    /// The next stakes on the edge of the hub's ground.
    Stakes,
    Wagon,
}

impl Step {
    pub fn fixture(self) -> Fixture {
        match self {
            Step::Own | Step::Spare => Fixture::Tent,
            Step::Stakes => Fixture::Stakes,
            Step::Wagon => Fixture::Wagon,
        }
    }

    /// The number a store keeps the step by, given once.
    pub fn id(self) -> u32 {
        match self {
            Step::Own => 1,
            Step::Spare => 2,
            Step::Stakes => 3,
            Step::Wagon => 4,
        }
    }

    pub fn kept_as(id: u32) -> Option<Step> {
        [Step::Own, Step::Spare, Step::Stakes, Step::Wagon].into_iter().find(|step| step.id() == id)
    }
}

/// A camp: the hearth a player built, by its tile, and what its settler
/// has made of the ground round it.
#[derive(Clone, Debug, PartialEq)]
pub struct Camp {
    /// When its settler comes.
    pub arrives: When,
    /// Whether its settler has walked in, and stands by the hearth when
    /// it is next in the world.
    pub arrived: bool,
    /// The tiles its settler's tent, the tent a player claims and its
    /// wagon stand on, each once raised.
    pub own: Option<(i32, i32)>,
    pub spare: Option<(i32, i32)>,
    pub wagon: Option<(i32, i32)>,
    /// Whether a player has claimed the spare tent as its home.
    pub claimed: bool,
    /// Whether the fortification it is raising is paid for: every stake
    /// the hub's edge wants is then raised with nothing more spent.
    pub fortifying: bool,
    /// What it has begun and not yet raised, the tile it is on, and the
    /// work its stage still wants.
    pub rising: Option<(Step, (i32, i32), Duration)>,
    /// The materials it was handed and has not yet built with.
    pub stock: Vec<Stack>,
    /// The moment its settler's work is counted up to: all the daytime
    /// since is work it has yet to be given.
    pub worked: When,
}

impl Camp {
    pub fn founded(arrives: When) -> Self {
        Self { arrives, arrived: false, own: None, spare: None, wagon: None, claimed: false, fortifying: false, rising: None, stock: Vec::new(), worked: arrives }
    }

    /// What raising `step` takes of the camp's materials: a fixture's
    /// needs, but nothing for a stake of a fortification paid for.
    fn needs(&self, step: Step) -> Needs {
        if step == Step::Stakes && self.fortifying { Needs::default() } else { step.fixture().needs() }
    }

    /// What its settler raises next, the stakes while `staked` says the
    /// hub's edge still wants some; none once its line is done.
    fn next(&self, staked: bool) -> Option<Step> {
        match (self.own, self.spare, self.wagon) {
            (None, ..) => Some(Step::Own),
            (_, None, _) => Some(Step::Spare),
            _ if !staked => Some(Step::Stakes),
            (.., None) => Some(Step::Wagon),
            _ => None,
        }
    }
}

/// Every camp, by its hearth's tile, and the settler each has in the
/// world.
#[derive(Resource, Default)]
pub struct Camps {
    camps: HashMap<(i32, i32), Camp>,
    settlers: HashMap<(i32, i32), Entity>,
    changed: Vec<(i32, i32)>,
}

impl Camps {
    pub fn on(&self, hearth: (i32, i32)) -> Option<&Camp> {
        self.camps.get(&hearth)
    }

    /// A hearth was built on `hearth`: its settler comes at `arrives`.
    pub fn found(&mut self, hearth: (i32, i32), arrives: When) {
        self.camps.insert(hearth, Camp::founded(arrives));
        self.changed.push(hearth);
    }

    /// Lays a camp kept from an earlier run, where the hearth has none.
    pub fn recall(&mut self, hearth: (i32, i32), camp: Camp) {
        self.camps.entry(hearth).or_insert(camp);
    }

    /// The hearth on `hearth` came down: its camp is no more. Returns
    /// what it held and the settler it had in the world.
    pub fn razed(&mut self, hearth: (i32, i32)) -> (Vec<Stack>, Option<Entity>) {
        self.changed.push(hearth);
        (self.camps.remove(&hearth).map_or(Vec::new(), |camp| camp.stock), self.settlers.remove(&hearth))
    }

    /// The hearth of the camp whose spare tent stands on `tile`.
    pub fn of_spare(&self, tile: (i32, i32)) -> Option<(i32, i32)> {
        self.camps.iter().find(|(_, camp)| camp.spare == Some(tile)).map(|(&hearth, _)| hearth)
    }

    /// The spare tent on `tile` was claimed as a home.
    pub fn claimed(&mut self, hearth: (i32, i32)) {
        if let Some(camp) = self.camps.get_mut(&hearth).filter(|camp| !camp.claimed) {
            camp.claimed = true;
            self.changed.push(hearth);
        }
    }

    /// The hearths whose camp changed since the last call.
    pub fn take_changed(&mut self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.changed)
    }

    fn change(&mut self, hearth: (i32, i32)) -> Option<&mut Camp> {
        let camp = self.camps.get_mut(&hearth)?;
        self.changed.push(hearth);
        Some(camp)
    }
}

/// Where a settler is headed: its hearth, or the tile it raises a step
/// of its line on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Hearth,
    Site(Step, (i32, i32)),
}

/// What a settler is doing.
#[derive(Clone, Debug, PartialEq)]
enum Errand {
    /// Standing, to look for work at `think`.
    Idle { think: Moment },
    /// Walking `way`, the tiles still ahead of it; `since` is when it
    /// last stepped onto a new one.
    Going { goal: Goal, way: Vec<Qrz>, since: Moment },
    /// At work on what its camp has begun on `site`.
    Raising { site: (i32, i32) },
    /// Asleep on the ground through the night; `told` is when those who
    /// see it were last told so.
    Asleep { told: Moment },
}

/// A settler: the camp it builds, by its hearth, what it is doing, and
/// the sites it found no way to since it last raised anything.
#[derive(Component, Clone, Debug)]
pub struct Settler {
    pub hearth: (i32, i32),
    errand: Errand,
    unreached: HashSet<(i32, i32)>,
}

fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    qrz::hex_distance(a, b)
}

/// Every tile within `reach` tiles of `centre`.
fn around(centre: (i32, i32), reach: i32) -> impl Iterator<Item = (i32, i32)> {
    (-reach..=reach).flat_map(move |dq| ((-reach).max(-dq - reach)..=reach.min(-dq + reach)).map(move |dr| (centre.0 + dq, centre.1 + dr)))
}

/// The edge of hub `id`'s ground: every tile of it with a neighbour
/// outside it. Stakes stand along it, but on the tile of it nearest the
/// haven, which is the way in.
pub fn edge(hubs: &Hubs, id: HubId) -> Vec<(i32, i32)> {
    let grounds = hubs.grounds(id);
    let inside = |tile: (i32, i32)| grounds.iter().any(|&(hearth, held)| distance(hearth, tile) as f32 <= held);
    let mut edge: Vec<(i32, i32)> = grounds
        .iter()
        .filter(|&&(_, held)| held >= 0.0)
        .flat_map(|&(hearth, held)| around(hearth, held as i32).filter(move |&tile| distance(hearth, tile) == held as i32))
        .filter(|&tile| qrz::DIRECTIONS.iter().any(|d| !inside((tile.0 + d.q, tile.1 + d.r))))
        .collect();
    edge.sort();
    edge.dedup();
    let haven = hubs.haven();
    if let Some(way_in) = edge.iter().copied().min_by_key(|&tile| (distance(tile, haven), tile)) {
        edge.retain(|&tile| tile != way_in);
    }
    edge
}

/// Whether `fixture` may be begun on `tile` as the ground lies: the map
/// holds it dry and its cover takes it.
fn open(site: &Site, fixture: Fixture, tile: (i32, i32)) -> bool {
    site.ground.lies(tile.0, tile.1).is_some_and(|(_, cover)| fixture.begun(cover).is_some()) && site.ground.map().water_at(tile.0, tile.1).is_none()
}

/// Whether stakes already stand or are begun on `tile`, or nothing ever
/// will be: it is built on already, or what grows or lies there is in the
/// way.
fn staked(site: &Site, tile: (i32, i32)) -> bool {
    site.ground.lies(tile.0, tile.1).is_some_and(|(_, cover)| Fixture::Stakes.begun(cover).is_none())
}

/// Where a settler standing on `from` raises `step` of the camp round
/// `hearth`, of the places no one of `unreached` names: a tent or a
/// wagon on the open tile of the yard nearest the hearth, clear of the
/// camp's other tents; stakes on the open tile of the hub's edge nearest
/// the settler.
fn site_for(site: &Site, camp: &Camp, hearth: (i32, i32), step: Step, from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(i32, i32)> {
    let fixture = step.fixture();
    let free = |tile: &(i32, i32)| !unreached.contains(tile) && open(site, fixture, *tile);
    match step {
        Step::Stakes => {
            let id = site.hubs.of_hearth(hearth)?;
            edge(&site.hubs, id).into_iter().filter(free).min_by_key(|&tile| (distance(tile, from), tile))
        }
        _ => {
            let level = site.ground.lies(hearth.0, hearth.1)?.0;
            let stood = [camp.own, camp.spare, camp.wagon];
            around(hearth, YARD.1)
                .filter(|&tile| distance(tile, hearth) >= YARD.0)
                .filter(|&tile| stood.iter().flatten().all(|&other| distance(tile, other) >= 2))
                .filter(|tile| site.ground.lies(tile.0, tile.1).is_some_and(|(z, _)| (z - level).abs() <= 1))
                .filter(free)
                .min_by_key(|&tile| (distance(tile, hearth), tile))
        }
    }
}

/// What a settler wears.
fn outfit() -> Equipment {
    let mut worn = Equipment::default();
    for name in ["acolyte-cap", "acolyte-robe", "acolyte-wraps", "acolyte-sash", "acolyte-hose", "acolyte-shoes"] {
        worn.wear(Item { piece: Piece::named(name), seed: 3 });
    }
    worn
}

/// Begins `step` of the camp round `hearth` on `at`: what it takes is
/// spent from what the camp holds, and the tile is the hub's from then.
/// False, with nothing changed, where the tile no longer takes it or the
/// camp no longer holds what it takes.
fn begin(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), step: Step, at: (i32, i32)) -> bool {
    let fixture = step.fixture();
    let Some(cover) = site.ground.cover(at.0, at.1) else { return false };
    let Some(camp) = camps.on(hearth).filter(|camp| camp.rising.is_none()) else { return false };
    let (Some(begun), Some(spent)) = (fixture.begun(cover), camp.needs(step).taken(&camp.stock)) else { return false };
    let camp = camps.change(hearth).expect("its camp stands");
    for stack in spent {
        remove(&mut camp.stock, stack);
        site.moved.write(Moved::stack(stack, Holder::Camp(hearth.0, hearth.1), Holder::World, Cause::Build));
    }
    camp.rising = Some((step, at, settling.stage(step)));
    camp.fortifying |= step == Step::Stakes;
    site.constructs.raise(at, Built { by: Owner::Camp(hearth.0, hearth.1) });
    site.ground.set(&mut site.writer, at.0, at.1, begun);
    info!("settlers: the settler of {hearth:?} began {} on {at:?}", fixture.name());
    true
}

/// What a spell of work on what a camp has begun came to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Worked {
    /// Nothing: someone stands where the stakes would fill the tile.
    Waiting,
    /// It stands a stage further.
    Further,
    /// It is raised, or was no longer there to raise.
    Raised,
}

/// Carries what the camp round `hearth` has begun a stage further, with
/// a stage's work to do again, and raises it from its last.
fn advance(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32)) -> Worked {
    let Some((step, at, _)) = camps.on(hearth).and_then(|camp| camp.rising) else { return Worked::Raised };
    let next = site.ground.lies(at.0, at.1).and_then(|(z, cover)| Some((z, building::advanced(cover)?)));
    let Some((z, next)) = next else {
        camps.change(hearth).expect("its camp stands").rising = None;
        return Worked::Raised;
    };
    if building::rising(next).is_some() {
        camps.change(hearth).expect("its camp stands").rising = Some((step, at, settling.stage(step)));
        site.ground.set(&mut site.writer, at.0, at.1, next);
        return Worked::Further;
    }
    if step == Step::Stakes && occupied(&site.nntree, at, z) {
        return Worked::Waiting;
    }
    let camp = camps.change(hearth).expect("its camp stands");
    camp.rising = None;
    match step {
        Step::Own => camp.own = Some(at),
        Step::Spare => camp.spare = Some(at),
        Step::Wagon => camp.wagon = Some(at),
        Step::Stakes => {}
    }
    site.ground.set(&mut site.writer, at.0, at.1, next);
    info!("settlers: the settler of {hearth:?} raised {} on {at:?}", step.fixture().name());
    Worked::Raised
}

/// What the camp round `hearth` wants done next, by a settler standing
/// on `from`: the thing it has begun, to be raised; or else the next of
/// its line that it holds the materials for, and the tile to begin it
/// on, of those no one of `unreached` names. A fortification whose every
/// stake stands is done with, and the next the edge wants is paid for
/// anew.
fn work_for(site: &Site, camps: &mut Camps, hearth: (i32, i32), from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(Step, (i32, i32))> {
    let camp = camps.on(hearth)?;
    if let Some((step, at, _)) = camp.rising {
        return Some((step, at));
    }
    let done = site.hubs.of_hearth(hearth).is_none_or(|id| edge(&site.hubs, id).into_iter().all(|tile| staked(site, tile)));
    if done && camp.fortifying {
        camps.change(hearth)?.fortifying = false;
    }
    let camp = camps.on(hearth)?;
    let step = camp.next(done).filter(|&step| camp.needs(step).less(&camp.stock).is_none())?;
    Some((step, site_for(site, camp, hearth, step, from, unreached)?))
}

/// Gives the camp round `hearth` what its settler would have raised in
/// `spare`, daytime not yet counted: each thing next in its line that the
/// camp holds the materials for and the settler has a way to, a stage at
/// a time, while the walk there and the work fit in what is left; what is
/// left over comes off the stage it is on. Returns where the settler
/// ends, beside the last thing it worked on or at its hearth.
fn catch_up(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), mut spare: Duration) -> (i32, i32) {
    let mut at = hearth;
    let mut unreached = HashSet::new();
    loop {
        let Some((step, to)) = work_for(site, camps, hearth, at, &unreached) else { return at };
        let begun = camps.on(hearth).is_some_and(|camp| camp.rising.is_some());
        let way = site.ground.map().get_by_qr(at.0, at.1).zip(site.ground.map().get_by_qr(to.0, to.1)).and_then(|((from, _), (goal, _))| path::find(site.ground.map(), from, goal, 1));
        // What is begun is raised from wherever the way to it starts
        let Some(way) = way.or_else(|| begun.then(Vec::new)) else {
            unreached.insert(to);
            continue;
        };
        let walk = Duration::from_secs_f32(way.len() as f32 / walked());
        if walk > spare {
            return at;
        }
        if !begun && !begin(site, camps, settling, hearth, step, to) {
            unreached.insert(to);
            continue;
        }
        spare -= walk;
        at = way.last().map_or(at, |end| (end.q, end.r));
        // A stage at a time, for as long as the time left covers one
        loop {
            let left = camps.on(hearth).and_then(|camp| camp.rising).map_or(Duration::ZERO, |(.., left)| left);
            if left > spare {
                if let Some((.., left)) = camps.change(hearth).and_then(|camp| camp.rising.as_mut()) {
                    *left -= spare;
                }
                return at;
            }
            spare -= left;
            match advance(site, camps, settling, hearth) {
                Worked::Waiting => return at,
                Worked::Further => {}
                Worked::Raised => break,
            }
        }
    }
}

/// Brings a camp's settler into the world while a player is near its
/// hearth and its time has come, and takes it out once none is. By night
/// one with a tent is in it, out of the world; one with none stays, to
/// lie by its hearth. One that
/// has yet to walk in comes from the edge of the hearth's ground, on the
/// haven's side; one that has stands by its hearth, or by what it last
/// raised while it was away ([`catch_up`]).
#[allow(clippy::too_many_arguments)]
pub fn tend_camps(
    mut site: Site,
    mut camps: ResMut<Camps>,
    settling: Res<Settling>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    players: Query<&Loc, With<PlayerControlled>>,
    settlers: Query<(), With<Settler>>,
    mut commands: Commands,
    mut next: Local<Moment>,
    mut keep: Local<Moment>,
) {
    let now = Moment::ZERO + time.elapsed();
    if now < *next {
        return;
    }
    *next = now + THINK;
    let when = runtime.when(&time);
    let asleep = when.is_night();
    let camps = &mut *camps;
    camps.settlers.retain(|_, settler| settlers.contains(*settler));
    // A camp whose settler is at work is kept as it goes
    if now >= *keep {
        *keep = now + KEEP;
        camps.changed.extend(camps.settlers.keys().copied());
    }
    let nearest = |hearth: (i32, i32)| players.iter().map(|loc| distance((loc.q, loc.r), hearth)).min();
    let hearths: Vec<(i32, i32)> = camps.camps.keys().copied().collect();
    for hearth in hearths {
        let camp = &camps.camps[&hearth];
        match (camps.settlers.get(&hearth), nearest(hearth)) {
            (Some(&settler), far) if (asleep && camp.own.is_some()) || far.is_none_or(|far| far > UNWATCHED) => {
                commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
                camps.settlers.remove(&hearth);
                camps.changed.push(hearth);
            }
            (None, Some(near)) if !(asleep && camp.own.is_some()) && near <= WATCHED && when >= camp.arrives => {
                if site.ground.map().get_by_qr(hearth.0, hearth.1).is_none() {
                    continue;
                }
                // Away long enough, it walked in unseen and worked on,
                // through the days and never the nights
                let away = when.daytime_since(camp.worked);
                let mut stands = hearth;
                if away >= AWAY {
                    let camp = camps.change(hearth).expect("its camp stands");
                    (camp.arrived, camp.worked) = (true, when);
                    stands = catch_up(&mut site, camps, &settling, hearth, away);
                }
                let camp = &camps.camps[&hearth];
                let (hubs, map) = (&site.hubs, site.ground.map());
                let Some((floor, _)) = map.get_by_qr(stands.0, stands.1) else { continue };
                let beside = Some(floor)
                    .filter(|_| stands != hearth)
                    .or_else(|| map.neighbors(floor).into_iter().map(|(tile, _)| tile).find(|tile| !common_bevy::systems::movement::is_full_cover(map, tile.q, tile.r)));
                let haven = hubs.haven();
                let from_edge = || {
                    let reach = hubs.of_hearth(hearth).map_or(0.0, |id| hubs.core(id)) as i32;
                    around(hearth, reach)
                        .filter(|&tile| distance(tile, hearth) == reach)
                        .filter_map(|tile| map.get_by_qr(tile.0, tile.1).map(|(floor, _)| floor))
                        .filter(|at| path::find(map, *at, floor, 1).is_some())
                        .min_by_key(|at| (distance((at.q, at.r), haven), at.q, at.r))
                };
                let Some(at) = (if camp.arrived { None } else { from_edge() }).or(beside) else { continue };
                let stands = at + Qrz::Z;
                let loc = Loc::new(stands);
                let typ = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Settler));
                let settler = commands
                    .spawn((
                        typ,
                        loc,
                        Side::PLAYERS,
                        // Who sees it: every actor a client is shown holds this
                        LoadedBy::default(),
                        outfit(),
                        Settler { hearth, errand: Errand::Idle { think: now }, unreached: HashSet::new() },
                        (Heading::default(), Turn::default(), Position::at_tile(stands), AirTime::default(), MovementIntentState::default()),
                    ))
                    .id();
                commands.entity(settler).insert(NearestNeighbor::new(settler, loc));
                camps.settlers.insert(hearth, settler);
                info!("settlers: the settler of the hearth on {hearth:?} is at ({}, {})", at.q, at.r);
            }
            _ => {}
        }
    }
}

/// Tells everyone who sees `settler` what it is doing where it stands.
fn show(settler: Entity, activity: Option<common::gathering::Activity>, commands: &mut Commands) {
    commands.write_message(Do { event: Event::Activity { ent: settler, activity } });
}

const WORKING: Option<common::gathering::Activity> = Some(common::gathering::Activity::Work(common::gathering::Work::Build));
const SLEEPING: Option<common::gathering::Activity> = Some(common::gathering::Activity::Sleep);

/// How often those who see a sleeping settler are told it sleeps, so one
/// who comes on it in the night sees it lying.
const TOLD: Duration = Duration::from_secs(5);

/// Moves every settler through its errand. Standing, it looks for what
/// its camp wants done next and a way to where that is, or else a way
/// back to its hearth. Walking, it takes its way a tile at a time. Come
/// beside a site it begins what goes there, of what the camp holds, and
/// works on it: the calendar's daytime as it passes comes off the work
/// its stage wants, until what it raised stands, the hub's. Daytime gone
/// by faster than it steps, the calendar moved on, is given the camp
/// whole ([`catch_up`]). At night it leaves its work, goes to its hearth
/// and lies down there until day.
pub fn settle(
    mut site: Site,
    mut camps: ResMut<Camps>,
    settling: Res<Settling>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    mut settlers: Query<(Entity, &mut Settler, &Loc, Body)>,
    mut commands: Commands,
) {
    let now = Moment::ZERO + time.elapsed();
    let dt = time.delta().as_millis() as i16;
    let when = runtime.when(&time);
    let night = when.is_night();
    let camps = &mut *camps;
    for (ent, mut settler, loc, mut body) in &mut settlers {
        let hearth = settler.hearth;
        let Some(camp) = camps.camps.get_mut(&hearth) else {
            commands.write_message(Do { event: Event::Despawn { ent, died: false } });
            continue;
        };
        // The daytime since its work was last counted: a step's worth,
        // or all of a calendar moved on
        let gap = when.daytime_since(camp.worked);
        camp.worked = when;
        if gap >= AWAY {
            if matches!(settler.errand, Errand::Raising { .. }) {
                show(ent, None, &mut commands);
            }
            catch_up(&mut site, camps, &settling, hearth, gap);
            settler.errand = Errand::Idle { think: now };
            continue;
        }
        let Some((floor, _)) = site.ground.map().get_by_qr(loc.q, loc.r) else { continue };
        let here = (loc.q, loc.r);
        let home = |site: &Site| site.ground.map().get_by_qr(hearth.0, hearth.1).and_then(|(at, _)| path::find(site.ground.map(), floor, at, 1));
        if night && !matches!(settler.errand, Errand::Asleep { .. } | Errand::Going { goal: Goal::Hearth, .. }) {
            if matches!(settler.errand, Errand::Raising { .. }) {
                show(ent, None, &mut commands);
            }
            // By its hearth it lies down; from further off it goes there
            // first, or lies where it is where it finds no way
            settler.errand = match home(&site).filter(|way| !way.is_empty()) {
                Some(way) => Errand::Going { goal: Goal::Hearth, way, since: now },
                None => {
                    show(ent, SLEEPING, &mut commands);
                    Errand::Asleep { told: now }
                }
            };
            continue;
        }
        match settler.errand.clone() {
            Errand::Asleep { told } => {
                if !night {
                    show(ent, None, &mut commands);
                    settler.errand = Errand::Idle { think: now };
                } else if now.since(told) >= TOLD {
                    show(ent, SLEEPING, &mut commands);
                    settler.errand = Errand::Asleep { told: now };
                }
            }
            Errand::Idle { think } => {
                if now < think {
                    continue;
                }
                let work = work_for(&site, camps, hearth, here, &settler.unreached).and_then(|(step, at)| {
                    let (z, _) = site.ground.lies(at.0, at.1)?;
                    Some((step, at, path::find(site.ground.map(), floor, Qrz { q: at.0, r: at.1, z }, 1)))
                });
                let arrived = camps.on(hearth).is_some_and(|camp| camp.arrived);
                settler.errand = match work {
                    Some((step, at, Some(way))) => Errand::Going { goal: Goal::Site(step, at), way, since: now },
                    Some((_, at, None)) => {
                        settler.unreached.insert(at);
                        Errand::Idle { think: now + THINK }
                    }
                    // With nothing to raise it keeps to its hearth
                    None if distance(here, hearth) > 1 => match home(&site) {
                        Some(way) => Errand::Going { goal: Goal::Hearth, way, since: now },
                        None => Errand::Idle { think: now + STUCK },
                    },
                    None => {
                        if !arrived {
                            camps.change(hearth).expect("its camp stands").arrived = true;
                        }
                        Errand::Idle { think: now + THINK }
                    }
                };
            }
            Errand::Going { goal, mut way, mut since } => {
                if way.first().is_some_and(|next| (next.q, next.r) == here) {
                    way.remove(0);
                    since = now;
                }
                match way.first().copied() {
                    None => {
                        settler.errand = match goal {
                            Goal::Hearth => Errand::Idle { think: now },
                            // Beside its site: it begins what goes there,
                            // where the camp has not, and sets to work
                            Goal::Site(step, at) => {
                                let begun = camps.on(hearth).and_then(|camp| camp.rising).is_some_and(|(of, on, _)| (of, on) == (step, at));
                                if begun || begin(&mut site, camps, &settling, hearth, step, at) {
                                    show(ent, WORKING, &mut commands);
                                    Errand::Raising { site: at }
                                } else {
                                    settler.unreached.insert(at);
                                    Errand::Idle { think: now }
                                }
                            }
                        };
                    }
                    // Off its way, or held on one tile of it: it looks again
                    Some(next) if floor.flat_distance(&next) != 1 || now.since(since) > STUCK => {
                        if let Goal::Site(_, at) = goal {
                            settler.unreached.insert(at);
                        }
                        settler.errand = Errand::Idle { think: now };
                    }
                    Some(next) => {
                        body.step_toward(loc, floor, next, MOVEMENT_SPEED, PACE, dt, site.ground.map(), &site.nntree);
                        settler.errand = Errand::Going { goal, way, since };
                    }
                }
            }
            Errand::Raising { site: at } => {
                let Some((z, _)) = site.ground.lies(at.0, at.1) else { continue };
                body.face(loc, Qrz { q: at.0, r: at.1, z: z + 1 }, 1.0, dt, site.ground.map(), &site.nntree);
                // The daytime just gone comes off the work its stage wants
                let left = camps.camps.get_mut(&hearth).and_then(|camp| camp.rising.as_mut()).map_or(Duration::ZERO, |(.., left)| {
                    *left = left.saturating_sub(gap);
                    *left
                });
                if !left.is_zero() {
                    continue;
                }
                match advance(&mut site, camps, &settling, hearth) {
                    // Its work done, it waits on anyone standing in the
                    // way; a stage further, it works on
                    Worked::Waiting | Worked::Further => {}
                    Worked::Raised => {
                        show(ent, None, &mut commands);
                        settler.errand = Errand::Idle { think: now };
                        settler.unreached.clear();
                    }
                }
            }
        }
    }
}

/// Takes `stack` out of `stock`, which holds it.
fn remove(stock: &mut Vec<Stack>, stack: Stack) {
    if let Some(held) = stock.iter_mut().find(|held| held.kind == stack.kind) {
        held.count = held.count.saturating_sub(stack.count);
    }
    stock.retain(|held| held.count > 0);
}

/// What `camp`'s settler raises next and what of its materials the camp
/// still lacks: what a player who hails it is told.
fn wants(camp: &Camp, staked: bool) -> Wants {
    if let Some((step, ..)) = camp.rising {
        return Wants { raising: Some(step.fixture()), needs: Needs::default() };
    }
    let step = camp.next(staked);
    Wants { raising: step.map(Step::fixture), needs: step.map_or(Needs::default(), |step| camp.needs(step).less(&camp.stock)) }
}

/// Answers each player that hails a settler: the nearest within reach
/// says what it raises next and what it still lacks for it. Asked to, the
/// player hands it what its bag holds of that, no more than is lacking.
/// At night it sleeps and answers no one.
#[allow(clippy::too_many_arguments)]
pub fn try_hail(
    mut reader: MessageReader<Try>,
    mut camps: ResMut<Camps>,
    hubs: Res<Hubs>,
    map: Res<Map>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    settlers: Query<(&Settler, &Loc)>,
    mut players: Query<(&Loc, &mut Inventory, &Character)>,
    mut writer: MessageWriter<Do>,
    mut moved: MessageWriter<Moved>,
) {
    for message in reader.read() {
        let Try { event: Event::Hail { ent, give } } = message else { continue };
        let (ent, give) = (*ent, *give);
        let Ok((loc, mut bag, &Character(who))) = players.get_mut(ent) else { continue };
        let night = runtime.when(&time).is_night();
        let near = settlers.iter().map(|(settler, at)| (loc.flat_distance(at), settler.hearth)).filter(|&(far, _)| far <= HAIL).min();
        let Some((_, hearth)) = near.filter(|&(_, hearth)| !night && camps.on(hearth).is_some()) else {
            writer.write(Do { event: Event::Wants { ent, wants: None } });
            continue;
        };
        let staked = hubs.of_hearth(hearth).is_none_or(|id| {
            edge(&hubs, id).into_iter().all(|tile| map.get_by_qr(tile.0, tile.1).is_some() && Fixture::Stakes.begun(map.cover_at(tile.0, tile.1)).is_none())
        });
        if give {
            let mut lacking = wants(&camps.camps[&hearth], staked).needs;
            let held: Vec<Stack> = bag.stock.clone();
            for stack in held {
                let Stackable::Material(material) = stack.kind;
                let want = if material.is_wood() { &mut lacking.wood } else if material.is_stone() { &mut lacking.stone } else { &mut lacking.hide };
                let count = stack.count.min(*want);
                if count == 0 {
                    continue;
                }
                *want -= count;
                let given = bag.remove(stack.kind, count);
                let camp = camps.change(hearth).expect("its camp stands");
                match camp.stock.iter_mut().find(|held| held.kind == given.kind) {
                    Some(held) => held.count += given.count,
                    None => camp.stock.push(given),
                }
                moved.write(Moved::stack(given, Holder::Character(who), Holder::Camp(hearth.0, hearth.1), Cause::Give));
                info!("settlers: {ent} handed {} {:?} to the settler of {hearth:?}", given.count, given.kind);
            }
            writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        }
        writer.write(Do { event: Event::Wants { ent, wants: Some(wants(&camps.camps[&hearth], staked)) } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(material: common::Material, count: u32) -> Stack {
        Stack { kind: Stackable::Material(material), count }
    }

    /// A camp's line runs one way: its settler's tent, the tent a player
    /// claims, the stakes while the edge wants any, the wagon, and then
    /// nothing.
    #[test]
    fn a_camps_line_runs_in_one_order() {
        let mut camp = Camp::founded(When(0));
        assert_eq!(camp.next(false), Some(Step::Own));
        camp.own = Some((2, 0));
        assert_eq!(camp.next(false), Some(Step::Spare));
        camp.spare = Some((0, 2));
        assert_eq!((camp.next(false), camp.next(true)), (Some(Step::Stakes), Some(Step::Wagon)));
        camp.wagon = Some((-2, 0));
        assert_eq!((camp.next(false), camp.next(true)), (Some(Step::Stakes), None), "a hub that grows wants stakes again");
    }

    /// A settler says what it raises next and only what its camp still
    /// lacks of it.
    #[test]
    fn a_settler_wants_what_its_camp_lacks() {
        let mut camp = Camp::founded(When(0));
        let tent = Fixture::Tent.needs();
        assert_eq!(wants(&camp, false), Wants { raising: Some(Fixture::Tent), needs: tent });
        camp.stock = vec![of(common::Material::Hide, tent.hide), of(common::Material::Hardwood, 1)];
        assert_eq!(wants(&camp, false).needs, Needs { hide: 0, wood: tent.wood - 1, stone: 0 });
        remove(&mut camp.stock, of(common::Material::Hide, tent.hide));
        assert_eq!(camp.stock, vec![of(common::Material::Hardwood, 1)]);
        (camp.own, camp.spare) = (Some((2, 0)), Some((0, 2)));
        assert_eq!(wants(&camp, false).raising, Some(Fixture::Stakes));
        assert!(!wants(&camp, false).needs.is_none(), "a fortification is paid for before its first stake");
        camp.fortifying = true;
        assert!(wants(&camp, false).needs.is_none(), "and its every stake after is raised with nothing more");
        camp.rising = Some((Step::Wagon, (9, 9), Duration::from_secs(1)));
        assert_eq!(wants(&camp, false), Wants { raising: Some(Fixture::Wagon), needs: Needs::default() }, "what is begun is paid for");
        assert!([Step::Own, Step::Spare, Step::Stakes, Step::Wagon].into_iter().all(|step| Step::kept_as(step.id()) == Some(step)));
    }

    /// The edge of a lone hearth's ground is the ring at its reach, less
    /// the tile nearest the haven, and every tile of it has ground
    /// outside beside it.
    #[test]
    fn stakes_border_the_hubs_ground_but_for_the_way_in() {
        let mut hubs = Hubs::new((0, 0));
        let id = hubs.founded((400, 0));
        let reach = hubs.core(id) as i32;
        let edge = edge(&hubs, id);
        assert_eq!(edge.len() as i32, 6 * reach - 1);
        assert!(edge.iter().all(|&tile| distance(tile, (400, 0)) == reach));
        assert!(edge.iter().all(|&tile| hubs.outside(id, tile) == Some(0.0)));
        assert!(!edge.contains(&(400 - reach, 0)), "the way in faces the haven");
    }
}
