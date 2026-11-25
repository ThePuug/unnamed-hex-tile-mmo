//! Hubs: the places players live, as a world holds them. A hub is its
//! build sites and the wall round them, by the rules of [`crate::sites`];
//! this holds every hub there is, where every player known to have
//! played of late is at home, and what each plan would change.
//!
//! A player is at home in a home it claimed, or at a hub's hearth, which
//! anyone may make its home whatever the hub has open: one at home at a
//! hearth returns there and lives in none of the hub's homes, so it is
//! not of the hub's size.
//!
//! Nothing moves a hub but a plan: [`Hubs::grown`] and [`Hubs::shrunk`]
//! say what one would do, and [`Hubs::carry`] does it. Players coming and
//! going change who lives in a hub's homes, and so what its people hold,
//! and never its sites or its wall.
//!
//! The haven is no hub. It is the point the world names, where a player
//! who has claimed no home is at home: it has no size, takes nothing in
//! and is taken in by nothing. Its guard keeps the ground round it, and
//! its influence is whole, so the country round it is the mildest there
//! is.
//!
//! Hubs are few and known whole, wherever their ground is. How they are
//! kept past the server is not this module's to know. The server holds
//! them, and the world viewer plans a scenario by these same rules to
//! draw it.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use common::network::Identity;
use qrz::Qrz;

use crate::sites::{self, Ground, Lived, Site, Standing};

/// How far round its point the haven's guard keeps the ground, in tiles.
pub const HAVEN_GROUND: f32 = 76.0;

/// How far past its guarded ground the haven's influence carries, in
/// tiles.
pub const HAVEN_REACH: f32 = 1900.0;

/// How far past its wall a hub's influence carries, in lengths of that
/// wall.
pub const INFLUENCE_WALLS: f32 = 1.5;

/// The players at home in a hub whose influence is whole over its ground.
pub const INFLUENCE_WHOLE: f32 = 1000.0;

/// Which hub: the tile it was founded on. It is the hub's for as long as
/// the hub is, wherever a plan lays its hearth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HubId(pub i32, pub i32);

/// A hub as it is held: its sites, the ground they make, the way in
/// through its wall, and the hubs it took in, whose names are its own
/// from then.
#[derive(Clone, Debug, PartialEq)]
struct Held {
    hub: sites::Hub,
    ground: Ground,
    gate: Option<(i32, i32)>,
    /// The path from its hearth to its way in.
    path: Vec<(i32, i32)>,
    took: Vec<HubId>,
}

/// How high the ground lies on a tile, in levels: what a wall's way in
/// is placed by.
#[derive(Clone)]
pub struct Lie(std::sync::Arc<dyn Fn(i32, i32) -> i32 + Send + Sync>);

impl std::fmt::Debug for Lie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Lie")
    }
}

/// A number from a hub and a tile, the same on every run and spread
/// evenly: what picks one tile of several as good.
fn drawn(id: HubId, tile: (i32, i32)) -> u64 {
    let mut x = [id.0, id.1, tile.0, tile.1].into_iter().fold(0x9E37_79B9_7F4A_7C15u64, |x, n| (x ^ n as u32 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9).rotate_left(31));
    x ^= x >> 29;
    x.wrapping_mul(0x94D0_49BB_1331_11EB)
}

/// A hub as a store keeps it: the tile it is known by, each site with
/// the number of what stands on it, and the hubs it took in.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub id: (i32, i32),
    pub sites: Vec<(i32, i32, u32)>,
    pub took: Vec<(i32, i32)>,
}

impl Kept {
    /// A hub kept as its hearths, before hubs had sites: a camp round
    /// the first, known by it, the others names it answers to.
    pub fn of_hearths(hearths: &[(i32, i32)]) -> Option<Kept> {
        let (&id, took) = hearths.split_first()?;
        let camp = sites::camp(sites::site_near(id));
        Some(Kept { id, sites: camp.sites.iter().map(|site| (site.at.0, site.at.1, site.what.id())).collect(), took: took.to_vec() })
    }
}

/// What a plan would do to a hub, and to the hubs round it.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The hub planned for.
    pub hub: HubId,
    /// Every hub the plan changes or makes, as it leaves it.
    after: Vec<(HubId, sites::Hub, Vec<HubId>)>,
    /// The hubs it takes in, which are no more.
    pub taken: Vec<HubId>,
    /// The sites it builds, and those it gives up.
    pub built: Vec<Site>,
    pub gone: Vec<Site>,
    /// The tiles it raises stakes on, and those it pulls them from.
    pub raised: Vec<(i32, i32)>,
    pub pulled: Vec<(i32, i32)>,
    /// The tiles its paths come to cross, and those they cross no more.
    pub worn: Vec<(i32, i32)>,
    pub faded: Vec<(i32, i32)>,
}

impl Plan {
    /// The hubs the plan parts off, each known by the tile its hearth is
    /// laid on.
    pub fn parted(&self) -> impl Iterator<Item = HubId> + '_ {
        self.after.iter().map(|(id, ..)| *id).filter(move |id| *id != self.hub)
    }
}

/// Every hub, and where every player known to have played of late is at
/// home.
#[derive(Resource, Debug)]
pub struct Hubs {
    haven: (i32, i32),
    hubs: HashMap<HubId, Held>,
    /// Each recent player's home, a home's site, or none where it is the
    /// haven.
    residents: HashMap<Identity, Option<(i32, i32)>>,
    /// Players each hub counts as at home besides those who are: a
    /// fixture, for seeing what a hub's people hold with no one else in
    /// the world.
    extra: u32,
    changed: bool,
    lie: Lie,
}

/// The tiles between two tiles of the flat grid.
fn distance(a: (i32, i32), b: (i32, i32)) -> f32 {
    Qrz { q: a.0, r: a.1, z: 0 }.flat_distance(&Qrz { q: b.0, r: b.1, z: 0 }) as f32
}

impl Hubs {
    /// The hubs of a world whose haven is the tile `haven`: none, with
    /// nothing built.
    pub fn new(haven: (i32, i32)) -> Self {
        Self { haven, hubs: HashMap::new(), residents: HashMap::new(), extra: 0, changed: false, lie: Lie(std::sync::Arc::new(|_, _| 0)) }
    }

    /// These hubs on ground that lies as `level` says, a tile's height
    /// in levels, where they were on the flat.
    pub fn on(mut self, level: impl Fn(i32, i32) -> i32 + Send + Sync + 'static) -> Self {
        self.lie = Lie(std::sync::Arc::new(level));
        self
    }

    /// `hub` as it is held, known by `id`.
    fn hold(&self, id: HubId, hub: sites::Hub, took: Vec<HubId>) -> Held {
        let ground = sites::ground(&hub.sites);
        let (gate, path) = self.way(id, &hub, &ground);
        Held { path, gate, ground, hub, took }
    }

    /// The way in through the wall of `ground`, hub `id`'s, the one tile
    /// of it no stakes stand on, and the path to it from the hearth. The
    /// way in is the tile of the wall the shortest walk from the hearth
    /// reaches, a walk that climbs or drops no more than a level a step
    /// and can go on out of the hub the same; of those as near, the one
    /// on the gentlest ground, which differs least in height from the
    /// tiles round it; of those as gentle, one drawn by the hub and the
    /// tile. Where no walk reaches the wall, the ground is taken as flat.
    fn way(&self, id: HubId, hub: &sites::Hub, ground: &Ground) -> (Option<(i32, i32)>, Vec<(i32, i32)>) {
        let Some(hearth) = hub.sites.iter().find(|site| site.what == Standing::Hearth).map(|site| site.at) else { return (None, Vec::new()) };
        let level = &self.lie.0;
        let walked = |a: (i32, i32), b: (i32, i32)| (level(a.0, a.1) - level(b.0, b.1)).abs() <= 1;
        let steep = |(q, r): (i32, i32)| qrz::DIRECTIONS.iter().map(|d| (level(q + d.q, r + d.r) - level(q, r)).abs()).max().unwrap_or(0);
        let leaves = |tile: (i32, i32)| qrz::DIRECTIONS.iter().map(|d| (tile.0 + d.q, tile.1 + d.r)).any(|out| ground.tiles.binary_search(&out).is_err() && walked(tile, out));
        let near = |steps: &HashMap<(i32, i32), u32>, open: &dyn Fn((i32, i32)) -> bool| {
            ground.wall.iter().copied().filter(|&tile| open(tile)).filter_map(|tile| Some((steps.get(&tile)?, steep(tile), drawn(id, tile), tile))).min().map(|(.., tile)| tile)
        };
        if let Some(gate) = near(&sites::reach(ground, hearth, walked), &leaves) {
            return (Some(gate), sites::path(ground, hearth, gate, walked));
        }
        let gate = near(&sites::reach(ground, hearth, |_, _| true), &|_| true);
        (gate, gate.map_or(Vec::new(), |gate| sites::path(ground, hearth, gate, |_, _| true)))
    }

    /// The hub known by `tile`: founded on it, or a hub that took in the
    /// one that was.
    pub fn of_hearth(&self, tile: (i32, i32)) -> Option<HubId> {
        let known = HubId(tile.0, tile.1);
        self.hubs.iter().find(|(id, held)| **id == known || held.took.contains(&known)).map(|(&id, _)| id)
    }

    /// The hub whose ground holds `tile`.
    pub fn holding(&self, tile: (i32, i32)) -> Option<HubId> {
        self.hubs.iter().filter(|(_, held)| held.ground.tiles.binary_search(&tile).is_ok()).map(|(&id, _)| id).min()
    }

    /// Hub `id`'s build sites, in the order they were built.
    pub fn sites(&self, id: HubId) -> &[Site] {
        self.hubs.get(&id).map_or(&[], |held| &held.hub.sites)
    }

    /// The ground hub `id` holds, and its wall.
    pub fn ground(&self, id: HubId) -> Option<&Ground> {
        self.hubs.get(&id).map(|held| &held.ground)
    }

    /// The tiles of a wall that stakes stand on: all but its way in.
    fn staked(wall: &[(i32, i32)], gate: Option<(i32, i32)>) -> Vec<(i32, i32)> {
        wall.iter().copied().filter(|&tile| Some(tile) != gate).collect()
    }

    /// The tiles hub `id`'s stakes stand on: its wall, but for the way in.
    pub fn stakes(&self, id: HubId) -> Vec<(i32, i32)> {
        self.hubs.get(&id).map_or(Vec::new(), |held| Self::staked(&held.ground.wall, held.gate))
    }

    /// The way the stakes on `tile` of hub `id`'s wall face, out of its
    /// ground, as a place in `common::cover::FACINGS`.
    pub fn faces(&self, id: HubId, tile: (i32, i32)) -> Option<usize> {
        Some(sites::outward(self.ground(id)?, tile))
    }

    /// The way in through hub `id`'s wall.
    pub fn way_in(&self, id: HubId) -> Option<(i32, i32)> {
        self.hubs.get(&id)?.gate
    }

    /// The path from hub `id`'s hearth to its way in, the way in the last:
    /// where a plan last carried out put it. Unbuilt: nothing draws it.
    pub fn path(&self, id: HubId) -> &[(i32, i32)] {
        self.hubs.get(&id).map_or(&[], |held| &held.path)
    }

    /// Whether a player may be at home on `tile`: a home's site or a
    /// hearth's is there.
    pub fn homed(&self, tile: (i32, i32)) -> bool {
        self.hubs.values().any(|held| held.hub.sites.iter().any(|site| site.at == tile && matches!(site.what, Standing::Home | Standing::Hearth)))
    }

    /// Whether a hub's hearth is laid on `tile`.
    fn hearthed(&self, tile: (i32, i32)) -> bool {
        self.hubs.values().any(|held| held.hub.sites.iter().any(|site| site.at == tile && site.what == Standing::Hearth))
    }

    /// The tiles of hub `id` that `what` stands on, in the order built.
    pub fn standing(&self, id: HubId, what: Standing) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.sites(id).iter().filter(move |site| site.what == what).map(|site| site.at)
    }

    /// The tile hub `id`'s hearth is laid on.
    pub fn hearth(&self, id: HubId) -> Option<(i32, i32)> {
        self.standing(id, Standing::Hearth).next()
    }

    /// The tier hub `id` is built out to.
    pub fn tier(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |held| held.hub.tier)
    }

    /// How many players are at home in hub `id`.
    pub fn size(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |held| sites::players(&held.hub.sites) as u32 + self.extra)
    }

    /// How many stakes of wall hub `id`'s people hold between them.
    pub fn held(&self, id: HubId) -> usize {
        sites::held(self.size(id) as usize)
    }

    /// Whether a hearth may be laid on `tile`, the build site it would
    /// stand on: no hub's ground holds it, and the camp round it would
    /// touch no hub's.
    pub fn founds(&self, tile: (i32, i32)) -> bool {
        let camp = sites::ground(&sites::camp(tile).sites);
        self.hubs.values().all(|held| !sites::touches(&held.ground, &camp))
    }

    /// A hearth was laid on `tile`, a build site: the camp round it is a
    /// hub, known by that tile. Nothing where a hub is known by it
    /// already.
    pub fn founded(&mut self, tile: (i32, i32)) -> HubId {
        let id = HubId(tile.0, tile.1);
        if self.of_hearth(tile).is_none() {
            self.hubs.insert(id, self.hold(id, sites::camp(tile), Vec::new()));
            self.changed = true;
        }
        self.of_hearth(tile).expect("a hub just founded is known by its tile")
    }

    /// Hub `id` is no more: its hearth came down. The players at home in
    /// it are at home at the haven.
    pub fn razed(&mut self, id: HubId) {
        let Some(held) = self.hubs.remove(&id) else { return };
        let homes: HashSet<(i32, i32)> = held.hub.sites.iter().map(|site| site.at).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.changed = true;
    }

    /// The home on `tile`, where one stands there that a player may
    /// claim: no other player lives in it.
    pub fn open(&self, tile: (i32, i32)) -> Option<HubId> {
        self.hubs.iter().find(|(_, held)| held.hub.sites.iter().any(|site| site.at == tile && site.what == Standing::Home && site.lived != Lived::Player)).map(|(&id, _)| id)
    }

    /// `who` played of late with its home on `home`, or at the haven. A
    /// hearth is anyone's to be at home at. A home that stands no more,
    /// or is another player's, is the haven.
    pub fn resident(&mut self, who: Identity, home: Option<(i32, i32)>) {
        let mine = |at: (i32, i32)| self.residents.get(&who) == Some(&Some(at)) && self.homed(at);
        let home = home.filter(|&at| self.hearthed(at) || mine(at) || self.open(at).is_some());
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

    /// Where `who` is at home, where it is in a hub.
    pub fn home_of(&self, who: Identity) -> Option<(i32, i32)> {
        self.residents.get(&who).copied().flatten()
    }

    /// Makes every home lived in by the players at home in it, and the
    /// settlers who came with them.
    fn people(&mut self) {
        let homes: HashSet<(i32, i32)> = self.residents.values().flatten().copied().collect();
        for held in self.hubs.values_mut() {
            for site in &mut held.hub.sites {
                match (homes.contains(&site.at) && site.what == Standing::Home, site.lived) {
                    (true, _) => site.lived = Lived::Player,
                    (false, Lived::Player) => site.lived = Lived::Empty,
                    _ => {}
                }
            }
            sites::peopled(&mut held.hub.sites);
        }
    }

    /// Every hub in the order of its name, so a plan reads the same hubs
    /// the same way on every server and every run.
    fn ordered(&self) -> Vec<HubId> {
        let mut ids: Vec<HubId> = self.hubs.keys().copied().collect();
        ids.sort();
        ids
    }

    /// What `after` changes of hub `id`'s sites and of every wall, the
    /// hubs in `taken` gone.
    fn plan(&self, id: HubId, after: Vec<(HubId, sites::Hub, Vec<HubId>)>, taken: Vec<HubId>) -> Plan {
        let was: Vec<&Held> = std::iter::once(id).chain(taken.iter().copied()).filter_map(|id| self.hubs.get(&id)).collect();
        let before: HashSet<Site> = was.iter().flat_map(|held| held.hub.sites.iter().map(|site| Site { lived: Lived::Empty, ..*site })).collect();
        let since: HashSet<Site> = after.iter().flat_map(|(_, hub, _)| hub.sites.iter().map(|site| Site { lived: Lived::Empty, ..*site })).collect();
        let stood: HashSet<(i32, i32)> = was.iter().flat_map(|held| Self::staked(&held.ground.wall, held.gate)).collect();
        let laid: Vec<Held> = after.iter().map(|(id, hub, took)| self.hold(*id, hub.clone(), took.clone())).collect();
        let stands: HashSet<(i32, i32)> = laid.iter().flat_map(|held| Self::staked(&held.ground.wall, held.gate)).collect();
        let trod: HashSet<(i32, i32)> = was.iter().flat_map(|held| held.path.iter().copied()).collect();
        let treads: HashSet<(i32, i32)> = laid.iter().flat_map(|held| held.path.iter().copied()).collect();
        let sorted = |mut tiles: Vec<(i32, i32)>| {
            tiles.sort();
            tiles
        };
        let by_tile = |mut sites: Vec<Site>| {
            sites.sort_by_key(|site| site.at);
            sites
        };
        Plan {
            hub: id,
            built: by_tile(since.difference(&before).copied().collect()),
            gone: by_tile(before.difference(&since).copied().collect()),
            raised: sorted(stands.difference(&stood).copied().collect()),
            pulled: sorted(stood.difference(&stands).copied().collect()),
            worn: sorted(treads.difference(&trod).copied().collect()),
            faded: sorted(trod.difference(&treads).copied().collect()),
            after,
            taken,
        }
    }

    /// What growing hub `id` a tier would do, however few live in it.
    pub fn grown(&self, id: HubId) -> Option<Plan> {
        let ids = self.ordered();
        let which = ids.iter().position(|&other| other == id)?;
        let hubs: Vec<sites::Hub> = ids.iter().map(|id| self.hubs[id].hub.clone()).collect();
        let mut left = sites::grow(&hubs, which);
        let grown = left.pop()?;
        // Hubs the grow left alone are as they were; the rest it took in
        let taken: Vec<HubId> = ids.iter().copied().filter(|&other| other != id && !left.contains(&self.hubs[&other].hub)).collect();
        let took = self.hubs[&id].took.iter().copied().chain(taken.iter().flat_map(|other| std::iter::once(*other).chain(self.hubs[other].took.iter().copied()))).collect();
        Some(self.plan(id, vec![(id, grown, took)], taken))
    }

    /// What shrinking hub `id` to what its people hold would do, or none
    /// where they hold its wall already. The part that keeps the hub's
    /// hearth, or else the first, keeps its name; each other is known by
    /// the tile its hearth is laid on.
    pub fn shrunk(&self, id: HubId) -> Option<Plan> {
        let held = self.hubs.get(&id)?;
        let mut hub = held.hub.clone();
        // Counted as the hub's people are: the fixture's players live in
        // homes no one else does
        let mut extra = self.extra;
        for site in hub.sites.iter_mut().filter(|site| site.what == Standing::Home && site.lived != Lived::Player) {
            if extra == 0 {
                break;
            }
            site.lived = Lived::Player;
            extra -= 1;
        }
        sites::peopled(&mut hub.sites);
        let parts = sites::shrink(&hub)?;
        let hearth = self.hearth(id);
        let keeps = parts.iter().position(|part| part.sites.iter().any(|site| site.what == Standing::Hearth && Some(site.at) == hearth)).unwrap_or(0);
        let after = parts
            .into_iter()
            .enumerate()
            .map(|(i, part)| {
                let at = part.sites.iter().find(|site| site.what == Standing::Hearth).map_or(id, |site| HubId(site.at.0, site.at.1));
                if i == keeps { (id, part, held.took.clone()) } else { (at, part, Vec::new()) }
            })
            .collect();
        Some(self.plan(id, after, Vec::new()))
    }

    /// Does what `plan` said: its hubs are as it leaves them, and those it
    /// took in are no more.
    pub fn carry(&mut self, plan: &Plan) {
        let fires: HashSet<(i32, i32)> = std::iter::once(plan.hub).chain(plan.taken.iter().copied()).filter_map(|id| self.hearth(id)).collect();
        for id in &plan.taken {
            self.hubs.remove(id);
        }
        for (id, hub, took) in &plan.after {
            self.hubs.insert(*id, self.hold(*id, hub.clone(), took.clone()));
        }
        // One at home at a hearth that is one no more is at home at its
        // hub's; a home given up is no one's home
        let hearth = self.hearth(plan.hub);
        let gone: Vec<(i32, i32)> = fires.into_iter().filter(|&at| !self.hearthed(at)).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| gone.contains(&at))) {
            *home = hearth;
        }
        let homes: Vec<(i32, i32)> = self.residents.values().flatten().copied().filter(|&at| !self.homed(at)).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.people();
        self.changed = true;
        info!("hubs: {:?} planned: {} sites built, {} given up, {} stakes raised, {} pulled, {} hubs taken in", plan.hub, plan.built.len(), plan.gone.len(), plan.raised.len(), plan.pulled.len(), plan.taken.len());
    }

    /// Counts every hub as `players` at home besides those who are.
    pub fn count_extra(&mut self, players: u32) {
        self.extra = players;
    }

    /// Whether any hub's sites changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Every hub as a store keeps it, in the order of its name.
    pub fn kept(&self) -> Vec<Kept> {
        self.ordered()
            .into_iter()
            .map(|id| {
                let held = &self.hubs[&id];
                Kept { id: (id.0, id.1), sites: held.hub.sites.iter().map(|site| (site.at.0, site.at.1, site.what.id())).collect(), took: held.took.iter().map(|id| (id.0, id.1)).collect() }
            })
            .collect()
    }

    /// Lays the hubs kept from an earlier run, and the recent players'
    /// homes. A site whose standing this build has no number for is a
    /// home.
    pub fn recall(&mut self, hubs: Vec<Kept>, residents: Vec<(Identity, Option<(i32, i32)>)>) {
        for kept in hubs {
            let sites: Vec<Site> = kept.sites.iter().map(|&(q, r, what)| Site { at: (q, r), what: Standing::kept_as(what).unwrap_or(Standing::Home), lived: Lived::Empty }).collect();
            let tier = (0..).take_while(|&tier| sites::sites_for(tier) <= sites.len()).last().unwrap_or(0);
            let mut hub = sites::Hub { sites, tier };
            sites::peopled(&mut hub.sites);
            let id = HubId(kept.id.0, kept.id.1);
            self.hubs.insert(id, self.hold(id, hub, kept.took.into_iter().map(|(q, r)| HubId(q, r)).collect()));
        }
        for (who, home) in residents {
            self.residents.insert(who, home);
        }
        // A home that stands no more is the haven
        let homes: Vec<(i32, i32)> = self.residents.values().flatten().copied().filter(|&at| !self.homed(at)).collect();
        for home in self.residents.values_mut().filter(|home| home.is_some_and(|at| homes.contains(&at))) {
            *home = None;
        }
        self.people();
    }

    /// Each hub with the tile its hearth is laid on and its size.
    pub fn each(&self) -> impl Iterator<Item = (HubId, (i32, i32), u32)> + '_ {
        self.ordered().into_iter().filter_map(|id| Some((id, self.hearth(id)?, self.size(id))))
    }

    /// Where the haven is.
    pub fn haven(&self) -> (i32, i32) {
        self.haven
    }

    /// Whether the haven's guard keeps `tile`.
    pub fn guarded(&self, tile: (i32, i32)) -> bool {
        distance(self.haven, tile) <= HAVEN_GROUND
    }

    /// How far `tile` lies outside hub `id`'s wall, in tiles: none on its
    /// ground.
    pub fn outside(&self, id: HubId, tile: (i32, i32)) -> Option<f32> {
        let ground = self.ground(id)?;
        if ground.tiles.binary_search(&tile).is_ok() {
            return Some(0.0);
        }
        ground.wall.iter().map(|&at| distance(at, tile)).min_by(f32::total_cmp)
    }

    /// How far past its wall hub `id`'s influence carries, in tiles.
    pub fn reach(&self, id: HubId) -> f32 {
        self.ground(id).map_or(0.0, |ground| INFLUENCE_WALLS * ground.wall.len() as f32)
    }

    /// Hub `id`'s influence over its ground, 0 to 1: the square root of
    /// its players as a share of [`INFLUENCE_WHOLE`].
    pub fn strength(&self, id: HubId) -> f32 {
        ((self.size(id).max(1) as f32) / INFLUENCE_WHOLE).sqrt().min(1.0)
    }

    /// How far `tile` lies outside the haven's guarded ground, in tiles.
    fn past_haven(&self, tile: (i32, i32)) -> f32 {
        (distance(self.haven, tile) - HAVEN_GROUND).max(0.0)
    }

    /// Whether the haven's influence reaches `tile`.
    pub fn haven_reaches(&self, tile: (i32, i32)) -> bool {
        self.past_haven(tile) < HAVEN_REACH
    }

    /// Whether hub `id`'s influence reaches `tile`.
    pub fn reaches(&self, id: HubId, tile: (i32, i32)) -> bool {
        self.outside(id, tile).is_some_and(|outside| outside < self.reach(id))
    }

    /// The influence the haven and every hub have on `tile` together, 0
    /// to 1. Each one's is its strength, the haven's whole, over all the
    /// ground it holds; past that edge it falls as the cube of what is
    /// left of the way to its reach. It is what pushes the hard country
    /// back. Unbuilt: nothing in the game reads it; a den's level will.
    pub fn influence(&self, tile: (i32, i32)) -> f32 {
        let felt = |strength: f32, outside: f32, reach: f32| strength * (1.0 - (outside / reach.max(1.0)).clamp(0.0, 1.0)).powi(3);
        let hubs = self.hubs.keys().filter_map(|&id| Some(felt(self.strength(id), self.outside(id, tile)?, self.reach(id))));
        (felt(1.0, self.past_haven(tile), HAVEN_REACH) + hubs.sum::<f32>()).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(n: u8) -> Identity {
        Identity::Key([n; 16])
    }

    /// `count` players, numbered from `from`, each in an open home of hub
    /// `id`. Panics where it has too few.
    fn settle(hubs: &mut Hubs, id: HubId, from: u8, count: u8) {
        for n in from..from + count {
            let home = hubs.sites(id).iter().find(|site| site.what == Standing::Home && site.lived == Lived::Empty).expect("a home is open").at;
            hubs.resident(player(n), Some(home));
        }
    }

    /// A hearth founds a camp, a hub known by its tile that holds the
    /// ground round it; none is laid on a hub's ground or against it, and
    /// one beside the haven is a hub like any other.
    #[test]
    fn a_hearth_founds_a_hub() {
        let mut hubs = Hubs::new((0, 0));
        assert!(hubs.founds((201, 0)));
        let camp = hubs.founded((201, 0));
        assert_eq!((camp, hubs.hearth(camp), hubs.tier(camp), hubs.size(camp)), (HubId(201, 0), Some((201, 0)), 0, 0));
        assert_eq!(hubs.holding((201, 0)), Some(camp));
        assert!(hubs.sites(camp).iter().all(|site| hubs.holding(site.at) == Some(camp)));
        assert_eq!(hubs.holding((230, 0)), None);
        assert!(!hubs.founds((204, 0)) && !hubs.founds((210, 0)) && hubs.founds((240, 0)));
        assert_eq!(hubs.founded((9, 0)), HubId(9, 0));
        assert!(hubs.guarded((9, 0)) && !hubs.guarded((201, 0)));
        assert!(hubs.take_changed() && !hubs.take_changed());
    }

    /// A player is at home in a home it claimed, and a settler comes with
    /// it; a home another player lives in is not claimed, and one whose
    /// hub is razed leaves its player at home at the haven.
    #[test]
    fn players_live_in_the_homes_they_claim() {
        let mut hubs = Hubs::new((0, 0));
        let camp = hubs.founded((300, 0));
        let home = hubs.sites(camp).iter().find(|site| site.lived == Lived::Empty && site.what == Standing::Home).unwrap().at;
        assert_eq!(hubs.open(home), Some(camp));
        hubs.resident(player(1), Some(home));
        assert_eq!((hubs.home_of(player(1)), hubs.size(camp), hubs.open(home)), (Some(home), 1, None));
        hubs.resident(player(2), Some(home));
        assert_eq!(hubs.home_of(player(2)), None, "the home is another's");
        hubs.resident(player(1), Some(home));
        assert_eq!(hubs.home_of(player(1)), Some(home), "and still its own");
        hubs.gone(player(1));
        assert_eq!((hubs.size(camp), hubs.open(home)), (0, Some(camp)));
        hubs.resident(player(1), Some(home));
        let hearth = hubs.hearth(camp).unwrap();
        for visitor in 3..6 {
            hubs.resident(player(visitor), Some(hearth));
            assert_eq!(hubs.home_of(player(visitor)), Some(hearth), "a hearth is anyone's to be at home at");
        }
        assert_eq!(hubs.size(camp), 1, "and one at home there lives in none of its homes");
        hubs.razed(camp);
        assert_eq!((hubs.home_of(player(1)), hubs.home_of(player(3)), hubs.each().count()), (None, None, 0));
    }

    /// A plan says what it would do and changes nothing until it is
    /// carried: a grow builds the next tier's homes and raises the wall
    /// round them, pulling what it leaves inside.
    #[test]
    fn a_plan_changes_nothing_until_it_is_carried() {
        let mut hubs = Hubs::new((0, 0));
        let camp = hubs.founded((300, 0));
        let wall = hubs.ground(camp).unwrap().wall.clone();
        let plan = hubs.grown(camp).expect("a hub grows with no one living in it");
        assert_eq!((hubs.sites(camp).len(), hubs.tier(camp)), (sites::sites_for(0), 0));
        assert_eq!(plan.built.len(), sites::sites_for(1) - sites::sites_for(0));
        assert!(plan.built.iter().all(|site| site.what == Standing::Home) && plan.gone.is_empty() && plan.taken.is_empty());
        assert!(!plan.raised.is_empty() && plan.pulled.iter().all(|tile| wall.contains(tile)));
        hubs.take_changed();
        hubs.carry(&plan);
        assert_eq!((hubs.sites(camp).len(), hubs.tier(camp)), (sites::sites_for(1), 1));
        let now = hubs.stakes(camp);
        assert!(plan.raised.iter().all(|tile| now.contains(tile)) && plan.pulled.iter().all(|tile| !now.contains(tile)));
        assert_eq!(now.len() + 1, hubs.ground(camp).unwrap().wall.len(), "the way in is left open");
        assert!(hubs.take_changed());
    }

    /// A hub that grows to another takes it in: it answers to the other's
    /// name, and the other's players are at home in it.
    #[test]
    fn a_hub_that_grows_to_another_takes_it_in() {
        let mut hubs = Hubs::new((0, 0));
        let (town, camp) = (hubs.founded((300, 0)), hubs.founded((315, 0)));
        settle(&mut hubs, camp, 1, 1);
        let home = hubs.home_of(player(1));
        hubs.resident(player(2), Some((315, 0)));
        let first = hubs.grown(town).unwrap();
        assert!(first.taken.is_empty(), "a tier up, it has not reached the camp");
        hubs.carry(&first);
        let second = hubs.grown(town).unwrap();
        assert_eq!(second.taken, vec![camp]);
        hubs.carry(&second);
        assert_eq!((hubs.each().count(), hubs.of_hearth((315, 0)), hubs.holding((315, 0))), (1, Some(town), Some(town)));
        assert_eq!((hubs.home_of(player(1)), hubs.size(town)), (home, 1));
        assert_eq!(hubs.home_of(player(2)), hubs.hearth(town), "one at home at the hearth taken in is at home at the hub's");
        assert_eq!(hubs.standing(town, Standing::Hearth).count(), 1);
    }

    /// A shrink is to what a hub's people hold: nothing where they hold
    /// its wall, and otherwise the sites it gives up and the stakes it
    /// pulls, every player still at home.
    #[test]
    fn a_hub_shrinks_to_what_its_people_hold() {
        let mut hubs = Hubs::new((0, 0));
        let town = hubs.founded((300, 0));
        assert_eq!(hubs.shrunk(town), None, "a camp is the least a hub is");
        for _ in 0..2 {
            let plan = hubs.grown(town).unwrap();
            hubs.carry(&plan);
        }
        settle(&mut hubs, town, 1, 2);
        let homes = (hubs.home_of(player(1)), hubs.home_of(player(2)));
        let plan = hubs.shrunk(town).expect("two do not hold a hub built for nine");
        // What it builds is a hearth, post or wagon laid again, never a home
        assert!(!plan.gone.is_empty() && plan.built.iter().all(|site| site.what != Standing::Home) && !plan.pulled.is_empty());
        hubs.carry(&plan);
        assert_eq!((hubs.home_of(player(1)), hubs.home_of(player(2))), homes);
        assert!(hubs.sites(town).len() < sites::sites_for(2));
        assert_eq!(hubs.shrunk(town), None, "what is left, they hold");
    }

    /// What is kept of the hubs lays them again as they were, and a hub
    /// kept as its hearths, before hubs had sites, is a camp round the
    /// first that answers to the others.
    #[test]
    fn hubs_are_recalled_as_they_were_kept() {
        let mut hubs = Hubs::new((0, 0));
        let town = hubs.founded((300, 0));
        hubs.founded((600, 0));
        settle(&mut hubs, town, 1, 1);
        let plan = hubs.grown(town).unwrap();
        hubs.carry(&plan);
        let mut back = Hubs::new((0, 0));
        back.recall(hubs.kept(), vec![(player(1), hubs.home_of(player(1))), (player(2), Some((9, 9)))]);
        assert_eq!(back.hubs, hubs.hubs);
        assert_eq!((back.size(town), back.home_of(player(2))), (1, None), "a home that stands no more is the haven");

        let old = Kept::of_hearths(&[(301, 2), (320, 0)]).unwrap();
        let mut converted = Hubs::new((0, 0));
        converted.recall(vec![old], Vec::new());
        let id = HubId(301, 2);
        assert_eq!((converted.of_hearth((301, 2)), converted.of_hearth((320, 0)), converted.tier(id)), (Some(id), Some(id), 0));
        assert_eq!(converted.hearth(id), Some(sites::site_near((301, 2))));
    }

    /// The way in through a wall is the tile of it the shortest walk from
    /// the hearth reaches: on the flat one of the nearest, the same each
    /// time; on a slope too steep to walk, a tile the level ground
    /// leads to.
    #[test]
    fn the_way_in_is_on_the_gentlest_ground() {
        let flat = |at: (i32, i32)| {
            let mut hubs = Hubs::new((0, 0));
            let id = hubs.founded(at);
            hubs.way_in(id).unwrap()
        };
        assert_eq!(flat((300, 0)), flat((300, 0)));
        let ways: HashSet<(i32, i32)> = (0..12).map(|k| flat((300 + 30 * k, 0))).map(|(q, r)| (q % 30, r)).collect();
        assert!(ways.len() > 1, "hubs on the flat open different ways: {ways:?}");
        let mut hubs = Hubs::new((0, 0));
        let id = hubs.founded((300, 0));
        let nearest = hubs.ground(id).unwrap().wall.iter().map(|&tile| sites::path(hubs.ground(id).unwrap(), (300, 0), tile, |_, _| true).len()).min();
        assert_eq!(Some(hubs.path(id).len()), nearest, "no tile of its wall is a shorter walk");

        // Ground that climbs two levels a tile with q, but lies level
        // along q = 300 and the tile either side of it
        let mut hubs = Hubs::new((0, 0)).on(|q, _| 2 * ((q - 300).abs() - 1).max(0));
        let id = hubs.founded((300, 0));
        let way = hubs.way_in(id).unwrap();
        assert!((way.0 - 300).abs() <= 1, "of all its wall, where the level ground leads: {way:?}");
        assert!(hubs.path(id).iter().all(|tile| (tile.0 - 300).abs() <= 1), "and its path keeps to ground it can walk");
        assert!(hubs.ground(id).unwrap().wall.contains(&way) && !hubs.stakes(id).contains(&way));
        assert_eq!(hubs.path(id).last(), Some(&way), "its path runs from the hearth to the way in");
        assert!(hubs.path(id).iter().all(|&tile| tile == way || !hubs.sites(id).iter().any(|site| site.at == tile)));
        hubs.resident(player(1), hubs.sites(id).iter().find(|site| site.what == Standing::Home).map(|site| site.at));
        let plan = hubs.grown(id).unwrap();
        let before: Vec<(i32, i32)> = hubs.path(id).to_vec();
        hubs.carry(&plan);
        let after = hubs.path(id);
        assert!(plan.worn.iter().all(|tile| after.contains(tile) && !before.contains(tile)), "a plan says what its path comes to cross");
        assert!(plan.faded.iter().all(|tile| before.contains(tile) && !after.contains(tile)), "and what it crosses no more");
        assert_eq!(before.len() + plan.worn.len() - plan.faded.len(), after.len());
    }

    /// A hub's influence is the same over all its ground, falls past its
    /// wall to nothing at its reach, and is stronger the more live there.
    #[test]
    fn influence_falls_with_distance_and_grows_with_the_hub() {
        let mut hubs = Hubs::new((0, 0));
        let camp = hubs.founded((5001, 0));
        let at = |hubs: &Hubs, far: i32| hubs.influence((5001 + far, 0));
        assert_eq!(at(&hubs, 0), at(&hubs, 3), "the same over its ground");
        assert!(at(&hubs, 0) > at(&hubs, 20) && at(&hubs, 20) > at(&hubs, 40));
        assert_eq!(at(&hubs, hubs.reach(camp) as i32 + 12), 0.0);
        let small = at(&hubs, 0);
        settle(&mut hubs, camp, 1, 1);
        assert!(at(&hubs, 0) >= small);
        assert_eq!((hubs.influence((0, 0)), hubs.influence((HAVEN_GROUND as i32, 0))), (1.0, 1.0), "the haven's is whole over its ground");
    }
}
