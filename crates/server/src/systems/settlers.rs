//! Settlers: the NPCs that raise a hub. The first comes to a hub once a
//! player has raised it a hitching post, a while after, riding its
//! horse; no settler comes to a bare hearth. Hired as a zone's planner
//! (`Hubs::hire`), it makes the ground what the hub's plans say
//! (`common_bevy::plan`): it takes down what a plan gave up, raises each
//! thing its zone was asked for, a wagon, a storehouse, a tent, and
//! stakes along the hub's wall once that is asked for. A step waits
//! while its zone has no planner. It raises only what the camp holds the
//! materials for, handed over by a player who hails it, and what is
//! raised is the hub's and no player's to take down. What is taken down
//! is built with again. Every settler living in the hub has a body: a
//! zone's planner does that zone's steps, each at its own thing at once
//! with the others, and a settler with nothing to do walks the hub by day
//! and is in its home by night. A player hails the planner it wants: it
//! answers for its own zone, and takes that zone's orders.
//!
//! Asked to by a player who hails it, it says what an order to the hub's
//! planners would do, and carries that out ([`try_plan`]): the hub
//! planned for more residents or fewer, drawn in to what its residents
//! defend, its wall raised, a thing raised, a zone centred.
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
//! A hub's storehouse is where players leave materials and take their
//! own out ([`try_store`]), and where a wagon come to the hub leaves
//! each player's share of its load (`haul::Depots`).
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
    building::{self, Construct, Fixture, Kind, Needs, Order, Planned, Task, Wants},
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
    plan::{What, Zone},
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
    haul::{destinations, homes, hub_at, put, totals, Depots, Owned, Road, Trip},
};

/// How fast settlers come and work: what a hub's growth may one day
/// quicken.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Settling {
    /// How long after a hearth is built its settler comes.
    pub arrives: Duration,
    /// How long a settler works on a tent, on one tile of stakes, on a
    /// wagon, on a post and on a storehouse, from begun to raised: long
    /// enough that a camp is earned. A hearth takes a post's work, and taking a thing down
    /// a stake's.
    pub tent: Duration,
    pub stakes: Duration,
    pub wagon: Duration,
    pub post: Duration,
    pub store: Duration,
}

impl Default for Settling {
    fn default() -> Self {
        Self { arrives: Duration::from_secs(120), tent: Duration::from_secs(600), stakes: Duration::from_secs(45), wagon: Duration::from_secs(1200), post: Duration::from_secs(180), store: Duration::from_secs(900) }
    }
}

impl Settling {
    fn work(&self, step: Step) -> Duration {
        match step {
            Step::Home => self.tent,
            // A larger home takes a tent's work for each tile it stands on
            Step::Pavilion | Step::House | Step::Tenement => match step.task() {
                Task::Raise(fixture) => self.tent * fixture.tiles().len() as u32,
                _ => self.tent,
            },
            Step::Stakes | Step::Clear => self.stakes,
            Step::Wagon => self.wagon,
            Step::Post | Step::Hearth => self.post,
            Step::Store => self.store,
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
        Self { arrives: self.arrives / times, tent: self.tent / times, stakes: self.stakes / times, wagon: self.wagon / times, post: self.post / times, store: self.store / times }
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

/// The share of a run a settler walks at: the one a client simulates it
/// by.
const PACE: f32 = common_bevy::components::entity_type::actor::SETTLER_PACE;

use common::building::HAIL;

/// What a settler does to one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The hitching post its horse is tied at.
    Post,
    /// A tent, a home.
    Home,
    /// The homes a tent is upgraded to, each on more tiles than the last.
    Pavilion,
    House,
    Tenement,
    /// Stakes on a tile of the hub's wall.
    Stakes,
    Wagon,
    /// The storehouse.
    Store,
    /// The hub's hearth, laid where a plan put it.
    Hearth,
    /// Taking down what a plan gave up.
    Clear,
}

impl Step {
    const ALL: [Step; 10] = [Step::Post, Step::Home, Step::Pavilion, Step::House, Step::Tenement, Step::Stakes, Step::Wagon, Step::Store, Step::Hearth, Step::Clear];

    /// The step that raises a thing of `kind`. Unbuilt: a home of the
    /// last level, any other building past its first, and a market's, a
    /// workshop's and a farm's buildings have none, and no settler raises
    /// one.
    fn of(kind: common_bevy::plan::Kind) -> Option<Step> {
        match (kind.what, kind.level) {
            (What::Hearth, _) => Some(Step::Hearth),
            (What::Home, 1) => Some(Step::Home),
            (What::Home, 2) => Some(Step::Pavilion),
            (What::Home, 3) => Some(Step::House),
            (What::Home, 4) => Some(Step::Tenement),
            (What::Post, 1) => Some(Step::Post),
            (What::Wagon, 1) => Some(Step::Wagon),
            (What::Store, 1) => Some(Step::Store),
            _ => None,
        }
    }

    /// Where it raises `item`: the tile that holds the thing, and how
    /// many sixths of a turn the thing is turned to lie on the item's
    /// tiles. None where its fixture lies on them no way.
    fn lies(self, item: &common_bevy::plan::Item) -> Option<((i32, i32), u8)> {
        match self.task() {
            Task::Raise(fixture) => fixture.lies(&item.tiles),
            Task::Hearth | Task::Clear => item.tiles.first().map(|&tile| (tile, 0)),
        }
    }

    /// Whether what it raises stands on a tile with `cover`.
    fn stood(self, cover: common::Cover) -> bool {
        match self.task() {
            Task::Raise(Fixture::Post) => building::fixture(cover) == Some(Fixture::Post) || building::emptied_post(cover),
            Task::Raise(fixture) => building::fixture(cover) == Some(fixture),
            Task::Hearth => building::standing(cover).is_some_and(|construct| construct.kind == Kind::Hearth),
            Task::Clear => false,
        }
    }

    /// The zone whose planner does it. Taking a thing down and laying a
    /// hearth again are the logistics planner's, as the wall is.
    fn zone(self) -> Zone {
        match self {
            Step::Home | Step::Pavilion | Step::House | Step::Tenement => Zone::Residential,
            Step::Post | Step::Stakes | Step::Wagon | Step::Store | Step::Hearth | Step::Clear => Zone::Logistics,
        }
    }

    pub fn task(self) -> Task {
        match self {
            Step::Home => Task::Raise(Fixture::Tent),
            Step::Pavilion => Task::Raise(Fixture::Pavilion),
            Step::House => Task::Raise(Fixture::House),
            Step::Tenement => Task::Raise(Fixture::Tenement),
            Step::Stakes => Task::Raise(Fixture::Stakes),
            Step::Wagon => Task::Raise(Fixture::Wagon),
            Step::Post => Task::Raise(Fixture::Post),
            Step::Store => Task::Raise(Fixture::Storehouse),
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
            Step::Store => 8,
            Step::Pavilion => 9,
            Step::House => 10,
            Step::Tenement => 11,
        }
    }

    pub fn kept_as(id: u32) -> Option<Step> {
        Step::ALL.into_iter().find(|step| step.id() == id).or((id == 2).then_some(Step::Home))
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
    /// How many of its other settlers have walked in: one whose place is
    /// past this comes in from far off when it is next in the world, and
    /// any other comes out beside its home.
    pub joined: u8,
    /// What its planners have begun and not yet finished, one thing to a
    /// zone: the step, the tile it is on, and the work its stage still
    /// wants.
    pub rising: Vec<(Step, (i32, i32), Duration)>,
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
    /// What is loaded on its wagon, to go with it when it is next sent,
    /// each stack its loader's.
    pub load: Vec<Owned>,
    /// Its wagon's trip, while the wagon and its settler are on the road.
    pub trip: Option<Trip>,
}

impl Camp {
    pub fn founded(arrives: When) -> Self {
        Self { arrives, arrived: false, joined: 0, rising: Vec::new(), stock: Vec::new(), credit: Needs::default(), clearing: Vec::new(), worked: arrives, load: Vec::new(), trip: None }
    }

    /// What the planner of `zone` has begun and not yet finished.
    pub fn rising_of(&self, zone: Zone) -> Option<(Step, (i32, i32), Duration)> {
        self.rising.iter().copied().find(|(step, ..)| step.zone() == zone)
    }

    /// The planner of `step`'s zone has it begun on `at`, with `left` of
    /// its stage's work to do, in place of what it had begun.
    fn begins(&mut self, step: Step, at: (i32, i32), left: Duration) {
        self.finished(step.zone());
        self.rising.push((step, at, left));
    }

    /// The planner of `zone` has nothing begun.
    fn finished(&mut self, zone: Zone) {
        self.rising.retain(|(step, ..)| step.zone() != zone);
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
    /// Each camp's first settler in the world: the one that came for its
    /// post, drives its wagon and answers for it.
    settlers: HashMap<(i32, i32), Entity>,
    /// Its other settlers in the world, by their place among them.
    others: HashMap<((i32, i32), u8), Entity>,
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

    /// A hitching post stands at the camp known by `hearth`, which had
    /// none: its first settler comes at `arrives`.
    pub fn posted(&mut self, hearth: (i32, i32), arrives: When) {
        if let Some(camp) = self.change(hearth).filter(|camp| !camp.arrived) {
            (camp.arrives, camp.worked) = (arrives, arrives);
        }
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

    /// The camp known by `hearth` is to take down what stands on `tiles`,
    /// which a plan gave up.
    fn give_up(&mut self, hearth: (i32, i32), tiles: impl Iterator<Item = (i32, i32)>) {
        let Some(camp) = self.change(hearth) else { return };
        camp.clearing.extend(tiles);
        camp.clearing.sort();
        camp.clearing.dedup();
    }

    fn change(&mut self, hearth: (i32, i32)) -> Option<&mut Camp> {
        let camp = self.camps.get_mut(&hearth)?;
        self.changed.push(hearth);
        Some(camp)
    }
}

/// Where a settler is headed: its hearth, the tile it does a step on,
/// the tile the leg its wagon is on ends at, or nowhere in particular
/// about its hub.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Hearth,
    Site(Step, (i32, i32)),
    Road((i32, i32)),
    Stroll,
}

/// What a settler is doing.
#[derive(Clone, Debug, PartialEq)]
enum Errand {
    /// Standing, to look for work at `think`.
    Idle { think: Moment },
    /// Walking `way`, the corners still ahead of it (`path::corners`),
    /// heading for the first; `since` is when it last came to one.
    Going { goal: Goal, way: Vec<Qrz>, since: Moment },
    /// At work on what its camp's planner of `zone` has begun on `site`.
    Raising { site: (i32, i32), zone: Zone },
    /// Asleep on the ground through the night; `told` is when those who
    /// see it were last told so.
    Asleep { told: Moment },
}

/// A settler: the camp it builds, by its hearth, what it is doing, and
/// the sites it found no way to since it last raised anything.
#[derive(Component, Clone, Debug)]
pub struct Settler {
    pub hearth: (i32, i32),
    /// Which of its hub's settlers it is: 0 the first, which came for the
    /// post.
    place: u8,
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

/// What a hub has of `fixture`: none for stakes, which are its wall.
pub fn what_of(fixture: Fixture) -> Option<What> {
    match fixture {
        Fixture::Tent | Fixture::Pavilion | Fixture::House | Fixture::Tenement => Some(What::Home),
        Fixture::Post => Some(What::Post),
        Fixture::Wagon => Some(What::Wagon),
        Fixture::Storehouse => Some(What::Store),
        Fixture::Stakes => None,
    }
}

/// Whether a `what` stands raised on `tile`.
fn stands(site: &Site, what: What, tile: (i32, i32)) -> bool {
    let Some(cover) = site.ground.cover(tile.0, tile.1) else { return false };
    match what {
        What::Hearth => building::standing(cover).is_some_and(|construct| construct.kind == Kind::Hearth),
        What::Post => building::fixture(cover) == Some(Fixture::Post) || building::emptied_post(cover),
        What::Wagon => building::fixture(cover) == Some(Fixture::Wagon),
        What::Home => building::fixture(cover).is_some_and(|fixture| what_of(fixture) == Some(What::Home)),
        What::Store => building::fixture(cover) == Some(Fixture::Storehouse),
        What::Market | What::Crafting | What::Farming => false,
    }
}

/// The first `what` of the hub of the camp known by `key` that stands
/// raised.
fn raised(site: &Site, key: (i32, i32), what: What) -> Option<(i32, i32)> {
    let id = site.hubs.of_hearth(key)?;
    site.hubs.standing(id, what).find(|&tile| stands(site, what, tile))
}

/// The tile the hub of the camp known by `key` has its first `what` on.
fn planned(hubs: &Hubs, key: (i32, i32), what: What) -> Option<(i32, i32)> {
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

/// What the planner of `zone` of the camp known by `key` does next, from
/// the tile `from`, and the tile it is done on, of those no one of
/// `unreached` names. The logistics planner takes down what a plan gave
/// up, the nearest first; then every planner raises each thing of its
/// zone that does not stand yet, in the order they were asked for; then
/// the logistics planner drives stakes on the tile of the wall nearest,
/// and, of the stakes that stand, takes down those facing another way
/// than out, to be raised again. A tile that does not take its step as
/// the ground lies, a tree where a tent is to stand, is passed over. None
/// while the zone has no planner, and once the ground is as the hub's
/// plans say. A thing on more tiles than one is raised on the tile its
/// fixture stands on to lie on them all, once every one of them is free.
fn next_step(site: &Site, camp: &Camp, key: (i32, i32), zone: Zone, from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(Step, (i32, i32))> {
    let id = site.hubs.of_hearth(key)?;
    if !site.hubs.hired(id).any(|hired| hired == zone) {
        return None;
    }
    let logistics = zone == Zone::Logistics;
    let free = |tile: &(i32, i32)| !unreached.contains(tile);
    if let Some(tile) = camp.clearing.iter().copied().filter(free).filter(|_| logistics).min_by_key(|&tile| (distance(tile, from), tile)) {
        return Some((Step::Clear, tile));
    }
    let wanted = site
        .hubs
        .items(id)
        .iter()
        .filter(|item| item.kind.what.zone() == zone)
        .filter_map(|item| {
            let step = Step::of(item.kind)?;
            Some((step, step.lies(item)?.0, item))
        })
        .find(|&(step, first, item)| !site.ground.cover(first.0, first.1).is_some_and(|cover| step.stood(cover)) && free(&first) && item.tiles.iter().all(|&tile| takes(site, step, tile)))
        .map(|(step, first, _)| (step, first));
    let nearest = |tiles: &mut dyn Iterator<Item = (i32, i32)>| tiles.min_by_key(|&tile| (distance(tile, from), tile));
    let turned = |tile: &(i32, i32)| site.ground.cover(tile.0, tile.1).is_some_and(|cover| building::fixture(cover) == Some(Fixture::Stakes) && cover.facing() != site.hubs.faces(id, *tile));
    let stakes = || site.hubs.stakes(id).into_iter().filter(|_| logistics).filter(free);
    wanted
        .or_else(|| nearest(&mut stakes().filter(|&tile| open(site, Fixture::Stakes, tile))).map(|tile| (Step::Stakes, tile)))
        .or_else(|| nearest(&mut stakes().filter(turned)).map(|tile| (Step::Clear, tile)))
}

/// How far from its hearth a settler with nothing to do strolls, in tiles,
/// and how long it stands where it came to before it strolls on.
const STROLL: i32 = 6;
const LINGER: Duration = Duration::from_secs(6);

/// The zone the settler in `place` of the hub known by `key` is the
/// planner of: its planners take the places in the order of their zones.
fn zone_of(hubs: &Hubs, key: (i32, i32), place: u8) -> Option<Zone> {
    hubs.hired(hubs.of_hearth(key)?).nth(place as usize)
}

/// The zones whose steps the settler in `place` does, `bodies` of its
/// hub's settlers in the world: its own, and for the first settler those
/// of every planner that has no body.
fn zones_of(hubs: &Hubs, key: (i32, i32), place: u8, bodies: u8) -> Vec<Zone> {
    let Some(id) = hubs.of_hearth(key) else { return Vec::new() };
    hubs.hired(id).enumerate().filter(|&(at, _)| at as u8 == place || place == 0 && at as u8 >= bodies).map(|(_, zone)| zone).collect()
}

/// The zone whose planner `order` is asked of, where it is one zone's:
/// the hub's ground and its wall are the logistics planner's, a thing its
/// own zone's, and a zone's centre that zone's.
fn asked_of(hubs: &Hubs, id: common_bevy::hubs::HubId, order: Order) -> Option<Zone> {
    match order {
        Order::Shrink | Order::Residents(_) | Order::Wall => Some(Zone::Logistics),
        Order::Raise(fixture) => what_of(fixture).map(What::zone),
        Order::Zone(zone, ..) => Zone::kept_as(zone),
        Order::Upgrade(q, r) | Order::Downgrade(q, r) => hubs.item_on(id, (q, r)).map(|item| item.kind.what.zone()),
        Order::Hire(_) => None,
    }
}

/// Where the settler in `place` of a hub stands by `centre`, its zone's
/// or its hearth's: the tile beside it on the side its place names, or
/// the next round from there that can be stood on. Each place has a side
/// of its own, so planners whose zones share a centre stand apart, and
/// none on the centre.
fn side(map: &common_bevy::resources::map::Map, centre: (i32, i32), place: u8) -> Option<Qrz> {
    (0..6).map(|turn| qrz::DIRECTIONS[(place as usize + turn) % 6]).find_map(|dir| {
        let (tile, _) = map.get_by_qr(centre.0 + dir.q, centre.1 + dir.r)?;
        (!common_bevy::systems::movement::is_full_cover(map, tile.q, tile.r)).then_some(tile)
    })
}

/// A number from `seed`, the same on every run and spread evenly.
fn drawn(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A tile of the hub round `fire` for a settler to stroll to, drawn by
/// `seed`: within [`STROLL`] of the hearth, on the hub's own ground.
fn stroll(hubs: &Hubs, fire: (i32, i32), seed: u64) -> Option<(i32, i32)> {
    let span = (2 * STROLL + 1) as u64;
    let roll = drawn(seed);
    let tile = (fire.0 + (roll % span) as i32 - STROLL, fire.1 + ((roll / span) % span) as i32 - STROLL);
    (distance(tile, fire) <= STROLL && hubs.holding(tile).is_some() && hubs.holding(tile) == hubs.holding(fire)).then_some(tile)
}

/// The bundle a settler of the camp round `hearth` is in the world as,
/// its `place` among them, standing on `stands`.
fn body(hearth: (i32, i32), place: u8, stands: Qrz, rides: bool, now: Moment) -> impl Bundle {
    let typ = EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Settler));
    (
        typ,
        Loc::new(stands),
        Side::PLAYERS,
        // Who sees it: every actor a client is shown holds this
        LoadedBy::default(),
        outfit(),
        Settler { hearth, place, errand: Errand::Idle { think: now }, unreached: HashSet::new(), hitched: Moment::ZERO, rides },
        Status { mounted: rides, ..default() },
        (Heading::default(), Turn::default(), Position::at_tile(stands), AirTime::default(), MovementIntentState::default()),
    )
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
/// takes it, the camp no longer has what it takes, or the planner whose
/// step it is has a thing begun already.
fn begin(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), step: Step, at: (i32, i32)) -> bool {
    let Some(cover) = site.ground.cover(at.0, at.1) else { return false };
    let Some(camp) = camps.on(hearth).filter(|camp| camp.rising_of(step.zone()).is_none()) else { return false };
    // How the thing is turned, and the tiles it lies on besides the one that holds it
    let item = site.hubs.of_hearth(hearth).and_then(|id| site.hubs.item_on(id, at)).filter(|item| Step::of(item.kind) == Some(step));
    let turn = item.and_then(|item| step.lies(item)).filter(|&(first, _)| first == at).map_or(0, |(_, turn)| turn);
    let beside: Vec<(i32, i32)> = item.map_or(Vec::new(), |item| item.tiles.iter().copied().filter(|&tile| tile != at).collect());
    if !takes(site, step, at) || beside.iter().any(|&tile| !takes(site, step, tile)) || !camp.affords(step.needs()) {
        return false;
    }
    let camp = camps.change(hearth).expect("its camp stands");
    let spent = camp.pay(step.needs()).expect("it has what the step takes");
    for stack in &spent {
        site.moved.write(Moved::stack(*stack, Holder::Camp(hearth.0, hearth.1), Holder::World, Cause::Build));
    }
    camp.begins(step, at, settling.stage(step));
    let stone = spent.iter().find_map(|stack| match stack.kind {
        Stackable::Material(material) if material.is_stone() => Some(material),
        _ => None,
    });
    let begun = match step.task() {
        // Stakes face out of the hub's ground
        Task::Raise(Fixture::Stakes) => Fixture::Stakes.begun(cover).map(|begun| begun.faced(site.hubs.of_hearth(hearth).and_then(|id| site.hubs.faces(id, at)))),
        Task::Raise(fixture) => fixture.begun(cover).map(|begun| building::turning(begun, turn)),
        Task::Hearth => Construct { of: stone.unwrap_or(LAID.of), ..LAID }.raised(cover),
        Task::Clear => None,
    };
    if let Some(begun) = begun {
        site.constructs.raise(at, Built { by: Owner::Camp(hearth.0, hearth.1) });
        site.ground.set(&mut site.writer, at.0, at.1, begun);
        // The other tiles it lies on are its own from now, and the hub's
        for tile in beside {
            if let Some(held) = site.ground.cover(tile.0, tile.1).and_then(|cover| building::annexed_to(cover, tile, at)) {
                site.constructs.raise(tile, Built { by: Owner::Camp(hearth.0, hearth.1) });
                site.ground.set(&mut site.writer, tile.0, tile.1, held);
            }
        }
    }
    info!("settlers: a planner of {hearth:?} began {step:?} on {at:?}");
    true
}

/// What a spell of work on what a camp has begun came to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Worked {
    /// Nothing: someone stands on the tile what is raised would fill.
    Waiting,
    /// It stands a stage further.
    Further,
    /// It is raised, or was no longer there to raise.
    Raised,
}

/// Carries what the planner of `zone` of the camp round `hearth` has
/// begun a stage further, with a stage's work to do again, and raises it
/// from its last.
fn advance(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), zone: Zone) -> Worked {
    let Some((step, at, _)) = camps.on(hearth).and_then(|camp| camp.rising_of(zone)) else { return Worked::Raised };
    if step == Step::Clear {
        let down = clearable(site, at);
        let camp = camps.change(hearth).expect("its camp stands");
        camp.finished(zone);
        camp.clearing.retain(|tile| *tile != at);
        if let Some((cover, back)) = down {
            camp.credit = camp.credit.plus(back);
            site.constructs.fell(at);
            site.ground.set(&mut site.writer, at.0, at.1, cover);
            info!("settlers: a planner of {hearth:?} took down what stood on {at:?}");
        }
        return Worked::Raised;
    }
    let next = site.ground.lies(at.0, at.1).and_then(|(z, cover)| Some((z, building::advanced(cover)?)));
    let Some((z, next)) = next else {
        camps.change(hearth).expect("its camp stands").finished(zone);
        return Worked::Raised;
    };
    if building::rising(next).is_some() {
        camps.change(hearth).expect("its camp stands").begins(step, at, settling.stage(step));
        site.ground.set(&mut site.writer, at.0, at.1, next);
        return Worked::Further;
    }
    // What fills its tile waits for whoever stands there to step off
    if matches!(step.task(), Task::Raise(fixture) if fixture.fills()) && occupied(&site.nntree, at, z) {
        return Worked::Waiting;
    }
    camps.change(hearth).expect("its camp stands").finished(zone);
    site.ground.set(&mut site.writer, at.0, at.1, next);
    info!("settlers: a planner of {hearth:?} raised {step:?} on {at:?}");
    Worked::Raised
}

/// What the planner of `zone` of the camp known by `hearth` does next,
/// from the tile `from`: the thing it has begun, to be finished; or else
/// its next step ([`next_step`]), where the camp has what that takes. A
/// tile it was to clear that holds nothing of the hub's is done with.
fn work_for(site: &Site, camps: &mut Camps, hearth: (i32, i32), zone: Zone, from: (i32, i32), unreached: &HashSet<(i32, i32)>) -> Option<(Step, (i32, i32))> {
    let camp = camps.on(hearth)?;
    if let Some((step, at, _)) = camp.rising_of(zone) {
        return Some((step, at));
    }
    let bare = |tile: &(i32, i32)| site.ground.cover(tile.0, tile.1).is_some() && clearable(site, *tile).is_none();
    if camp.clearing.iter().any(bare) {
        camps.change(hearth)?.clearing.retain(|tile| !bare(tile));
    }
    let camp = camps.on(hearth)?;
    next_step(site, camp, hearth, zone, from, unreached).filter(|(step, _)| camp.affords(step.needs()))
}

/// Gives the camp round `hearth` what its planners would have raised in
/// `spare`, daytime not yet counted, each planner the whole of it: each
/// thing next in its zone's line that the camp holds the materials for
/// and it has a way to, a stage at a time, while the walk there and the
/// work fit in what is left; what is left over comes off the stage it is
/// on. Returns where the first settler ends, beside the last thing it
/// worked on or at its hearth.
fn catch_up(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), spare: Duration) -> (i32, i32) {
    let fire = hearth_of(&site.hubs, hearth);
    let zones: Vec<Zone> = site.hubs.of_hearth(hearth).map_or(Vec::new(), |id| site.hubs.hired(id).collect());
    let mut ends = fire;
    for (place, zone) in zones.into_iter().enumerate() {
        let at = caught_up(site, camps, settling, hearth, zone, fire, spare);
        if place == 0 {
            ends = at;
        }
    }
    ends
}

/// What the planner of `zone` of the camp round `hearth` would have done
/// in `spare`, starting from `at`: where it ends.
fn caught_up(site: &mut Site, camps: &mut Camps, settling: &Settling, hearth: (i32, i32), zone: Zone, mut at: (i32, i32), mut spare: Duration) -> (i32, i32) {
    let mut unreached = HashSet::new();
    loop {
        let Some((step, to)) = work_for(site, camps, hearth, zone, at, &unreached) else { return at };
        let begun = camps.on(hearth).is_some_and(|camp| camp.rising_of(zone).is_some());
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
            let left = camps.on(hearth).and_then(|camp| camp.rising_of(zone)).map_or(Duration::ZERO, |(.., left)| left);
            if left > spare {
                if let Some((.., left)) = camps.change(hearth).and_then(|camp| camp.rising.iter_mut().find(|(step, ..)| step.zone() == zone)) {
                    *left -= spare;
                }
                return at;
            }
            spare -= left;
            match advance(site, camps, settling, hearth, zone) {
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
    let place = planned(&site.hubs, hearth, What::Wagon).and_then(|tile| Some((tile, building::parked(site.ground.cover(tile.0, tile.1)?)?)));
    match place {
        Some((tile, cover)) => {
            site.constructs.raise(tile, Built { by: Owner::Camp(hearth.0, hearth.1) });
            site.ground.set(&mut site.writer, tile.0, tile.1, cover);
        }
        None => warn!("settlers: the wagon of {hearth:?} came home to no open ground, and is lost"),
    }
    // Its horse is tied at its post again
    let post = planned(&site.hubs, hearth, What::Post);
    if let Some((tile, cover)) = post.and_then(|tile| Some((tile, building::tied(site.ground.cover(tile.0, tile.1)?)?))) {
        site.ground.set(&mut site.writer, tile.0, tile.1, cover);
    }
}

/// Carries the wagon of the camp round `hearth` on through the daytime
/// since its trip was last counted. Stopped by what stands in its way it
/// waits, but on the ground of the hub it is bound for, where it finds
/// its way anew; while it waits it is `stopped`, which those it matters
/// to are told (`tell_stopped`). Its last leg out ends at the hub
/// standing now where its map recorded one (`haul::hub_at`): come to it,
/// the wagon leaves what it carried in that hub's storehouse, each
/// loader's share its own there, and turns for home, and with no hub there it turns with its load. Come home, it
/// stands in its camp again once the ground there is held, what it
/// brought back is the camp's to load again, and its trip is over.
fn haul(site: &mut Site, camps: &mut Camps, charts: &Charts, depots: &mut Depots, hearth: (i32, i32), when: When) {
    let Some(trip) = camps.camps.get_mut(&hearth).and_then(|camp| camp.trip.as_mut()) else { return };
    let mut gap = when.daytime_since(trip.counted);
    (trip.counted, trip.stopped) = (when, false);
    let step = Duration::from_secs_f32(1.0 / walked());
    let last = if trip.back { usize::MAX } else { trip.way.len().saturating_sub(1) };
    let hub = hub_at(&site.hubs, trip.to);
    if trip.carry_to(charts.lies(), &mut gap, step, last) == Road::Stalled {
        // Stopped on the ground of the hub it is bound for, out or home,
        // it finds the rest of its way as that ground lies now: what a
        // hub raises on its own ground is no blockade. Anywhere else it
        // waits for the way its map held to open
        let bound = if trip.back { site.hubs.of_hearth(hearth).and_then(|id| Some((id, site.hubs.hearth(id)?))) } else { hub };
        let within = |tile: (i32, i32)| bound.is_some_and(|(id, _)| site.hubs.holding(tile) == Some(id));
        let there = trip.leg().is_some_and(|leg| within(leg.from) || within(leg.to));
        let stopped = trip.leg();
        // No other way there yet, or none but the one stopped: it waits on
        if !there || !trip.aim(charts.lies(), bound.map(|(_, hearth)| hearth)) || trip.leg() == stopped {
            trip.stopped = true;
            return;
        }
        info!("settlers: the wagon of {hearth:?} found its way round what its hub raised since its map was drawn");
        camps.changed.push(hearth);
        return;
    }
    if trip.nearly() {
        let turned = hub.is_none();
        if !trip.aim(charts.lies(), hub.map(|(_, hearth)| hearth)) {
            trip.stopped = true;
            return;
        }
        if turned {
            info!("settlers: the wagon of {hearth:?} found no hub where its map held one at {:?}, and turned for home with its load", trip.to);
            camps.changed.push(hearth);
            return;
        }
    }
    if trip.carry_to(charts.lies(), &mut gap, step, usize::MAX) != Road::Arrived {
        return;
    }
    if trip.back {
        if site.ground.lies(hearth.0, hearth.1).is_some() {
            info!("settlers: the wagon of {hearth:?} is home");
            let camp = camps.change(hearth).expect("its camp stands");
            for owned in camp.trip.take().map_or(Vec::new(), |trip| trip.cargo) {
                put(&mut camp.load, owned);
            }
            park(site, camps, hearth);
        }
        return;
    }
    let Some((common_bevy::hubs::HubId(q, r), _)) = hub else {
        trip.turned();
        camps.changed.push(hearth);
        return;
    };
    let cargo = std::mem::take(&mut trip.cargo);
    trip.turned();
    depots.leave((q, r), &cargo);
    for owned in cargo {
        site.moved.write(Moved::stack(owned.stack, Holder::Camp(hearth.0, hearth.1), Holder::Depot(q, r), Cause::Haul));
    }
    camps.changed.push(hearth);
    info!("settlers: the wagon of {hearth:?} left its load at the hub of {:?} and turned for home", (q, r));
}

/// Where one new to the hub of the camp round `hearth` comes into sight,
/// to walk to `floor`, its hearth's tile: out through the hub's way in
/// and straight on, as far as a player sees an actor from
/// (`aoi::AOI_RADIUS`), or the furthest of a few nearer tiles of that
/// line the ground gives a way in from; the way in itself where it gives
/// none of them.
fn approach(hubs: &Hubs, map: &common_bevy::resources::map::Map, hearth: (i32, i32), floor: Qrz) -> Option<Qrz> {
    let way_in = hubs.way_in(hubs.of_hearth(hearth)?)?;
    let (gate, _) = map.get_by_qr(way_in.0, way_in.1)?;
    let line = gate.beyond(floor, crate::systems::aoi::AOI_RADIUS as usize);
    let open = |tile: &Qrz| {
        let (at, _) = map.get_by_qr(tile.q, tile.r)?;
        (!common_bevy::systems::movement::is_full_cover(map, at.q, at.r) && path::find(map, at, floor, 1).is_some()).then_some(at)
    };
    (1..=4).rev().filter_map(|k| line.get((line.len() * k / 4).checked_sub(1)?)).find_map(open).or_else(|| open(&gate))
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
    camps.others.retain(|_, settler| settlers.contains(*settler));
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
        let housed = asleep && raised(&site, hearth, What::Home).is_some() && !hauling;
        // No settler comes to a hearth with no hitching post
        let posted = hauling || planned(&site.hubs, hearth, What::Post).is_some();
        match (camps.settlers.get(&hearth), nearest(post)) {
            (Some(&settler), far) if housed || far.is_none_or(|far| far > UNWATCHED) => {
                commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
                camps.settlers.remove(&hearth);
                camps.changed.push(hearth);
            }
            (None, Some(near)) if !housed && posted && near <= WATCHED && when >= camp.arrives => {
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
                let Some(at) = (if camp.arrived { None } else { approach(hubs, map, hearth, floor) }).or(beside) else { continue };
                let stands = at + Qrz::Z;
                // It rides in, and sits its horse until it has come to its hearth
                let rides = !camp.arrived && !hauling;
                let settler = commands.spawn(body(hearth, 0, stands, rides, now)).id();
                commands.entity(settler).insert(NearestNeighbor::new(settler, Loc::new(stands)));
                camps.settlers.insert(hearth, settler);
                info!("settlers: the settler of the hearth on {hearth:?} is at ({}, {})", at.q, at.r);
            }
            _ => {}
        }
        // Its other settlers: each lives in a home of the hub, is out of
        // it by day while a player is near, and in it by night
        let homes: Vec<(i32, i32)> = site.hubs.of_hearth(hearth).map_or(Vec::new(), |id| site.hubs.items(id).iter().flat_map(|item| std::iter::repeat_n(item.tiles[0], item.settlers as usize)).collect());
        let near = nearest(fire);
        let out = !asleep && near.is_some_and(|near| near <= UNWATCHED);
        let gone: Vec<((i32, i32), u8)> = camps.others.keys().copied().filter(|&(of, place)| of == hearth && (!out || place as usize >= homes.len())).collect();
        for key in gone {
            if let Some(settler) = camps.others.remove(&key) {
                commands.write_message(Do { event: Event::Despawn { ent: settler, died: false } });
            }
        }
        // A home emptied takes its settler's place with it: one who comes
        // to it later walks in as any newcomer does
        let places = homes.len().saturating_sub(1).min(u8::MAX as usize) as u8;
        if camps.camps[&hearth].joined > places {
            camps.change(hearth).expect("its camp stands").joined = places;
        }
        if !asleep && near.is_some_and(|near| near <= WATCHED) {
            let map = site.ground.map();
            let joined = camps.camps[&hearth].joined;
            let far = (joined < places).then(|| map.get_by_qr(fire.0, fire.1).and_then(|(floor, _)| approach(&site.hubs, map, hearth, floor))).flatten();
            for place in 1..=places {
                if camps.others.contains_key(&(hearth, place)) {
                    continue;
                }
                // One that has walked in comes out beside its home; one
                // new to the hub comes from far off
                let home = homes[place as usize];
                let beside = || map.get_by_qr(home.0, home.1).and_then(|(floor, _)| map.neighbors(floor).into_iter().map(|(tile, _)| tile).find(|tile| !common_bevy::systems::movement::is_full_cover(map, tile.q, tile.r)));
                let Some(at) = far.filter(|_| place > joined).or_else(beside) else { continue };
                let stands = at + Qrz::Z;
                let settler = commands.spawn(body(hearth, place, stands, false, now)).id();
                commands.entity(settler).insert(NearestNeighbor::new(settler, Loc::new(stands)));
                camps.others.insert((hearth, place), settler);
            }
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
    // The daytime since each camp's work was last counted, counted once
    // however many of its settlers are in the world
    let mut gaps: HashMap<(i32, i32), Duration> = HashMap::new();
    for (ent, mut settler, loc, mut body) in &mut settlers {
        let hearth = settler.hearth;
        let lead = settler.place == 0;
        let bodies = 1 + camps.others.keys().filter(|(of, _)| *of == hearth).count() as u8;
        let mine = zones_of(&site.hubs, hearth, settler.place, bodies);
        let Some(camp) = camps.camps.get_mut(&hearth) else {
            commands.write_message(Do { event: Event::Despawn { ent, died: false } });
            continue;
        };
        // Come onto its hub's ground, one new to the hub has walked in
        if !lead && settler.place > camp.joined && site.hubs.holding((loc.q, loc.r)).is_some_and(|id| Some(id) == site.hubs.of_hearth(hearth)) {
            camp.joined = settler.place;
            camps.changed.push(hearth);
        }
        // It gets down once it has come to its hearth
        if settler.rides && camp.arrived {
            settler.rides = false;
            commands.entity(ent).insert(Status::default());
            commands.write_message(Do { event: Event::Incremental { ent, component: common_bevy::message::Component::Status(Status::default()) } });
        }
        // On the road the first settler walks the leg its wagon's trip is
        // on, which the calendar carries on whether or not it keeps up
        if let Some(leg) = camp.trip.as_ref().filter(|_| lead).map(Trip::leg) {
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
                        Some(next) if floor.flat_distance(&next) <= path::LOOK as i32 && now.since(since) <= STUCK => {
                            body.step_toward(floor, next, MOVEMENT_SPEED, PACE, dt, site.ground.map(), &site.nntree);
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
                    let way = [leg.to, leg.by].into_iter().find_map(|tile| path::walk(map, floor, map.get_by_qr(tile.0, tile.1)?.0, 0).filter(|way| !way.is_empty()));
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
        let gap = *gaps.entry(hearth).or_insert_with(|| {
            let gap = when.daytime_since(camp.worked);
            camp.worked = when;
            gap
        });
        if gap >= AWAY {
            if matches!(settler.errand, Errand::Raising { .. }) {
                show(ent, None, &mut commands);
            }
            if lead {
                catch_up(&mut site, camps, &settling, hearth, gap);
            }
            settler.errand = Errand::Idle { think: now };
            continue;
        }
        let Some((floor, _)) = site.ground.map().get_by_qr(loc.q, loc.r) else { continue };
        let here = (loc.q, loc.r);
        let fire = hearth_of(&site.hubs, hearth);
        let home = |site: &Site| site.ground.map().get_by_qr(fire.0, fire.1).and_then(|(at, _)| path::walk(site.ground.map(), floor, at, 1));
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
                // What its own zones want done next
                let work = mine.iter().find_map(|&zone| work_for(&site, camps, hearth, zone, here, &settler.unreached)).and_then(|(step, at)| {
                    let (z, _) = site.ground.lies(at.0, at.1)?;
                    Some((step, at, path::walk(site.ground.map(), floor, Qrz { q: at.0, r: at.1, z }, 1)))
                });
                let arrived = camps.on(hearth).is_some_and(|camp| camp.arrived);
                settler.errand = match work {
                    Some((step, at, Some(way))) => Errand::Going { goal: Goal::Site(step, at), way, since: now },
                    Some((_, at, None)) => {
                        settler.unreached.insert(at);
                        Errand::Idle { think: now + THINK }
                    }
                    // With nothing to do a planner stands by its zone's
                    // centre, and one that is no planner walks the hub
                    None if !lead => {
                        let centre = site.hubs.of_hearth(hearth).and_then(|id| site.hubs.hub(id)).zip(zone_of(&site.hubs, hearth, settler.place)).and_then(|(hub, zone)| hub.zones.get(&zone).copied());
                        let seed = ent.to_bits() ^ (now.since(Moment::ZERO).as_millis() as u64);
                        let to = match centre {
                            Some(centre) => side(site.ground.map(), centre, settler.place).filter(|to| (to.q, to.r) != here),
                            None => stroll(&site.hubs, fire, seed).and_then(|to| site.ground.map().get_by_qr(to.0, to.1)).map(|(to, _)| to),
                        };
                        let way = to.and_then(|to| path::walk(site.ground.map(), floor, to, 0)).filter(|way| !way.is_empty());
                        match way {
                            Some(way) => Errand::Going { goal: Goal::Stroll, way, since: now },
                            None => Errand::Idle { think: now + THINK },
                        }
                    }
                    // With nothing to raise the first keeps to its side of its hearth
                    None if distance(here, fire) > 1 || side(site.ground.map(), fire, 0).is_some_and(|to| (to.q, to.r) != here) => {
                        let way = side(site.ground.map(), fire, 0).and_then(|to| path::walk(site.ground.map(), floor, to, 0)).filter(|way| !way.is_empty()).or_else(|| home(&site).filter(|way| !way.is_empty()));
                        match way {
                            Some(way) => Errand::Going { goal: Goal::Hearth, way, since: now },
                            None if distance(here, fire) > 1 => Errand::Idle { think: now + STUCK },
                            None => {
                                if !arrived {
                                    camps.change(hearth).expect("its camp stands").arrived = true;
                                }
                                Errand::Idle { think: now + THINK }
                            }
                        }
                    }
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
                            Goal::Stroll => Errand::Idle { think: now + LINGER },
                            // Beside its site: it begins what goes there,
                            // where the camp has not, and sets to work
                            Goal::Site(step, at) => {
                                let begun = camps.on(hearth).and_then(|camp| camp.rising_of(step.zone())).is_some_and(|(of, on, _)| (of, on) == (step, at));
                                if begun || begin(&mut site, camps, &settling, hearth, step, at) {
                                    show(ent, WORKING, &mut commands);
                                    Errand::Raising { site: at, zone: step.zone() }
                                } else {
                                    settler.unreached.insert(at);
                                    Errand::Idle { think: now }
                                }
                            }
                        };
                    }
                    // Off its way, or held on one tile of it: it looks again
                    Some(next) if floor.flat_distance(&next) > path::LOOK as i32 || now.since(since) > STUCK => {
                        if let Goal::Site(_, at) = goal {
                            settler.unreached.insert(at);
                        }
                        settler.errand = Errand::Idle { think: now };
                    }
                    Some(next) => {
                        body.step_toward(floor, next, MOVEMENT_SPEED, PACE, dt, site.ground.map(), &site.nntree);
                        settler.errand = Errand::Going { goal, way, since };
                    }
                }
            }
            Errand::Raising { site: at, zone } => {
                let Some((z, _)) = site.ground.lies(at.0, at.1) else { continue };
                body.face(loc, Qrz { q: at.0, r: at.1, z: z + 1 }, 1.0, dt, site.ground.map(), &site.nntree);
                // The daytime just gone comes off the work its stage wants
                let left = camps.camps.get_mut(&hearth).and_then(|camp| camp.rising.iter_mut().find(|(step, ..)| step.zone() == zone)).map_or(Duration::ZERO, |(.., left)| {
                    *left = left.saturating_sub(gap);
                    *left
                });
                if !left.is_zero() {
                    continue;
                }
                match advance(&mut site, camps, &settling, hearth, zone) {
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
    let begun: Vec<(i32, i32)> = camp.rising.iter().map(|&(_, at, _)| at).collect();
    let sites = site
        .hubs
        .items(id)
        .iter()
        .filter_map(|item| {
            let step = Step::of(item.kind)?;
            Some((step, step.lies(item)?.0))
        })
        .filter(|&(step, first)| !begun.contains(&first) && !site.ground.cover(first.0, first.1).is_some_and(|cover| step.stood(cover)))
        .map(|(step, _)| step.needs());
    let stakes = site.hubs.stakes(id).into_iter().filter(|tile| !begun.contains(tile) && open(site, Fixture::Stakes, *tile)).map(|_| Step::Stakes.needs());
    sites.chain(stakes).fold(Needs::default(), Needs::plus)
}

/// What the settler of `camp`, known by `key`, does next, and what the
/// camp still lacks for all its hub has yet to see raised, counting what
/// it will have of the things it has yet to take down: what a player who
/// hails it is told.
fn wants(site: &Site, camp: &Camp, key: (i32, i32), zone: Option<Zone>) -> Wants {
    let coming = camp.clearing.iter().filter_map(|&tile| clearable(site, tile)).fold(camp.credit, |coming, (_, back)| coming.plus(back));
    let needs = remaining(site, camp, key).minus(coming).less(&camp.stock);
    let step = zone.and_then(|zone| camp.rising_of(zone).map(|(step, ..)| step).or_else(|| next_step(site, camp, key, zone, hearth_of(&site.hubs, key), &HashSet::new()).map(|(step, _)| step)));
    let id = site.hubs.of_hearth(key);
    let hired = id.map_or(0, |id| site.hubs.hired(id).fold(0u8, |bits, zone| bits | 1 << zone.id()));
    let hires = id.and_then(|id| Zone::ALL.into_iter().find(|&zone| site.hubs.hires(id, zone))).map_or(0, |zone| zone.id() as u8);
    Wants { task: step.map(Step::task), needs, planned: id.map_or(0, |id| site.hubs.wants(id)), hired, hires, walled: id.is_some_and(|id| site.hubs.walled(id)), zone: zone.map_or(0, |zone| zone.id() as u8), busy: zone.is_some_and(|zone| camp.rising_of(zone).is_some()) }
}

/// Whether the wagon of `camp`, known by `key`, stands ready to be
/// loaded and sent: raised, home, and nothing more for its settler to do.
fn carts(site: &Site, camp: &Camp, key: (i32, i32)) -> bool {
    camp.trip.is_none() && raised(site, key, What::Wagon).is_some() && wants(site, camp, key, Some(Zone::Logistics)).task.is_none()
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

/// The settler `loc` hails, the nearest within reach: its hearth, and
/// its place among its hub's settlers.
fn hailed(settlers: &Query<(&Settler, &Loc)>, loc: &Loc) -> Option<((i32, i32), u8)> {
    settlers.iter().map(|(settler, at)| (loc.flat_distance(at), settler.hearth, settler.place)).filter(|&(far, ..)| far <= HAIL).min().map(|(_, hearth, place)| (hearth, place))
}

/// Where the wagon of the camp round `hearth` stands stopped on the
/// road, where the settler in `place` is its driver.
fn waits(camps: &Camps, hearth: (i32, i32), place: u8) -> Option<(i32, i32)> {
    camps.on(hearth)?.trip.as_ref().filter(|trip| place == 0 && trip.stopped)?.at()
}

/// Tells each player where the stopped wagons that matter to it stand,
/// as it enters the world and whenever they change: those of the hub it
/// lives in, and those carrying what it loaded.
pub fn tell_stopped(
    camps: Res<Camps>,
    hubs: Res<Hubs>,
    players: Query<(Entity, &Character, Option<&common_bevy::components::Lives>)>,
    mut told: Local<HashMap<Entity, Vec<(i32, i32)>>>,
    mut writer: MessageWriter<Do>,
) {
    let stopped: Vec<((i32, i32), &Trip, (i32, i32))> = camps.camps.iter().filter_map(|(&hearth, camp)| camp.trip.as_ref().filter(|trip| trip.stopped).and_then(|trip| Some((hearth, trip, trip.at()?)))).collect();
    told.retain(|ent, _| players.contains(*ent));
    for (ent, &Character(who), lives) in &players {
        let own = lives.and_then(|lives| hubs.holding((lives.0.q, lives.0.r)));
        let matters = |hearth: (i32, i32), trip: &Trip| own.is_some() && hubs.of_hearth(hearth) == own || trip.cargo.iter().any(|owned| owned.who == Some(who));
        let mut wagons: Vec<(i32, i32)> = stopped.iter().filter(|(hearth, trip, _)| matters(*hearth, trip)).map(|&(.., at)| at).collect();
        wagons.sort();
        if told.get(&ent).map_or(wagons.is_empty(), |said| *said == wagons) {
            continue;
        }
        told.insert(ent, wagons.clone());
        writer.write(Do { event: Event::Stopped { ent, wagons } });
    }
}

/// Answers each player that hails a settler: the nearest within reach
/// says what it does next and what it still lacks for all it has yet to
/// raise. Asked to, the player hands it what its bag holds of that, no
/// more than is lacking.
/// One with nothing more to do speaks of its wagon besides: what is
/// loaded on it, and the hubs the map in the player's hand would send it
/// to. At night it sleeps and answers no one. On the road it answers
/// only where its wagon is stopped ([`waits`]), and only of the wagon:
/// what it carries, and where the map in hand would send it from there.
pub fn try_hail(
    mut reader: MessageReader<Try>,
    settings: Res<common_bevy::settings::Settings>,
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
        let near = hailed(&settlers, loc).filter(|_| !night);
        if let Some((hearth, stands)) = near.and_then(|(hearth, place)| Some((hearth, waits(&camps, hearth, place)?))) {
            let reached = held(worn).and_then(|map| charts.map(map)).map_or(Vec::new(), |chart| destinations(chart, &site.hubs, hearth, Some(stands)));
            let load = camps.camps[&hearth].trip.as_ref().map_or(Vec::new(), |trip| totals(&trip.cargo));
            site.writer.write(Do { event: Event::Wants { ent, wants: Some(common::building::Wants::default()) } });
            site.writer.write(Do { event: Event::Wagon { ent, wagon: Some(common::building::Wagon { load, hubs: reached.into_iter().map(|(hub, _)| hub).collect(), away: true }) } });
            continue;
        }
        let Some((hearth, place)) = near.filter(|&(hearth, _)| camps.on(hearth).is_some_and(|camp| camp.trip.is_none())) else {
            site.writer.write(Do { event: Event::Wants { ent, wants: None } });
            continue;
        };
        // The one hailed answers for its own zone; the wagon is the logistics planner's
        let zone = zone_of(&site.hubs, hearth, place);
        let carted = zone == Some(Zone::Logistics) && carts(&site, &camps.camps[&hearth], hearth);
        if give {
            let lacking = wants(&site, &camps.camps[&hearth], hearth, zone).needs;
            hand(&mut site, camps.change(hearth).expect("its camp stands"), hearth, lacking, ent, who, &mut bag);
        }
        stray(&site, &mut camps, hearth);
        if provided(&settings) {
            grant(&mut site, &mut camps, hearth);
        }
        let said = wants(&site, &camps.camps[&hearth], hearth, zone);
        site.writer.write(Do { event: Event::Wants { ent, wants: Some(said) } });
        let wagon = carted.then(|| {
            let reached = held(worn).and_then(|map| charts.map(map)).map_or(Vec::new(), |chart| destinations(chart, &site.hubs, hearth, None));
            common::building::Wagon { load: totals(&camps.camps[&hearth].load), hubs: reached.into_iter().map(|(hub, _)| hub).collect(), away: false }
        });
        site.writer.write(Do { event: Event::Wagon { ent, wagon } });
    }
}

/// The camp whose `what`, a wagon or a storehouse, a player at `loc`
/// looks at: the nearest raised within reach, a wagon only while it
/// stands at home. The player asks after the one it aims at, so each is
/// found among its own kind.
fn looked(site: &Site, camps: &Camps, loc: &Loc, what: What) -> Option<(i32, i32)> {
    let home = |camp: &Camp| what != What::Wagon || camp.trip.is_none();
    let beside = |key: (i32, i32)| Some((qrz::hex_distance((loc.q, loc.r), raised(site, key, what)?), key));
    camps.camps.iter().filter(|(_, camp)| home(camp)).filter_map(|(&key, _)| beside(key)).filter(|&(far, _)| far <= HAIL).min().map(|(_, key)| key)
}

/// Answers each player that looks at a storehouse: the nearest raised
/// within reach says what the player has in it, and on its hub's wagon
/// while that stands at home. Asked to, the player moves so many of one
/// material: out of its bag into the store, out of the store into its
/// bag as far as that has room and weight for, out of the store onto the
/// wagon, or off the wagon into the store. What it holds is each
/// player's own, in the store and on the wagon, by night as by day.
pub fn try_store(mut reader: MessageReader<Try>, mut site: Site, mut camps: ResMut<Camps>, mut depots: ResMut<Depots>, mut players: Query<(&Loc, &mut Inventory, &Equipment, &Character)>) {
    use common::building::Stowing;
    for message in reader.read() {
        let Try { event: Event::Store { ent, moved } } = message else { continue };
        let (ent, moved) = (*ent, *moved);
        let Ok((loc, mut bag, worn, &Character(who))) = players.get_mut(ent) else { continue };
        let near = looked(&site, &camps, loc, What::Store).and_then(|key| Some((key, site.hubs.of_hearth(key)?)));
        let Some((key, common_bevy::hubs::HubId(q, r))) = near else {
            site.writer.write(Do { event: Event::Stored { ent, held: None, cart: None } });
            continue;
        };
        let carts = camps.on(key).is_some_and(|camp| camp.trip.is_none()) && raised(&site, key, What::Wagon).is_some();
        let (depot, wagon, player) = (Holder::Depot(q, r), Holder::Camp(key.0, key.1), Holder::Character(who));
        match moved {
            Some((Stowing::Leave, asked)) => {
                // No more than the bag holds of it
                let count = asked.count.min(bag.count(asked.kind));
                if count > 0 {
                    let stack = bag.remove(asked.kind, count);
                    depots.leave((q, r), &[Owned { who: Some(who), stack }]);
                    site.moved.write(Moved::stack(stack, player, depot, Cause::Store));
                    site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
                }
            }
            Some((Stowing::Take, asked)) => {
                let fits = Stack { kind: asked.kind, count: asked.count.min(bag.room_for(worn, asked.kind)) };
                if let Some(stack) = depots.take((q, r), who, fits) {
                    bag.add(stack);
                    site.moved.write(Moved::stack(stack, depot, player, Cause::Withdraw));
                    site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
                }
            }
            Some((Stowing::Load, asked)) if carts => {
                if let Some(stack) = depots.take((q, r), who, asked) {
                    put(&mut camps.change(key).expect("its camp stands").load, Owned { who: Some(who), stack });
                    site.moved.write(Moved::stack(stack, depot, wagon, Cause::Give));
                }
            }
            Some((Stowing::Unload, asked)) if carts => {
                let camp = camps.change(key).expect("its camp stands");
                let own = camp.load.iter_mut().find(|owned| owned.who == Some(who) && owned.stack.kind == asked.kind);
                if let Some(owned) = own {
                    let stack = Stack { kind: asked.kind, count: asked.count.min(owned.stack.count) };
                    owned.stack.count -= stack.count;
                    camp.load.retain(|owned| owned.stack.count > 0);
                    depots.leave((q, r), &[Owned { who: Some(who), stack }]);
                    site.moved.write(Moved::stack(stack, wagon, depot, Cause::Store));
                }
            }
            _ => {}
        }
        if let Some((stowing, stack)) = moved {
            info!("settlers: {ent} asked for {stowing:?} of {} {:?} at the storehouse of {:?}", stack.count, stack.kind, (q, r));
        }
        let own = |camp: &Camp| camp.load.iter().filter(|owned| owned.who == Some(who)).map(|owned| owned.stack).collect::<Vec<_>>();
        let cart = camps.on(key).filter(|_| carts).map(own);
        site.writer.write(Do { event: Event::Stored { ent, held: Some(depots.of((q, r), who)), cart } });
    }
}

/// Answers each player that looks at a wagon: the nearest standing at
/// home within reach says what is loaded on it. Asked to, the player
/// loads on it so many of one material from its bag. A wagon is its own to load,
/// whatever its settler is at and by night as by day; one on the road is
/// not there to look at.
pub fn try_load(mut reader: MessageReader<Try>, mut site: Site, mut camps: ResMut<Camps>, mut players: Query<(&Loc, &mut Inventory, &Equipment, &Character)>) {
    for message in reader.read() {
        let Try { event: Event::Load { ent, give, take } } = message else { continue };
        let (ent, give, take) = (*ent, *give, *take);
        let Ok((loc, mut bag, worn, &Character(who))) = players.get_mut(ent) else { continue };
        let Some(key) = looked(&site, &camps, loc, What::Wagon) else {
            site.writer.write(Do { event: Event::Loaded { ent, load: None } });
            continue;
        };
        // No more than the bag holds of it
        let count = give.map_or(0, |stack| stack.count.min(bag.count(stack.kind)));
        if let Some(asked) = give.filter(|_| count > 0) {
            let stack = bag.remove(asked.kind, count);
            let camp = camps.change(key).expect("its camp stands");
            put(&mut camp.load, Owned { who: Some(who), stack });
            site.moved.write(Moved::stack(stack, Holder::Character(who), Holder::Camp(key.0, key.1), Cause::Give));
            info!("settlers: {ent} loaded {} {:?} on the wagon of {key:?}", stack.count, stack.kind);
            site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
        }
        // Its own comes off again, as far as its bag has room and weight for
        if let Some(asked) = take {
            let camp = camps.change(key).expect("its camp stands");
            if let Some(owned) = camp.load.iter_mut().find(|owned| owned.who == Some(who) && owned.stack.kind == asked.kind) {
                let stack = Stack { kind: asked.kind, count: asked.count.min(owned.stack.count).min(bag.room_for(worn, asked.kind)) };
                if stack.count > 0 {
                    owned.stack.count -= stack.count;
                    camp.load.retain(|owned| owned.stack.count > 0);
                    bag.add(stack);
                    site.moved.write(Moved::stack(stack, Holder::Camp(key.0, key.1), Holder::Character(who), Cause::Withdraw));
                    info!("settlers: {ent} took {} {:?} off the wagon of {key:?}", stack.count, stack.kind);
                    site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
                }
            }
        }
        // What is the player's own on it: what it may take off again
        let own: Vec<Stack> = camps.camps[&key].load.iter().filter(|owned| owned.who == Some(who)).map(|owned| owned.stack).collect();
        site.writer.write(Do { event: Event::Loaded { ent, load: Some(own) } });
    }
}

/// Has the camp known by `key` take down what is its hub's and stands
/// where the hub's plans have nothing: on no thing's tile, on no tile of
/// the wall it is to raise, and not a thing being raised. A hub kept
/// before its plans were, and one taken in, leave such things.
fn stray(site: &Site, camps: &mut Camps, key: (i32, i32)) {
    let Some(id) = site.hubs.of_hearth(key) else { return };
    let Some(camp) = camps.on(key) else { return };
    let wall = site.hubs.stakes(id);
    let planned = |tile: &(i32, i32)| site.hubs.item_on(id, *tile).is_some() || wall.contains(tile) || camp.rising.iter().any(|(_, at, _)| at == tile) || camp.clearing.contains(tile);
    let strays: Vec<(i32, i32)> = site.constructs.of_camps().filter(|&(tile, of)| site.hubs.of_hearth(of) == Some(id) && !planned(&tile) && clearable(site, tile).is_some()).map(|(tile, _)| tile).collect();
    if !strays.is_empty() {
        info!("settlers: the camp of {key:?} takes down {} things its hub's plans have no place for", strays.len());
        camps.give_up(key, strays.into_iter());
    }
}

/// Whether an admin has materials provided (`Setting::FreeMaterials`).
pub(crate) fn provided(settings: &common_bevy::settings::Settings) -> bool {
    settings.get(common::settings::Setting::FreeMaterials) != 0
}

/// `needs` as stacks, of one material to a class.
pub(crate) fn stacks(needs: Needs) -> impl Iterator<Item = Stack> {
    [(common::Material::Hide, needs.hide), (common::Material::Softwood, needs.wood), (common::Material::Sandstone, needs.stone)]
        .into_iter()
        .filter(|&(_, count)| count > 0)
        .map(|(material, count)| Stack { kind: Stackable::Material(material), count })
}

/// Gives the camp known by `key` what its hub's plans still lack, from
/// the world.
fn grant(site: &mut Site, camps: &mut Camps, key: (i32, i32)) {
    let Some(camp) = camps.on(key) else { return };
    let lacking = wants(site, camp, key, None).needs;
    let Some(camp) = camps.change(key).filter(|_| !lacking.is_none()) else { return };
    for given in stacks(lacking) {
        match camp.stock.iter_mut().find(|held| held.kind == given.kind) {
            Some(held) => held.count += given.count,
            None => camp.stock.push(given),
        }
        site.moved.write(Moved::stack(given, Holder::World, Holder::Camp(key.0, key.1), Cause::Grant));
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
    let needs = plan.built.iter().filter_map(|thing| Step::of(thing.kind)).fold(stakes, |needs, step| needs.plus(step.needs()));
    let down = plan.pulled.iter().copied().chain(plan.gone.iter().flat_map(|thing| thing.tiles.iter().copied()));
    let back = down.filter_map(|tile| clearable(site, tile)).fold(Needs::default(), |back, (_, of)| back.plus(of));
    let theirs: Vec<&Camp> = std::iter::once(key).chain(plan.taken.iter().map(|id| (id.0, id.1))).filter_map(|key| camps.on(key)).collect();
    let credit = theirs.iter().fold(back, |credit, camp| credit.plus(camp.credit));
    let stock: Vec<Stack> = theirs.iter().flat_map(|camp| camp.stock.iter().copied()).collect();
    needs.minus(credit).less(&stock)
}

/// How far past a hub's ground its ground is looked at, in tiles: where
/// a plan for more would claim.
const SURVEYED: i32 = 12;

/// Tells hub `id` how its ground lies, out to [`SURVEYED`] past it
/// (`Hubs::survey`): nothing is planned on a tile that holds a tree, a
/// stump, a boulder, reeds, a player's blockade or water, nor on both of
/// two tiles side by side whose levels a walker does not step between.
/// A tile the map does not hold yet is neither. Looked at anew before
/// anything is planned, since trees are felled and blockades built.
pub(crate) fn survey(site: &mut Site, id: common_bevy::hubs::HubId) {
    use common::cover::Content;
    use std::collections::BTreeSet;
    let (Some(hearth), Some(ground)) = (site.hubs.hearth(id), site.hubs.ground(id)) else { return };
    let reach = ground.tiles.iter().map(|&tile| qrz::hex_distance(tile, hearth)).max().unwrap_or(0) + SURVEYED;
    let (mut barred, mut cliffs) = (BTreeSet::new(), BTreeSet::new());
    for q in -reach..=reach {
        for r in (-reach).max(-q - reach)..=reach.min(-q + reach) {
            let tile = (hearth.0 + q, hearth.1 + r);
            let Some((z, cover)) = site.ground.lies(tile.0, tile.1) else { continue };
            let grown = (0..common::TILE_SLOTS as usize).any(|k| matches!(cover.content(k), Content::Pine | Content::Deciduous | Content::PineStump | Content::DeciduousStump | Content::Boulder | Content::Reed));
            let blocked = building::standing(cover).is_some_and(|construct| construct.kind == Kind::Blockade);
            if grown || blocked || site.ground.map().water_at(tile.0, tile.1).is_some() {
                barred.insert(tile);
            }
            // Each pair once, from its lesser tile's side
            for (dq, dr) in [(1, 0), (0, 1), (-1, 1)] {
                let other = (tile.0 + dq, tile.1 + dr);
                if site.ground.lies(other.0, other.1).is_some_and(|(level, _)| (level - z).abs() > 1) {
                    cliffs.insert((tile.min(other), tile.max(other)));
                }
            }
        }
    }
    site.hubs.survey(id, barred, cliffs);
}

/// Answers each player that asks for the hub it stands in as its
/// planning shows it.
pub fn try_layout(mut reader: MessageReader<Try>, mut site: Site, camps: Res<Camps>, players: Query<&Loc>) {
    for message in reader.read() {
        let Try { event: Event::Layout { ent, .. } } = message else { continue };
        let Ok(loc) = players.get(*ent) else {
            warn!("settlers: {ent} asked for a hub's layout and is no player in the world");
            continue;
        };
        let held = site.hubs.holding((loc.q, loc.r));
        if let Some(id) = held {
            survey(&mut site, id);
        }
        let layout = held.and_then(|id| laid_out(&site, &camps, id));
        site.writer.write(Do { event: Event::Layout { ent: *ent, layout } });
    }
}

/// Hub `id` as its planning shows it.
fn laid_out(site: &Site, camps: &Camps, id: common_bevy::hubs::HubId) -> Option<common::building::Layout> {
    let (hub, ground) = (site.hubs.hub(id)?, site.hubs.ground(id)?);
    let camp = camps.camps.iter().find(|(key, _)| site.hubs.of_hearth(**key) == Some(id)).map(|(_, camp)| camp);
    let bits = |zones: &mut dyn Iterator<Item = Zone>| zones.fold(0u8, |bits, zone| bits | 1 << zone.id());
    let items = hub
        .items
        .iter()
        .map(|item| {
            let stands = Step::of(item.kind).and_then(|step| Some((step, step.lies(item)?.0))).is_some_and(|(step, first)| site.ground.cover(first.0, first.1).is_some_and(|cover| step.stood(cover)));
            common::building::Laid { what: item.kind.what.id() as u8, level: item.kind.level, tiles: item.tiles.clone(), players: item.players as u8, settlers: item.settlers as u8, stands }
        })
        .collect();
    Some(common::building::Layout {
        residents: hub.residents(),
        wants: hub.wants,
        hired: bits(&mut site.hubs.hired(id)),
        busy: bits(&mut camp.into_iter().flat_map(|camp| camp.rising.iter().map(|(step, ..)| step.zone()))),
        hires: Zone::ALL.into_iter().find(|&zone| site.hubs.hires(id, zone)).map_or(0, |zone| zone.id() as u8),
        walled: site.hubs.walled(id),
        ground: ground.tiles.clone(),
        wall: ground.wall.clone(),
        barred: site.hubs.barred(id),
        zones: hub.zones.iter().map(|(zone, at)| (zone.id() as u8, at.0, at.1)).collect(),
        items,
    })
}

/// The plan `order` asks of hub `id`, or none where the hub does not take
/// it: its zone has no planner, or the rules refuse it. A thing is
/// upgraded or downgraded only to one a settler raises (`Step::of`).
fn ordered(hubs: &Hubs, id: common_bevy::hubs::HubId, order: Order) -> Option<Plan> {
    let hired = |zone: Zone| hubs.hired(id).any(|has| has == zone);
    match order {
        Order::Shrink => hubs.shrunk(id),
        Order::Residents(more) => hubs.planned(id, (hubs.wants(id) as i64 + more as i64).max(common_bevy::plan::CAMP as i64) as u32).filter(|_| hired(Zone::Logistics)),
        Order::Wall => hubs.walling(id),
        Order::Raise(fixture) => {
            let what = what_of(fixture)?;
            hubs.raised(id, what).filter(|_| hired(what.zone()))
        }
        Order::Zone(zone, q, r) => hubs.zoned(id, Zone::kept_as(zone)?, (q, r)),
        Order::Upgrade(q, r) | Order::Downgrade(q, r) => {
            let kind = hubs.item_on(id, (q, r))?.kind;
            let to = if matches!(order, Order::Upgrade(..)) { kind.next()? } else { kind.before()? };
            let plan = if matches!(order, Order::Upgrade(..)) { hubs.upgraded(id, (q, r)) } else { hubs.downgraded(id, (q, r)) };
            plan.filter(|_| Step::of(to).is_some() && hired(kind.what.zone()))
        }
        Order::Hire(_) => None,
    }
}

/// Answers each player that asks the settler it hails for a plan: what
/// an order to its hub's planners would do, and what the camp still
/// lacks for it. A planner is hired on the asking, where the hub may
/// hire one. Asked to carry a plan
/// out, it does where the camp lacks nothing, and one that only gives
/// up whatever it lacks; one the camp lacks materials for takes what the
/// player's bag holds of those instead, and is asked for again. Carried out, the
/// hubs are as the plan leaves them from then, a hub taken in
/// gives its camp all it held and its storehouse's all to the hub's, each
/// stack still its player's, each hub parted off has a camp and a
/// settler of its own, and the settlers make the ground so in their own
/// time. No plan is made by night, with the wagon on the road, or while
/// the settler, or that of a hub the plan takes in, has a thing begun.
#[allow(clippy::too_many_arguments)]
pub fn try_plan(
    mut reader: MessageReader<Try>,
    settings: Res<common_bevy::settings::Settings>,
    mut site: Site,
    mut camps: ResMut<Camps>,
    mut depots: ResMut<Depots>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    settlers: Query<(&Settler, &Loc)>,
    mut players: Query<(&Loc, &mut Inventory, &Character)>,
    homes: Query<(Entity, &Character, Option<&common_bevy::components::Home>, Option<&common_bevy::components::Lives>)>,
    mut commands: Commands,
) {
    for message in reader.read() {
        let Try { event: Event::Plan { ent, order, carry } } = message else { continue };
        let (ent, order, carry, grow) = (*ent, *order, *carry, order.builds());
        let Ok((loc, mut bag, &Character(who))) = players.get_mut(ent) else { continue };
        let when = runtime.when(&time);
        // The ground is looked at before anything is planned on it
        let surveyed = hailed(&settlers, loc).and_then(|(key, _)| site.hubs.of_hearth(key)).or_else(|| site.hubs.holding((loc.q, loc.r)));
        if let Some(id) = surveyed {
            survey(&mut site, id);
        }
        // A planner takes no order while it has a thing begun, which its
        // client is told (`Wants::busy`). A hub taken in has its wagon home
        // first, or its load is lost
        let busy = |key: (i32, i32), zone: Option<Zone>| zone.is_some_and(|zone| camps.on(key).is_some_and(|camp| camp.rising_of(zone).is_some()));
        let home = |key: (i32, i32)| camps.on(key).is_none_or(|camp| camp.trip.is_none());
        let near = hailed(&settlers, loc).filter(|_| !when.is_night());
        // Away from every settler, on a hub's ground, an order goes to the
        // planner whose it is: one that acts on what the player faces is
        // carried there
        let afar = || {
            let id = site.hubs.holding((loc.q, loc.r)).filter(|_| !when.is_night())?;
            camps.camps.keys().copied().find(|key| site.hubs.of_hearth(*key) == Some(id))
        };
        let camp = near.map(|(key, _)| key).or_else(afar);
        // An order is its zone's planner's to take, and only a hire is any settler's
        let asked = camp.and_then(|key| asked_of(&site.hubs, site.hubs.of_hearth(key)?, order));
        let own = match near {
            Some((key, place)) => zone_of(&site.hubs, key, place),
            None => asked,
        };
        // One at work still says what an order would do, and only does none
        let hailed = camp.filter(|&key| (asked.is_none() || asked == own) && !(carry && busy(key, asked)));
        if let (Order::Hire(zone), Some(key), true) = (order, hailed, carry) {
            let hired = site.hubs.of_hearth(key).is_some_and(|id| Zone::kept_as(zone).or_else(|| Zone::ALL.into_iter().find(|&zone| site.hubs.hires(id, zone))).is_some_and(|zone| site.hubs.hire(id, zone)));
            info!("settlers: {ent} asked the hub of {key:?} to hire a planner for zone {zone}: {}", if hired { "hired" } else { "refused" });
            site.writer.write(Do { event: Event::Planned { ent, plan: None } });
            let said = wants(&site, &camps.camps[&key], key, near.and_then(|(key, place)| zone_of(&site.hubs, key, place)));
            site.writer.write(Do { event: Event::Wants { ent, wants: Some(said) } });
            continue;
        }
        let plan = hailed.and_then(|key| {
            let plan = ordered(&site.hubs, site.hubs.of_hearth(key)?, order)?;
            plan.taken.iter().all(|id| home((id.0, id.1))).then_some((key, plan))
        });
        let Some((key, plan)) = plan else {
            site.writer.write(Do { event: Event::Planned { ent, plan: None } });
            continue;
        };
        // Provided, an order lacks nothing: the camp is given what it takes as it is carried
        let free = provided(&settings);
        let mut needs = if free { Needs::default() } else { lacking(&site, &camps, key, &plan) };
        if carry && grow && !needs.is_none() {
            hand(&mut site, camps.change(key).expect("its camp stands"), key, needs, ent, who, &mut bag);
            needs = lacking(&site, &camps, key, &plan);
        }
        if !carry || (grow && !needs.is_none()) {
            let tiles = |things: &[common_bevy::hubs::Thing]| things.iter().flat_map(|thing| thing.tiles.iter().copied()).collect();
            let said = Planned { order, built: tiles(&plan.built), gone: tiles(&plan.gone), raised: plan.raised.clone(), pulled: plan.pulled.clone(), worn: plan.worn.clone(), faded: plan.faded.clone(), hubs: (plan.taken.len() + plan.parted().count()) as u32, needs };
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
            for stack in camp.stock {
                match ours.stock.iter_mut().find(|held| held.kind == stack.kind) {
                    Some(held) => held.count += stack.count,
                    None => ours.stock.push(stack),
                }
                site.moved.write(Moved::stack(stack, Holder::Camp(taken.0, taken.1), Holder::Camp(key.0, key.1), Cause::Give));
            }
            for owned in camp.load {
                put(&mut ours.load, owned);
                site.moved.write(Moved::stack(owned.stack, Holder::Camp(taken.0, taken.1), Holder::Camp(key.0, key.1), Cause::Give));
            }
        }
        for taken in plan.taken.iter().map(|id| (id.0, id.1)) {
            for owned in depots.join(taken, (plan.hub.0, plan.hub.1)) {
                site.moved.write(Moved::stack(owned.stack, Holder::Depot(taken.0, taken.1), Holder::Depot(plan.hub.0, plan.hub.1), Cause::Give));
            }
        }
        // What a planner had begun on a tile the plan gives up is begun no more, and comes down with the rest
        let given: Vec<(i32, i32)> = plan.pulled.iter().copied().chain(plan.gone.iter().flat_map(|thing| thing.tiles.iter().copied())).collect();
        if let Some(camp) = camps.change(key).filter(|camp| camp.rising.iter().any(|(_, at, _)| given.contains(at))) {
            camp.rising.retain(|(_, at, _)| !given.contains(at));
        }
        camps.give_up(key, given.into_iter());
        for parted in plan.parted() {
            camps.found((parted.0, parted.1), when);
        }
        site.hubs.carry(&plan);
        if provided(&settings) {
            grant(&mut site, &mut camps, key);
        }
        crate::systems::building::rehouse(&site, &homes, &mut commands);
        info!("settlers: {ent} had the planners of {key:?} carry out {order:?}");
        site.writer.write(Do { event: Event::Planned { ent, plan: None } });
        let said = wants(&site, &camps.camps[&key], key, own);
        site.writer.write(Do { event: Event::Wants { ent, wants: Some(said) } });
        site.writer.write(Do { event: Event::Wagon { ent, wagon: None } });
    }
}

/// Answers each player that sends a wagon: the settler it hails takes
/// the map in its main hand, which is used up, and leaves with its
/// camp's wagon, off the tile it stood on, and all loaded on it for the
/// hub named, by the way the map holds. Nothing leaves where the wagon is not ready, holds
/// nothing, or the map holds no way to that hub. A wagon stopped on the
/// road ([`waits`]) is sent the same way by its driver, from where it
/// stands with what it carries, and to its own hub is sent home.
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
        let near = hailed(&settlers, loc).filter(|_| !when.is_night());
        // A wagon stopped on the road is sent on from where it stands
        let stands = near.and_then(|(hearth, place)| waits(&camps, hearth, place));
        let ready = |hearth: &(i32, i32)| stands.is_some() || camps.on(*hearth).is_some_and(|camp| carts(&site, camp, *hearth) && !camp.load.is_empty());
        let Some(hearth) = near.map(|(hearth, _)| hearth).filter(ready) else {
            info!("settlers: {ent} has no wagon ready to send");
            continue;
        };
        let Some((map, number)) = worn.worn(Slot::MainHand).and_then(|item| Some((item, item.map()?))) else { continue };
        let way = charts.map(number).and_then(|chart| destinations(chart, &site.hubs, hearth, stands).into_iter().find(|(hub, _)| *hub == to));
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
        if stands.is_some() {
            // What it carries goes on with it; sent to its own hub it is on its way home
            let camp = camps.change(hearth).expect("its camp stands");
            let cargo = camp.trip.take().map_or(Vec::new(), |trip| trip.cargo);
            info!("settlers: the wagon of {hearth:?}, stopped at {stands:?}, goes on to the hub of {to:?} by map {number}, {} legs", way.len());
            camp.trip = Some(Trip { back: homes(&site.hubs, hearth, to), ..Trip::begun(to, way, cargo, when) });
            site.writer.write(Do { event: Event::Incremental { ent, component: common_bevy::message::Component::Equipment(*worn) } });
            site.writer.write(Do { event: Event::Inventory { ent, bag: bag.clone() } });
            site.writer.write(Do { event: Event::Wants { ent, wants: None } });
            site.writer.write(Do { event: Event::Wagon { ent, wagon: None } });
            continue;
        }
        // Its horse is led from the post to draw it
        if let Some((tile, led)) = planned(&site.hubs, hearth, What::Post).and_then(|tile| Some((tile, building::untied(site.ground.cover(tile.0, tile.1)?)?))) {
            site.ground.set(&mut site.writer, tile.0, tile.1, led);
        }
        // The wagon leaves its site, which stays its place
        if let Some((tile, gone)) = planned(&site.hubs, hearth, What::Wagon).and_then(|tile| Some((tile, building::driven(site.ground.cover(tile.0, tile.1)?)?))) {
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
        assert!(Step::ALL.into_iter().all(|step| Step::kept_as(step.id()) == Some(step)));
        assert_eq!((Step::kept_as(2), Step::kept_as(99)), (Some(Step::Home), None));
        assert!(What::ALL.into_iter().flat_map(|what| (1..=common_bevy::plan::SIZES.len() as u8).map(move |level| common_bevy::plan::Kind { what, level })).filter_map(Step::of).all(|step| !step.needs().is_none()), "whatever a settler raises takes materials");
        assert!(Step::Clear.needs().is_none());
    }
}
