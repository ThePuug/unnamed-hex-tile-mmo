//! Settlers: the NPCs that build a hub. One comes to every hearth a
//! player builds, a while after it is built, riding its horse, and
//! makes the ground what its hub's plan says (`common_bevy::sites`): it
//! takes down what a plan gave up, raises what each build site wants,
//! a hitching post, where it ties the horse, tents and a wagon, and
//! then stakes along the hub's wall. It raises only what it holds the
//! materials for, handed it by a player who hails it, and what it
//! raises is the hub's and no player's to take down. What it takes down
//! it builds with again.
//!
//! Asked to by a player who hails it, it says what growing its hub a
//! tier, or shrinking it to what its people hold, would do, and carries
//! that out ([`try_plan`]).
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
//! A camp whose line is raised sends its wagon to other hubs
//! (`haul`): players load it through its settler, and one hands the
//! settler a map and names the hub. The wagon leaves its tile and its
//! settler goes with it, so both are in the world while a player is
//! near where the trip has come to, the settler driving the wagon
//! along the leg the trip is on, and no one hails it at its hearth
//! until it is home, where the wagon stands again.
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
    building::{self, Construct, Fixture, Kind, Needs, Planned, Task, Wants},
    Stack, Stackable,
};
use common_bevy::{
    settings::{Setting, Settings},
    components::{
        behaviour::{PlayerControlled, Side},
        entity_type::{
            actor::{ActorIdentity, ActorImpl, Approach, Origin, Resilience},
            EntityType,
        },
        equipment::{Equipment, Inventory, Item, Piece, Slot},
        heading::Heading,
        loaded_by::LoadedBy,
        status::Status,
        movement_intent_state::MovementIntentState,
        position::Position,
        AirTime, Loc, Turn,
    },
    hubs::{Hubs, Plan},
    sites::Standing,
    message::{Do, Event, Try},
    moment::Moment,
    path,
    systems::When,
    plugins::nntree::NearestNeighbor,
    systems::movement::MOVEMENT_SPEED,
};
use qrz::Qrz;

use crate::systems::{
    building::{occupied, Built, Owner, Site},
    characters::Character,
    charts::{held, Charts},
    economy::{Cause, Holder, Moved, Thing},
    haul::{destinations, Depots, Road, Trip},
};

/// How fast settlers come and work: what a hub's growth may one day
/// quicken.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Settling {
    /// How long after a hearth is built its settler comes.
    pub arrives: Duration,
    /// How long a settler works on a tent, on one tile of stakes, on a
    /// wagon and on a post, from begun to raised: long enough that a camp
    /// is earned. A hearth takes a post's work, and taking a thing down
    /// a stake's.
    pub tent: Duration,
    pub stakes: Duration,
    pub wagon: Duration,
    pub post: Duration,
}

impl Default for Settling {
    fn default() -> Self {
        Self { arrives: Duration::from_secs(120), tent: Duration::from_secs(600), stakes: Duration::from_secs(45), wagon: Duration::from_secs(1200), post: Duration::from_secs(180) }
    }
}

impl Settling {
    fn work(&self, step: Step) -> Duration {
        match step {
            Step::Home => self.tent,
            Step::Stakes | Step::Clear => self.stakes,
            Step::Wagon => self.wagon,
            Step::Post | Step::Hearth => self.post,
        }
    }

    /// How long it works to carry a step one stage on: a fixture rises
    /// by stages, and the rest is done in one.
    fn stage(&self, step: Step) -> Duration {
        match step.task() {
            Task::Raise(_) => self.work(step) / (building::STAGES as u32 + 1),
            Task::Hearth | Task::Clear => self.work(step),
        }
    }

    /// Everything `times` as fast.
    pub fn hastened(self, times: u32) -> Self {
        Self { arrives: self.arrives / times, tent: self.tent / times, stakes: self.stakes / times, wagon: self.wagon / times, post: self.post / times }
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

/// What a settler does to one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The hitching post its horse is tied at.
    Post,
    /// A tent, a home.
    Home,
    /// Stakes on a tile of the hub's wall.
    Stakes,
    Wagon,
    /// The hub's hearth, laid where a plan put it.
    Hearth,
    /// Taking down what a plan gave up.
    Clear,
}

impl Step {
    /// The step that raises what `what` asks of its site.
    fn of(what: Standing) -> Step {
        match what {
            Standing::Hearth => Step::Hearth,
            Standing::Post => Step::Post,
            Standing::Wagon => Step::Wagon,
            Standing::Home => Step::Home,
        }
    }

    pub fn task(self) -> Task {
        match self {
            Step::Home => Task::Raise(Fixture::Tent),
            Step::Stakes => Task::Raise(Fixture::Stakes),
            Step::Wagon => Task::Raise(Fixture::Wagon),
            Step::Post => Task::Raise(Fixture::Post),
            Step::Hearth => Task::Hearth,
            Step::Clear => Task::Clear,
        }
    }

    /// What it takes of its camp's materials.
    fn needs(self) -> Needs {
        match self.task() {
            Task::Raise(fixture) => fixture.needs(),
            Task::Hearth => Needs { stone: Kind::Hearth.cost(), ..Needs::default() },
            Task::Clear => Needs::default(),
        }
    }

    /// The number a store keeps the step by, given once. 2 was a second
    /// kind of tent.
    pub fn id(self) -> u32 {
        match self {
            Step::Home => 1,
            Step::Stakes => 3,
            Step::Wagon => 4,
            Step::Post => 5,
            Step::Hearth => 6,
            Step::Clear => 7,
        }
    }

    pub fn kept_as(id: u32) -> Option<Step> {
        [Step::Post, Step::Home, Step::Stakes, Step::Wagon, Step::Hearth, Step::Clear].into_iter().find(|step| step.id() == id).or((id == 2).then_some(Step::Home))
    }
}

/// A camp: a hub's settler and what it holds, known by the tile the hub
/// is.
#[derive(Clone, Debug, PartialEq)]
pub struct Camp {
    /// When its settler comes.
    pub arrives: When,
    /// Whether its settler has walked in, and stands by the hearth when
    /// it is next in the world.
    pub arrived: bool,
    /// What it has begun and not yet finished, the tile it is on, and the
    /// work its stage still wants.
    pub rising: Option<(Step, (i32, i32), Duration)>,
    /// The materials it was handed and has not yet built with.
    pub stock: Vec<Stack>,
    /// What it has of the things it took down, to build with again
    /// before its materials.
    pub credit: Needs,
    /// The tiles a plan gave up, whose stakes or fixture it has yet to
    /// take down.
    pub clearing: Vec<(i32, i32)>,
    /// The moment its settler's work is counted up to: all the daytime
    /// since is work it has yet to be given.
    pub worked: When,
    /// What is loaded on its wagon, to go with it when it is next sent.
    pub load: Vec<Stack>,
    /// Its wagon's trip, while the wagon and its settler are on the road.
    pub trip: Option<Trip>,
}

impl Camp {
    pub fn founded(arrives: When) -> Self {
        Self { arrives, arrived: false, rising: None, stock: Vec::new(), credit: Needs::default(), clearing: Vec::new(), worked: arrives, load: Vec::new(), trip: None }
    }

    /// Whether it has what `needs` asks, of what it took down and its
    /// materials together.
    fn affords(&self, needs: Needs) -> bool {
        needs.minus(self.credit).less(&self.stock).is_none()
    }

    /// Takes `needs` out of what it has: of what it took down first,
    /// then of its materials, which are returned. None, with nothing
    /// taken, where it falls short.
    fn pay(&mut self, needs: Needs) -> Option<Vec<Stack>> {
        let spent = needs.minus(self.credit).taken(&self.stock)?;
        self.credit = self.credit.minus(needs);
        for stack in &spent {
            remove(&mut self.stock, *stack);
        }
        Some(spent)
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

    /// The camp known by `key` is no more, its hub taken in: the camp
    /// as it was, and the settler it had in the world.
    fn taken(&mut self, key: (i32, i32)) -> (Option<Camp>, Option<Entity>) {
        self.changed.push(key);
        (self.camps.remove(&key), self.settlers.remove(&key))
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

/// Where a settler is headed: its hearth, the tile it does a step on,
/// or the tile the leg its wagon is on ends at.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Hearth,
    Site(Step, (i32, i32)),
    Road((i32, i32)),
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
    /// When those who see it were last told it hauls its wagon.
    hitched: Moment,
    /// Whether it sits its horse, as it does until its camp has a post
    /// begun to tie it at.
    rides: bool,
}

fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    qrz::hex_distance(a, b)
}

/// The tile the hearth of the camp known by `key` is laid on: that tile,
/// until a plan lays it elsewhere.
fn hearth_of(hubs: &Hubs, key: (i32, i32)) -> (i32, i32) {
    hubs.of_hearth(key).and_then(|id| hubs.hearth(id)).unwrap_or(key)
}

/// Whether what `what` asks of a site stands on `tile`.
fn stands(site: &Site, what: Standing, tile: (i32, i32)) -> bool {
    let Some(cover) = site.ground.cover(tile.0, tile.1) else { return false };
    match what {
        Standing::Hearth => building::standing(cover).is_some_and(|construct| construct.kind == Kind::Hearth),
        Standing::Post => building::fixture(cover) == Some(Fixture::Post) || building::emptied_post(cover),
        Standing::Wagon => building::fixture(cover) == Some(Fixture::Wagon),
        Standing::Home => building::fixture(cover) == Some(Fixture::Tent),
    }
}

/// The first site of the hub of the camp known by `key` that `what`
/// stands raised on.
fn raised(site: &Site, key: (i32, i32), what: Standing) -> Option<(i32, i32)> {
    let id = site.hubs.of_hearth(key)?;
    site.hubs.standing(id, what).find(|&tile| stands(site, what, tile))
}

/// The site of the hub of the camp known by `key` its first `what` is
/// planned on.
fn planned(hubs: &Hubs, key: (i32, i32), what: Standing) -> Option<(i32, i32)> {
    hubs.standing(hubs.of_hearth(key)?, what).next()
}

/// What stands on `tile` that a settler takes down, as the tile is left
/// and what its camp has of it: a fixture or a hearth, and never a
/// blockade, which is its builder's.
fn clearable(site: &Site, tile: (i32, i32)) -> Option<(common::Cover, Needs)> {
    let cover = site.ground.cover(tile.0, tile.1)?;
    building::cleared(cover).filter(|_| building::standing(cover).is_none_or(|construct| construct.kind == Kind::Hearth))
}

/// Whether `fixture` may be begun on `tile` as the ground lies: the map
/// holds it dry and its cover takes it.
fn open(site: &Site, fixture: Fixture, tile: (i32, i32)) -> bool {
    site.ground.lies(tile.0, tile.1).is_some_and(|(_, cover)| fixture.begun(cover).is_some()) && site.ground.map().water_at(tile.0, tile.1).is_none()
}

/// Whether `step` may be begun on `tile` as the ground lies.
fn takes(site: &Site, step: Step, tile: (i32, i32)) -> bool {
    match step.task() {
        Task::Raise(fixture) => open(site, fixture, tile),
        Task::Hearth => site.ground.cover(tile.0, tile.1).is_some_and(|cover| LAID.raised(cover).is_some()) && site.ground.map().water_at(tile.0, tile.1).is_none(),
        Task::Clear => clearable(site, tile).is_some(),
    }
}

/// The hearth a settler lays where its camp paid for it out of what it
/// took down, which has no material of its own.
const LAID: Construct = Construct { kind: Kind::Hearth, of: common::Material::Sandstone };

/// What the settler of the camp known by `key` does next, standing on
/// `from`, and the tile it does it on, of those no one of `unreached`
/// names: what a plan gave up taken down, the nearest first; then what
/// each of its hub's sites wants, in the order they were built; then
/// stakes on the tile of its wall nearest it. A tile that does not take
/// its step as the ground lies, a tree on a site, is passed over. None
/// once the ground is as the plan says.
fn next_step(site: &Site, camp: &Camp, key: (i32, i32), from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(Step, (i32, i32))> {
    let id = site.hubs.of_hearth(key)?;
    let free = |tile: &(i32, i32)| !unreached.contains(tile);
    if let Some(tile) = camp.clearing.iter().copied().filter(free).min_by_key(|&tile| (distance(tile, from), tile)) {
        return Some((Step::Clear, tile));
    }
    let wanted = site.hubs.sites(id).iter().map(|at| (Step::of(at.what), at.at)).find(|&(step, tile)| !stands(site, site.hubs.sites(id).iter().find(|at| at.at == tile).expect("its own site").what, tile) && free(&tile) && takes(site, step, tile));
    wanted.or_else(|| site.hubs.stakes(id).into_iter().filter(free).filter(|&tile| open(site, Fixture::Stakes, tile)).min_by_key(|&tile| (distance(tile, from), tile)).map(|tile| (Step::Stakes, tile)))
}

/// What a settler wears.
fn outfit() -> Equipment {
    let mut worn = Equipment::default();
    for name in ["acolyte-cap", "acolyte-robe", "acolyte-wraps", "acolyte-sash", "acolyte-hose", "acolyte-shoes"] {
        worn.wear(Item::of(Piece::named(name), 3));
    }
    worn
}

/// Begins `step` of the camp known by `hearth` on `at`: what it takes is
/// spent from what the camp has, and what is raised there is the hub's
/// from then. False, with nothing changed, where the tile no longer
/// takes it or the camp no longer has what it takes.
fn begin(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), step: Step, at: (i32, i32)) -> bool {
    let Some(cover) = site.ground.cover(at.0, at.1) else { return false };
    let Some(camp) = camps.on(hearth).filter(|camp| camp.rising.is_none()) else { return false };
    if !takes(site, step, at) || !camp.affords(step.needs()) {
        return false;
    }
    let camp = camps.change(hearth).expect("its camp stands");
    let spent = camp.pay(step.needs()).expect("it has what the step takes");
    for stack in &spent {
        site.moved.write(Moved::stack(*stack, Holder::Camp(hearth.0, hearth.1), Holder::World, Cause::Build));
    }
    camp.rising = Some((step, at, settling.stage(step)));
    let stone = spent.iter().find_map(|stack| match stack.kind {
        Stackable::Material(material) if material.is_stone() => Some(material),
        _ => None,
    });
    let begun = match step.task() {
        Task::Raise(fixture) => fixture.begun(cover),
        Task::Hearth => Construct { of: stone.unwrap_or(LAID.of), ..LAID }.raised(cover),
        Task::Clear => None,
    };
    if let Some(begun) = begun {
        site.constructs.raise(at, Built { by: Owner::Camp(hearth.0, hearth.1) });
        site.ground.set(&mut site.writer, at.0, at.1, begun);
    }
    info!("settlers: the settler of {hearth:?} began {step:?} on {at:?}");
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
    if step == Step::Clear {
        let down = clearable(site, at);
        let camp = camps.change(hearth).expect("its camp stands");
        camp.rising = None;
        camp.clearing.retain(|tile| *tile != at);
        if let Some((cover, back)) = down {
            camp.credit = camp.credit.plus(back);
            site.constructs.fell(at);
            site.ground.set(&mut site.writer, at.0, at.1, cover);
            info!("settlers: the settler of {hearth:?} took down what stood on {at:?}");
        }
        return Worked::Raised;
    }
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
    site.ground.set(&mut site.writer, at.0, at.1, next);
    info!("settlers: the settler of {hearth:?} raised {step:?} on {at:?}");
    Worked::Raised
}

/// What the camp known by `hearth` wants done next, by a settler
/// standing on `from`: the thing it has begun, to be finished; or else
/// its next step ([`next_step`]), where it has what that takes. A tile it
/// was to clear that holds nothing of the hub's is done with.
fn work_for(site: &Site, camps: &mut Camps, hearth: (i32, i32), from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(Step, (i32, i32))> {
    let camp = camps.on(hearth)?;
    if let Some((step, at, _)) = camp.rising {
        return Some((step, at));
    }
    let bare = |tile: &(i32, i32)| site.ground.cover(tile.0, tile.1).is_some() && clearable(site, *tile).is_none();
    if camp.clearing.iter().any(bare) {
        camps.change(hearth)?.clearing.retain(|tile| !bare(tile));
    }
    let camp = camps.on(hearth)?;
    next_step(site, camp, hearth, from, unreached).filter(|(step, _)| camp.affords(step.needs()))
}

/// Gives the camp round `hearth` what its settler would have raised in
/// `spare`, daytime not yet counted: each thing next in its line that the
/// camp holds the materials for and the settler has a way to, a stage at
/// a time, while the walk there and the work fit in what is left; what is
/// left over comes off the stage it is on. Returns where the settler
/// ends, beside the last thing it worked on or at its hearth.
fn catch_up(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), mut spare: Duration) -> (i32, i32) {
    let mut at = hearth_of(&site.hubs, hearth);
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

/// Stands the wagon of the camp known by `hearth` on the ground again, on
/// its site. Where that no longer takes it the wagon is lost.
fn park(site: &mut Site, camps: &mut Camps, hearth: (i32, i32)) {
    if camps.change(hearth).is_none() {
        return;
    }
    let place = planned(&site.hubs, hearth, Standing::Wagon).and_then(|tile| Some((tile, building::parked(site.ground.cover(tile.0, tile.1)?)?)));
    match place {
        Some((tile, cover)) => {
            site.constructs.raise(tile, Built { by: Owner::Camp(hearth.0, hearth.1) });
            site.ground.set(&mut site.writer, tile.0, tile.1, cover);
        }
        None => warn!("settlers: the wagon of {hearth:?} came home to no open ground, and is lost"),
    }
    // Its horse is tied at its post again
    let post = planned(&site.hubs, hearth, Standing::Post);
    if let Some((tile, cover)) = post.and_then(|tile| Some((tile, building::tied(site.ground.cover(tile.0, tile.1)?)?))) {
        site.ground.set(&mut site.writer, tile.0, tile.1, cover);
    }
}

/// Carries the wagon of the camp round `hearth` on through the daytime
/// since its trip was last counted. Come to its hub, it leaves what it
/// carried in that hub's depot and turns for home; come home, it stands
/// in its camp again once the ground there is held, and its trip is
/// over.
fn haul(site: &mut Site, camps: &mut Camps, charts: &Charts, depots: &mut Depots, hearth: (i32, i32), when: When) {
    let Some(trip) = camps.camps.get_mut(&hearth).and_then(|camp| camp.trip.as_mut()) else { return };
    let gap = when.daytime_since(trip.counted);
    trip.counted = when;
    let step = Duration::from_secs_f32(1.0 / walked());
    if trip.carry(charts.lies(), gap, step) != Road::Arrived {
        return;
    }
    if trip.back {
        if site.ground.lies(hearth.0, hearth.1).is_some() {
            info!("settlers: the wagon of {hearth:?} is home");
            camps.change(hearth).expect("its camp stands").trip = None;
            park(site, camps, hearth);
        }
        return;
    }
    let (to, cargo) = (trip.to, std::mem::take(&mut trip.cargo));
    trip.turned();
    depots.leave(to, &cargo);
    for stack in cargo {
        site.moved.write(Moved::stack(stack, Holder::Camp(hearth.0, hearth.1), Holder::Depot(to.0, to.1), Cause::Haul));
    }
    camps.changed.push(hearth);
    info!("settlers: the wagon of {hearth:?} left its load at the hub of {to:?} and turned for home");
}

/// Brings a camp's settler into the world while a player is near its
/// hearth, or near where its wagon's trip has come to, and its time has
/// come, and takes it out once none is. By night
/// one with a tent is in it, out of the world; one with none stays, to
/// lie by its hearth. One that has yet to walk in comes by the way in
/// through its hub's wall; one that has stands by its hearth, or by what
/// it last raised while it was away ([`catch_up`]).
#[allow(clippy::too_many_arguments)]
pub fn tend_camps(
    mut site: Site,
    mut camps: ResMut<Camps>,
    settling: Res<Settling>,
    charts: Res<Charts>,
    mut depots: ResMut<Depots>,
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
        haul(&mut site, camps, &charts, &mut depots, hearth, when);
        let camp = &camps.camps[&hearth];
        // On the road its settler is where the trip has come to, and
        // sleeps on the ground there
        let hauling = camp.trip.is_some();
        let fire = hearth_of(&site.hubs, hearth);
        let post = camp.trip.as_ref().and_then(Trip::at).unwrap_or(fire);
        let housed = asleep && raised(&site, hearth, Standing::Home).is_some() && !hauling;
        match (camps.settlers.get(&hearth), nearest(post)) {
            (Some(&settler), far) if housed || far.is_none_or(|far| far > UNWATCHED) => {
                commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
                camps.settlers.remove(&hearth);
                camps.changed.push(hearth);
            }
            (None, Some(near)) if !housed && near <= WATCHED && when >= camp.arrives => {
                if site.ground.map().get_by_qr(post.0, post.1).is_none() {
                    continue;
                }
                // Away long enough, it walked in unseen and worked on,
                // through the days and never the nights
                let away = when.daytime_since(camp.worked);
                let mut stands = post;
                if !hauling && away >= AWAY {
                    let camp = camps.change(hearth).expect("its camp stands");
                    (camp.arrived, camp.worked) = (true, when);
                    stands = catch_up(&mut site, camps, &settling, hearth, away);
                }
                let camp = &camps.camps[&hearth];
                let (hubs, map) = (&site.hubs, site.ground.map());
                let Some((floor, _)) = map.get_by_qr(stands.0, stands.1) else { continue };
                let beside = Some(floor)
                    .filter(|_| stands != fire)
                    .or_else(|| map.neighbors(floor).into_iter().map(|(tile, _)| tile).find(|tile| !common_bevy::systems::movement::is_full_cover(map, tile.q, tile.r)));
                let from_edge = || {
                    let way_in = hubs.way_in(hubs.of_hearth(hearth)?)?;
                    map.get_by_qr(way_in.0, way_in.1).map(|(at, _)| at).filter(|at| path::find(map, *at, floor, 1).is_some())
                };
                let Some(at) = (if camp.arrived { None } else { from_edge() }).or(beside) else { continue };
                let stands = at + Qrz::Z;
                let loc = Loc::new(stands);
                // It sits its horse until it sets to the post it ties it at
                let rides = raised(&site, hearth, Standing::Post).is_none() && camp.rising.is_none() && !hauling;
                let typ = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Settler));
                let settler = commands
                    .spawn((
                        typ,
                        loc,
                        Side::PLAYERS,
                        // Who sees it: every actor a client is shown holds this
                        LoadedBy::default(),
                        outfit(),
                        Settler { hearth, errand: Errand::Idle { think: now }, unreached: HashSet::new(), hitched: Moment::ZERO, rides },
                        Status { mounted: rides, ..default() },
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
const HAULING: Option<common::gathering::Activity> = Some(common::gathering::Activity::Haul);

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
        // It gets down once its camp has a post, or one begun
        if settler.rides && (camp.rising.is_some() || raised(&site, hearth, Standing::Post).is_some()) {
            settler.rides = false;
            commands.entity(ent).insert(Status::default());
            commands.write_message(Do { event: Event::Incremental { ent, component: common_bevy::message::Component::Status(Status::default()) } });
        }
        // On the road it walks the leg its wagon's trip is on, which the
        // calendar carries on whether or not it keeps up
        if let Some(leg) = camp.trip.as_ref().map(Trip::leg) {
            camp.worked = when;
            let Some((floor, _)) = site.ground.map().get_by_qr(loc.q, loc.r) else { continue };
            let here = (loc.q, loc.r);
            if now.since(settler.hitched) >= TOLD {
                show(ent, HAULING, &mut commands);
                settler.hitched = now;
            }
            // By night it keeps its seat, and the wagon stands
            if night {
                continue;
            }
            let Some(leg) = leg.filter(|leg| leg.to != here) else { continue };
            settler.errand = match settler.errand.clone() {
                Errand::Going { goal: Goal::Road(to), mut way, mut since } if to == leg.to => {
                    if way.first().is_some_and(|next| (next.q, next.r) == here) {
                        way.remove(0);
                        since = now;
                    }
                    match way.first().copied() {
                        Some(next) if floor.flat_distance(&next) == 1 && now.since(since) <= STUCK => {
                            body.step_toward(loc, floor, next, MOVEMENT_SPEED, PACE, dt, site.ground.map(), &site.nntree);
                            Errand::Going { goal: Goal::Road(to), way, since }
                        }
                        _ => Errand::Idle { think: now + THINK },
                    }
                }
                Errand::Idle { think } if now < think => Errand::Idle { think },
                // To the leg's end, or as near the step out of its chunk
                // as the ground lets it come
                _ => {
                    let map = site.ground.map();
                    let way = [leg.to, leg.by].into_iter().find_map(|tile| path::find(map, floor, map.get_by_qr(tile.0, tile.1)?.0, 0).filter(|way| !way.is_empty()));
                    match way {
                        Some(way) => Errand::Going { goal: Goal::Road(leg.to), way, since: now },
                        None => Errand::Idle { think: now + STUCK },
                    }
                }
            };
            continue;
        }
        // Home from the road, it lets its wagon go
        if settler.hitched != Moment::ZERO {
            show(ent, None, &mut commands);
            settler.hitched = Moment::ZERO;
        }
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
        let fire = hearth_of(&site.hubs, hearth);
        let home = |site: &Site| site.ground.map().get_by_qr(fire.0, fire.1).and_then(|(at, _)| path::find(site.ground.map(), floor, at, 1));
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
                    None if distance(here, fire) > 1 => match home(&site) {
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
                            Goal::Hearth | Goal::Road(_) => Errand::Idle { think: now },
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

/// What everything the hub of the camp known by `key` has yet to see
/// raised takes, as the ground lies: each site with nothing on it, and
/// each tile of its wall that takes stakes and has none. What is begun
/// is paid for.
fn remaining(site: &Site, camp: &Camp, key: (i32, i32)) -> Needs {
    let Some(id) = site.hubs.of_hearth(key) else { return Needs::default() };
    let begun = camp.rising.map(|(_, at, _)| at);
    let sites = site.hubs.sites(id).iter().filter(|at| Some(at.at) != begun && !stands(site, at.what, at.at)).map(|at| Step::of(at.what).needs());
    let stakes = site.hubs.stakes(id).into_iter().filter(|&tile| Some(tile) != begun && open(site, Fixture::Stakes, tile)).map(|_| Step::Stakes.needs());
    sites.chain(stakes).fold(Needs::default(), Needs::plus)
}

/// What the settler of `camp`, known by `key`, does next, and what the
/// camp still lacks for all its hub has yet to see raised, counting what
/// it will have of the things it has yet to take down: what a player who
/// hails it is told.
fn wants(site: &Site, camp: &Camp, key: (i32, i32)) -> Wants {
    let coming = camp.clearing.iter().filter_map(|&tile| clearable(site, tile)).fold(camp.credit, |coming, (_, back)| coming.plus(back));
    let needs = remaining(site, camp, key).minus(coming).less(&camp.stock);
    let step = camp.rising.map(|(step, ..)| step).or_else(|| next_step(site, camp, key, hearth_of(&site.hubs, key), &HashSet::new()).map(|(step, _)| step));
    Wants { task: step.map(Step::task), needs }
}

/// Whether the wagon of `camp`, known by `key`, stands ready to be
/// loaded and sent: raised, home, and nothing more for its settler to do.
fn carts(site: &Site, camp: &Camp, key: (i32, i32)) -> bool {
    camp.rising.is_none() && camp.trip.is_none() && raised(site, key, Standing::Wagon).is_some() && wants(site, camp, key).task.is_none()
}

/// Holds the settlers and the hubs to the settings as they change: how
/// many times as fast settlers come and work, and how many players
/// every hearth counts as at home besides those who are.
pub fn heed_settings(settings: Res<Settings>, mut settling: ResMut<Settling>, mut hubs: ResMut<Hubs>, mut heeded: Local<Option<(i32, i32)>>) {
    if !settings.is_changed() {
        return;
    }
    let (haste, players) = (settings.get(Setting::SettlerHaste), settings.get(Setting::HearthPlayers));
    let (hasted, counted) = heeded.replace((haste, players)).unwrap_or((Setting::SettlerHaste.default(), Setting::HearthPlayers.default()));
    if haste != hasted {
        *settling = Settling::default().hastened(haste.max(1) as u32);
    }
    if players != counted {
        hubs.count_extra(players.max(0) as u32);
    }
}

/// The hearth of the settler `loc` hails: the nearest within reach.
fn hailed(settlers: &Query<(&Settler, &Loc)>, loc: &Loc) -> Option<(i32, i32)> {
    settlers.iter().map(|(settler, at)| (loc.flat_distance(at), settler.hearth)).filter(|&(far, _)| far <= HAIL).min().map(|(_, hearth)| hearth)
}

/// Answers each player that hails a settler: the nearest within reach
/// says what it does next and what it still lacks for all it has yet to
/// raise. Asked to, the player hands it what its bag holds of that, no
/// more than is lacking.
/// One with nothing more to do speaks of its wagon besides: what is
/// loaded on it, and the hubs the map in the player's hand would send it
/// to; asked to, the player loads on it every material its bag holds.
/// At night it sleeps and answers no one.
pub fn try_hail(
    mut reader: MessageReader<Try>,
    mut site: Site,
    mut camps: ResMut<Camps>,
    charts: Res<Charts>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    settlers: Query<(&Settler, &Loc)>,
    mut players: Query<(&Loc, &mut Inventory, &Equipment, &Character)>,
) {
    for message in reader.read() {
        let Try { event: Event::Hail { ent, give } } = message else { continue };
        let (ent, give) = (*ent, *give);
        let Ok((loc, mut bag, worn, &Character(who))) = players.get_mut(ent) else { continue };
        let night = runtime.when(&time).is_night();
        let Some(hearth) = hailed(&settlers, loc).filter(|&hearth| !night && camps.on(hearth).is_some_and(|camp| camp.trip.is_none())) else {
            site.writer.write(Do { event: Event::Wants { ent, wants: None } });
            continue;
        };
        let carted = carts(&site, &camps.camps[&hearth], hearth);
        if give && carted {
            let camp = camps.change(hearth).expect("its camp stands");
            for stack in std::mem::take(&mut bag.stock) {
                match camp.load.iter_mut().find(|held| held.kind == stack.kind) {
                    Some(held) => held.count += stack.count,
                    None => camp.load.push(stack),
                }
                site.moved.write(Moved::stack(stack, Holder::Character(who), Holder::Camp(hearth.0, hearth.1), Cause::Give));
                info!("settlers: {ent} loaded {} {:?} on the wagon of {hearth:?}", stack.count, stack.kind);
            }
            site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        } else if give {
            let lacking = wants(&site, &camps.camps[&hearth], hearth).needs;
            hand(&mut site, camps.change(hearth).expect("its camp stands"), hearth, lacking, ent, who, &mut bag);
        }
        let said = wants(&site, &camps.camps[&hearth], hearth);
        site.writer.write(Do { event: Event::Wants { ent, wants: Some(said) } });
        let wagon = carted.then(|| {
            let reached = held(worn).and_then(|map| charts.map(map)).map_or(Vec::new(), |chart| destinations(chart, &site.hubs, hearth));
            common::building::Wagon { load: camps.camps[&hearth].load.clone(), hubs: reached.into_iter().map(|(hub, _)| hub).collect() }
        });
        site.writer.write(Do { event: Event::Wagon { ent, wagon } });
    }
}

/// Hands the camp known by `key` what `bag` holds of `lacking`, no more
/// than is lacking, and tells the player's client its bag.
fn hand(site: &mut Site, camp: &mut Camp, key: (i32, i32), mut lacking: Needs, ent: Entity, who: common::network::Identity, bag: &mut Inventory) {
    let held: Vec<Stack> = bag.stock.clone();
    for stack in held {
        let Stackable::Material(material) = stack.kind;
        let want = if material.is_wood() {
            &mut lacking.wood
        } else if material.is_stone() {
            &mut lacking.stone
        } else if material == common::Material::Hide {
            &mut lacking.hide
        } else {
            continue;
        };
        let count = stack.count.min(*want);
        if count == 0 {
            continue;
        }
        *want -= count;
        let given = bag.remove(stack.kind, count);
        match camp.stock.iter_mut().find(|held| held.kind == given.kind) {
            Some(held) => held.count += given.count,
            None => camp.stock.push(given),
        }
        site.moved.write(Moved::stack(given, Holder::Character(who), Holder::Camp(key.0, key.1), Cause::Give));
        info!("settlers: {ent} handed {} {:?} to the settler of {key:?}", given.count, given.kind);
    }
    site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
}

/// What `plan` would take of the camp known by `key` that it and the
/// camps of the hubs taken in have not got: what the plan builds and the
/// stakes it raises, less what it takes down as the ground lies.
fn lacking(site: &Site, camps: &Camps, key: (i32, i32), plan: &Plan) -> Needs {
    let stakes = (0..plan.raised.len()).fold(Needs::default(), |needs, _| needs.plus(Fixture::Stakes.needs()));
    let needs = plan.built.iter().fold(stakes, |needs, at| needs.plus(Step::of(at.what).needs()));
    let down = plan.pulled.iter().copied().chain(plan.gone.iter().map(|at| at.at));
    let back = down.filter_map(|tile| clearable(site, tile)).fold(Needs::default(), |back, (_, of)| back.plus(of));
    let theirs: Vec<&Camp> = std::iter::once(key).chain(plan.taken.iter().map(|id| (id.0, id.1))).filter_map(|key| camps.on(key)).collect();
    let credit = theirs.iter().fold(back, |credit, camp| credit.plus(camp.credit));
    let stock: Vec<Stack> = theirs.iter().flat_map(|camp| camp.stock.iter().copied()).collect();
    needs.minus(credit).less(&stock)
}

/// Answers each player that asks the settler it hails for a plan: what
/// growing its hub a tier, or shrinking it to what its people hold,
/// would do, and what the camp still lacks for it. Asked to carry it
/// out, it does where the camp lacks nothing, and a shrink whatever it
/// lacks; a grow the camp lacks materials for takes what the player's
/// bag holds of those instead, and is asked for again. Carried out, the
/// hubs are as the plan leaves them from then, a hub taken in
/// gives its camp all it held, each hub parted off has a camp and a
/// settler of its own, and the settlers make the ground so in their own
/// time. No plan is made by night, with the wagon on the road, or while
/// the settler, or that of a hub the plan takes in, has a thing begun.
#[allow(clippy::too_many_arguments)]
pub fn try_plan(
    mut reader: MessageReader<Try>,
    mut site: Site,
    mut camps: ResMut<Camps>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    settlers: Query<(&Settler, &Loc)>,
    mut players: Query<(&Loc, &mut Inventory, &Character)>,
    homes: Query<(Entity, &Character, &common_bevy::components::Home)>,
    mut commands: Commands,
) {
    for message in reader.read() {
        let Try { event: Event::Plan { ent, grow, carry } } = message else { continue };
        let (ent, grow, carry) = (*ent, *grow, *carry);
        let Ok((loc, mut bag, &Character(who))) = players.get_mut(ent) else { continue };
        let when = runtime.when(&time);
        let idle = |key: (i32, i32)| camps.on(key).is_none_or(|camp| camp.rising.is_none() && camp.trip.is_none());
        let plan = hailed(&settlers, loc).filter(|&key| !when.is_night() && idle(key)).and_then(|key| {
            let id = site.hubs.of_hearth(key)?;
            let plan = if grow { site.hubs.grown(id) } else { site.hubs.shrunk(id) }?;
            plan.taken.iter().all(|id| idle((id.0, id.1))).then_some((key, plan))
        });
        let Some((key, plan)) = plan else {
            site.writer.write(Do { event: Event::Planned { ent, plan: None } });
            continue;
        };
        let mut needs = lacking(&site, &camps, key, &plan);
        if carry && grow && !needs.is_none() {
            hand(&mut site, camps.change(key).expect("its camp stands"), key, needs, ent, who, &mut bag);
            needs = lacking(&site, &camps, key, &plan);
        }
        if !carry || (grow && !needs.is_none()) {
            let tiles = |sites: &[common_bevy::sites::Site]| sites.iter().map(|at| at.at).collect();
            let said = Planned { grow, built: tiles(&plan.built), gone: tiles(&plan.gone), raised: plan.raised.clone(), pulled: plan.pulled.clone(), worn: plan.worn.clone(), faded: plan.faded.clone(), hubs: (plan.taken.len() + plan.parted().count()) as u32, needs };
            site.writer.write(Do { event: Event::Planned { ent, plan: Some(said) } });
            continue;
        }
        for taken in plan.taken.iter().map(|id| (id.0, id.1)) {
            let (camp, settler) = camps.taken(taken);
            if let Some(settler) = settler {
                commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
            }
            let Some(camp) = camp else { continue };
            let ours = camps.change(key).expect("its camp stands");
            ours.credit = ours.credit.plus(camp.credit);
            ours.clearing.extend(camp.clearing);
            for (stack, loaded) in camp.stock.into_iter().map(|stack| (stack, false)).chain(camp.load.into_iter().map(|stack| (stack, true))) {
                let held = if loaded { &mut ours.load } else { &mut ours.stock };
                match held.iter_mut().find(|held| held.kind == stack.kind) {
                    Some(held) => held.count += stack.count,
                    None => held.push(stack),
                }
                site.moved.write(Moved::stack(stack, Holder::Camp(taken.0, taken.1), Holder::Camp(key.0, key.1), Cause::Give));
            }
        }
        let ours = camps.change(key).expect("its camp stands");
        ours.clearing.extend(plan.pulled.iter().copied().chain(plan.gone.iter().map(|at| at.at)));
        ours.clearing.sort();
        ours.clearing.dedup();
        for parted in plan.parted() {
            camps.found((parted.0, parted.1), when);
        }
        site.hubs.carry(&plan);
        crate::systems::building::rehouse(&site, &homes, &mut commands);
        info!("settlers: {ent} had the settler of {key:?} {} its hub", if grow { "grow" } else { "shrink" });
        site.writer.write(Do { event: Event::Planned { ent, plan: None } });
        let said = wants(&site, &camps.camps[&key], key);
        site.writer.write(Do { event: Event::Wants { ent, wants: Some(said) } });
        site.writer.write(Do { event: Event::Wagon { ent, wagon: None } });
    }
}

/// Answers each player that sends a wagon: the settler it hails takes
/// the map in its main hand, which is used up, and leaves with its
/// camp's wagon, off the tile it stood on, and all loaded on it for the
/// hub named, by the way the map holds. Nothing leaves where the wagon is not ready, holds
/// nothing, or the map holds no way to that hub.
#[allow(clippy::too_many_arguments)]
pub fn try_send(
    mut reader: MessageReader<Try>,
    mut site: Site,
    mut camps: ResMut<Camps>,
    mut charts: ResMut<Charts>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    settlers: Query<(&Settler, &Loc)>,
    mut players: Query<(&Loc, &mut Inventory, &mut Equipment, &Character)>,
) {
    for message in reader.read() {
        let Try { event: Event::Send { ent, to } } = message else { continue };
        let (ent, to) = (*ent, *to);
        let Ok((loc, mut bag, mut worn, &Character(who))) = players.get_mut(ent) else { continue };
        let when = runtime.when(&time);
        let ready = |hearth: &(i32, i32)| !when.is_night() && camps.on(*hearth).is_some_and(|camp| carts(&site, camp, *hearth) && !camp.load.is_empty());
        let Some(hearth) = hailed(&settlers, loc).filter(ready) else {
            info!("settlers: {ent} has no wagon ready to send");
            continue;
        };
        let Some((map, number)) = worn.worn(Slot::MainHand).and_then(|item| Some((item, item.map()?))) else { continue };
        let way = charts.map(number).and_then(|chart| destinations(chart, &site.hubs, hearth).into_iter().find(|(hub, _)| *hub == to));
        let Some((_, way)) = way else {
            info!("settlers: map {number} holds no way from {hearth:?} to the hub of {to:?}");
            continue;
        };
        worn.take_off(map);
        if let Some(at) = bag.items.iter().position(|item| *item == map) {
            bag.items.remove(at);
        }
        charts.spend(number);
        site.moved.write(Moved { what: Thing::Piece(map), count: 1, from: Holder::Character(who), to: Holder::World, cause: Cause::Haul });
        // Its horse is led from the post to draw it
        if let Some((tile, led)) = planned(&site.hubs, hearth, Standing::Post).and_then(|tile| Some((tile, building::untied(site.ground.cover(tile.0, tile.1)?)?))) {
            site.ground.set(&mut site.writer, tile.0, tile.1, led);
        }
        // The wagon leaves its site, which stays its place
        if let Some((tile, gone)) = planned(&site.hubs, hearth, Standing::Wagon).and_then(|tile| Some((tile, building::driven(site.ground.cover(tile.0, tile.1)?)?))) {
            site.ground.set(&mut site.writer, tile.0, tile.1, gone);
        }
        let camp = camps.change(hearth).expect("its camp stands");
        let cargo = std::mem::take(&mut camp.load);
        info!("settlers: the wagon of {hearth:?} left for the hub of {to:?} by map {number}, {} legs, carrying {cargo:?}", way.len());
        camp.trip = Some(Trip::begun(to, way, cargo, when));
        site.writer.write(Do { event: Event::Incremental { ent, component: common_bevy::message::Component::Equipment(*worn) } });
        site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        site.writer.write(Do { event: Event::Wants { ent, wants: None } });
        site.writer.write(Do { event: Event::Wagon { ent, wagon: None } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(material: common::Material, count: u32) -> Stack {
        Stack { kind: Stackable::Material(material), count }
    }

    /// A camp builds with what it took down before its materials, and
    /// pays nothing where the two together fall short.
    #[test]
    fn a_camp_builds_with_what_it_took_down_first() {
        let mut camp = Camp::founded(When(0));
        let tent = Fixture::Tent.needs();
        assert!(!camp.affords(tent) && camp.pay(tent).is_none());
        camp.credit = Needs { wood: tent.wood, ..Needs::default() };
        camp.stock = vec![of(common::Material::Hide, tent.hide + 1), of(common::Material::Hardwood, 2)];
        assert!(camp.affords(tent));
        assert_eq!(camp.pay(tent), Some(vec![of(common::Material::Hide, tent.hide)]), "the wood it took down, the hide it was handed");
        assert_eq!((camp.credit, camp.stock.clone()), (Needs::default(), vec![of(common::Material::Hide, 1), of(common::Material::Hardwood, 2)]));
        assert!(!camp.affords(tent));
    }

    /// Every step is kept by a number of its own, and a camp kept at the
    /// second kind of tent there was is at a home.
    #[test]
    fn a_step_is_kept_by_its_number() {
        assert!([Step::Post, Step::Home, Step::Stakes, Step::Wagon, Step::Hearth, Step::Clear].into_iter().all(|step| Step::kept_as(step.id()) == Some(step)));
        assert_eq!((Step::kept_as(2), Step::kept_as(99)), (Some(Step::Home), None));
        assert!(Standing::ALL.into_iter().all(|what| !Step::of(what).needs().is_none()), "whatever stands on a site takes materials");
        assert!(Step::Clear.needs().is_none());
    }
}
