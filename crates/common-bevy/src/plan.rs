//! A hub as its planners see it: the ground it claims, its zones, and
//! what stands where. Nothing here knows who the players are, how a hub is
//! kept or what the ground is like; the server holds hubs and asks these
//! rules what a change would do.
//!
//! A hub is planned for a number of residents and claims ground out to
//! the wall that number takes ([`wall_for`]); its wall is the tightest
//! outline round that claim, [`PAST`] tiles out. A zone is a centre: a
//! tile lies in the zone whose centre is nearest, and a thing is built in
//! its own zone as near that centre as it fits. Every building starts on
//! one tile and is upgraded through the footprints of [`SIZES`], and no
//! two things stand nearer than [`SPACE`].
//!
//! Every change is asked for as an [`Outcome`]: the hub as it would be,
//! and what was built, moved and taken down to make it so. Nothing here
//! changes a hub in place.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use qrz::outline::{apart, enclosed, flat, girth, Point};

/// A tile, as `(q, r)`.
pub type Tile = (i32, i32);

/// How far apart the nearest tiles of two things stand, in tile widths.
pub const SPACE: f64 = 3.0;

/// How far past its claim a hub's wall stands, in tiles.
pub const PAST: f64 = 3.0;

/// How many stakes past the wall its residents hold a hub defends: the
/// same for every hub, so more of a small hub's wall than a large one's.
pub const FORGIVEN: usize = 5;

/// How the wall a hub is planned to grows with its residents: a camp's
/// wall, times how many camps' worth of residents to this power.
pub const POWER: f64 = 0.42;

/// How many residents a camp is planned for.
pub const CAMP: u32 = 2;

/// The tiles a building stands on at each level.
pub const SIZES: [usize; 5] = [1, 3, 4, 6, 7];

/// How many a home houses at each level.
pub const HOUSES: [u32; 5] = [1, 4, 8, 16, 24];

const EPS: f64 = 1e-6;

/// A zone of a hub, in the order they come to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Zone {
    /// The hearth, and what a hub is run from.
    Logistics,
    Residential,
    Market,
    Crafting,
    Farming,
    Plaza,
}

impl Zone {
    pub const ALL: [Zone; 6] = [Zone::Logistics, Zone::Residential, Zone::Market, Zone::Crafting, Zone::Farming, Zone::Plaza];

    /// How many residents a hub is planned for before it has this zone.
    pub fn from(self) -> u32 {
        match self {
            Zone::Logistics => 0,
            Zone::Residential => 1,
            Zone::Market => 10,
            Zone::Crafting => 40,
            Zone::Farming => 100,
            Zone::Plaza => 250,
        }
    }

    /// The number a store keeps it by, given once.
    pub fn id(self) -> u32 {
        match self {
            Zone::Logistics => 1,
            Zone::Residential => 2,
            Zone::Market => 3,
            Zone::Crafting => 4,
            Zone::Farming => 5,
            Zone::Plaza => 6,
        }
    }

    /// What is kept as `id`, or none where the game no longer has it.
    pub fn kept_as(id: u32) -> Option<Zone> {
        Self::ALL.into_iter().find(|zone| zone.id() == id)
    }
}

/// What a thing of a hub is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum What {
    /// The fireplace a hub is founded on. It is the logistics zone's
    /// centre, and is neither upgraded nor taken down.
    Hearth,
    /// The hitching post: a hub takes settlers once one stands.
    Post,
    Wagon,
    /// The store, which holds more at each level.
    Store,
    Home,
    Market,
    Crafting,
    Farming,
}

impl What {
    pub const ALL: [What; 8] = [What::Hearth, What::Post, What::Wagon, What::Store, What::Home, What::Market, What::Crafting, What::Farming];

    /// The zone it is built in.
    pub fn zone(self) -> Zone {
        match self {
            What::Hearth | What::Post | What::Wagon | What::Store => Zone::Logistics,
            What::Home => Zone::Residential,
            What::Market => Zone::Market,
            What::Crafting => Zone::Crafting,
            What::Farming => Zone::Farming,
        }
    }

    /// The number a store keeps it by, given once.
    pub fn id(self) -> u32 {
        match self {
            What::Hearth => 1,
            What::Post => 2,
            What::Wagon => 3,
            What::Home => 4,
            What::Store => 5,
            What::Market => 6,
            What::Crafting => 7,
            What::Farming => 8,
        }
    }

    /// What is kept as `id`, or none where the game no longer has it.
    pub fn kept_as(id: u32) -> Option<What> {
        Self::ALL.into_iter().find(|what| what.id() == id)
    }
}

/// A thing of a hub by what it is and how far it is upgraded. A hearth
/// has no level; a building's runs from 1 to as many as [`SIZES`] has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Kind {
    pub what: What,
    pub level: u8,
}

impl Kind {
    pub const HEARTH: Kind = Kind { what: What::Hearth, level: 0 };

    /// `what` as it is first raised.
    pub fn first(what: What) -> Kind {
        Kind { what, level: if what == What::Hearth { 0 } else { 1 } }
    }

    /// Whether it is a building: a thing with a level, raised, upgraded
    /// and taken down as any other.
    pub fn building(self) -> bool {
        self.level > 0
    }

    /// How many tiles it stands on.
    pub fn size(self) -> usize {
        SIZES[(self.level.max(1) - 1) as usize]
    }

    /// How many it houses.
    pub fn houses(self) -> u32 {
        if self.what == What::Home { HOUSES[(self.level - 1) as usize] } else { 0 }
    }

    /// What it is upgraded to, or none at the last level.
    pub fn next(self) -> Option<Kind> {
        (self.building() && (self.level as usize) < SIZES.len()).then(|| Kind { level: self.level + 1, ..self })
    }

    /// What it is downgraded to, or none from the first level.
    pub fn before(self) -> Option<Kind> {
        (self.level > 1).then(|| Kind { level: self.level - 1, ..self })
    }
}

/// A thing standing in a hub: the tiles it stands on, and who lives in
/// it where it is a home.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its own number in its hub, kept through every change to it.
    pub id: u32,
    pub kind: Kind,
    pub tiles: Vec<Tile>,
    pub players: u32,
    pub settlers: u32,
}

impl Item {
    fn lived(&self) -> u32 {
        self.players + self.settlers
    }
}

/// A hub: how many residents it is planned for, its zones' centres, what
/// stands in it, and the ground it claims and has nothing on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hub {
    pub wants: u32,
    pub zones: BTreeMap<Zone, Tile>,
    pub items: Vec<Item>,
    pub open: BTreeSet<Tile>,
    /// The number the next thing raised in it takes.
    pub next: u32,
    /// The tiles nothing is planned on, as the ground lies: a tree, a
    /// boulder, water, a cliff's edge. What holds the hub says which
    /// (`Hubs::survey`); the rules only plan round them.
    pub barred: BTreeSet<Tile>,
    /// Each pair of tiles side by side with a cliff between them, the
    /// lesser first: nothing is planned on both of a pair.
    pub cliffs: BTreeSet<(Tile, Tile)>,
}

/// A thing moved by a change, from the tiles it stood on to those it
/// stands on after.
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
    pub id: u32,
    pub kind: Kind,
    pub from: Vec<Tile>,
    pub to: Vec<Tile>,
}

/// What a change would do: the hub after it, and what it raised or
/// upgraded, moved and took down, by the numbers they had before it.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub hub: Hub,
    pub built: Vec<u32>,
    pub moved: Vec<Moved>,
    pub razed: Vec<u32>,
}

/// How a hub's claim lies on the ground.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lie {
    /// Every tile inside the wall and of it, in `q` then `r` order.
    pub ground: Vec<Tile>,
    /// The tiles of that ground with a neighbour outside it.
    pub wall: Vec<Tile>,
    /// The tile of the wall the fewest steps from the hearth over ground
    /// nothing stands on, the least tile where several are as near. None
    /// where no walk from the hearth reaches the wall.
    pub gate: Option<Tile>,
    /// The tiles a thing may be built on: those inside the claim's own
    /// outline.
    pub buildable: BTreeSet<Tile>,
}

const NEAR: [Tile; 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// A footprint's tiles about its first, by how many it has.
fn shape(size: usize) -> &'static [Tile] {
    match size {
        1 => &[(0, 0)],
        3 => &[(0, 0), (1, 0), (0, 1)],
        4 => &[(0, 0), (1, 0), (0, 1), (1, 1)],
        6 => &[(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)],
        _ => &[(0, 0), (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)],
    }
}

/// Every way a footprint of `size` tiles can lie with one of them on
/// `(0, 0)`, each sorted.
fn ways(size: usize) -> Vec<Vec<Tile>> {
    let mut turned: Vec<Tile> = shape(size).to_vec();
    let mut out: BTreeSet<Vec<Tile>> = BTreeSet::new();
    for _ in 0..6 {
        for &(aq, ar) in &turned {
            let mut tiles: Vec<Tile> = turned.iter().map(|&(q, r)| (q - aq, r - ar)).collect();
            tiles.sort();
            out.insert(tiles);
        }
        turned = turned.iter().map(|&(q, r)| (-r, q + r)).collect();
    }
    out.into_iter().collect()
}

/// Every way a footprint of `size` can lie with a tile on one of `at`,
/// each sorted, none twice.
fn lying(size: usize, at: &[Tile]) -> Vec<Vec<Tile>> {
    let ways = ways(size);
    let mut out: BTreeSet<Vec<Tile>> = BTreeSet::new();
    for &(q, r) in at {
        for way in &ways {
            out.insert(way.iter().map(|&(a, b)| (q + a, r + b)).collect());
        }
    }
    out.into_iter().collect()
}

fn middle(tiles: &[Tile]) -> Point {
    let sum = tiles.iter().map(|&tile| flat(tile)).fold((0.0, 0.0), |sum, p| (sum.0 + p.0, sum.1 + p.1));
    (sum.0 / tiles.len() as f64, sum.1 / tiles.len() as f64)
}

/// How far apart the nearest tiles of two footprints lie.
fn gap(a: &[Tile], b: &[Tile]) -> f64 {
    a.iter().flat_map(|&s| b.iter().map(move |&t| apart(flat(s), flat(t)))).fold(f64::INFINITY, f64::min)
}

/// The tile a point of the flat lies on.
fn tile_at((x, y): Point) -> Tile {
    let fr = y * 2.0 / 3f64.sqrt();
    let fq = x - fr / 2.0;
    let fs = -fq - fr;
    let (mut q, mut r, s) = (fq.round(), fr.round(), fs.round());
    let (dq, dr, ds) = ((q - fq).abs(), (r - fr).abs(), (s - fs).abs());
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

impl Hub {
    /// The tiles it claims: every tile built on, and every one left open.
    pub fn claim(&self) -> BTreeSet<Tile> {
        let mut all = self.open.clone();
        all.extend(self.items.iter().flat_map(|item| item.tiles.iter().copied()));
        all
    }

    pub fn item(&self, id: u32) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    /// The thing standing on `tile`.
    pub fn on(&self, tile: Tile) -> Option<&Item> {
        self.items.iter().find(|item| item.tiles.contains(&tile))
    }

    pub fn hearth(&self) -> Option<Tile> {
        self.items.iter().find(|item| item.kind.what == What::Hearth).map(|item| item.tiles[0])
    }

    pub fn players(&self) -> u32 {
        self.items.iter().map(|item| item.players).sum()
    }

    pub fn settlers(&self) -> u32 {
        self.items.iter().map(|item| item.settlers).sum()
    }

    /// Its players and its settlers.
    pub fn residents(&self) -> u32 {
        self.players() + self.settlers()
    }

    /// How many its homes house.
    pub fn room(&self) -> u32 {
        self.items.iter().map(|item| item.kind.houses()).sum()
    }

    /// How many settlers it takes: none before a hitching post stands,
    /// then one for the post and one for each player living in it.
    pub fn takes(&self) -> u32 {
        if self.items.iter().any(|item| item.kind.what == What::Post) { 1 + self.players() } else { 0 }
    }

    fn push(&mut self, kind: Kind, tiles: Vec<Tile>) -> u32 {
        let id = self.next;
        self.next += 1;
        for tile in &tiles {
            self.open.remove(tile);
        }
        self.items.push(Item { id, kind, tiles, players: 0, settlers: 0 });
        id
    }

    /// Takes thing `id` out of it, its tiles left open ground.
    fn pull(&mut self, id: u32) -> Option<Item> {
        let at = self.items.iter().position(|item| item.id == id)?;
        let item = self.items.remove(at);
        self.open.extend(item.tiles.iter().copied());
        Some(item)
    }

    /// Whether the ground takes a thing on `tiles`: none of them barred,
    /// and no two of them either side of a cliff.
    fn grounded(&self, tiles: &[Tile]) -> bool {
        let astride = |a: &Tile, b: &Tile| a < b && self.cliffs.contains(&(*a, *b));
        tiles.iter().all(|tile| !self.barred.contains(tile)) && !tiles.iter().any(|a| tiles.iter().any(|b| astride(a, b)))
    }

    /// Whether a thing may stand on `tiles`: the ground takes it there
    /// ([`Hub::grounded`]), and it is [`SPACE`] from every other thing
    /// but those `but` names.
    fn fits(&self, tiles: &[Tile], but: &[u32]) -> bool {
        self.grounded(tiles) && self.items.iter().all(|item| but.contains(&item.id) || gap(tiles, &item.tiles) >= SPACE - EPS)
    }

    /// The zone `tile` is drawn as: that whose centre is nearest it, the
    /// earlier of [`Zone::ALL`] where two are as near.
    pub fn zone_at(&self, tile: Tile) -> Option<Zone> {
        let here = flat(tile);
        let mut best: Option<(Zone, f64)> = None;
        for (&zone, &centre) in &self.zones {
            let far = apart(here, flat(centre));
            if best.is_none_or(|(_, near)| far < near - EPS) {
                best = Some((zone, far));
            }
        }
        best.map(|(zone, _)| zone)
    }

    /// Whether `tile` lies in `zone`: no centre is nearer it than the
    /// zone's, so zones centred on one tile share all their ground.
    pub fn in_zone(&self, zone: Zone, tile: Tile) -> bool {
        let Some(&centre) = self.zones.get(&zone) else { return false };
        let (here, far) = (flat(tile), apart(flat(tile), flat(centre)));
        self.zones.values().all(|&other| apart(here, flat(other)) >= far - EPS)
    }

    /// Whether a building has no zone under it: its zone is gone, or it
    /// stands with no tile in it. Such a one is never upgraded.
    pub fn stranded(&self, item: &Item) -> bool {
        item.kind.building() && !item.tiles.iter().any(|&tile| self.in_zone(item.kind.what.zone(), tile))
    }

    /// Whether a building is stranded with no one living in it: it comes
    /// down by hand, or to make room for a thing no open ground takes.
    fn derelict(&self, item: &Item) -> bool {
        self.stranded(item) && item.lived() == 0
    }

    /// Those of `tiles` in the zone of `kind`.
    fn cell(&self, kind: Kind, tiles: &BTreeSet<Tile>) -> BTreeSet<Tile> {
        tiles.iter().copied().filter(|&tile| self.in_zone(kind.what.zone(), tile)).collect()
    }

    /// How its claim lies on the ground.
    pub fn lies(&self) -> Lie {
        let claim: Vec<Tile> = self.claim().into_iter().collect();
        let ground = enclosed(&claim, PAST);
        let held: HashSet<Tile> = ground.iter().copied().collect();
        let wall: Vec<Tile> = ground.iter().copied().filter(|&(q, r)| NEAR.iter().any(|&(a, b)| !held.contains(&(q + a, r + b)))).collect();
        let built: HashSet<Tile> = self.items.iter().flat_map(|item| item.tiles.iter().copied()).collect();
        let mut steps: HashMap<Tile, u32> = HashMap::new();
        if let Some(hearth) = self.hearth() {
            let mut open = VecDeque::from([hearth]);
            steps.insert(hearth, 0);
            while let Some((q, r)) = open.pop_front() {
                for (a, b) in NEAR {
                    let beside = (q + a, r + b);
                    if steps.contains_key(&beside) || !held.contains(&beside) || built.contains(&beside) {
                        continue;
                    }
                    steps.insert(beside, steps[&(q, r)] + 1);
                    open.push_back(beside);
                }
            }
        }
        let gate = wall.iter().copied().filter(|tile| steps.contains_key(tile)).min_by_key(|tile| (steps[tile], *tile));
        Lie { ground, wall, gate, buildable: enclosed(&claim, 0.0).into_iter().collect() }
    }

    /// Settles it once it has changed: its logistics zone on its hearth
    /// where it has none yet, and its settlers as many as it takes and has
    /// room for. One who
    /// comes takes the first room free, and the last to come is the first
    /// to go.
    pub fn peopled(mut self) -> Hub {
        if let Some(hearth) = self.hearth() {
            self.zones.entry(Zone::Logistics).or_insert(hearth);
        }
        let need = self.takes();
        let mut have = self.settlers();
        for item in self.items.iter_mut().filter(|item| item.kind.houses() > 0) {
            while have < need && item.lived() < item.kind.houses() {
                item.settlers += 1;
                have += 1;
            }
        }
        for item in self.items.iter_mut().rev() {
            while have > need && item.settlers > 0 {
                item.settlers -= 1;
                have -= 1;
            }
        }
        self
    }

    /// The place to raise a one-tile thing of `kind` next, in its zone:
    /// beside the most homes lived in, then where it lengthens the wall
    /// least, then nearest its zone's centre.
    fn beside(&self, kind: Kind) -> Option<Tile> {
        let zone = kind.what.zone();
        let centre = flat(*self.zones.get(&zone)?);
        let all = self.claim();
        let tiles: Vec<Tile> = all.iter().copied().collect();
        let now = girth(&tiles);
        let mut seen: HashSet<Tile> = HashSet::new();
        let mut best: Option<(usize, f64, f64, Tile)> = None;
        for &(cq, cr) in &tiles {
            for a in -4..=4 {
                for b in -4..=4 {
                    let tile = (cq + a, cr + b);
                    if !seen.insert(tile) || !self.fits(&[tile], &[]) || !self.in_zone(zone, tile) {
                        continue;
                    }
                    let lived = self.items.iter().filter(|item| item.kind.houses() > 0 && item.lived() > 0 && gap(&[tile], &item.tiles) < SPACE + EPS).count();
                    let longer = if all.contains(&tile) { 0.0 } else { girth(&[&tiles[..], &[tile]].concat()) - now };
                    let far = apart(flat(tile), centre);
                    let better = best.is_none_or(|(most, least, near, at)| {
                        lived > most || lived == most && (longer < least - 1e-9 || (longer - least).abs() <= 1e-9 && (far < near - 1e-9 || (far - near).abs() <= 1e-9 && tile < at))
                    });
                    if better {
                        best = Some((lived, longer, far, tile));
                    }
                }
            }
        }
        best.map(|(.., tile)| tile)
    }

    /// The nearest place to `from` a footprint of `size` may stand, all of
    /// it on `on`, [`SPACE`] from everything but those `but` names.
    fn placed(&self, size: usize, from: Point, on: &BTreeSet<Tile>, but: &[u32]) -> Option<(Vec<Tile>, f64)> {
        let at: Vec<Tile> = on.iter().copied().collect();
        let mut best: Option<(Vec<Tile>, f64)> = None;
        for tiles in lying(size, &at) {
            if !tiles.iter().all(|tile| on.contains(tile)) || !self.fits(&tiles, but) {
                continue;
            }
            let far = apart(middle(&tiles), from);
            if best.as_ref().is_none_or(|(_, near)| far < near - 1e-9) {
                best = Some((tiles, far));
            }
        }
        best
    }

    /// A place for a thing of `kind` and `size` as near `from` as it fits
    /// on `on`: in its own zone, in another where its own has no room,
    /// and where no open ground takes it, on ground a derelict building
    /// holds, which is taken out of the hub and handed back.
    fn seated(&mut self, kind: Kind, size: usize, from: Point, on: &BTreeSet<Tile>, but: &[u32]) -> Option<(Vec<Tile>, f64, Vec<Item>)> {
        let own = self.cell(kind, on);
        if let Some((tiles, far)) = self.placed(size, from, &own, but).or_else(|| self.placed(size, from, on, but)) {
            return Some((tiles, far, Vec::new()));
        }
        let derelict: Vec<u32> = self.items.iter().filter(|item| self.derelict(item)).map(|item| item.id).collect();
        let skip = [but, &derelict[..]].concat();
        let (tiles, far) = self.placed(size, from, &own, &skip).or_else(|| self.placed(size, from, on, &skip))?;
        let near: Vec<u32> = derelict.into_iter().filter(|&id| self.item(id).is_some_and(|item| gap(&tiles, &item.tiles) < SPACE - EPS)).collect();
        let wrecked = near.into_iter().filter_map(|id| self.pull(id)).collect();
        Some((tiles, far, wrecked))
    }

    /// The ground it may build on: inside its claim's outline, and what
    /// it has left open.
    fn land(&self) -> BTreeSet<Tile> {
        let mut land = self.lies().buildable;
        land.extend(self.open.iter().copied());
        land
    }
}

/// A camp: the hearth on `tile`, and ground claimed for four things round
/// it, left open until they are raised.
pub fn camp(tile: Tile) -> Hub {
    let mut hub = Hub { wants: CAMP, zones: BTreeMap::from([(Zone::Logistics, tile), (Zone::Residential, tile)]), items: Vec::new(), open: BTreeSet::new(), next: 1, barred: BTreeSet::new(), cliffs: BTreeSet::new() };
    hub.push(Kind::HEARTH, vec![tile]);
    for what in [What::Post, What::Home, What::Home, What::Wagon] {
        if let Some(at) = hub.beside(Kind::first(what)) {
            hub.push(Kind::first(what), vec![at]);
        }
    }
    let marks: Vec<u32> = hub.items.iter().skip(1).map(|item| item.id).collect();
    for id in marks {
        hub.pull(id);
    }
    hub.next = 2;
    hub.peopled()
}

/// How long a wall a hub for `residents` is planned to: a camp's, times
/// how many camps' worth they are to [`POWER`]. None for none.
pub fn wall_for(residents: u32) -> usize {
    static CAMP_WALL: OnceLock<usize> = OnceLock::new();
    if residents == 0 {
        return 0;
    }
    let camp_wall = *CAMP_WALL.get_or_init(|| camp((0, 0)).lies().wall.len());
    (camp_wall as f64 * (residents as f64 / CAMP as f64).powf(POWER)).round() as usize
}

/// How long a wall a hub's residents hold between them: that planned for
/// as many, and none where no one lives.
pub fn held(hub: &Hub) -> usize {
    wall_for(hub.residents())
}

/// The longest wall a hub's residents defend: what they hold and
/// [`FORGIVEN`] past it. A hub no one lives in defends none.
pub fn defended(hub: &Hub) -> usize {
    if hub.residents() == 0 { 0 } else { held(hub) + FORGIVEN }
}

/// A player moved into home `id` of `hub`, or out of it where `out`. One
/// moving into a home full of settlers puts one out only where another
/// home has room for that settler, who goes there. None where the home is
/// full of players, or the settler has nowhere to go, or no player lives
/// there to leave.
pub fn lodge(hub: &Hub, id: u32, out: bool) -> Option<Outcome> {
    let mut hub = hub.clone();
    let at = hub.items.iter().position(|item| item.id == id && item.kind.houses() > 0)?;
    if out {
        if hub.items[at].players == 0 {
            return None;
        }
        hub.items[at].players -= 1;
    } else {
        let houses = hub.items[at].kind.houses();
        if hub.items[at].players >= houses {
            return None;
        }
        if hub.items[at].lived() >= houses {
            let other = hub.items.iter().position(|item| item.id != id && item.lived() < item.kind.houses())?;
            hub.items[at].settlers -= 1;
            hub.items[other].settlers += 1;
        }
        hub.items[at].players += 1;
    }
    Some(Outcome { hub: hub.peopled(), built: Vec::new(), moved: Vec::new(), razed: Vec::new() })
}

/// A building of `what` raised in `hub` as near its zone's centre as it
/// fits on the hub's claim: in its zone, in another only where its own
/// has no room, and where no open ground takes it, on ground a derelict
/// building holds, which comes down for it. None where its zone is not
/// placed, it fits nowhere, or it would close the walk from the hearth to
/// the wall.
pub fn raise(hub: &Hub, what: What) -> Option<Outcome> {
    let kind = Kind::first(what);
    if !kind.building() {
        return None;
    }
    let mut hub = hub.clone();
    let centre = flat(*hub.zones.get(&what.zone())?);
    let land = hub.land();
    let (tiles, _, wrecked) = hub.seated(kind, kind.size(), centre, &land, &[])?;
    let id = hub.push(kind, tiles);
    hub.lies().gate?;
    Some(Outcome { hub: hub.peopled(), built: vec![id], moved: Vec::new(), razed: wrecked.iter().map(|item| item.id).collect() })
}

/// A building of `what` raised on `tile` of `hub`, where a player puts it:
/// the tile is on the hub's claim, in the building's zone, and
/// [`SPACE`] from everything standing. None where it is not.
pub fn raise_at(hub: &Hub, what: What, tile: Tile) -> Option<Outcome> {
    let kind = Kind::first(what);
    if !kind.building() || !hub.land().contains(&tile) || !hub.in_zone(what.zone(), tile) || !hub.fits(&[tile], &[]) {
        return None;
    }
    let mut hub = hub.clone();
    let id = hub.push(kind, vec![tile]);
    hub.lies().gate?;
    Some(Outcome { hub: hub.peopled(), built: vec![id], moved: Vec::new(), razed: Vec::new() })
}

/// The tiles of `hub` a player may raise a building of `what` on, as
/// [`raise_at`] takes them.
pub fn sites(hub: &Hub, what: What) -> Vec<Tile> {
    hub.land().into_iter().filter(|&tile| hub.in_zone(what.zone(), tile) && hub.fits(&[tile], &[])).collect()
}

/// Building `id` of `hub` upgraded: the change that changes least. Its
/// footprint grows over as many of its own tiles as the new shape lies
/// on. A home takes in the lesser homes that then stand too near, and the
/// nearest after them, for as long as it has the room theirs gave, so the
/// hub houses as many as it did on less ground. Whatever else stands too
/// near moves to the nearest place it fits, and a derelict building comes
/// down. Nothing leaves the hub's claim unless there is no room on it at
/// all. None where it is upgraded no further or is stranded.
pub fn upgrade(hub0: &Hub, id: u32) -> Option<Outcome> {
    let from = hub0.item(id)?.clone();
    let to = from.kind.next()?;
    if hub0.stranded(&from) {
        return None;
    }
    let land = hub0.lies().buildable;
    let kept = |tiles: &[Tile]| from.tiles.iter().filter(|tile| tiles.contains(tile)).count();
    // Every way the larger footprint lies over the thing's own tiles, that the ground takes
    let each: Vec<Vec<Tile>> = lying(to.size(), &from.tiles).into_iter().filter(|tiles| hub0.grounded(tiles)).collect();
    let most = each.iter().map(|tiles| kept(tiles)).max()?;
    let mut best: Option<(Vec<f64>, Outcome)> = None;
    for tiles in each.into_iter().filter(|tiles| kept(tiles) == most) {
        let mut hub = hub0.clone();
        let mut razed: Vec<Item> = Vec::new();
        let mut moved: Vec<Moved> = Vec::new();
        let mut wrecked = 0;
        let mut mine = hub.pull(id)?;
        mine.kind = to;
        mine.tiles = tiles.clone();
        let mut spare = to.houses() as i64 - from.kind.houses() as i64;
        let lesser = |item: &Item, spare: i64| item.kind.houses() > 0 && item.kind.houses() < to.houses() && item.kind.houses() as i64 <= spare;
        // What then stands too near: a lesser home is taken in, a derelict building comes down, the rest move
        let mut near: Vec<(u32, u32)> = hub.items.iter().filter(|item| gap(&tiles, &item.tiles) < SPACE - EPS).map(|item| (item.lived(), item.id)).collect();
        near.sort();
        let mut shift: Vec<u32> = Vec::new();
        for (_, other) in near {
            let item = hub.item(other)?;
            if lesser(item, spare) {
                let item = hub.pull(other)?;
                mine.players += item.players;
                mine.settlers += item.settlers;
                spare -= item.kind.houses() as i64;
                razed.push(item);
            } else if hub.derelict(item) {
                razed.extend(hub.pull(other));
                wrecked += 1;
            } else {
                shift.push(other);
            }
        }
        // The hearth is the hub's heart: a change that would move it is the last taken
        let heart = shift.iter().any(|&other| hub.item(other).is_some_and(|item| item.kind.what == What::Hearth));
        let mut out = tiles.iter().any(|tile| !land.contains(tile));
        for tile in &tiles {
            hub.open.remove(tile);
        }
        hub.items.push(mine);
        shift.sort_by_key(|&other| (std::cmp::Reverse(hub.item(other).map_or(0, |item| item.tiles.len())), other));
        let mut seats = true;
        for other in shift {
            let item = hub.item(other)?.clone();
            let was = middle(&item.tiles);
            let mut spot = None;
            loop {
                let mut all = land.clone();
                all.extend(hub.open.iter().copied());
                if let Some((to, far, down)) = hub.seated(item.kind, item.tiles.len(), was, &all, &[other]) {
                    wrecked += down.len();
                    razed.extend(down);
                    spot = Some((to, far));
                    break;
                }
                // No room: the nearest lesser home is taken in to make it
                let give = hub.items.iter().filter(|o| o.id != id && o.id != other && lesser(o, spare)).min_by(|a, b| apart(middle(&a.tiles), was).total_cmp(&apart(middle(&b.tiles), was)).then(a.id.cmp(&b.id))).map(|o| o.id);
                let Some(give) = give else { break };
                let taken = hub.pull(give)?;
                let mine = hub.items.iter_mut().find(|item| item.id == id)?;
                mine.players += taken.players;
                mine.settlers += taken.settlers;
                spare -= taken.kind.houses() as i64;
                razed.push(taken);
            }
            if spot.is_none() {
                // No room on the claim: the nearest place past it, and the wall goes out
                let wide: BTreeSet<Tile> = land.iter().flat_map(|&(q, r)| (-6..=6).flat_map(move |a| (-6..=6).map(move |b| (q + a, r + b)))).collect();
                spot = hub.placed(item.tiles.len(), was, &hub.cell(item.kind, &wide), &[other]).or_else(|| hub.placed(item.tiles.len(), was, &wide, &[other]));
                out = true;
            }
            let Some((to, _)) = spot else {
                seats = false;
                break;
            };
            hub.open.extend(item.tiles.iter().copied());
            for tile in &to {
                hub.open.remove(tile);
            }
            moved.push(Moved { id: other, kind: item.kind, from: item.tiles.clone(), to: to.clone() });
            hub.items.iter_mut().find(|item| item.id == other)?.tiles = to;
        }
        if !seats {
            continue;
        }
        // Then it fills: the nearest lesser homes come in for as long as it has the room theirs gave
        let mid = middle(&tiles);
        while let Some(give) = hub.items.iter().filter(|o| o.id != id && lesser(o, spare)).min_by(|a, b| apart(middle(&a.tiles), mid).total_cmp(&apart(middle(&b.tiles), mid)).then(a.id.cmp(&b.id))).map(|o| o.id) {
            let taken = hub.pull(give)?;
            let mine = hub.items.iter_mut().find(|item| item.id == id)?;
            mine.players += taken.players;
            mine.settlers += taken.settlers;
            spare -= taken.kind.houses() as i64;
            razed.push(taken);
        }
        let hub = hub.peopled();
        if hub.lies().gate.is_none() {
            continue;
        }
        let astray = tiles.iter().filter(|&&tile| !hub.in_zone(to.what.zone(), tile)).count();
        let changed: usize = moved.iter().map(|m| m.from.len()).sum::<usize>() + razed.iter().map(|item| item.tiles.len()).sum::<usize>();
        let walked: f64 = moved.iter().map(|m| apart(middle(&m.from), middle(&m.to))).sum();
        let score = vec![heart as u8 as f64, out as u8 as f64, wrecked as f64, astray as f64, (moved.len() + razed.len()) as f64, changed as f64, walked];
        let less = |a: &[f64], b: &[f64]| a.iter().zip(b).find(|(a, b)| (**a - **b).abs() > 1e-9).is_some_and(|(a, b)| a < b);
        if best.as_ref().is_none_or(|(least, _)| less(&score, least)) {
            best = Some((score, Outcome { hub, built: vec![id], moved, razed: razed.iter().map(|item| item.id).collect() }));
        }
    }
    best.map(|(_, outcome)| outcome)
}

/// Building `id` of `hub` a level down: on as many of its own tiles as
/// the smaller shape lies on, the rest left open ground, and taken down
/// from the first level. Its settlers find room elsewhere. None for the
/// hearth, or a home with more players living in it than it would house.
pub fn downgrade(hub: &Hub, id: u32) -> Option<Outcome> {
    let from = hub.item(id)?.clone();
    if !from.kind.building() {
        return None;
    }
    let to = from.kind.before();
    let houses = to.map_or(0, Kind::houses);
    if from.players > houses {
        return None;
    }
    let mut hub = hub.clone();
    let mut mine = hub.pull(id)?;
    if let Some(to) = to {
        let kept = |tiles: &[Tile]| from.tiles.iter().filter(|tile| tiles.contains(tile)).count();
        let tiles = lying(to.size(), &from.tiles).into_iter().filter(|tiles| hub.fits(tiles, &[])).max_by(|a, b| kept(a).cmp(&kept(b)).then(b.cmp(a)))?;
        for tile in &tiles {
            hub.open.remove(tile);
        }
        mine.kind = to;
        mine.tiles = tiles;
        mine.settlers = mine.settlers.min(houses - mine.players);
        hub.items.push(mine);
    }
    let hub = lapsed(hub).peopled();
    hub.lies().gate?;
    Some(Outcome { hub, built: to.map(|_| id).into_iter().collect(), moved: Vec::new(), razed: to.is_none().then_some(id).into_iter().collect() })
}

/// `zone` of `hub` centred on `tile`, placed or moved: nothing standing
/// changes, and what is built from then on follows it. None before the
/// hub is planned for as many as the zone takes, or anywhere but inside
/// its wall. The logistics zone starts on the hearth, as the residential
/// does, and is moved as any other.
pub fn zone(hub: &Hub, zone: Zone, tile: Tile) -> Option<Outcome> {
    let lie = hub.lies();
    if hub.wants < zone.from() || lie.ground.binary_search(&tile).is_err() || lie.wall.contains(&tile) {
        return None;
    }
    let mut hub = hub.clone();
    hub.zones.insert(zone, tile);
    Some(Outcome { hub, built: Vec::new(), moved: Vec::new(), razed: Vec::new() })
}

/// A zone the hub is planned for too few to keep is gone at once, its
/// centre with it. What was built in it is left standing, stranded.
fn lapsed(mut hub: Hub) -> Hub {
    let wants = hub.wants;
    hub.zones.retain(|zone, _| zone.from() <= wants);
    hub
}

/// Each zone's centre left outside `hub`'s wall moved toward its hearth
/// until it is inside.
fn drawn_in(mut hub: Hub) -> Hub {
    let Some(hearth) = hub.hearth() else { return hub };
    let lie = hub.lies();
    let inside = |tile: &Tile| lie.ground.binary_search(tile).is_ok() && !lie.wall.contains(tile);
    let to = flat(hearth);
    for centre in hub.zones.values_mut() {
        let from = flat(*centre);
        if let Some(tile) = (0..=64).map(|k| tile_at((from.0 + (to.0 - from.0) * k as f64 / 64.0, from.1 + (to.1 - from.1) * k as f64 / 64.0))).find(inside) {
            *centre = tile;
        }
    }
    hub
}

/// A piece of what a hub claims, weighed as it is drawn in: a thing
/// standing, or a tile left open.
#[derive(Clone)]
enum Part {
    Thing(Item),
    Open(Tile),
}

impl Part {
    fn tiles(&self) -> Vec<Tile> {
        match self {
            Part::Thing(item) => item.tiles.clone(),
            Part::Open(tile) => vec![*tile],
        }
    }
}

/// `hub` drawn in until its wall is no longer than `allowed`, planned for
/// `wants` after. Every home someone lives in is kept. Its hearth stays
/// where it stands while the wall round it and those homes is one
/// allowed; past that it stays if it stands among the homes, and is laid
/// again beside them if not. Then what it claims is weighed a part at a
/// time: a home it lacks room without first, then what stands by the
/// order its zones come in, then open ground, each the part that
/// lengthens the wall least and of those the nearest its people. A part
/// the wall cannot hold is given up and the next tried. A home given up is
/// raised again inside the wall where it fits. A hub is never less than
/// its hearth and room for its people: only there is its wall left longer
/// than allowed.
fn fitted(hub: &Hub, allowed: usize, wants: u32) -> Outcome {
    let mut parts: Vec<Part> = hub.items.iter().cloned().map(Part::Thing).chain(hub.open.iter().copied().map(Part::Open)).collect();
    let mut next = hub.next;
    let of = |parts: &[Part], which: &[usize]| -> Vec<Tile> { which.iter().flat_map(|&k| parts[k].tiles()).collect() };
    let hub_of = |parts: &[Part], which: &[usize], next: u32| -> Hub {
        let mut made = Hub { wants: hub.wants, zones: hub.zones.clone(), items: Vec::new(), open: BTreeSet::new(), next, barred: hub.barred.clone(), cliffs: hub.cliffs.clone() };
        for &k in which {
            match &parts[k] {
                Part::Thing(item) => made.items.push(item.clone()),
                Part::Open(tile) => {
                    made.open.insert(*tile);
                }
            }
        }
        made
    };
    let land = hub.land();
    let mut kept: Vec<usize> = (0..parts.len()).filter(|&k| matches!(&parts[k], Part::Thing(item) if item.kind.houses() > 0 && item.lived() > 0)).collect();
    let heart = (0..parts.len()).find(|&k| matches!(&parts[k], Part::Thing(item) if item.kind.what == What::Hearth));
    let about = middle(&of(&parts, if kept.is_empty() { heart.as_slice() } else { &kept }));
    let wide: BTreeSet<Tile> = land.iter().flat_map(|&(q, r)| (-6..=6).flat_map(move |a| (-6..=6).map(move |b| (q + a, r + b)))).collect();
    let mut moved: Vec<Moved> = Vec::new();
    let mut built: Vec<u32> = Vec::new();
    let whole = hub_of(&parts, &[&kept[..], heart.as_slice()].concat(), next).lies().wall.len() <= allowed;
    let among = heart.is_some_and(|heart| {
        let now = hub_of(&parts, &kept, next);
        (kept.is_empty() || girth(&of(&parts, &[&kept[..], &[heart]].concat())) - girth(&of(&parts, &kept)) <= 1e-9) && now.fits(&parts[heart].tiles(), &[])
    });
    match heart {
        Some(heart) if whole || among => kept.push(heart),
        _ => {
            let now = hub_of(&parts, &kept, next);
            let spot = now.placed(1, about, &land, &[]).or_else(|| now.placed(1, about, &wide, &[]));
            if let Some((to, far)) = spot {
                match heart {
                    // A part of a hub that parted has none of its own: one is laid beside its people
                    None => {
                        parts.push(Part::Thing(Item { id: next, kind: Kind::HEARTH, tiles: to, players: 0, settlers: 0 }));
                        built.push(next);
                        next += 1;
                        kept.push(parts.len() - 1);
                    }
                    Some(heart) => {
                        let from = parts[heart].tiles();
                        if !now.fits(&from, &[]) || far < apart(middle(&from), about) - 1e-9 {
                            if let Part::Thing(item) = &mut parts[heart] {
                                moved.push(Moved { id: item.id, kind: item.kind, from, to: to.clone() });
                                item.tiles = to;
                            }
                        }
                        kept.push(heart);
                    }
                }
            } else if let Some(heart) = heart {
                kept.push(heart);
            }
        }
    }
    let need = 2 * hub.players().max(1);
    let mut rest: Vec<usize> = (0..parts.len()).filter(|k| !kept.contains(k)).collect();
    let mut gone: Vec<usize> = Vec::new();
    while !rest.is_empty() {
        // What no longer fits beside what was moved comes down with the rest
        let now = hub_of(&parts, &kept, next);
        let under: HashSet<Tile> = now.items.iter().flat_map(|item| item.tiles.iter().copied()).collect();
        let clash: Vec<usize> = rest.iter().copied().filter(|&k| match &parts[k] { Part::Thing(item) => !now.fits(&item.tiles, &[]), Part::Open(tile) => under.contains(tile) }).collect();
        if !clash.is_empty() {
            rest.retain(|k| !clash.contains(k));
            gone.extend(clash);
            continue;
        }
        let held = of(&parts, &kept);
        let edge = girth(&held);
        let short = now.room() < need;
        let mut best: Option<(i32, f64, f64, usize)> = None;
        for &k in &rest {
            let owed = match &parts[k] {
                Part::Thing(item) if short && item.kind.houses() > 0 => -1,
                Part::Thing(item) => Zone::ALL.iter().position(|&zone| zone == item.kind.what.zone()).unwrap_or(0) as i32,
                Part::Open(_) => Zone::ALL.len() as i32,
            };
            let tiles = parts[k].tiles();
            let longer = girth(&[&held[..], &tiles[..]].concat()) - edge;
            let far = apart(middle(&tiles), about);
            if best.is_none_or(|(first, least, near, _)| owed < first || owed == first && (longer < least - 1e-9 || (longer - least).abs() <= 1e-9 && far < near - 1e-9)) {
                best = Some((owed, longer, far, k));
            }
        }
        let Some((owed, _, _, k)) = best else { break };
        rest.retain(|&other| other != k);
        // What the wall cannot hold is given up, and the next is tried
        if owed >= 0 && hub_of(&parts, &[&kept[..], &[k]].concat(), next).lies().wall.len() > allowed {
            gone.push(k);
        } else {
            kept.push(k);
        }
    }
    let mut out = lapsed(Hub { wants, ..hub_of(&parts, &kept, next) });
    // A home the wall could not hold is raised again inside it, the largest first
    let mut razed: Vec<u32> = Vec::new();
    let mut homes: Vec<Item> = gone.iter().filter_map(|&k| match &parts[k] { Part::Thing(item) if item.kind.houses() > 0 => Some(item.clone()), _ => None }).collect();
    homes.sort_by_key(|item| (std::cmp::Reverse(item.tiles.len()), item.id));
    let mut back: Vec<u32> = Vec::new();
    for mut home in homes {
        let Some(&centre) = out.zones.get(&Zone::Residential) else { break };
        let land = out.land();
        let Some((to, _, wrecked)) = out.seated(home.kind, home.tiles.len(), flat(centre), &land, &[]) else { continue };
        razed.extend(wrecked.iter().map(|item| item.id));
        moved.push(Moved { id: home.id, kind: home.kind, from: home.tiles.clone(), to: to.clone() });
        for tile in &to {
            out.open.remove(tile);
        }
        back.push(home.id);
        home.tiles = to;
        home.settlers = 0;
        out.items.push(home);
    }
    razed.extend(gone.iter().filter_map(|&k| match &parts[k] { Part::Thing(item) if !back.contains(&item.id) => Some(item.id), _ => None }));
    Outcome { hub: drawn_in(out.peopled()), built, moved, razed }
}

/// Whether the grounds of two hubs touch.
pub fn touch(a: &Hub, b: &Hub) -> bool {
    let other: HashSet<Tile> = b.lies().ground.into_iter().collect();
    a.lies().ground.into_iter().any(|(q, r)| other.contains(&(q, r)) || NEAR.iter().any(|&(x, y)| other.contains(&(q + x, r + y))))
}

/// What `hub` becomes once its residents defend every wall, an outcome to
/// each hub left. Where even the wall round its players' homes alone is
/// more than they defend, it parts in two between the two of those homes
/// furthest apart, each thing and each open tile going with the nearer,
/// and each part is drawn in on its own. Parts whose grounds would touch
/// are no parts.
fn parted(hub: &Hub, deep: u32) -> Vec<Outcome> {
    let homes: Vec<&Item> = hub.items.iter().filter(|item| item.players > 0).collect();
    let alone = Hub { wants: hub.wants, zones: BTreeMap::new(), items: homes.iter().map(|&item| item.clone()).collect(), open: BTreeSet::new(), next: hub.next, barred: hub.barred.clone(), cliffs: hub.cliffs.clone() };
    if homes.len() > 1 && deep < 6 && alone.lies().wall.len() > defended(hub) {
        let mut far: Option<(f64, Point, Point)> = None;
        for a in &homes {
            for b in &homes {
                let (one, other) = (middle(&a.tiles), middle(&b.tiles));
                if far.is_none_or(|(most, ..)| apart(one, other) > most + 1e-9) {
                    far = Some((apart(one, other), one, other));
                }
            }
        }
        if let Some((_, one, other)) = far {
            let nearer = |tiles: &[Tile]| apart(middle(tiles), one) <= apart(middle(tiles), other);
            let side = |which: bool| Hub {
                wants: hub.wants,
                zones: hub.zones.clone(),
                items: hub.items.iter().filter(|item| nearer(&item.tiles) == which).cloned().collect(),
                open: hub.open.iter().copied().filter(|&tile| nearer(&[tile]) == which).collect(),
                next: hub.next,
                barred: hub.barred.clone(),
                cliffs: hub.cliffs.clone(),
            };
            let parts: Vec<Outcome> = [parted(&side(true), deep + 1), parted(&side(false), deep + 1)].concat();
            if !(0..parts.len()).any(|i| (i + 1..parts.len()).any(|j| touch(&parts[i].hub, &parts[j].hub))) {
                return parts;
            }
        }
    }
    vec![fitted(hub, defended(hub), 2 * hub.players().max(1))]
}

/// `hub` drawn in until its residents defend its wall, in as many hubs as
/// that takes: an outcome to each, or none where they defend it already,
/// or where no player lives in it to draw it in round.
pub fn shrink(hub: &Hub) -> Option<Vec<Outcome>> {
    (hub.players() > 0 && hub.lies().wall.len() > defended(hub)).then(|| parted(hub, 0))
}

/// `hub` planned for `want` residents. Planned for more, it claims ground
/// and nothing else: a little at a time, each piece the free ground
/// nearest any zone's centre, for as long as its wall is no longer than
/// `want` are planned, so its zones' centres are where it swells. Nothing
/// is built and nothing standing changes. Planned for fewer, it is drawn
/// in to that wall as [`shrink`] draws it; none where more live in it.
pub fn plan(hub: &Hub, want: u32) -> Option<Outcome> {
    if want < hub.wants {
        return (hub.residents() <= want).then(|| fitted(hub, wall_for(want), want));
    }
    let most = wall_for(want);
    let mut hub = hub.clone();
    hub.wants = want;
    let centres: Vec<Point> = hub.zones.values().map(|&centre| flat(centre)).collect();
    let reach = most as f64 / 3.0;
    let span = (reach * 1.2).ceil() as i32;
    let mut seen: HashSet<Tile> = HashSet::new();
    let mut sites: Vec<(f64, Tile)> = Vec::new();
    for &(cq, cr) in hub.zones.values() {
        for a in -span..=span {
            for b in -span..=span {
                let tile = (cq + a, cr + b);
                if !seen.insert(tile) {
                    continue;
                }
                let far = centres.iter().map(|&centre| apart(centre, flat(tile))).fold(f64::INFINITY, f64::min);
                if far <= reach {
                    sites.push((far, tile));
                }
            }
        }
    }
    sites.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    // A piece of ground stands as far from the next as a home would: marked as one while the rest are found
    let mut lie = hub.lies();
    let mut marks: Vec<Tile> = Vec::new();
    let from = hub.items.len();
    if lie.wall.len() <= most {
        for (_, tile) in sites {
            if !hub.fits(&[tile], &[]) {
                continue;
            }
            hub.items.push(Item { id: 0, kind: Kind::first(What::Home), tiles: vec![tile], players: 0, settlers: 0 });
            if !lie.buildable.contains(&tile) {
                let then = hub.lies();
                if then.wall.len() > most {
                    hub.items.pop();
                    break;
                }
                lie = then;
            }
            marks.push(tile);
        }
    }
    hub.items.truncate(from);
    hub.open.extend(marks);
    Some(Outcome { hub: hub.peopled(), built: Vec::new(), moved: Vec::new(), razed: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(outcome: Option<Outcome>) -> Hub {
        outcome.expect("the change is one the rules take").hub
    }

    /// What must hold of any hub, however it came to be.
    fn sound(hub: &Hub) {
        let lie = hub.lies();
        let mut stood: HashSet<Tile> = HashSet::new();
        for item in &hub.items {
            assert_eq!(item.tiles.len(), item.kind.size(), "{:?} stands on its footprint", item.kind);
            assert!(item.lived() <= item.kind.houses(), "{:?} houses no more than it has room for", item.kind);
            for tile in &item.tiles {
                assert!(stood.insert(*tile), "one thing to a tile");
                assert!(lie.ground.binary_search(tile).is_ok() && !lie.wall.contains(tile), "what stands is inside the wall");
            }
        }
        for (a, one) in hub.items.iter().enumerate() {
            for other in &hub.items[a + 1..] {
                assert!(gap(&one.tiles, &other.tiles) >= SPACE - EPS, "{:?} and {:?} stand apart", one.kind, other.kind);
            }
        }
        assert_eq!(hub.items.iter().filter(|item| item.kind.what == What::Hearth).count(), 1, "a hub has one hearth");
        assert!(hub.zones.contains_key(&Zone::Logistics), "a hub has its logistics zone");
        for &centre in hub.zones.values() {
            assert!(lie.ground.binary_search(&centre).is_ok() && !lie.wall.contains(&centre), "a zone's centre is inside the wall");
        }
        assert!(lie.gate.is_some(), "the wall is walked to from the hearth");
        assert!(hub.settlers() <= hub.takes(), "no more settlers than a hub takes");
    }

    fn homes(hub: &Hub) -> Vec<u32> {
        hub.items.iter().filter(|item| item.kind.houses() > 0).map(|item| item.id).collect()
    }

    /// A camp with the homes its plan has room for raised.
    fn housed(mut hub: Hub) -> Hub {
        while hub.room() < hub.wants {
            let Some(more) = raise(&hub, What::Home) else { break };
            hub = more.hub;
        }
        hub
    }

    /// A camp is its hearth alone, on the wall a hub for two is planned
    /// to, and that ground takes four things raised round the hearth
    /// without the wall moving.
    #[test]
    fn a_camp_is_its_hearth_on_ground_for_four() {
        let camp = camp((3, -3));
        sound(&camp);
        assert_eq!(camp.items.len(), 1);
        assert_eq!(camp.residents(), 0);
        let wall = camp.lies().wall.len();
        assert_eq!(wall, wall_for(CAMP));
        assert_eq!(defended(&camp), 0);
        let mut hub = camp;
        for what in [What::Post, What::Home, What::Wagon, What::Store] {
            hub = done(raise(&hub, what));
            sound(&hub);
            assert_eq!(hub.lies().wall.len(), wall, "a camp's ground takes its {what:?}");
        }
    }

    /// A hub planned for four has ground for a post, a wagon, a store and
    /// four homes.
    #[test]
    fn a_hub_for_four_takes_four_homes() {
        let mut hub = done(plan(&camp((0, 0)), 4));
        let wall = hub.lies().wall.len();
        assert!(wall <= wall_for(4));
        for what in [What::Post, What::Wagon, What::Store, What::Home, What::Home, What::Home, What::Home] {
            hub = done(raise(&hub, what));
        }
        sound(&hub);
        assert_eq!(hub.lies().wall.len(), wall);
    }

    /// No settler comes before a hitching post stands. Then one comes for
    /// the post once it has a home, and one more for each player.
    #[test]
    fn settlers_come_for_the_post_and_for_each_player() {
        let mut hub = done(raise(&camp((0, 0)), What::Home));
        assert_eq!(hub.settlers(), 0);
        hub = done(raise(&hub, What::Post));
        assert_eq!(hub.settlers(), 1);
        hub = housed(done(plan(&hub, 4)));
        let open = hub.items.iter().find(|item| item.kind.houses() > 0 && item.lived() == 0).unwrap().id;
        hub = done(lodge(&hub, open, false));
        assert_eq!((hub.players(), hub.settlers()), (1, 2));
        hub = done(lodge(&hub, open, true));
        assert_eq!((hub.players(), hub.settlers()), (0, 1));
        sound(&hub);
    }

    /// A player puts a settler out of a home only where another home has
    /// room for that settler.
    #[test]
    fn a_player_displaces_a_settler_only_with_room_elsewhere() {
        let hub = done(raise(&done(raise(&camp((0, 0)), What::Post)), What::Home));
        let first = homes(&hub)[0];
        assert_eq!(hub.item(first).unwrap().settlers, 1);
        assert!(lodge(&hub, first, false).is_none());
        let roomier = done(raise(&done(plan(&hub, 4)), What::Home));
        let moved = done(lodge(&roomier, first, false));
        assert_eq!(moved.item(first).unwrap().settlers, 0);
        // Two homes of one: the player in the first, the settler in the other, and no room for the player's own settler
        assert_eq!((moved.players(), moved.settlers()), (1, 1));
        assert!(lodge(&moved, first, false).is_none());
        sound(&moved);
    }

    /// The wall a hub is planned to grows with its residents, less for
    /// each as they grow, and planning for more builds nothing: it claims
    /// ground out to that wall and no further.
    #[test]
    fn planning_for_more_claims_ground_and_builds_nothing() {
        assert_eq!(wall_for(0), 0);
        let camp = camp((0, 0));
        for n in [3, 4, 10, 27, 50] {
            let (most, less) = (wall_for(n), wall_for(n - 1));
            assert!(most > less);
            assert!((most as f64 / n as f64) < less as f64 / (n - 1) as f64);
            let planned = done(plan(&camp, n));
            assert_eq!(planned.items, camp.items);
            let wall = planned.lies().wall.len();
            assert!(wall <= most && wall + 4 >= most, "planned for {n}: a wall of {wall}, planned {most}");
            sound(&planned);
        }
    }

    /// A building goes up in its own zone, and none before its zone is
    /// placed. A zone is placed once the hub is planned for enough, on any
    /// tile inside the wall, and placing it changes nothing standing.
    #[test]
    fn a_building_is_raised_in_its_own_zone() {
        let small = housed(done(plan(&camp((0, 0)), Zone::Market.from() - 1)));
        assert!(zone(&small, Zone::Market, (2, 0)).is_none());
        assert!(raise(&small, What::Market).is_none());
        let hub = housed(done(plan(&camp((0, 0)), 20)));
        let moved = done(zone(&hub, Zone::Logistics, (2, 0)));
        assert_eq!((moved.zones[&Zone::Logistics], moved.hearth()), ((2, 0), hub.hearth()), "the logistics zone is moved as any other, and the hearth stays");
        assert_eq!(moved.clone().peopled().zones[&Zone::Logistics], (2, 0), "and stays where it was put");
        let wall = hub.lies().wall[0];
        assert!(zone(&hub, Zone::Market, wall).is_none());
        let zoned = done(zone(&hub, Zone::Market, (4, -2)));
        assert_eq!(zoned.items, hub.items);
        let built = raise(&zoned, What::Market).unwrap();
        let shop = built.hub.item(built.built[0]).unwrap();
        assert!(built.hub.in_zone(Zone::Market, shop.tiles[0]));
        assert!(!built.hub.stranded(shop));
        sound(&built.hub);
    }

    /// Every building is upgraded through the same footprints and comes
    /// down through them again, and an upgrade or a downgrade never
    /// lengthens the wall where the hub has room.
    #[test]
    fn every_building_grows_by_the_same_footprints() {
        let mut hub = housed(done(plan(&camp((0, 0)), 40)));
        hub = done(zone(&hub, Zone::Market, (0, 0)));
        let built = raise(&hub, What::Market).unwrap();
        let shop = built.built[0];
        hub = built.hub;
        let wall = hub.lies().wall.len();
        for size in &SIZES[1..] {
            hub = done(upgrade(&hub, shop));
            assert_eq!(hub.item(shop).unwrap().tiles.len(), *size);
            assert!(hub.lies().wall.len() <= wall);
            sound(&hub);
        }
        assert!(upgrade(&hub, shop).is_none());
        for size in SIZES[..SIZES.len() - 1].iter().rev() {
            hub = done(downgrade(&hub, shop));
            assert_eq!(hub.item(shop).unwrap().tiles.len(), *size);
            sound(&hub);
        }
        hub = done(downgrade(&hub, shop));
        assert!(hub.item(shop).is_none());
        assert!(hub.hearth().and_then(|tile| hub.on(tile)).is_some_and(|hearth| upgrade(&hub, hearth.id).is_none() && downgrade(&hub, hearth.id).is_none()));
    }

    /// A home upgraded takes in lesser homes while it has the room theirs
    /// gave: the hub houses no fewer, puts no one out, and has open ground
    /// it had not.
    #[test]
    fn an_upgrade_houses_as_many_on_less_ground() {
        let mut hub = housed(done(plan(&done(raise(&camp((0, 0)), What::Post)), 12)));
        for id in homes(&hub).into_iter().take(4) {
            if let Some(lodged) = lodge(&hub, id, false) {
                hub = lodged.hub;
            }
        }
        let (people, room, open, wall) = (hub.residents(), hub.room(), hub.open.len(), hub.lies().wall.len());
        let first = homes(&hub)[0];
        let grown = upgrade(&hub, first).unwrap();
        assert!(!grown.razed.is_empty(), "a lesser home is taken in");
        assert_eq!(grown.hub.players(), hub.players());
        assert!(grown.hub.residents() >= people.min(grown.hub.room()) && grown.hub.room() >= room);
        assert!(grown.hub.open.len() > open);
        assert_eq!(grown.hub.lies().wall.len(), wall);
        sound(&grown.hub);
        let full = grown.hub.item(first).unwrap();
        let mut packed = grown.hub.clone();
        let at = packed.items.iter().position(|item| item.id == first).unwrap();
        packed.items[at].settlers = 0;
        packed.items[at].players = full.kind.houses();
        assert!(downgrade(&packed, first).is_none(), "a full home is not downgraded under its players");
    }

    /// Nothing is planned on a barred tile, and nothing on both of two
    /// tiles with a cliff between them: a thing raised or upgraded goes
    /// round them, or is not planned at all.
    #[test]
    fn a_plan_goes_round_what_the_ground_bars() {
        let hub = done(plan(&camp((0, 0)), 4));
        let free = sites(&hub, What::Home);
        let first = done(raise(&hub, What::Home)).items.last().unwrap().tiles[0];
        let barred = Hub { barred: BTreeSet::from([first]), ..hub.clone() };
        let moved = done(raise(&barred, What::Home)).items.last().unwrap().tiles[0];
        assert!(free.contains(&first) && moved != first && !sites(&barred, What::Home).contains(&first));
        assert_eq!(raise_at(&barred, What::Home, first), None);
        // A home raised, then every pair of tiles about it parted by a cliff: it has nowhere to grow
        let homed = done(raise(&hub, What::Home));
        let home = homed.items.last().unwrap().clone();
        assert!(upgrade(&homed, home.id).is_some());
        let at = home.tiles[0];
        let about: Vec<Tile> = (-3..=3).flat_map(|q| (-3..=3).map(move |r| (at.0 + q, at.1 + r))).collect();
        let cliffs = about.iter().flat_map(|&a| NEAR.iter().map(move |&(q, r)| (a, (a.0 + q, a.1 + r)))).filter(|(a, b)| a < b).collect();
        assert_eq!(upgrade(&Hub { cliffs, ..homed.clone() }, home.id), None, "no footprint lies either side of a cliff");
    }

    /// A zone the hub is planned for too few to keep is gone at once and
    /// what stood in it stays, stranded: never upgraded, no more of its
    /// kind raised, and it comes down only by hand or for a building no
    /// open ground takes. A building left outside its zone when the centre
    /// moves is stranded the same, until the centre comes back.
    #[test]
    fn a_building_outside_its_zone_is_stranded() {
        let from = Zone::Market.from();
        let hub = housed(done(plan(&camp((0, 0)), from)));
        let built = raise(&done(zone(&hub, Zone::Market, (2, 0))), What::Market).unwrap();
        let (shop, hub) = (built.built[0], built.hub);
        let down = plan(&hub, from - 1).unwrap();
        assert!(!down.hub.zones.contains_key(&Zone::Market));
        assert!(down.razed.is_empty() && down.moved.is_empty());
        let stood = down.hub.item(shop).unwrap();
        assert!(down.hub.stranded(stood));
        assert!(raise(&down.hub, What::Market).is_none() && upgrade(&down.hub, shop).is_none());
        sound(&down.hub);
        let (mut now, mut spared, mut fell) = (down.hub.clone(), 0, false);
        while let Some(more) = raise(&now, What::Home) {
            fell = more.razed.contains(&shop);
            now = more.hub;
            sound(&now);
            if fell {
                break;
            }
            spared += 1;
        }
        assert!(fell && spared > 0, "a stranded building comes down for a home only once the open ground is gone");
        assert!(downgrade(&down.hub, shop).is_some());

        let lie = hub.lies();
        let far = lie.ground.iter().copied().filter(|tile| !lie.wall.contains(tile)).min_by(|a, b| flat(*a).0.total_cmp(&flat(*b).0)).unwrap();
        let away = done(zone(&hub, Zone::Market, far));
        assert_eq!(away.item(shop), hub.item(shop));
        if !away.in_zone(Zone::Market, away.item(shop).unwrap().tiles[0]) {
            assert!(away.stranded(away.item(shop).unwrap()) && upgrade(&away, shop).is_none());
            let back = done(zone(&away, Zone::Market, (2, 0)));
            assert!(!back.stranded(back.item(shop).unwrap()));
        }
    }

    /// Planned for one fewer, a hub gives up ground and nothing else:
    /// nothing moves and no home someone lives in comes down.
    #[test]
    fn planned_for_fewer_a_hub_keeps_what_stands() {
        let mut hub = housed(done(plan(&done(raise(&camp((0, 0)), What::Post)), 12)));
        for id in homes(&hub).into_iter().take(3) {
            if let Some(lodged) = lodge(&hub, id, false) {
                hub = lodged.hub;
            }
        }
        for want in (hub.residents().max(CAMP)..12).rev() {
            let lived = hub.items.iter().filter(|item| item.lived() > 0).count();
            let (people, wall) = (hub.residents(), hub.lies().wall.len());
            let down = plan(&hub, want).expect("a hub is planned for as few as live in it");
            assert!(down.moved.is_empty(), "planned for {want}, nothing moves");
            assert!(down.hub.items.iter().filter(|item| item.lived() > 0).count() >= lived);
            assert_eq!(down.hub.residents(), people);
            assert!(down.hub.lies().wall.len() <= wall);
            hub = down.hub;
            sound(&hub);
        }
        assert!(plan(&hub, hub.residents().saturating_sub(1)).is_none() || hub.residents() == 0);
    }

    /// Most of its players gone, a hub draws in round those left: no
    /// player is put out, its wall is one they defend or the hub is at its
    /// least, and the same hub asked twice is drawn in the same.
    #[test]
    fn a_hub_draws_in_to_what_its_residents_defend() {
        let mut hub = housed(done(plan(&done(raise(&camp((0, 0)), What::Post)), 40)));
        let lived = homes(&hub);
        for &id in lived.iter().step_by(9).take(2) {
            if let Some(lodged) = lodge(&hub, id, false) {
                hub = lodged.hub;
            }
        }
        assert!(hub.lies().wall.len() > defended(&hub));
        let parts = shrink(&hub).expect("a wall too long is drawn in");
        assert_eq!(parts.iter().map(|part| part.hub.players()).sum::<u32>(), hub.players());
        for part in &parts {
            sound(&part.hub);
            let least = part.hub.open.is_empty() && part.hub.room() <= 2 * part.hub.players().max(1);
            assert!(part.hub.lies().wall.len() <= defended(&part.hub) || least);
        }
        assert_eq!(shrink(&hub), Some(parts));
        assert!(shrink(&camp((0, 0))).is_none());
    }
}
