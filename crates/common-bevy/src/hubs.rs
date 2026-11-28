//! Hubs: the places players live, as a world holds them. A hub is the
//! ground it claims, its zones and what stands in it, by the rules of
//! [`crate::plan`]; this holds every hub there is, where every player
//! known to have played of late is at home, which of each hub's zones
//! have a planner, and what each change would do.
//!
//! A player lives in the home it claimed, and is one of that hub's
//! residents. Where it returns when it falls is another thing, a hearth,
//! and not held here: [`Hubs::returns`] says where a hearth claimed is
//! returned to now.
//!
//! Nothing changes a hub but a plan. Each of the asking methods says what
//! one would do, and [`Hubs::carry`] does it. Players coming and going
//! change who lives in a hub's homes, and so what its residents defend,
//! and never its ground.
//!
//! A zone is its planner's: a settler hired for it ([`Hubs::hire`]), and
//! nothing is raised in a zone that has none. The logistics planner also
//! raises the hub's wall, so a hub without one has no stakes
//! ([`Hubs::stakes`]).
//!
//! The haven is no hub. It is the point the world names, where a player
//! who has claimed no home is at home: it has no size, takes nothing in
//! and is taken in by nothing. Its guard keeps the ground round it, and
//! the country's level is counted from it, so the country round it is
//! the mildest there is.
//!
//! A hub pushes the country back ([`Hubs::influence`]), and the country
//! gives way to it a level a day ([`Hubs::age`]): the level a nest's
//! creatures are at is the country's own less what the hubs have pushed
//! back ([`Hubs::level`]), and no nest stands on quiet ground
//! ([`Hubs::quiet`]).
//!
//! Hubs are few and known whole, wherever their ground is. How they are
//! kept past the server is not this module's to know. The server holds
//! them, and the world viewer plans a scenario by these same rules to
//! draw it.

use std::collections::{BTreeSet, HashMap, HashSet};

use bevy::prelude::*;
use common::network::Identity;
use qrz::{
    outline::{apart, flat},
    Qrz,
};

use crate::plan::{self, Item, Kind, Tile, What, Zone};
use crate::systems::When;

/// How far round its point the haven's guard keeps the ground, in tiles.
pub const HAVEN_GROUND: f32 = 76.0;

/// Tiles from the haven each level of the country lies further out, where
/// no hub pushes it back.
pub const LEVEL_TILES: f32 = 400.0;

/// How far past its wall a hub's push is whole, in tiles: as far as a
/// creature follows what it fights from its nest
/// (`combat::behaviour::LEASH_DISTANCE`), so nothing a nest out there
/// sends reaches the wall.
pub const QUIET: f32 = 60.0;

/// How far past its quiet ground a hub's influence carries, in lengths
/// of its wall.
pub const INFLUENCE_WALLS: f32 = 1.5;

/// The players at home in a hub whose influence is whole over its ground.
pub const INFLUENCE_WHOLE: f32 = 1000.0;

/// Which hub: the tile it was founded on. It is the hub's for as long as
/// the hub is, wherever a plan lays its hearth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HubId(pub i32, pub i32);

/// The ground a hub holds, and its wall: each tile of that ground with a
/// neighbour outside it. Both in `q` then `r` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ground {
    pub tiles: Vec<Tile>,
    pub wall: Vec<Tile>,
}

/// A hub as it is held: the ground it makes, the way in through its
/// wall, the zones that have a planner, whether its wall is to be raised,
/// and the hubs it took in, whose names are its own from then.
#[derive(Clone, Debug, PartialEq)]
struct Held {
    hub: plan::Hub,
    ground: Ground,
    gate: Option<Tile>,
    /// The path from its hearth to its way in.
    path: Vec<Tile>,
    hired: BTreeSet<Zone>,
    walled: bool,
    took: Vec<HubId>,
    /// When the country began to give way to it, on the calendar; none
    /// until it is first aged ([`Hubs::age`]).
    since: Option<When>,
    /// How much of its push the country has given way to, 0 to 1, as it
    /// was last aged.
    given: f32,
}

/// A hub as a plan leaves it.
#[derive(Clone, Debug, PartialEq)]
struct After {
    id: HubId,
    hub: plan::Hub,
    hired: BTreeSet<Zone>,
    walled: bool,
    took: Vec<HubId>,
}

/// A number from a hub and a tile, the same on every run and spread
/// evenly: what picks one tile of several as good.
fn drawn(id: HubId, tile: Tile) -> u64 {
    let mut x = [id.0, id.1, tile.0, tile.1].into_iter().fold(0x9E37_79B9_7F4A_7C15u64, |x, n| (x ^ n as u32 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9).rotate_left(31));
    x ^= x >> 29;
    x.wrapping_mul(0x94D0_49BB_1331_11EB)
}

/// A thing of a hub as a store keeps it: its number in the hub, the
/// number of what it is, its level and its tiles.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeptItem {
    pub id: u32,
    pub what: u32,
    pub level: u8,
    pub tiles: Vec<Tile>,
}

/// A hub as a store keeps it: the tile it is known by, how many it is
/// planned for, its zones by their numbers, what stands in it, the ground
/// it has open, the zones with a planner, the way in through its wall and
/// the path to it as a plan last put them, and the hubs it took in. `sites`
/// is how a hub was kept while it was build sites on a lattice, each with
/// the number of what stood on it: one kept so has no `items`, and is laid
/// again from them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub id: Tile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<(i32, i32, u32)>,
    #[serde(default)]
    pub wants: u32,
    #[serde(default)]
    pub zones: Vec<(u32, i32, i32)>,
    #[serde(default)]
    pub items: Vec<KeptItem>,
    #[serde(default)]
    pub open: Vec<Tile>,
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub hired: Vec<u32>,
    #[serde(default)]
    pub walled: bool,
    #[serde(default)]
    pub gate: Option<Tile>,
    #[serde(default)]
    pub path: Vec<Tile>,
    pub took: Vec<Tile>,
    /// When the country began to give way to it, in the calendar's
    /// milliseconds.
    #[serde(default)]
    pub since: Option<u64>,
}

impl Kept {
    /// A hub kept as its hearths, before hubs had ground: a camp round
    /// the first, known by it, the others names it answers to.
    pub fn of_hearths(hearths: &[Tile]) -> Option<Kept> {
        let (&id, took) = hearths.split_first()?;
        Some(Kept { took: took.to_vec(), ..Kept::of(id, &plan::camp(id), &BTreeSet::new(), false, &[]) })
    }

    fn of(id: Tile, hub: &plan::Hub, hired: &BTreeSet<Zone>, walled: bool, took: &[HubId]) -> Kept {
        Kept {
            id,
            sites: Vec::new(),
            wants: hub.wants,
            zones: hub.zones.iter().map(|(zone, at)| (zone.id(), at.0, at.1)).collect(),
            items: hub.items.iter().map(|item| KeptItem { id: item.id, what: item.kind.what.id(), level: item.kind.level, tiles: item.tiles.clone() }).collect(),
            open: hub.open.iter().copied().collect(),
            next: hub.next,
            hired: hired.iter().map(|zone| zone.id()).collect(),
            walled,
            gate: None,
            path: Vec::new(),
            took: took.iter().map(|id| (id.0, id.1)).collect(),
            since: None,
        }
    }

    /// The hub kept, the zones it has a planner for and whether its wall
    /// is raised. One kept as build sites is a hub of one-tile things on
    /// those sites, planned for as many as its homes house, with the
    /// planners and the wall every hub had then. A thing this build has
    /// no number for is a home.
    fn laid(&self) -> (plan::Hub, BTreeSet<Zone>, bool) {
        if self.items.is_empty() {
            let mut hub = plan::Hub { wants: plan::CAMP, zones: Default::default(), items: Vec::new(), open: Default::default(), next: 1, barred: Default::default(), cliffs: Default::default() };
            for &(q, r, what) in &self.sites {
                let what = What::kept_as(what).unwrap_or(What::Home);
                hub.items.push(Item { id: hub.next, kind: Kind::first(what), tiles: vec![(q, r)], players: 0, settlers: 0 });
                hub.next += 1;
            }
            let hearth = hub.hearth().unwrap_or(self.id);
            hub.zones.insert(Zone::Logistics, hearth);
            hub.zones.insert(Zone::Residential, hearth);
            hub.wants = hub.room().max(plan::CAMP);
            return (hub.peopled(), BTreeSet::from([Zone::Logistics, Zone::Residential]), true);
        }
        let hub = plan::Hub {
            wants: self.wants,
            zones: self.zones.iter().filter_map(|&(zone, q, r)| Some((Zone::kept_as(zone)?, (q, r)))).collect(),
            items: self
                .items
                .iter()
                .map(|item| {
                    let what = What::kept_as(item.what).unwrap_or(What::Home);
                    Item { id: item.id, kind: Kind { what, level: if what == What::Hearth { 0 } else { item.level.max(1) } }, tiles: item.tiles.clone(), players: 0, settlers: 0 }
                })
                .collect(),
            open: self.open.iter().copied().collect(),
            next: self.next,
            // The ground is looked at anew before anything is planned (`Hubs::survey`)
            barred: Default::default(),
            cliffs: Default::default(),
        };
        (hub.peopled(), self.hired.iter().filter_map(|&zone| Zone::kept_as(zone)).collect(), self.walled)
    }
}

/// A thing a plan raises or takes down: the first of its tiles, what it
/// is, and every tile it stands on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Thing {
    pub at: Tile,
    pub kind: Kind,
    pub tiles: Vec<Tile>,
}

impl Thing {
    fn of(item: &Item) -> Thing {
        Thing { at: item.tiles[0], kind: item.kind, tiles: item.tiles.clone() }
    }
}

/// What a plan would do to a hub, and to the hubs round it.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The hub planned for.
    pub hub: HubId,
    /// Every hub the plan changes or makes, as it leaves it.
    after: Vec<After>,
    /// The hubs it takes in, which are no more.
    pub taken: Vec<HubId>,
    /// The things it raises where none stood so, and those it takes down.
    pub built: Vec<Thing>,
    pub gone: Vec<Thing>,
    /// The tiles it raises stakes on, and those it pulls them from.
    pub raised: Vec<Tile>,
    pub pulled: Vec<Tile>,
    /// The tiles its paths come to cross, and those they cross no more.
    pub worn: Vec<Tile>,
    pub faded: Vec<Tile>,
}

impl Plan {
    /// The hubs the plan parts off, each known by the tile its hearth is
    /// laid on.
    pub fn parted(&self) -> impl Iterator<Item = HubId> + '_ {
        self.after.iter().map(|after| after.id).filter(move |id| *id != self.hub)
    }
}

/// Every hub, and where every player known to have played of late is at
/// home.
#[derive(Resource, Debug)]
pub struct Hubs {
    haven: Tile,
    hubs: HashMap<HubId, Held>,
    /// Each recent player's home, a tile of a home, or none where it
    /// lives in none.
    residents: HashMap<Identity, Option<Tile>>,
    /// Players each hub counts as living in it besides those who do: a
    /// fixture, for seeing what a hub's residents defend with no one else
    /// in the world.
    extra: u32,
    changed: bool,
    /// How high each tile lies, in levels, as a survey last said
    /// ([`Hubs::survey`]): what a wall's way in is placed by.
    levels: HashMap<Tile, i32>,
}

/// The tiles between two tiles of the flat grid.
fn distance(a: Tile, b: Tile) -> f32 {
    Qrz { q: a.0, r: a.1, z: 0 }.flat_distance(&Qrz { q: b.0, r: b.1, z: 0 }) as f32
}

/// The way out of `ground` from `tile` of its wall, as a place in
/// `common::cover::FACINGS`: the one nearest the way its tiles beside it
/// that are off the ground lie, taken together.
fn outward(ground: &Ground, tile: Tile) -> usize {
    let here = flat(tile);
    let out = qrz::DIRECTIONS.iter().map(|d| (tile.0 + d.q, tile.1 + d.r)).filter(|beside| ground.tiles.binary_search(beside).is_err()).fold((0.0, 0.0), |sum, beside| {
        let at = flat(beside);
        (sum.0 + at.0 - here.0, sum.1 + at.1 - here.1)
    });
    let along = |&(q, r): &Tile| {
        let way = flat((q, r));
        (way.0 * out.0 + way.1 * out.1) / apart(way, (0.0, 0.0))
    };
    let ways = common::cover::FACINGS;
    (0..ways.len()).max_by(|&a, &b| along(&ways[a]).total_cmp(&along(&ways[b])).then(b.cmp(&a))).unwrap_or(0)
}

/// How many steps the shortest walk over `ground` from the tile `from`
/// takes to each tile it reaches: a walk that crosses nothing of `built`
/// and takes only steps `walked` allows, from one tile to the next.
fn reach(ground: &Ground, built: &HashSet<Tile>, from: Tile, walked: impl Fn(Tile, Tile) -> bool) -> HashMap<Tile, u32> {
    let mut steps = HashMap::from([(from, 0u32)]);
    let mut open = std::collections::VecDeque::from([from]);
    while let Some(tile) = open.pop_front() {
        for d in qrz::DIRECTIONS.iter() {
            let next = (tile.0 + d.q, tile.1 + d.r);
            if steps.contains_key(&next) || built.contains(&next) || ground.tiles.binary_search(&next).is_err() || !walked(tile, next) {
                continue;
            }
            steps.insert(next, steps[&tile] + 1);
            open.push_back(next);
        }
    }
    steps
}

/// The path over `ground` from the tile `from` to the tile `to`: the
/// tiles walked after `from`, `to` the last. It is the shortest that
/// crosses nothing of `built` and takes only steps `walked` allows; of
/// those as short, the one that keeps nearest the straight line between
/// the two. Empty where the ground holds no such way.
fn path(ground: &Ground, built: &HashSet<Tile>, from: Tile, to: Tile, walked: impl Fn(Tile, Tile) -> bool) -> Vec<Tile> {
    use std::{cmp::Reverse, collections::BinaryHeap};
    let (a, b) = (flat(from), flat(to));
    let long = apart(a, b).max(1e-9);
    // How far a tile lies off the line, in thousandths of a tile
    let off = |tile: Tile| {
        let p = flat(tile);
        let along = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / (long * long)).clamp(0.0, 1.0);
        (apart(p, (a.0 + (b.0 - a.0) * along, a.1 + (b.1 - a.1) * along)) * 1000.0).round() as u64
    };
    let mut best: HashMap<Tile, ((u32, u64), Tile)> = HashMap::new();
    let mut open = BinaryHeap::from([Reverse(((0u32, 0u64), from, from))]);
    while let Some(Reverse((cost, tile, by))) = open.pop() {
        if best.contains_key(&tile) {
            continue;
        }
        best.insert(tile, (cost, by));
        if tile == to {
            break;
        }
        for d in qrz::DIRECTIONS.iter() {
            let next = (tile.0 + d.q, tile.1 + d.r);
            if best.contains_key(&next) || ground.tiles.binary_search(&next).is_err() || (built.contains(&next) && next != to) || !walked(tile, next) {
                continue;
            }
            open.push(Reverse(((cost.0 + 1, cost.1 + off(next)), next, tile)));
        }
    }
    let mut way = Vec::new();
    let mut at = to;
    while at != from {
        let Some(&(_, by)) = best.get(&at) else { return Vec::new() };
        way.push(at);
        at = by;
    }
    way.reverse();
    way
}

impl Hubs {
    /// The hubs of a world whose haven is the tile `haven`: none, with
    /// nothing built.
    pub fn new(haven: Tile) -> Self {
        Self { haven, hubs: HashMap::new(), residents: HashMap::new(), extra: 0, changed: false, levels: HashMap::new() }
    }

    /// `after` as it is held.
    fn hold(&self, after: &After) -> Held {
        let lie = after.hub.lies();
        let ground = Ground { tiles: lie.ground, wall: lie.wall };
        let (gate, path) = self.way(after.id, &after.hub, &ground);
        Held { path, gate, ground, hub: after.hub.clone(), hired: after.hired.clone(), walled: after.walled, took: after.took.clone(), since: None, given: 0.0 }
    }

    /// The way in through the wall of `ground`, hub `id`'s, the one tile
    /// of it no stakes stand on, and the path to it from the hearth. The
    /// way in is the tile of the wall the shortest walk from the hearth
    /// reaches, a walk that crosses nothing standing, climbs or drops no
    /// more than a level a step and can go on out of the hub the same; of
    /// those as near, the one on the gentlest ground, which differs least
    /// in height from the tiles round it; of those as gentle, one drawn by
    /// the hub and the tile. Where no walk reaches the wall, the ground is
    /// taken as flat. The ground lies as it was last surveyed
    /// ([`Hubs::survey`]), and a tile no survey told of is stepped on from
    /// any other.
    fn way(&self, id: HubId, hub: &plan::Hub, ground: &Ground) -> (Option<Tile>, Vec<Tile>) {
        let Some(hearth) = hub.hearth() else { return (None, Vec::new()) };
        let built: HashSet<Tile> = hub.items.iter().flat_map(|item| item.tiles.iter().copied()).collect();
        let level = |tile: Tile| self.levels.get(&tile).copied();
        let walked = |a: Tile, b: Tile| level(a).zip(level(b)).is_none_or(|(a, b)| (a - b).abs() <= 1);
        let steep = |(q, r): Tile| level((q, r)).map_or(0, |here| qrz::DIRECTIONS.iter().filter_map(|d| level((q + d.q, r + d.r))).map(|there| (there - here).abs()).max().unwrap_or(0));
        let leaves = |tile: Tile| qrz::DIRECTIONS.iter().map(|d| (tile.0 + d.q, tile.1 + d.r)).any(|out| ground.tiles.binary_search(&out).is_err() && walked(tile, out));
        let near = |steps: &HashMap<Tile, u32>, open: &dyn Fn(Tile) -> bool| {
            ground.wall.iter().copied().filter(|&tile| open(tile)).filter_map(|tile| Some((steps.get(&tile)?, steep(tile), drawn(id, tile), tile))).min().map(|(.., tile)| tile)
        };
        if let Some(gate) = near(&reach(ground, &built, hearth, walked), &leaves) {
            return (Some(gate), path(ground, &built, hearth, gate, walked));
        }
        let gate = near(&reach(ground, &built, hearth, |_, _| true), &|_| true);
        (gate, gate.map_or(Vec::new(), |gate| path(ground, &built, hearth, gate, |_, _| true)))
    }

    /// The hub known by `tile`: founded on it, or a hub that took in the
    /// one that was.
    pub fn of_hearth(&self, tile: Tile) -> Option<HubId> {
        let known = HubId(tile.0, tile.1);
        self.hubs.iter().find(|(id, held)| **id == known || held.took.contains(&known)).map(|(&id, _)| id)
    }

    /// The hub whose ground holds `tile`.
    pub fn holding(&self, tile: Tile) -> Option<HubId> {
        self.hubs.iter().filter(|(_, held)| held.ground.tiles.binary_search(&tile).is_ok()).map(|(&id, _)| id).min()
    }

    /// Hub `id` as its planners see it.
    pub fn hub(&self, id: HubId) -> Option<&plan::Hub> {
        self.hubs.get(&id).map(|held| &held.hub)
    }

    /// What stands in hub `id`, in the order it was raised.
    pub fn items(&self, id: HubId) -> &[Item] {
        self.hubs.get(&id).map_or(&[], |held| &held.hub.items)
    }

    /// The thing of hub `id` standing on `tile`.
    pub fn item_on(&self, id: HubId, tile: Tile) -> Option<&Item> {
        self.hubs.get(&id)?.hub.on(tile)
    }

    /// The ground hub `id` holds, and its wall.
    pub fn ground(&self, id: HubId) -> Option<&Ground> {
        self.hubs.get(&id).map(|held| &held.ground)
    }

    /// The tiles of a wall that stakes stand on: all but its way in, and
    /// none where the wall is not to be raised.
    fn staked(held: &Held) -> Vec<Tile> {
        if !held.walled {
            return Vec::new();
        }
        held.ground.wall.iter().copied().filter(|&tile| Some(tile) != held.gate).collect()
    }

    /// The tiles hub `id`'s stakes stand on: its wall, but for the way
    /// in. None before its logistics planner was asked for the wall.
    pub fn stakes(&self, id: HubId) -> Vec<Tile> {
        self.hubs.get(&id).map_or(Vec::new(), Self::staked)
    }

    /// Whether hub `id`'s wall is to be raised.
    pub fn walled(&self, id: HubId) -> bool {
        self.hubs.get(&id).is_some_and(|held| held.walled)
    }

    /// The way the stakes on `tile` of hub `id`'s wall face, out of its
    /// ground, as a place in `common::cover::FACINGS`.
    pub fn faces(&self, id: HubId, tile: Tile) -> Option<usize> {
        Some(outward(self.ground(id)?, tile))
    }

    /// The way in through hub `id`'s wall.
    pub fn way_in(&self, id: HubId) -> Option<Tile> {
        self.hubs.get(&id)?.gate
    }

    /// The path from hub `id`'s hearth to its way in, the way in the last:
    /// where a plan last carried out put it. Unbuilt: nothing draws it.
    pub fn path(&self, id: HubId) -> &[Tile] {
        self.hubs.get(&id).map_or(&[], |held| &held.path)
    }

    /// Whether a player may live on `tile`: a home stands there.
    pub fn homed(&self, tile: Tile) -> bool {
        self.hubs.values().any(|held| held.hub.on(tile).is_some_and(|item| item.kind.what == What::Home))
    }

    /// Whether a hub's hearth is laid on `tile`.
    pub fn hearthed(&self, tile: Tile) -> bool {
        self.hubs.values().any(|held| held.hub.hearth() == Some(tile))
    }

    /// Where a player returns that claimed the hearth on `tile`: that
    /// tile while a hearth is laid there, the hearth of the hub that holds
    /// the ground once it is one no more, and nowhere, the haven, where no
    /// hub does.
    pub fn returns(&self, tile: Tile) -> Option<Tile> {
        if self.hearthed(tile) {
            return Some(tile);
        }
        self.hearth(self.holding(tile)?)
    }

    /// The first tile of each `what` of hub `id`, in the order raised.
    pub fn standing(&self, id: HubId, what: What) -> impl Iterator<Item = Tile> + '_ {
        self.items(id).iter().filter(move |item| item.kind.what == what).map(|item| item.tiles[0])
    }

    /// The tile hub `id`'s hearth is laid on.
    pub fn hearth(&self, id: HubId) -> Option<Tile> {
        self.hubs.get(&id)?.hub.hearth()
    }

    /// How many residents hub `id` is planned for.
    pub fn wants(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |held| held.hub.wants)
    }

    /// How many players live in hub `id`.
    pub fn size(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |held| held.hub.players() + self.extra)
    }

    /// How many settlers live in hub `id`.
    pub fn settlers(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |held| held.hub.settlers())
    }

    /// How many stakes of wall hub `id`'s residents defend between them.
    pub fn held(&self, id: HubId) -> usize {
        self.hubs.get(&id).map_or(0, |held| match held.hub.residents() + self.extra {
            0 => 0,
            residents => plan::wall_for(residents) + plan::FORGIVEN,
        })
    }

    /// The zones of hub `id` that have a planner.
    pub fn hired(&self, id: HubId) -> impl Iterator<Item = Zone> + '_ {
        self.hubs.get(&id).into_iter().flat_map(|held| held.hired.iter().copied())
    }

    /// Whether hub `id` may hire a planner for `zone`: the hub is planned
    /// for enough to have the zone, the zone has none, and a settler lives
    /// in the hub who is no zone's planner yet.
    pub fn hires(&self, id: HubId, zone: Zone) -> bool {
        self.hubs.get(&id).is_some_and(|held| held.hub.wants >= zone.from() && !held.hired.contains(&zone) && held.hub.settlers() as usize > held.hired.len())
    }

    /// Hires a planner for `zone` of hub `id`, where it may
    /// ([`Hubs::hires`]). A zone hired for that has no centre yet is
    /// centred on the hearth.
    pub fn hire(&mut self, id: HubId, zone: Zone) -> bool {
        if !self.hires(id, zone) {
            return false;
        }
        let Some(held) = self.hubs.get_mut(&id) else { return false };
        held.hired.insert(zone);
        if let Some(hearth) = held.hub.hearth() {
            held.hub.zones.entry(zone).or_insert(hearth);
        }
        self.changed = true;
        true
    }

    /// Whether a hearth may be laid on `tile`: the camp round it would
    /// touch no hub's ground.
    pub fn founds(&self, tile: Tile) -> bool {
        let camp = plan::camp(tile);
        self.hubs.values().all(|held| !plan::touch(&held.hub, &camp))
    }

    /// A hearth was laid on `tile`: the camp round it is a hub, known by
    /// that tile. Nothing where a hub is known by it already.
    pub fn founded(&mut self, tile: Tile) -> HubId {
        let id = HubId(tile.0, tile.1);
        if self.of_hearth(tile).is_none() {
            let held = self.hold(&After { id, hub: plan::camp(tile), hired: BTreeSet::new(), walled: false, took: Vec::new() });
            self.hubs.insert(id, held);
            self.changed = true;
        }
        self.of_hearth(tile).expect("a hub just founded is known by its tile")
    }

    /// Hub `id` is no more: its hearth came down. The players at home in
    /// it are at home at the haven.
    pub fn razed(&mut self, id: HubId) {
        let Some(held) = self.hubs.remove(&id) else { return };
        let homes: HashSet<Tile> = held.hub.items.iter().flat_map(|item| item.tiles.iter().copied()).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.changed = true;
    }

    /// The hub of the home on `tile`, where one stands there with room
    /// for another player: a free room, or a settler in it who has room
    /// in another home to go to.
    pub fn open(&self, tile: Tile) -> Option<HubId> {
        self.hubs.iter().find(|(_, held)| held.hub.on(tile).is_some_and(|item| plan::lodge(&held.hub, item.id, false).is_some())).map(|(&id, _)| id)
    }

    /// `who` asks to live in the home on `tile`: it does from now where
    /// that is its home already or has room for it, and whether it does.
    /// Refused, it lives where it did.
    pub fn claim(&mut self, who: Identity, tile: Tile) -> bool {
        let lives = self.home_of(who) == Some(tile) || self.open(tile).is_some();
        if lives {
            self.resident(who, Some(tile));
        }
        lives && self.home_of(who) == Some(tile)
    }

    /// `who` played of late, living in the home on `home`, or in none. A
    /// home that stands no more, or has no room, is none.
    pub fn resident(&mut self, who: Identity, home: Option<Tile>) {
        let mine = |at: Tile| self.residents.get(&who) == Some(&Some(at)) && self.homed(at);
        let home = home.filter(|&at| mine(at) || self.open(at).is_some());
        if self.residents.insert(who, home) != Some(home) {
            self.people();
        }
    }

    /// `who` has not played of late, and lives in no hub's home.
    pub fn gone(&mut self, who: Identity) {
        if self.residents.remove(&who).flatten().is_some() {
            self.people();
        }
    }

    /// The home `who` lives in, where it lives in one.
    pub fn home_of(&self, who: Identity) -> Option<Tile> {
        self.residents.get(&who).copied().flatten()
    }

    /// Makes every home lived in by the players at home in it, and its
    /// hub's settlers as many as it takes and has room for.
    fn people(&mut self) {
        let mut homes: HashMap<Tile, u32> = HashMap::new();
        for &home in self.residents.values().flatten() {
            *homes.entry(home).or_default() += 1;
        }
        for held in self.hubs.values_mut() {
            for item in &mut held.hub.items {
                let living: u32 = item.tiles.iter().filter_map(|tile| homes.get(tile)).sum();
                item.players = living.min(item.kind.houses());
                item.settlers = item.settlers.min(item.kind.houses() - item.players);
            }
            held.hub = std::mem::replace(&mut held.hub, plan::camp((0, 0))).peopled();
        }
    }

    /// Every hub in the order of its name, so a plan reads the same hubs
    /// the same way on every server and every run.
    fn ordered(&self) -> Vec<HubId> {
        let mut ids: Vec<HubId> = self.hubs.keys().copied().collect();
        ids.sort();
        ids
    }

    /// What `after` changes of hub `id`'s things and of every wall, the
    /// hubs in `taken` gone.
    fn plan(&self, id: HubId, after: Vec<After>, taken: Vec<HubId>) -> Plan {
        let was: Vec<&Held> = std::iter::once(id).chain(taken.iter().copied()).filter_map(|id| self.hubs.get(&id)).collect();
        let before: HashSet<Thing> = was.iter().flat_map(|held| held.hub.items.iter().map(Thing::of)).collect();
        let since: HashSet<Thing> = after.iter().flat_map(|after| after.hub.items.iter().map(Thing::of)).collect();
        let stood: HashSet<Tile> = was.iter().flat_map(|held| Self::staked(held)).collect();
        let laid: Vec<Held> = after.iter().map(|after| self.hold(after)).collect();
        let stands: HashSet<Tile> = laid.iter().flat_map(Self::staked).collect();
        let trod: HashSet<Tile> = was.iter().flat_map(|held| held.path.iter().copied()).collect();
        let treads: HashSet<Tile> = laid.iter().flat_map(|held| held.path.iter().copied()).collect();
        let sorted = |mut tiles: Vec<Tile>| {
            tiles.sort();
            tiles
        };
        let by_tile = |mut things: Vec<Thing>| {
            things.sort_by_key(|thing| thing.at);
            things
        };
        Plan {
            hub: id,
            built: by_tile(since.difference(&before).cloned().collect()),
            gone: by_tile(before.difference(&since).cloned().collect()),
            raised: sorted(stands.difference(&stood).copied().collect()),
            pulled: sorted(stood.difference(&stands).copied().collect()),
            worn: sorted(treads.difference(&trod).copied().collect()),
            faded: sorted(trod.difference(&treads).copied().collect()),
            after,
            taken,
        }
    }

    /// Hub `id` with `hub` in place of what it is, as a plan leaves it.
    fn after(&self, id: HubId, hub: plan::Hub) -> Option<After> {
        let held = self.hubs.get(&id)?;
        Some(After { id, hub, hired: held.hired.iter().copied().filter(|zone| *zone == Zone::Logistics || held.hub.wants.max(1) >= 1).collect(), walled: held.walled, took: held.took.clone() })
    }

    /// The plan that leaves hub `id` as `outcome` does, taking in every
    /// hub its ground then touches: the hub taken in is no more, what
    /// stood in it stands in this one, its hearth a home from then, and
    /// its planners work for this one.
    fn planned_as(&self, id: HubId, outcome: plan::Outcome) -> Option<Plan> {
        let mut after = self.after(id, outcome.hub)?;
        let mut taken: Vec<HubId> = Vec::new();
        loop {
            let near = self.ordered().into_iter().find(|other| *other != id && !taken.contains(other) && plan::touch(&after.hub, &self.hubs[other].hub));
            let Some(other) = near else { break };
            let theirs = &self.hubs[&other];
            for item in &theirs.hub.items {
                let kind = if item.kind.what == What::Hearth { Kind::first(What::Home) } else { item.kind };
                after.hub.items.push(Item { id: after.hub.next, kind, tiles: item.tiles.clone(), players: 0, settlers: 0 });
                after.hub.next += 1;
            }
            after.hub.open.extend(theirs.hub.open.iter().copied());
            after.hired.extend(theirs.hired.iter().copied());
            after.walled |= theirs.walled;
            after.took.push(other);
            after.took.extend(theirs.took.iter().copied());
            taken.push(other);
        }
        after.hub = after.hub.peopled();
        Some(self.plan(id, vec![after], taken))
    }

    /// What planning hub `id` for `want` residents would do: ground
    /// claimed for more, or the hub drawn in for fewer. None where more
    /// live in it than `want`.
    pub fn planned(&self, id: HubId, want: u32) -> Option<Plan> {
        self.planned_as(id, plan::plan(self.hub(id)?, want)?)
    }

    /// What raising a `what` in hub `id` would do, where its zone's
    /// planner puts it.
    pub fn raised(&self, id: HubId, what: What) -> Option<Plan> {
        self.planned_as(id, plan::raise(self.hub(id)?, what)?)
    }

    /// What raising a `what` on `tile` of hub `id` would do, where a
    /// player puts it.
    pub fn raised_at(&self, id: HubId, what: What, tile: Tile) -> Option<Plan> {
        self.planned_as(id, plan::raise_at(self.hub(id)?, what, tile)?)
    }

    /// Tells hub `id` how its ground lies: the tiles nothing is planned
    /// on, each pair of tiles side by side with a cliff between them, the
    /// lesser first, and how high each tile looked at lies, in levels. Its
    /// planners plan round the first two from then, and a wall's way in is
    /// placed by the last. It is no plan and changes nothing that stands;
    /// the ground is the world's to say, and is said again before
    /// anything is planned.
    pub fn survey(&mut self, id: HubId, barred: BTreeSet<Tile>, cliffs: BTreeSet<(Tile, Tile)>, levels: impl IntoIterator<Item = (Tile, i32)>) {
        if let Some(held) = self.hubs.get_mut(&id) {
            (held.hub.barred, held.hub.cliffs) = (barred, cliffs);
            self.levels.extend(levels);
        }
    }

    /// The tiles of hub `id` nothing is planned on, as it was last told.
    pub fn barred(&self, id: HubId) -> Vec<Tile> {
        self.hub(id).map_or(Vec::new(), |hub| hub.barred.iter().copied().collect())
    }

    /// The tiles of hub `id` a player may raise a `what` on.
    pub fn sites(&self, id: HubId, what: What) -> Vec<Tile> {
        self.hub(id).map_or(Vec::new(), |hub| plan::sites(hub, what))
    }

    /// What upgrading the thing on `tile` of hub `id` would do.
    pub fn upgraded(&self, id: HubId, tile: Tile) -> Option<Plan> {
        let hub = self.hub(id)?;
        self.planned_as(id, plan::upgrade(hub, hub.on(tile)?.id)?)
    }

    /// What downgrading the thing on `tile` of hub `id` would do.
    pub fn downgraded(&self, id: HubId, tile: Tile) -> Option<Plan> {
        let hub = self.hub(id)?;
        self.planned_as(id, plan::downgrade(hub, hub.on(tile)?.id)?)
    }

    /// What centring `zone` of hub `id` on `tile` would do: nothing
    /// standing changes. None for a zone with no planner.
    pub fn zoned(&self, id: HubId, zone: Zone, tile: Tile) -> Option<Plan> {
        let held = self.hubs.get(&id)?;
        if !held.hired.contains(&zone) {
            return None;
        }
        self.planned_as(id, plan::zone(&held.hub, zone, tile)?)
    }

    /// What raising hub `id`'s wall would do: stakes on every tile of it
    /// but the way in. None where it is to be raised already, or the hub
    /// has no logistics planner.
    pub fn walling(&self, id: HubId) -> Option<Plan> {
        let held = self.hubs.get(&id)?;
        if held.walled || !held.hired.contains(&Zone::Logistics) {
            return None;
        }
        let after = After { walled: true, ..self.after(id, held.hub.clone())? };
        Some(self.plan(id, vec![after], Vec::new()))
    }

    /// What shrinking hub `id` to what its residents defend would do, or
    /// none where they defend its wall already. The part that keeps the
    /// hub's hearth, or else the first, keeps its name and its planners;
    /// each other is known by the tile its hearth is laid on.
    pub fn shrunk(&self, id: HubId) -> Option<Plan> {
        let held = self.hubs.get(&id)?;
        let mut hub = held.hub.clone();
        // Counted as the hub's people are: the fixture's players live in
        // homes with room for them
        let mut extra = self.extra;
        for item in hub.items.iter_mut() {
            let room = item.kind.houses().saturating_sub(item.players + item.settlers).min(extra);
            item.players += room;
            extra -= room;
        }
        let parts = plan::shrink(&hub.peopled())?;
        let hearth = self.hearth(id);
        let keeps = parts.iter().position(|part| part.hub.hearth() == hearth).unwrap_or(0);
        let after = parts
            .into_iter()
            .enumerate()
            .map(|(i, part)| {
                let at = part.hub.hearth().map_or(id, |tile| HubId(tile.0, tile.1));
                if i == keeps {
                    After { id, hub: part.hub, hired: held.hired.clone(), walled: held.walled, took: held.took.clone() }
                } else {
                    After { id: at, hub: part.hub, hired: BTreeSet::new(), walled: held.walled, took: Vec::new() }
                }
            })
            .collect();
        Some(self.plan(id, after, Vec::new()))
    }

    /// Does what `plan` said: its hubs are as it leaves them, and those it
    /// took in are no more. A zone a hub no longer has keeps no planner.
    pub fn carry(&mut self, plan: &Plan) {
        // A hub keeps how far the country has given way to it, and one a
        // plan makes of others starts as far along as the furthest of them
        let pushed = |id: &HubId| self.hubs.get(id).and_then(|held| Some((held.since?, held.given)));
        let eldest = plan.taken.iter().chain(plan.after.iter().map(|after| &after.id)).filter_map(pushed).min_by_key(|&(since, _)| since);
        let before: Vec<Option<(When, f32)>> = plan.after.iter().map(|after| pushed(&after.id).or(eldest)).collect();
        for id in &plan.taken {
            self.hubs.remove(id);
        }
        for (after, pushed) in plan.after.iter().zip(before) {
            let mut held = self.hold(after);
            held.hired.retain(|zone| *zone == Zone::Logistics || held.hub.zones.contains_key(zone));
            if let Some((since, given)) = pushed {
                (held.since, held.given) = (Some(since), given);
            }
            self.hubs.insert(after.id, held);
        }
        // A home given up is no one's home
        let homes: Vec<Tile> = self.residents.values().flatten().copied().filter(|&at| !self.homed(at)).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.people();
        self.changed = true;
        info!("hubs: {:?} planned: {} things raised, {} taken down, {} stakes raised, {} pulled, {} hubs taken in", plan.hub, plan.built.len(), plan.gone.len(), plan.raised.len(), plan.pulled.len(), plan.taken.len());
    }

    /// Counts every hub as `players` living in it besides those who do.
    pub fn count_extra(&mut self, players: u32) {
        self.extra = players;
    }

    /// Whether any hub changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Every hub as a store keeps it, in the order of its name.
    pub fn kept(&self) -> Vec<Kept> {
        self.ordered()
            .into_iter()
            .map(|id| {
                let held = &self.hubs[&id];
                Kept { gate: held.gate, path: held.path.clone(), since: held.since.map(|since| since.0), ..Kept::of((id.0, id.1), &held.hub, &held.hired, held.walled, &held.took) }
            })
            .collect()
    }

    /// Lays the hubs kept from an earlier run, and the recent players'
    /// homes. A hub's way in is where it was kept, a tile of its wall: no
    /// survey has told how the ground lies yet, and one placed anew now
    /// would stand where the stakes do.
    pub fn recall(&mut self, hubs: Vec<Kept>, residents: Vec<(Identity, Option<Tile>)>) {
        for kept in hubs {
            let (hub, hired, walled) = kept.laid();
            let id = HubId(kept.id.0, kept.id.1);
            let mut held = self.hold(&After { id, hub, hired, walled, took: kept.took.iter().map(|&(q, r)| HubId(q, r)).collect() });
            if let Some(gate) = kept.gate.filter(|gate| held.ground.wall.contains(gate)) {
                (held.gate, held.path) = (Some(gate), kept.path.clone());
            }
            held.since = kept.since.map(When);
            self.hubs.insert(id, held);
        }
        for (who, home) in residents {
            self.residents.insert(who, home);
        }
        // A home that stands no more is no one's
        let homes: Vec<Tile> = self.residents.values().flatten().copied().filter(|&at| !self.homed(at)).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.people();
    }

    /// Each hub with the tile its hearth is laid on and how many players
    /// live in it.
    pub fn each(&self) -> impl Iterator<Item = (HubId, Tile, u32)> + '_ {
        self.ordered().into_iter().filter_map(|id| Some((id, self.hearth(id)?, self.size(id))))
    }

    /// Where the haven is.
    pub fn haven(&self) -> Tile {
        self.haven
    }

    /// Whether the haven's guard keeps `tile`.
    pub fn guarded(&self, tile: Tile) -> bool {
        distance(self.haven, tile) <= HAVEN_GROUND
    }

    /// How far `tile` lies outside hub `id`'s wall, in tiles: none on its
    /// ground.
    pub fn outside(&self, id: HubId, tile: Tile) -> Option<f32> {
        let ground = self.ground(id)?;
        if ground.tiles.binary_search(&tile).is_ok() {
            return Some(0.0);
        }
        ground.wall.iter().map(|&at| distance(at, tile)).min_by(f32::total_cmp)
    }

    /// How far past its quiet ground hub `id`'s influence carries, in
    /// tiles.
    pub fn reach(&self, id: HubId) -> f32 {
        self.ground(id).map_or(0.0, |ground| INFLUENCE_WALLS * ground.wall.len() as f32)
    }

    /// Hub `id`'s influence over its ground, 0 to 1: the square root of
    /// its players as a share of [`INFLUENCE_WHOLE`].
    pub fn strength(&self, id: HubId) -> f32 {
        ((self.size(id).max(1) as f32) / INFLUENCE_WHOLE).sqrt().min(1.0)
    }

    /// Whether hub `id`'s influence reaches `tile`.
    pub fn reaches(&self, id: HubId, tile: Tile) -> bool {
        self.outside(id, tile).is_some_and(|outside| outside < QUIET + self.reach(id))
    }

    /// The level of the country at `tile` with no hub in the world: its
    /// distance from the haven, a level each [`LEVEL_TILES`], with no top.
    pub fn wild(&self, tile: Tile) -> f32 {
        distance(self.haven, tile) / LEVEL_TILES
    }

    /// Ages every hub to the calendar's `now`, the country giving way a
    /// level each `hours`: a hub's push is given way to in full once the
    /// levels its hearth stood at have passed, from when it was first
    /// aged. Whatever reads [`Hubs::influence`] reads it as last aged.
    pub fn age(&mut self, now: When, hours: i32) {
        let level = std::time::Duration::from_secs(3600 * hours.max(1) as u64).as_secs_f32();
        let wild: HashMap<HubId, f32> = self.hubs.iter().map(|(&id, held)| (id, self.wild(held.hub.hearth().unwrap_or((id.0, id.1))))).collect();
        for (id, held) in self.hubs.iter_mut() {
            self.changed |= held.since.is_none();
            let passed = now.since(*held.since.get_or_insert(now)).as_secs_f32() / level;
            held.given = if passed >= wild[id] { 1.0 } else { passed / wild[id] };
        }
    }

    /// How much of its push the country has given way to round hub `id`,
    /// 0 to 1, as it was last aged.
    pub fn given(&self, id: HubId) -> f32 {
        self.hubs.get(&id).map_or(0.0, |held| held.given)
    }

    /// The influence every hub has on `tile` together, 0 to 1: how far
    /// players have pushed the country back there. A hub's is whole over
    /// its ground and [`QUIET`] past its wall; further out it is its
    /// strength, falling as the cube of what is left of the way to its
    /// reach. Each counts for as much of that as the country has given
    /// way to it.
    pub fn influence(&self, tile: Tile) -> f32 {
        let felt = |strength: f32, outside: f32, reach: f32| match outside <= QUIET {
            true => 1.0,
            false => strength * (1.0 - ((outside - QUIET) / reach.max(1.0)).clamp(0.0, 1.0)).powi(3),
        };
        self.hubs.iter().filter_map(|(&id, held)| Some(held.given * felt(self.strength(id), self.outside(id, tile)?, self.reach(id)))).sum::<f32>().min(1.0)
    }

    /// The level a nest's creatures are at on `tile`: the country's own
    /// there, less the share of it the hubs have pushed back.
    pub fn level(&self, tile: Tile) -> u8 {
        (self.wild(tile) * (1.0 - self.influence(tile))).clamp(0.0, u8::MAX as f32) as u8
    }

    /// Whether no nest stands on `tile`: the haven's guard keeps it, a hub
    /// holds it as its ground, or it lies within [`QUIET`] of a wall the
    /// country has given way to in full.
    pub fn quiet(&self, tile: Tile) -> bool {
        self.guarded(tile)
            || self.hubs.iter().any(|(&id, held)| self.outside(id, tile).is_some_and(|outside| outside <= 0.0 || (held.given >= 1.0 && outside <= QUIET)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(n: u8) -> Identity {
        Identity::Key([n; 16])
    }

    fn carried(hubs: &mut Hubs, plan: Option<Plan>) -> Plan {
        let plan = plan.expect("the plan is one the hub takes");
        hubs.carry(&plan);
        plan
    }

    /// A hub with its post and a tent, its first settler hired as the
    /// logistics planner.
    fn outpost(hubs: &mut Hubs, at: Tile) -> HubId {
        let id = hubs.founded(at);
        for what in [What::Post, What::Home] {
            let tile = hubs.sites(id, what)[0];
            let plan = hubs.raised_at(id, what, tile);
            carried(hubs, plan);
        }
        assert!(hubs.hire(id, Zone::Logistics));
        id
    }

    /// A hearth founds a camp, a hub known by its tile that holds the
    /// ground round it, with its hearth and nothing else; none is laid on
    /// a hub's ground or against it, and one beside the haven is a hub
    /// like any other.
    #[test]
    fn a_hearth_founds_a_hub() {
        let mut hubs = Hubs::new((0, 0));
        assert!(hubs.founds((201, 0)));
        let camp = hubs.founded((201, 0));
        assert_eq!((camp, hubs.hearth(camp), hubs.items(camp).len(), hubs.size(camp), hubs.settlers(camp)), (HubId(201, 0), Some((201, 0)), 1, 0, 0));
        assert_eq!(hubs.holding((201, 0)), Some(camp));
        assert_eq!(hubs.holding((230, 0)), None);
        assert!(!hubs.founds((204, 0)) && !hubs.founds((210, 0)) && hubs.founds((240, 0)));
        assert_eq!(hubs.founded((9, 0)), HubId(9, 0));
        assert!(hubs.guarded((9, 0)) && !hubs.guarded((201, 0)));
        assert!(hubs.take_changed() && !hubs.take_changed());
    }

    /// No settler comes to a bare hearth. One comes for the hitching post
    /// once a tent stands for it, and only then is a logistics planner
    /// hired; a hub has no stakes before that planner is asked for its
    /// wall, and all but the way in after.
    #[test]
    fn a_hub_takes_a_planner_once_a_settler_has_a_home() {
        let mut hubs = Hubs::new((0, 0));
        let id = hubs.founded((300, 0));
        assert!(!hubs.hire(id, Zone::Logistics) && hubs.walling(id).is_none());
        let post = hubs.sites(id, What::Post)[0];
        let plan = hubs.raised_at(id, What::Post, post);
        assert_eq!(carried(&mut hubs, plan).built.len(), 1);
        assert!(hubs.raised_at(id, What::Home, (post.0 + 1, post.1)).is_none(), "a thing stands apart from the next");
        assert!(!hubs.hire(id, Zone::Logistics), "a settler with no home is no planner");
        let tent = hubs.sites(id, What::Home)[0];
        let plan = hubs.raised_at(id, What::Home, tent);
        carried(&mut hubs, plan);
        assert_eq!(hubs.settlers(id), 1);
        assert!(hubs.hire(id, Zone::Logistics) && !hubs.hire(id, Zone::Logistics));
        assert!(!hubs.hire(id, Zone::Residential), "one settler is one planner");
        assert!(hubs.stakes(id).is_empty());
        let wall = { let plan = hubs.walling(id); carried(&mut hubs, plan) };
        assert_eq!(hubs.stakes(id), wall.raised);
        assert_eq!(hubs.stakes(id).len() + 1, hubs.ground(id).unwrap().wall.len(), "the way in is left open");
    }

    /// A player is at home in a home it claimed, and a settler comes with
    /// it where it has room; a home with no room is not claimed, and one
    /// whose hub is razed leaves its player at home at the haven.
    #[test]
    fn players_live_in_the_homes_they_claim() {
        let mut hubs = Hubs::new((0, 0));
        let id = outpost(&mut hubs, (300, 0));
        let plan = hubs.planned(id, 4);
        carried(&mut hubs, plan);
        for _ in 0..3 {
            let plan = hubs.raised(id, What::Home);
            carried(&mut hubs, plan);
        }
        let home = hubs.items(id).iter().find(|item| item.kind.what == What::Home && item.settlers == 0).unwrap().tiles[0];
        assert_eq!(hubs.open(home), Some(id));
        hubs.resident(player(1), Some(home));
        assert_eq!((hubs.home_of(player(1)), hubs.size(id), hubs.settlers(id)), (Some(home), 1, 2));
        assert!(hubs.hire(id, Zone::Residential), "the second settler is the second planner");
        hubs.resident(player(2), Some(home));
        assert_eq!(hubs.home_of(player(2)), None, "the home is full");
        let settled = (hubs.size(id), hubs.settlers(id), hubs.items(id).to_vec());
        assert!(!hubs.claim(player(2), home) && hubs.claim(player(1), home), "a full home refuses another, and is its own player's still");
        assert!((hubs.size(id), hubs.settlers(id), hubs.items(id).to_vec()) == settled, "a claim refused moves no one");
        hubs.gone(player(1));
        assert_eq!((hubs.size(id), hubs.settlers(id)), (0, 1));
        hubs.resident(player(1), Some(home));
        let hearth = hubs.hearth(id).unwrap();
        hubs.resident(player(3), Some(hearth));
        assert_eq!((hubs.home_of(player(3)), hubs.size(id)), (None, 1), "no one lives at a hearth");
        assert_eq!((hubs.returns(hearth), hubs.returns(home), hubs.returns((9000, 9000))), (Some(hearth), Some(hearth), None), "a hearth is returned to, and its hub's from anywhere on its ground");
        hubs.razed(id);
        assert_eq!((hubs.home_of(player(1)), hubs.returns(hearth), hubs.each().count()), (None, None, 0));
    }

    /// A plan says what it would do and changes nothing until it is
    /// carried: planned for more, a walled hub raises stakes round the
    /// ground it claims and pulls those left inside, and builds nothing.
    #[test]
    fn a_plan_changes_nothing_until_it_is_carried() {
        let mut hubs = Hubs::new((0, 0));
        let id = outpost(&mut hubs, (300, 0));
        { let plan = hubs.walling(id); carried(&mut hubs, plan) };
        let (wall, items) = (hubs.stakes(id), hubs.items(id).to_vec());
        let plan = hubs.planned(id, 12).expect("a hub is planned for more however few live in it");
        assert_eq!((hubs.wants(id), hubs.stakes(id)), (plan::CAMP, wall.clone()));
        assert!(plan.built.is_empty() && plan.gone.is_empty() && plan.taken.is_empty());
        assert!(!plan.raised.is_empty() && plan.pulled.iter().all(|tile| wall.contains(tile)));
        hubs.take_changed();
        hubs.carry(&plan);
        assert_eq!((hubs.wants(id), hubs.items(id)), (12, &items[..]));
        let now = hubs.stakes(id);
        assert!(plan.raised.iter().all(|tile| now.contains(tile)) && plan.pulled.iter().all(|tile| !now.contains(tile)));
        assert!(hubs.take_changed());
    }

    /// A hub whose ground comes to touch another's takes it in: it
    /// answers to the other's name, and the other's hearth is a home.
    #[test]
    fn a_hub_that_grows_to_another_takes_it_in() {
        let mut hubs = Hubs::new((0, 0));
        let (town, camp) = (hubs.founded((300, 0)), hubs.founded((318, 0)));
        let near = hubs.planned(town, 4).unwrap();
        assert!(near.taken.is_empty(), "planned for four, it has not reached the camp");
        hubs.carry(&near);
        let mut want = 4;
        let taking = loop {
            want += 4;
            let plan = hubs.planned(town, want).unwrap();
            if !plan.taken.is_empty() {
                break plan;
            }
            hubs.carry(&plan);
        };
        assert_eq!(taking.taken, vec![camp]);
        hubs.carry(&taking);
        assert_eq!((hubs.each().count(), hubs.of_hearth((318, 0)), hubs.holding((318, 0))), (1, Some(town), Some(town)));
        assert_eq!(hubs.returns((318, 0)), hubs.hearth(town), "one who returned to the hearth taken in returns to the hub's");
        assert_eq!(hubs.standing(town, What::Hearth).count(), 1);
        assert_eq!(hubs.item_on(town, (318, 0)).map(|item| item.kind.what), Some(What::Home));
    }

    /// A shrink is to what a hub's residents defend: nothing where they
    /// defend its wall, and otherwise the ground it gives up, every player
    /// still at home.
    #[test]
    fn a_hub_shrinks_to_what_its_residents_defend() {
        let mut hubs = Hubs::new((0, 0));
        let town = outpost(&mut hubs, (300, 0));
        assert_eq!(hubs.shrunk(town), None, "no player lives there to draw it in round");
        { let plan = hubs.planned(town, 40); carried(&mut hubs, plan) };
        { let plan = hubs.walling(town); carried(&mut hubs, plan) };
        for _ in 0..3 {
            let plan = hubs.raised(town, What::Home);
            carried(&mut hubs, plan);
        }
        let home = hubs.items(town).iter().find(|item| item.kind.what == What::Home && item.settlers == 0).unwrap().tiles[0];
        hubs.resident(player(1), Some(home));
        let plan = hubs.shrunk(town).expect("one player and its settlers do not defend a wall planned for forty");
        assert!(!plan.pulled.is_empty() && plan.built.is_empty());
        hubs.carry(&plan);
        assert_eq!(hubs.home_of(player(1)), Some(home));
        assert!(hubs.wants(town) < 40);
        assert_eq!(hubs.shrunk(town), None, "what is left, they defend");
    }

    /// What is kept of the hubs lays them again as they were. A hub kept
    /// as build sites is laid as one-tile things on those sites, with the
    /// planners and the wall every hub had then, and a hub kept as its
    /// hearths is a camp round the first that answers to the others.
    #[test]
    fn hubs_are_recalled_as_they_were_kept() {
        let mut hubs = Hubs::new((0, 0));
        let town = outpost(&mut hubs, (300, 0));
        hubs.founded((600, 0));
        { let plan = hubs.planned(town, 12); carried(&mut hubs, plan) };
        { let plan = hubs.walling(town); carried(&mut hubs, plan) };
        let hearth = hubs.hearth(town).unwrap();
        let mut back = Hubs::new((0, 0));
        back.recall(hubs.kept(), vec![(player(1), Some(hearth)), (player(2), Some((9, 9)))]);
        assert!(back.hubs == hubs.hubs, "the hubs recalled are the hubs kept");
        assert_eq!((back.home_of(player(1)), back.home_of(player(2))), (None, None), "a hearth, and a home that stands no more, are no one's home");

        let sites = Kept { id: (300, 0), sites: vec![(300, 0, 1), (303, 0, 2), (297, 0, 3), (300, 3, 4), (303, -3, 4), (300, -3, 5)], took: vec![(320, 0)], ..Default::default() };
        let mut laid = Hubs::new((0, 0));
        laid.recall(vec![sites], Vec::new());
        let id = HubId(300, 0);
        assert_eq!((laid.hearth(id), laid.of_hearth((320, 0)), laid.items(id).len()), (Some((300, 0)), Some(id), 6));
        assert_eq!(laid.standing(id, What::Store).next(), Some((300, -3)));
        assert!(laid.hired(id).any(|zone| zone == Zone::Logistics) && !laid.stakes(id).is_empty());

        let old = Kept::of_hearths(&[(301, 2), (320, 0)]).unwrap();
        let mut converted = Hubs::new((0, 0));
        converted.recall(vec![old], Vec::new());
        let id = HubId(301, 2);
        assert_eq!((converted.of_hearth((301, 2)), converted.of_hearth((320, 0)), converted.hearth(id)), (Some(id), Some(id), Some((301, 2))));
    }

    /// A wall's way in is the tile of it nearest the hearth that leaves
    /// the hub, and of those as near the one on the gentlest ground.
    #[test]
    fn the_way_in_is_on_the_gentlest_ground() {
        let flat = Hubs::new((0, 0));
        let mut on_flat = flat;
        let id = on_flat.founded((300, 0));
        let gate = on_flat.way_in(id).expect("a camp has a way in");
        assert!(on_flat.ground(id).unwrap().wall.contains(&gate));
        assert_eq!(on_flat.path(id).last(), Some(&gate));
        // The same camp surveyed with that tile on a crag: the way in a plan leaves is elsewhere
        let ground = on_flat.ground(id).unwrap().tiles.clone();
        on_flat.survey(id, BTreeSet::new(), BTreeSet::new(), ground.into_iter().map(|tile| (tile, if tile == gate { 9 } else { 0 })));
        let plan = on_flat.planned(id, plan::CAMP);
        carried(&mut on_flat, plan);
        assert!(on_flat.ground(id).unwrap().wall.contains(&gate), "the wall stands where it did");
        assert_ne!(on_flat.way_in(id), Some(gate));
        // Kept and laid again with no survey, its way in is where it was
        let (kept, way) = (on_flat.kept(), on_flat.way_in(id));
        let mut again = Hubs::new((0, 0));
        again.recall(kept, Vec::new());
        assert_eq!(again.way_in(id), way);
        assert_eq!(again.path(id), on_flat.path(id));
    }

    /// The country is a level deeper each stretch from the haven with no
    /// top, a hub pushes none of it back the day it is founded, and the
    /// country gives way to it a level a day until the ground round its
    /// wall is quiet. Its influence reaches further the longer its wall.
    #[test]
    fn the_country_gives_way_to_a_hub_a_level_a_day() {
        let day = std::time::Duration::from_secs(86_400);
        let mut hubs = Hubs::new((0, 0));
        let far = (LEVEL_TILES as i32 * 20, 0);
        let (beside, past) = ((far.0 + 30, far.1), (far.0 + 300, far.1));
        assert_eq!((hubs.level((0, 0)), hubs.level(far), hubs.level((LEVEL_TILES as i32 * 200, 0))), (0, 20, 200));
        let id = hubs.founded(far);
        let founded = When(1_000);
        hubs.age(founded, 24);
        assert_eq!((hubs.influence(beside), hubs.level(beside)), (0.0, 20), "the day it is founded, nothing has given way");
        assert!(hubs.quiet(far) && !hubs.quiet(beside), "its ground holds no nest; the ground round it does yet");
        let levels: Vec<u8> = (0..=20u32).map(|days| {
            hubs.age(founded + day * days, 24);
            hubs.level(beside)
        }).collect();
        assert!(levels.windows(2).all(|pair| pair[1] <= pair[0] && pair[0] - pair[1] <= 1), "never faster than a level a day: {levels:?}");
        assert_eq!(levels[20], 0);
        assert!(hubs.quiet(beside), "then the ground round its wall is quiet");
        assert!(!hubs.quiet(past) && hubs.level(past) > 0, "and past it the country is only pushed back");
        let reach = hubs.reach(id);
        { let plan = hubs.planned(id, 40); carried(&mut hubs, plan) };
        assert!(hubs.reach(id) > reach);
        assert_eq!(hubs.given(id), 1.0, "a plan leaves what the country has given way to as it was");
        assert!(hubs.reaches(id, (far.0 + 20, far.1)) && !hubs.reaches(id, (far.0 + 3000, far.1)));
        // Kept and laid again, it began when it did
        let mut again = Hubs::new((0, 0));
        again.recall(hubs.kept(), Vec::new());
        again.age(founded + day * 20, 24);
        assert_eq!(again.given(id), 1.0);
    }
}
