//! A hub as its plans see it: build sites, what stands on each, and the
//! wall round them. Nothing here knows who the players are or how a hub
//! is kept; the server holds hubs, and the world viewer plans a scenario
//! by these same rules to draw it.
//!
//! A hub is its build sites, and its wall the tightest outline round
//! them, [`PAST`] tiles out. Sites lie on a lattice the world fixes, so a
//! site is where it was whatever plan comes next, and the sites of two
//! hubs that become one line up. A plan is the only thing that moves a
//! wall: [`grow`] builds a tier out beside where homes are lived in, at
//! the least added wall, and [`shrink`] keeps the homes players live in
//! and as much round them as the hub's people hold, in as many hubs as
//! that takes.

use std::collections::HashSet;

use qrz::outline::{apart, enclosed, flat, girth};

/// How far apart build sites lie, in tiles, each way.
pub const APART: i32 = 3;

/// How far past its outermost sites a hub's wall stands, in tiles.
pub const PAST: f64 = 3.0;

/// How many times the players of the tier below a tier is sized for.
pub const STEP: u32 = 3;

/// What a camp raises on the sites round its hearth, in turn.
pub const CAMP: [Standing; 4] = [Standing::Post, Standing::Home, Standing::Home, Standing::Wagon];

/// What a hub is never without, besides its homes.
pub const ESSENTIALS: [Standing; 3] = [Standing::Hearth, Standing::Post, Standing::Wagon];

/// The fewest sites a hub of `players` at home is: its essentials, and
/// two homes for each player, theirs and their settler's, or two with
/// none.
pub fn least(players: usize) -> usize {
    ESSENTIALS.len() + 2 * players.max(1)
}

/// The six sites beside a site.
const BESIDE: [(i32, i32); 6] = [(APART, 0), (-APART, 0), (0, APART), (0, -APART), (APART, -APART), (-APART, APART)];

/// What stands on a build site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standing {
    Hearth,
    Post,
    Wagon,
    Home,
}

impl Standing {
    pub const ALL: [Standing; 4] = [Standing::Hearth, Standing::Post, Standing::Wagon, Standing::Home];

    /// The number a store keeps it by, given once: a new thing to stand
    /// on a site takes the next never used.
    pub fn id(self) -> u32 {
        match self {
            Standing::Hearth => 1,
            Standing::Post => 2,
            Standing::Wagon => 3,
            Standing::Home => 4,
        }
    }

    /// What is kept as `id`, or none where the game no longer has it.
    pub fn kept_as(id: u32) -> Option<Standing> {
        Self::ALL.into_iter().find(|what| what.id() == id)
    }
}

/// Who lives in a home. One no one lives in is open to either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lived {
    Empty,
    Player,
    Settler,
}

/// A build site: its tile, what stands on it, and who lives there where
/// that is a home.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Site {
    pub at: (i32, i32),
    pub what: Standing,
    pub lived: Lived,
}

impl Site {
    fn home(at: (i32, i32)) -> Self {
        Self { at, what: Standing::Home, lived: Lived::Empty }
    }

    /// What it is kept after as a hub draws in: a home no one lives in is
    /// the first to go, then a fixture, then a settler's home, and a
    /// player's home is never given up.
    fn rank(self) -> u8 {
        match (self.what, self.lived) {
            (Standing::Home, Lived::Empty) => 0,
            (Standing::Home, Lived::Settler) => 2,
            (Standing::Home, Lived::Player) => 3,
            _ => 1,
        }
    }
}

/// A hub: its build sites, in the order they were built, and the tier it
/// is built out to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hub {
    pub sites: Vec<Site>,
    pub tier: u32,
}

/// The ground a hub holds, and its wall: each tile of that ground with a
/// neighbour outside it. Both in `q` then `r` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ground {
    pub tiles: Vec<(i32, i32)>,
    pub wall: Vec<(i32, i32)>,
}

/// Whether a build site lies on `tile`.
pub fn on_site((q, r): (i32, i32)) -> bool {
    q.rem_euclid(APART) == 0 && r.rem_euclid(APART) == 0
}

/// The build site nearest `tile`.
pub fn site_near((q, r): (i32, i32)) -> (i32, i32) {
    let near = |v: i32| (v as f64 / APART as f64).round() as i32 * APART;
    (near(q), near(r))
}

/// How many players tier `tier` is sized for.
pub fn sized_for(tier: u32) -> u32 {
    STEP.pow(tier)
}

/// How many build sites tier `tier` has: a camp's hearth, post and wagon,
/// and two homes for each player it is sized for, theirs and that of the
/// settler who comes with them.
pub fn sites_for(tier: u32) -> usize {
    3 + 2 * sized_for(tier) as usize
}

fn tiles(sites: &[Site]) -> Vec<(i32, i32)> {
    sites.iter().map(|site| site.at).collect()
}

/// The ground `sites` hold: every tile inside the tightest outline round
/// them, drawn [`PAST`] out.
pub fn ground(sites: &[Site]) -> Ground {
    let tiles = enclosed(&tiles(sites), PAST);
    let held: HashSet<(i32, i32)> = tiles.iter().copied().collect();
    let wall = tiles.iter().copied().filter(|&(q, r)| qrz::DIRECTIONS.iter().any(|d| !held.contains(&(q + d.q, r + d.r)))).collect();
    Ground { tiles, wall }
}

/// How many steps the shortest walk over `ground` from the tile `from`
/// takes to each tile it reaches: a walk that crosses no build site and
/// takes only steps `walked` allows, from one tile to the next.
pub fn reach(ground: &Ground, from: (i32, i32), walked: impl Fn((i32, i32), (i32, i32)) -> bool) -> std::collections::HashMap<(i32, i32), u32> {
    let mut steps = std::collections::HashMap::from([(from, 0u32)]);
    let mut open = std::collections::VecDeque::from([from]);
    while let Some(tile) = open.pop_front() {
        for d in qrz::DIRECTIONS.iter() {
            let next = (tile.0 + d.q, tile.1 + d.r);
            if steps.contains_key(&next) || on_site(next) || ground.tiles.binary_search(&next).is_err() || !walked(tile, next) {
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
/// crosses no build site, so nothing is ever built on it, and takes only
/// steps `walked` allows; of those as short, the one that keeps nearest
/// the straight line between the two. Empty where the ground holds no
/// such way.
pub fn path(ground: &Ground, from: (i32, i32), to: (i32, i32), walked: impl Fn((i32, i32), (i32, i32)) -> bool) -> Vec<(i32, i32)> {
    use std::{cmp::Reverse, collections::{BinaryHeap, HashMap}};
    let (a, b) = (flat(from), flat(to));
    let long = apart(a, b).max(1e-9);
    // How far a tile lies off the line, in thousandths of a tile
    let off = |tile: (i32, i32)| {
        let p = flat(tile);
        let along = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / (long * long)).clamp(0.0, 1.0);
        (apart(p, (a.0 + (b.0 - a.0) * along, a.1 + (b.1 - a.1) * along)) * 1000.0).round() as u64
    };
    let mut best: HashMap<(i32, i32), ((u32, u64), (i32, i32))> = HashMap::new();
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
            if best.contains_key(&next) || ground.tiles.binary_search(&next).is_err() || (on_site(next) && next != to) || !walked(tile, next) {
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

/// How many players live in `sites`.
pub fn players(sites: &[Site]) -> usize {
    sites.iter().filter(|site| site.lived == Lived::Player).count()
}

/// How many settlers live in `sites`.
pub fn settlers(sites: &[Site]) -> usize {
    sites.iter().filter(|site| site.lived == Lived::Settler).count()
}

/// Brings the settlers living in `sites` to one for each player at home,
/// and one who keeps the hearth where there is none: one who comes takes
/// the first empty home, and the last to come is the first to go.
pub fn peopled(sites: &mut [Site]) {
    let need = players(sites).max(1);
    let mut have = settlers(sites);
    for site in sites.iter_mut().filter(|site| site.what == Standing::Home) {
        if have >= need {
            break;
        }
        if site.lived == Lived::Empty {
            site.lived = Lived::Settler;
            have += 1;
        }
    }
    for site in sites.iter_mut().rev() {
        if have <= need {
            break;
        }
        if site.lived == Lived::Settler {
            site.lived = Lived::Empty;
            have -= 1;
        }
    }
}

/// How long a wall the people of a hub with `players` at home hold
/// between them, in stakes: that of a hub built for them. So the more
/// they are the less each holds, as a wall grows slower than what it
/// encloses; a hub with no players is held as a camp by its settler.
pub fn held(players: usize) -> usize {
    let want = least(players);
    let mut sites = vec![Site { at: (0, 0), what: Standing::Hearth, lived: Lived::Empty }];
    while sites.len() < want {
        let at = next(&sites).expect("open ground has a site beside every site");
        sites.push(Site::home(at));
    }
    ground(&sites).wall.len()
}

/// Two lengths the outline's arithmetic cannot tell apart.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

/// The open site beside `sites` to build on next: beside the most homes
/// lived in, then the one that lengthens the wall least, then the nearest
/// the hearth. None for no sites.
pub fn next(sites: &[Site]) -> Option<(i32, i32)> {
    let taken: HashSet<(i32, i32)> = sites.iter().map(|site| site.at).collect();
    let lived: HashSet<(i32, i32)> = sites.iter().filter(|site| site.lived != Lived::Empty).map(|site| site.at).collect();
    let hearth = flat(sites.iter().find(|site| site.what == Standing::Hearth).or(sites.first())?.at);
    let (round, mut stood) = (girth(&tiles(sites)), tiles(sites));
    let mut seen = HashSet::new();
    // Most lived homes beside, least wall added, nearest the hearth, then by tile
    let mut best: Option<(usize, f64, f64, (i32, i32))> = None;
    for site in sites {
        for (dq, dr) in BESIDE {
            let at = (site.at.0 + dq, site.at.1 + dr);
            if taken.contains(&at) || !seen.insert(at) {
                continue;
            }
            let beside = BESIDE.iter().filter(|(a, b)| lived.contains(&(at.0 + a, at.1 + b))).count();
            stood.push(at);
            let longer = girth(&stood) - round;
            stood.pop();
            let far = apart(flat(at), hearth);
            let better = best.is_none_or(|(most, least, near, tile)| {
                beside > most || beside == most && (longer < least - 1e-9 || same(longer, least) && (far < near - 1e-9 || same(far, near) && at < tile))
            });
            if better {
                best = Some((beside, longer, far, at));
            }
        }
    }
    best.map(|(.., at)| at)
}

/// A camp: the hearth on `hearth` and what it raises on the sites round
/// it, its settler in the first home.
pub fn camp(hearth: (i32, i32)) -> Hub {
    let mut sites = vec![Site { at: hearth, what: Standing::Hearth, lived: Lived::Empty }];
    for what in CAMP {
        let at = next(&sites).expect("a hearth has sites beside it");
        sites.push(Site { at, what, lived: Lived::Empty });
    }
    peopled(&mut sites);
    Hub { sites, tier: 0 }
}

/// Whether ground `other` touches ground `ours`: shares a tile with it,
/// or lies beside one.
pub fn touches(ours: &Ground, other: &Ground) -> bool {
    let held: HashSet<(i32, i32)> = ours.tiles.iter().copied().collect();
    other.tiles.iter().any(|&(q, r)| held.contains(&(q, r)) || qrz::DIRECTIONS.iter().any(|d| held.contains(&(q + d.q, r + d.r))))
}

/// Hub `which` of `hubs` a tier up: the hubs as the plan leaves them, it
/// last. It builds homes out, a site at a time, until it has the tier's
/// sites, and takes in every hub its ground comes to touch, with that
/// hub's sites: the hearth taken in is a home from then, so a hub has
/// one hearth.
pub fn grow(hubs: &[Hub], which: usize) -> Vec<Hub> {
    let tier = hubs[which].tier + 1;
    let mut sites = hubs[which].sites.clone();
    let mut others: Vec<(Hub, Ground)> = hubs.iter().enumerate().filter(|&(i, _)| i != which).map(|(_, hub)| (hub.clone(), ground(&hub.sites))).collect();
    loop {
        let ours = ground(&sites);
        if let Some(i) = others.iter().position(|(_, theirs)| touches(&ours, theirs)) {
            sites.extend(others.remove(i).0.sites.into_iter().map(|site| if site.what == Standing::Hearth { Site::home(site.at) } else { site }));
            continue;
        }
        if sites.len() >= sites_for(tier) {
            break;
        }
        let Some(at) = next(&sites) else { break };
        sites.push(Site::home(at));
    }
    peopled(&mut sites);
    let mut left: Vec<Hub> = others.into_iter().map(|(hub, _)| hub).collect();
    left.push(Hub { sites, tier });
    left
}

/// Each of a hub's essentials laid on one of `sites`, where they lack
/// it: in place of the home that matters least, nearest the middle of
/// the players' homes. False where they have too few homes no player
/// lives in to take them.
fn furnished(sites: &mut [Site]) -> bool {
    let people: Vec<Site> = sites.iter().copied().filter(|site| site.lived == Lived::Player).collect();
    let mid = middle(if people.is_empty() { sites } else { &people });
    for what in ESSENTIALS {
        if sites.iter().any(|site| site.what == what) {
            continue;
        }
        let to = sites.iter_mut().filter(|site| site.what == Standing::Home && site.lived != Lived::Player).reduce(|a, b| {
            let nearer = b.rank() < a.rank() || b.rank() == a.rank() && apart(flat(b.at), mid) < apart(flat(a.at), mid);
            if nearer { b } else { a }
        });
        let Some(to) = to else { return false };
        (to.what, to.lived) = (what, Lived::Empty);
    }
    true
}

/// The middle of `sites` on the flat.
fn middle(sites: &[Site]) -> (f64, f64) {
    let sum = sites.iter().map(|site| flat(site.at)).fold((0.0, 0.0), |sum, p| (sum.0 + p.0, sum.1 + p.1));
    (sum.0 / sites.len() as f64, sum.1 / sites.len() as f64)
}

/// The tier a hub of `count` sites is built out to.
fn tier_of(count: usize, most: u32) -> u32 {
    (0..=most).rev().find(|&tier| sites_for(tier) <= count).unwrap_or(0)
}

/// `sites` drawn in until their people hold their wall. It keeps every
/// home a player lives in, then takes back the sites round them, the one
/// that lengthens the wall least and of those the nearest them, for as
/// long as its people hold the wall that makes. So what it keeps is as
/// much as they hold, close about where they live, and never less than a
/// hub is ([`least`]). An essential left out is laid again on a site
/// kept.
fn fitted(sites: &[Site], tier: u32) -> Hub {
    let homes: Vec<Site> = sites.iter().copied().filter(|site| site.lived == Lived::Player).collect();
    let mut kept = if homes.is_empty() { sites.iter().copied().filter(|site| site.what == Standing::Hearth).collect() } else { homes };
    let mid = middle(&kept);
    let (allowed, least) = (held(players(sites)), sites.len().min(least(players(sites))));
    let mut rest: Vec<Site> = sites.iter().copied().filter(|site| !kept.contains(site)).collect();
    while !rest.is_empty() {
        let round = girth(&tiles(&kept));
        let mut best: Option<(usize, f64, f64)> = None;
        for (i, site) in rest.iter().enumerate() {
            let longer = girth(&tiles(&[&kept[..], &[*site]].concat())) - round;
            let far = apart(flat(site.at), mid);
            if best.is_none_or(|(_, least, near)| longer < least - 1e-9 || same(longer, least) && far < near) {
                best = Some((i, longer, far));
            }
        }
        let (i, ..) = best.expect("there is a site left");
        kept.push(rest[i]);
        if kept.len() > least && ground(&kept).wall.len() > allowed {
            kept.pop();
            break;
        }
        rest.remove(i);
    }
    furnished(&mut kept);
    peopled(&mut kept);
    Hub { tier: tier_of(kept.len(), tier), sites: kept }
}

/// The hubs `sites` make once their people hold every wall. Where even
/// the wall round the players' homes alone is more than their people
/// hold, the sites part in two between the two homes furthest apart,
/// each site going with the nearer, and each part is drawn in on its
/// own. Parts whose grounds would touch are no parts: the hub stays one.
fn parted(sites: &[Site], tier: u32, deep: u32) -> Vec<Hub> {
    let homes: Vec<Site> = sites.iter().copied().filter(|site| site.lived == Lived::Player).collect();
    if homes.len() > 1 && deep < 6 && ground(&homes).wall.len() > held(homes.len()) {
        let mut ends = (homes[0], homes[0], 0.0);
        for &a in &homes {
            for &b in &homes {
                let far = apart(flat(a.at), flat(b.at));
                if far > ends.2 {
                    ends = (a, b, far);
                }
            }
        }
        let nearer = |site: &Site| apart(flat(site.at), flat(ends.0.at)) <= apart(flat(site.at), flat(ends.1.at));
        let mut one: Vec<Site> = sites.iter().copied().filter(|site| nearer(site)).collect();
        let mut other: Vec<Site> = sites.iter().copied().filter(|site| !nearer(site)).collect();
        if furnished(&mut one) && furnished(&mut other) {
            let parts = [parted(&one, tier, deep + 1), parted(&other, tier, deep + 1)].concat();
            let grounds: Vec<Ground> = parts.iter().map(|part| ground(&part.sites)).collect();
            let apart = (0..grounds.len()).all(|i| (i + 1..grounds.len()).all(|j| !touches(&grounds[i], &grounds[j])));
            if apart {
                return parts;
            }
        }
    }
    vec![fitted(sites, tier)]
}

/// `hub` drawn in until its people hold its wall, in as many hubs as that
/// takes, or None where they hold it already.
pub fn shrink(hub: &Hub) -> Option<Vec<Hub>> {
    let parts = parted(&hub.sites, hub.tier, 0);
    (parts.len() > 1 || parts[0].sites.len() < hub.sites.len()).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hub built for 27 from a camp, with no one at home but its settler.
    fn town() -> Hub {
        let mut hubs = vec![camp((0, 0))];
        for _ in 0..3 {
            hubs = grow(&hubs, 0);
        }
        hubs.remove(0)
    }

    /// `hub` with players in the homes `pick` takes of its empty ones.
    fn lived(mut hub: Hub, pick: impl Fn(Vec<(i32, i32)>) -> Vec<(i32, i32)>) -> Hub {
        for site in &mut hub.sites {
            site.lived = Lived::Empty;
        }
        let homes = hub.sites.iter().filter(|site| site.what == Standing::Home).map(|site| site.at).collect();
        for at in pick(homes) {
            hub.sites.iter_mut().find(|site| site.at == at).expect("a home of the hub").lived = Lived::Player;
        }
        peopled(&mut hub.sites);
        hub
    }

    fn east(mut homes: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
        homes.sort_by(|a, b| flat(*b).0.total_cmp(&flat(*a).0));
        homes
    }

    /// A camp is a hearth, a post, a wagon and two homes on sites beside
    /// each other, its settler in one home and the other open.
    #[test]
    fn a_camp_is_five_sites() {
        let camp = camp(site_near((7, -11)));
        let count = |what| camp.sites.iter().filter(|site| site.what == what).count();
        assert_eq!((count(Standing::Hearth), count(Standing::Post), count(Standing::Wagon), count(Standing::Home)), (1, 1, 1, 2));
        assert!(camp.sites.iter().all(|site| on_site(site.at)));
        assert_eq!((players(&camp.sites), settlers(&camp.sites)), (0, 1));
        let ground = ground(&camp.sites);
        assert!(camp.sites.iter().all(|site| ground.tiles.contains(&site.at) && !ground.wall.contains(&site.at)));
        assert_eq!(ground.wall.len(), held(0), "its settler holds a camp");
        assert_eq!(held(0), held(1), "and so do a settler and a player");
    }

    /// A path runs from a hearth to a tile of its wall over ground that is
    /// no build site, a step at a time, and as short as the tiles between
    /// allow.
    #[test]
    fn a_path_crosses_no_build_site() {
        let hub = camp((0, 0));
        let ground = ground(&hub.sites);
        for &to in &ground.wall {
            let way = path(&ground, (0, 0), to, |_, _| true);
            // A walk ends on no build site, so a way in is never one
            let walk = reach(&ground, (0, 0), |_, _| true).get(&to).copied();
            assert_eq!(walk, (!on_site(to)).then_some(way.len() as u32), "as short as any walk there");
            assert_eq!(way.last(), Some(&to));
            assert!(way.iter().all(|&tile| tile == to || !on_site(tile)), "to {to:?}: {way:?}");
            let steps = std::iter::once((0, 0)).chain(way.iter().copied()).zip(way.iter().copied());
            assert!(steps.clone().all(|(a, b)| qrz::hex_distance(a, b) == 1), "a step at a time");
            assert!(way.len() as i32 <= qrz::hex_distance((0, 0), to) + 2, "to {to:?} it goes {} tiles round", way.len());
            assert_eq!(way, path(&ground, (0, 0), to, |_, _| true));
        }
        assert!(path(&ground, (0, 0), (90, 90), |_, _| true).is_empty(), "none to a tile off the ground");
        assert!(path(&ground, (0, 0), ground.wall[0], |_, _| false).is_empty(), "and none where no step is allowed");
    }

    /// A grow builds the whole tier out, every site a new one beside the
    /// rest, and the wall grows slower than the sites it holds; the same
    /// hubs grow the same way every time.
    #[test]
    fn a_grow_builds_the_tier_out() {
        let mut hubs = vec![camp((0, 0))];
        let mut per_site = f64::MAX;
        for tier in 1..=3 {
            hubs = grow(&hubs, 0);
            let hub = &hubs[0];
            let stood: HashSet<(i32, i32)> = hub.sites.iter().map(|site| site.at).collect();
            assert_eq!((hub.tier, hub.sites.len(), stood.len()), (tier, sites_for(tier), sites_for(tier)));
            assert!(hub.sites.iter().all(|site| on_site(site.at)));
            let wall = ground(&hub.sites).wall.len() as f64 / hub.sites.len() as f64;
            assert!(wall < per_site, "the larger the hub, the less wall to a site");
            per_site = wall;
        }
        assert_eq!(town(), town());
    }

    /// Building spreads from where homes are lived in: a hub whose
    /// players live to the east builds more to the east than one with
    /// none.
    #[test]
    fn building_spreads_from_where_people_live() {
        let start = grow(&[camp((0, 0))], 0).remove(0);
        let leaning = |hub: Hub| grow(&[hub], 0)[0].sites.iter().map(|site| flat(site.at).0).sum::<f64>();
        let settled = lived(start.clone(), |homes| east(homes).into_iter().take(2).collect());
        assert!(leaning(settled) > leaning(lived(start, |_| Vec::new())));
    }

    /// A hub whose ground comes to touch another's takes it in, sites
    /// and all; one far off stays its own.
    #[test]
    fn a_grow_takes_in_the_hub_it_touches() {
        let mut near = vec![camp((15, 0)), camp((0, 0))];
        assert_eq!(grow(&near, 1).len(), 2, "a tier up, it has not reached the camp");
        for _ in 0..2 {
            near = grow(&near, near.len() - 1);
        }
        assert_eq!(near.len(), 1, "two tiers up, it has");
        let stands = |what| near[0].sites.iter().filter(|site| site.what == what).count();
        assert_eq!((stands(Standing::Hearth), stands(Standing::Post), stands(Standing::Wagon)), (1, 2, 2), "the hearth taken in is a home, its post and wagon stand");
        assert!(near[0].sites.iter().any(|site| site.at == (15, 0) && site.what == Standing::Home));
        let far = grow(&[camp((0, 0)), camp((60, 0))], 0);
        assert_eq!((far.len(), far[0].clone()), (2, camp((60, 0))));
    }

    /// The more a hub's players, the longer the wall they hold and the
    /// less of it each holds; a hub as full as its tier holds its wall.
    #[test]
    fn people_hold_the_wall_of_a_hub_built_for_them() {
        let each = |players: usize| held(players) as f64 / players as f64;
        assert!(held(3) > held(1) && held(9) > held(3) && held(27) > held(9));
        assert!(each(3) < each(1) && each(9) < each(3) && each(27) < each(9));
        let full = lived(town(), |homes| homes.into_iter().take(27).collect());
        assert_eq!(shrink(&full), None, "a full hub has nothing to draw in");
    }

    /// A hub drawn in keeps every home a player lives in, close about
    /// them, with a hearth and no more wall than they hold; with no
    /// players it is a camp again.
    #[test]
    fn a_shrink_keeps_the_homes_players_live_in() {
        let empty = shrink(&lived(town(), |_| Vec::new())).expect("one settler does not hold a town");
        assert_eq!((empty.len(), empty[0].sites.len(), empty[0].tier), (1, 1 + CAMP.len(), 0));

        for count in [1, 3, 9] {
            let hub = lived(town(), |homes| east(homes).into_iter().take(count).collect());
            let homes: Vec<(i32, i32)> = hub.sites.iter().filter(|site| site.lived == Lived::Player).map(|site| site.at).collect();
            let parts = shrink(&hub).expect("a town built for 27 is more than these hold");
            assert_eq!(parts.len(), 1, "living together, they stay one hub");
            let kept = &parts[0];
            assert!(homes.iter().all(|&at| kept.sites.iter().any(|site| site.at == at && site.lived == Lived::Player)));
            for what in ESSENTIALS {
                assert!(kept.sites.iter().any(|site| site.what == what), "it keeps its {what:?}");
            }
            assert!(kept.sites.iter().filter(|site| site.what == Standing::Home).count() >= 2 * count, "and a home for each player and their settler");
            // No more wall than they hold, but never less than a hub is
            assert!(ground(&kept.sites).wall.len() <= held(count) || kept.sites.len() == least(count));
            assert!(kept.sites.len() < hub.sites.len() && kept.sites.iter().all(|site| hub.sites.iter().any(|was| was.at == site.at)), "it builds nothing new");
            assert_eq!(settlers(&kept.sites), count);
        }
    }

    /// Players who live too far apart to share a wall part into hubs of
    /// their own, each with its hearth and its players.
    #[test]
    fn a_hub_its_people_cannot_hold_in_one_wall_parts() {
        let hub = lived(town(), |homes| { let homes = east(homes); vec![homes[0], homes[homes.len() - 1]] });
        let parts = shrink(&hub).expect("two do not hold a town");
        assert_eq!(parts.len(), 2);
        for part in &parts {
            assert_eq!(players(&part.sites), 1);
            for what in ESSENTIALS {
                assert_eq!(part.sites.iter().filter(|site| site.what == what).count(), 1, "each part has its {what:?}");
            }
            assert_eq!(part.sites.iter().filter(|site| site.what == Standing::Home).count(), 2, "and a home for its player and its settler");
            assert!(ground(&part.sites).wall.len() <= held(1));
        }
        let stood: HashSet<(i32, i32)> = parts.iter().flat_map(|part| part.sites.iter().map(|site| site.at)).collect();
        assert_eq!(stood.len(), parts.iter().map(|part| part.sites.len()).sum::<usize>(), "no site is two hubs'");
        assert!(!touches(&ground(&parts[0].sites), &ground(&parts[1].sites)), "and their grounds stand apart");
    }

    /// Players who live near each other stay one hub however it is drawn
    /// in: whichever homes they live in, no shrink leaves hubs whose
    /// grounds touch.
    #[test]
    fn players_near_each_other_stay_one_hub() {
        let small = grow(&[camp((0, 0))], 0).remove(0);
        let homes: Vec<(i32, i32)> = small.sites.iter().filter(|site| site.what == Standing::Home).map(|site| site.at).collect();
        for a in 0..homes.len() {
            for b in a + 1..homes.len() {
                for c in b..homes.len() {
                    let hub = lived(small.clone(), |_| if c == b { vec![homes[a], homes[b]] } else { vec![homes[a], homes[b], homes[c]] });
                    let parts = shrink(&hub).unwrap_or_else(|| vec![hub.clone()]);
                    assert_eq!(parts.len(), 1, "players in {:?} of a hub built for three stay one hub", (homes[a], homes[b], homes[c]));
                }
            }
        }
    }

    /// A settler comes for each player at home and goes when one leaves,
    /// and one keeps a hub with no players.
    #[test]
    fn settlers_come_and_go_with_players() {
        let mut hub = grow(&[camp((0, 0))], 0).remove(0);
        assert_eq!((players(&hub.sites), settlers(&hub.sites)), (0, 1));
        let open = |hub: &Hub| hub.sites.iter().position(|site| site.what == Standing::Home && site.lived == Lived::Empty);
        for count in 1..=3 {
            let at = open(&hub).expect("a home is open");
            hub.sites[at].lived = Lived::Player;
            peopled(&mut hub.sites);
            assert_eq!((players(&hub.sites), settlers(&hub.sites)), (count, count));
        }
        assert_eq!(open(&hub), None, "a hub is full when its players and their settlers have come");
        let left = hub.sites.iter().position(|site| site.lived == Lived::Player).unwrap();
        hub.sites[left].lived = Lived::Empty;
        peopled(&mut hub.sites);
        assert_eq!((players(&hub.sites), settlers(&hub.sites)), (2, 2));
    }
}
