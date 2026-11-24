//! Hubs: the places players live. A hub is hearths that hold ground
//! together. It is as large as the players at home at them and centred
//! on where they live, and its core is the ground round its hearths, each
//! holding less the further it stands from the centre, so a hub is the
//! shape of where it was built.
//!
//! One rule says which hearths are one hub, whatever came before: a
//! hearth is a hub's while that hub holds ground round it. Hubs whose
//! grounds touch are one, where the one they make holds every hearth of
//! both; a hearth its hub holds no ground round is a hub of its own
//! again, as one newly built there would be. So a hub grows by the
//! outposts built at its edge, and one that shrinks sheds its far side as
//! outposts.
//!
//! The haven is no hub. It is the point the world names, where a player
//! with no hearth is at home: it has no size, takes nothing in and is
//! taken in by nothing, however many enter the world there. Its guard
//! keeps the ground round it, and its influence is whole and a city's
//! wide, so the country round it is the mildest there is.
//!
//! Hubs are few and known whole, wherever their ground is: a core far off
//! may reach here. How they are kept past the server is not this
//! module's to know. The server holds them, and the world viewer settles
//! a scenario by this same rule to draw it.

use std::collections::{BTreeSet, HashMap};

use bevy::prelude::*;
use common::network::Identity;
use qrz::Qrz;

/// The core one player's hub holds, in tiles from its centre: room for a
/// tent, a tied horse, a wagon and a ring of stakes round them.
pub const OUTPOST_CORE: f32 = 12.0;

/// The power of its players a hub's core widens by. Under a half, so
/// its ground grows slower than its players and each has less of it the
/// larger the hub: a camp is spread out and a city is close built.
pub const CORE_GROWTH: f32 = 0.4;

/// How far round its point the haven's guard keeps the ground, in tiles:
/// a town's of a hundred.
pub const HAVEN_GROUND: f32 = 76.0;

/// How much of a hearth's distance from its hub's centre comes off the
/// ground it holds. At none every hearth would hold a whole core,
/// however far it sprawled; at one a hub could hold nothing past the
/// core a single hearth at its centre does.
pub const DRAWN_IN: f32 = 0.5;

/// How far a hub's influence reaches, in cores.
pub const INFLUENCE_CORES: f32 = 10.0;

/// The players at home in a hub whose influence is whole at its centre.
pub const INFLUENCE_WHOLE: f32 = 1000.0;

/// How far from its centre the core of a hub of `players` reaches, in
/// tiles; a hub of none holds what one player's does.
pub fn core_of(players: f32) -> f32 {
    OUTPOST_CORE * players.max(1.0).powf(CORE_GROWTH)
}

/// How far past its guarded ground the haven's influence reaches, in
/// tiles: a hub's whose influence is whole.
pub fn haven_reach() -> f32 {
    core_of(INFLUENCE_WHOLE) * INFLUENCE_CORES
}

/// Which hub: the one whose least hearth is on this tile. A hub is its
/// hearths and nothing else, so its name follows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HubId(pub i32, pub i32);

/// Every hub, as the hearths that are one, and where every player known
/// to have played of late is at home.
#[derive(Resource, Debug)]
pub struct Hubs {
    haven: (i32, i32),
    hubs: HashMap<HubId, BTreeSet<(i32, i32)>>,
    /// Each recent player's home hearth, or none where it is the haven.
    residents: HashMap<Identity, Option<(i32, i32)>>,
    /// How many of them are at home at each hearth, and at none: the
    /// residents counted, kept beside them so a size is read without
    /// counting every player.
    at_home: HashMap<Option<(i32, i32)>, u32>,
    /// Players each hearth counts as at home besides those who are: a
    /// fixture, for seeing hubs grow with no one else in the world.
    pub per_hearth: u32,
    /// Whether a hub sheds a hearth it holds no ground round. Unbuilt in
    /// play: hubs only grow together there, and what becomes of ground a
    /// hub lets go of is not yet decided. The viewer and the tests shed.
    pub sheds: bool,
    changed: bool,
}

/// The tiles between two tiles of the flat grid.
fn distance(a: (i32, i32), b: (i32, i32)) -> f32 {
    Qrz { q: a.0, r: a.1, z: 0 }.flat_distance(&Qrz { q: b.0, r: b.1, z: 0 }) as f32
}

/// The hub of `hearths`, by its name.
fn named(hearths: BTreeSet<(i32, i32)>) -> Option<(HubId, BTreeSet<(i32, i32)>)> {
    let &(q, r) = hearths.first()?;
    Some((HubId(q, r), hearths))
}

impl Hubs {
    /// The hubs of a world whose haven is the tile `haven`: none, with
    /// nothing built.
    pub fn new(haven: (i32, i32)) -> Self {
        Self { haven, hubs: HashMap::new(), residents: HashMap::new(), at_home: HashMap::new(), per_hearth: 0, sheds: false, changed: false }
    }

    /// The hub the hearth on `tile` is a home in.
    pub fn of_hearth(&self, tile: (i32, i32)) -> Option<HubId> {
        self.hubs.iter().find(|(_, hearths)| hearths.contains(&tile)).map(|(&id, _)| id)
    }

    /// How many players are at home at the hearth on `hearth`.
    fn lived(&self, hearth: (i32, i32)) -> u32 {
        self.at_home.get(&Some(hearth)).copied().unwrap_or(0) + self.per_hearth
    }

    /// How many players are at home at `hearths` together.
    fn size_of(&self, hearths: &BTreeSet<(i32, i32)>) -> u32 {
        hearths.iter().map(|&hearth| self.lived(hearth)).sum()
    }

    /// The tile `hearths` are centred on as one hub: where they stand,
    /// each weighed by the players at home at it and one more, the hand
    /// that keeps even an empty hearth.
    fn centre_of(&self, hearths: &BTreeSet<(i32, i32)>) -> Option<(i32, i32)> {
        let (mut q, mut r, mut all) = (0.0, 0.0, 0.0);
        for &hearth in hearths {
            let weight = self.lived(hearth) as f64 + 1.0;
            (q, r, all) = (q + hearth.0 as f64 * weight, r + hearth.1 as f64 * weight, all + weight);
        }
        (all > 0.0).then(|| ((q / all).round() as i32, (r / all).round() as i32))
    }

    /// The ground each of `hearths` holds as one hub: how far round it,
    /// the hub's core less [`DRAWN_IN`] of its distance from the centre.
    /// None or less is none held.
    fn grounds_of(&self, hearths: &BTreeSet<(i32, i32)>) -> Vec<((i32, i32), f32)> {
        let Some(centre) = self.centre_of(hearths) else { return Vec::new() };
        let core = core_of(self.size_of(hearths) as f32);
        hearths.iter().map(|&hearth| (hearth, core - DRAWN_IN * distance(hearth, centre))).collect()
    }

    /// How many players are at home in hub `id`.
    pub fn size(&self, id: HubId) -> u32 {
        self.hubs.get(&id).map_or(0, |hearths| self.size_of(hearths))
    }

    /// The tile hub `id` is centred on: where its players live.
    pub fn centre(&self, id: HubId) -> Option<(i32, i32)> {
        self.centre_of(self.hubs.get(&id)?)
    }

    /// How far a hearth at hub `id`'s centre holds the ground round it,
    /// in tiles: the hub's core, were it built in one place.
    pub fn core(&self, id: HubId) -> f32 {
        core_of(self.size(id) as f32)
    }

    /// How far past its core hub `id`'s influence reaches, in tiles.
    pub fn reach(&self, id: HubId) -> f32 {
        self.core(id) * INFLUENCE_CORES
    }

    /// The ground each hearth of hub `id` holds: the hearth, and how far
    /// round it.
    pub fn grounds(&self, id: HubId) -> Vec<((i32, i32), f32)> {
        self.hubs.get(&id).map_or(Vec::new(), |hearths| self.grounds_of(hearths))
    }

    /// How far `tile` lies outside hub `id`'s core, in tiles: none on it.
    pub fn outside(&self, id: HubId, tile: (i32, i32)) -> Option<f32> {
        self.grounds(id).into_iter().map(|(hearth, held)| (distance(hearth, tile) - held).max(0.0)).min_by(f32::total_cmp)
    }

    /// Hub `id`'s influence over its core, 0 to 1: the square root of its
    /// players as a share of [`INFLUENCE_WHOLE`].
    pub fn strength(&self, id: HubId) -> f32 {
        ((self.size(id).max(1) as f32) / INFLUENCE_WHOLE).sqrt().min(1.0)
    }

    /// The influence the haven and every hub have on `tile` together, 0
    /// to 1. Each one's is its strength, the haven's whole, over all the
    /// ground it holds, a hub's core and the haven's guarded ground; past
    /// that edge, whatever its shape, it falls as the cube of what is
    /// left of the way to its reach. It is what pushes the hard country
    /// back. Unbuilt: nothing in the game reads it; a den's level will.
    pub fn influence(&self, tile: (i32, i32)) -> f32 {
        let felt = |strength: f32, outside: f32, reach: f32| strength * (1.0 - (outside / reach).clamp(0.0, 1.0)).powi(3);
        let hubs = self.hubs.keys().filter_map(|&id| Some(felt(self.strength(id), self.outside(id, tile)?, self.reach(id))));
        (felt(1.0, self.past_haven(tile), haven_reach()) + hubs.sum::<f32>()).min(1.0)
    }

    /// How far `tile` lies outside the haven's guarded ground, in tiles.
    fn past_haven(&self, tile: (i32, i32)) -> f32 {
        (distance(self.haven, tile) - HAVEN_GROUND).max(0.0)
    }

    /// Whether the haven's influence reaches `tile`.
    pub fn haven_reaches(&self, tile: (i32, i32)) -> bool {
        self.past_haven(tile) < haven_reach()
    }

    /// Whether hub `id`'s influence reaches `tile`.
    pub fn reaches(&self, id: HubId, tile: (i32, i32)) -> bool {
        self.outside(id, tile).is_some_and(|outside| outside < self.reach(id))
    }

    /// Where the haven is.
    pub fn haven(&self) -> (i32, i32) {
        self.haven
    }

    /// Whether the haven's guard keeps `tile`.
    pub fn guarded(&self, tile: (i32, i32)) -> bool {
        distance(self.haven, tile) <= HAVEN_GROUND
    }

    /// The hearths of hub `id`.
    pub fn hearths(&self, id: HubId) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.hubs.get(&id).into_iter().flat_map(|hearths| hearths.iter().copied())
    }

    /// The hub whose core holds `tile`: the one whose hearth's ground it
    /// lies deepest in, of those that do.
    pub fn holding(&self, tile: (i32, i32)) -> Option<HubId> {
        let deep = |id: HubId| self.grounds(id).into_iter().map(|(hearth, held)| held - distance(hearth, tile)).max_by(f32::total_cmp);
        self.hubs
            .keys()
            .filter_map(|&id| Some((id, deep(id)?)))
            .filter(|&(_, deep)| deep >= 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(id, _)| id)
    }

    /// A hearth came to stand on `tile`: a hub of its own, until the hubs
    /// settle. Returns the hub it is a home in once they have.
    pub fn founded(&mut self, tile: (i32, i32)) -> HubId {
        if self.of_hearth(tile).is_none() {
            self.hubs.insert(HubId(tile.0, tile.1), BTreeSet::from([tile]));
            self.changed = true;
            self.settle();
        }
        self.of_hearth(tile).expect("a hearth just built is a home in some hub")
    }

    /// The hearth on `tile` came down. The players at home at it are at
    /// home at the haven.
    pub fn razed(&mut self, tile: (i32, i32)) {
        for home in self.residents.values_mut().filter(|home| **home == Some(tile)) {
            *home = None;
        }
        let moved = self.at_home.remove(&Some(tile)).unwrap_or(0);
        *self.at_home.entry(None).or_default() += moved;
        let Some(mut hearths) = self.of_hearth(tile).and_then(|id| self.hubs.remove(&id)) else { return };
        hearths.remove(&tile);
        self.hubs.extend(named(hearths));
        self.changed = true;
        self.settle();
    }

    /// `who` played of late with its home at the hearth on `home`, or at
    /// the haven. A hearth that stands no more is the haven.
    pub fn resident(&mut self, who: Identity, home: Option<(i32, i32)>) {
        let home = home.filter(|&hearth| self.of_hearth(hearth).is_some());
        let was = self.residents.insert(who, home);
        if was != Some(home) {
            if let Some(count) = was.and_then(|was| self.at_home.get_mut(&was)) {
                *count -= 1;
            }
            *self.at_home.entry(home).or_default() += 1;
            self.settle();
        }
    }

    /// `who` has not played of late, and counts toward no hub's size.
    pub fn gone(&mut self, who: Identity) {
        if let Some(count) = self.residents.remove(&who).and_then(|was| self.at_home.get_mut(&was)) {
            *count -= 1;
            self.settle();
        }
    }

    /// Where `who` is at home, where it is at a hearth.
    pub fn home_of(&self, who: Identity) -> Option<(i32, i32)> {
        self.residents.get(&who).copied().flatten()
    }

    /// Makes the hubs what the one rule says of the hearths as they
    /// stand. Hubs whose grounds touch are made one, where the one they
    /// make holds ground round every hearth of both; then the hearth a
    /// hub holds the least ground round, where that is none, is a hub of
    /// its own again; and so on until neither is left to do. Hubs are
    /// taken in the order of their names, so the same hearths settle the
    /// same way on every server and every run. A hub that would shed a
    /// hearth is never made, so nothing made is at once undone.
    fn settle(&mut self) {
        // Each pass ends with a hub fewer or a hub more; more passes than
        // this is hearths trading places, which the rule above forbids
        let passes = 4 * self.hubs.values().map(BTreeSet::len).sum::<usize>() + 4;
        for _ in 0..passes {
            let mut ids: Vec<HubId> = self.hubs.keys().copied().collect();
            ids.sort();
            let grounds: HashMap<HubId, Vec<((i32, i32), f32)>> = ids.iter().map(|&id| (id, self.grounds(id))).collect();
            let touch = |a: HubId, b: HubId| {
                grounds[&a].iter().any(|&(at, held)| grounds[&b].iter().any(|&(other, theirs)| distance(at, other) <= held + theirs))
            };
            let one = |a: HubId, b: HubId| -> BTreeSet<(i32, i32)> { self.hubs[&a].union(&self.hubs[&b]).copied().collect() };
            let holds = |hearths: &BTreeSet<(i32, i32)>| self.grounds_of(hearths).iter().all(|&(_, held)| held > 0.0);
            let pairs = ids.iter().enumerate().flat_map(|(i, &a)| ids[i + 1..].iter().map(move |&b| (a, b)));
            if let Some((a, b)) = pairs.filter(|&(a, b)| touch(a, b)).find(|&(a, b)| holds(&one(a, b))) {
                let joined = one(a, b);
                self.hubs.remove(&a);
                self.hubs.remove(&b);
                self.hubs.extend(named(joined));
                self.changed = true;
                info!("hubs: {a:?} and {b:?} are one");
                continue;
            }
            let shed = ids.iter().filter(|_| self.sheds).find_map(|&id| {
                let (hearth, held) = grounds[&id].iter().copied().min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))?;
                (held <= 0.0 && grounds[&id].len() > 1).then_some((id, hearth))
            });
            let Some((id, hearth)) = shed else { return };
            let mut rest = self.hubs.remove(&id).expect("a hub that shed");
            rest.remove(&hearth);
            self.hubs.extend(named(rest));
            self.hubs.insert(HubId(hearth.0, hearth.1), BTreeSet::from([hearth]));
            self.changed = true;
            info!("hubs: {id:?} holds the hearth on {hearth:?} no more, and it is a hub of its own");
        }
        warn!("hubs: the hearths did not settle");
    }

    /// Counts every hearth as `players` at home besides those who are,
    /// and settles the hubs as that leaves them.
    pub fn count_per_hearth(&mut self, players: u32) {
        self.per_hearth = players;
        self.settle();
    }

    /// Whether which hearths are one hub changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Every hub as its hearths: what is kept of them.
    pub fn kept(&self) -> Vec<Vec<(i32, i32)>> {
        let mut hubs: Vec<Vec<(i32, i32)>> = self.hubs.values().map(|hearths| hearths.iter().copied().collect()).collect();
        hubs.sort();
        hubs
    }

    /// Lays the hubs kept from an earlier run, each as its hearths, and
    /// the recent players' homes, and settles them.
    pub fn recall(&mut self, hubs: Vec<Vec<(i32, i32)>>, residents: Vec<(Identity, Option<(i32, i32)>)>) {
        self.hubs.extend(hubs.into_iter().filter_map(|hearths| named(hearths.into_iter().collect())));
        for (who, home) in residents {
            self.resident(who, home);
        }
        self.settle();
    }

    /// Each hub with its centre, its core's reach and its size.
    pub fn each(&self) -> impl Iterator<Item = (HubId, (i32, i32), f32, u32)> + '_ {
        self.hubs.keys().filter_map(|&id| Some((id, self.centre(id)?, self.core(id), self.size(id))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(n: u8) -> Identity {
        Identity::Key([n; 16])
    }

    /// `count` players at home at `hearth`, numbered from `from`.
    fn settle(hubs: &mut Hubs, hearth: (i32, i32), from: u8, count: u8) {
        for n in from..from + count {
            hubs.resident(player(n), Some(hearth));
        }
    }

    /// A hearth founds a hub that holds the ground round it; one beside
    /// the haven is a hub like any other, which the haven never takes in.
    #[test]
    fn a_hearth_founds_a_hub() {
        let mut hubs = Hubs::new((0, 0));
        let outpost = hubs.founded((200, 0));
        assert_eq!((outpost, hubs.centre(outpost), hubs.core(outpost)), (HubId(200, 0), Some((200, 0)), OUTPOST_CORE));
        assert_eq!((hubs.holding((212, 0)), hubs.holding((213, 0))), (Some(outpost), None));
        assert_eq!(hubs.founded((10, 0)), HubId(10, 0));
        assert!(hubs.guarded((10, 0)) && !hubs.guarded((200, 0)));
        assert!(hubs.take_changed() && !hubs.take_changed());
    }

    /// Hubs whose grounds touch are one, and a hearth between two that
    /// stood apart joins them.
    #[test]
    fn hubs_whose_grounds_touch_are_one() {
        let mut hubs = Hubs::new((0, 0));
        let a = hubs.founded((300, 0));
        hubs.resident(player(1), Some((300, 0)));
        assert_ne!(hubs.founded((300, 34)), a, "past each other's ground, two outposts stand apart");
        assert_eq!(hubs.founded((300, 20)), a);
        assert_eq!((hubs.of_hearth((300, 34)), hubs.size(a), hubs.hearths(a).count()), (Some(a), 1, 3));
        assert_eq!(hubs.each().count(), 1);
    }

    /// A hub is as large as the players at home in it, and wider by less
    /// than they are more, so the larger it is the closer built; one that
    /// grows reaches its neighbour, and one whose last hearth comes down
    /// is gone, its players at home at the haven.
    #[test]
    fn a_hub_grows_with_its_players_and_ends_with_its_last_hearth() {
        let mut hubs = Hubs::new((0, 0));
        let town = hubs.founded((500, 0));
        assert_ne!(hubs.founded((540, 0)), town);
        let plot = |players: f32| core_of(players).powi(2) / players;
        assert!(plot(1.0) > plot(25.0) && plot(25.0) > plot(100.0) && plot(100.0) > plot(1000.0), "the larger the hub, the closer built");
        assert!((core_of(100.0) - HAVEN_GROUND).abs() < 1.0, "the haven's ground is a town's");
        settle(&mut hubs, (500, 0), 0, 120);
        assert_eq!(hubs.size(town), 120);
        assert!(hubs.core(town) > 6.0 * OUTPOST_CORE && hubs.core(town) < 8.0 * OUTPOST_CORE);
        assert_eq!(hubs.of_hearth((540, 0)), Some(town), "the grown core reached the camp");
        hubs.gone(player(0));
        assert_eq!(hubs.size(town), 119, "one who stops coming is no longer counted");
        hubs.razed((540, 0));
        hubs.razed((500, 0));
        assert_eq!((hubs.each().count(), hubs.holding((500, 0)), hubs.home_of(player(3))), (0, None, None));
    }

    /// A hub is centred on where its players live and is the shape of its
    /// hearths: a crowd settling at one edge draws the centre that way,
    /// and the core reaches out round them.
    #[test]
    fn a_hub_is_centred_on_its_players_and_shaped_by_its_hearths() {
        let mut hubs = Hubs::new((0, 0));
        let town = hubs.founded((1000, 0));
        settle(&mut hubs, (1000, 0), 0, 100);
        let core = hubs.core(town) as i32;
        assert!(hubs.holding((1000 + core, 0)) == Some(town) && hubs.holding((1000 - core, 0)) == Some(town), "one hearth holds a whole core");
        assert_eq!(hubs.founded((1080, 0)), town, "an outpost at the edge is taken in");
        assert_eq!(hubs.centre(town), Some((1001, 0)), "an empty hearth barely moves the centre");
        assert_eq!(hubs.holding((1110, 0)), Some(town), "and the core reaches out round it");
        assert_eq!(hubs.holding((1000 - core, 0)), Some(town));
        settle(&mut hubs, (1080, 0), 100, 100);
        assert_eq!(hubs.centre(town), Some((1040, 0)), "a crowd at the edge draws the centre to it");
        let (near, far) = (hubs.grounds(town)[0].1, hubs.grounds(town)[1].1);
        assert_eq!(near, far, "and the two ends hold alike");
    }

    /// A hub that shrinks sheds its far side: the hearth it holds no
    /// ground round is a hub of its own again, as one built there would
    /// be, its players with it. It is taken in again when the hub grows
    /// back to it.
    #[test]
    fn a_hub_that_shrinks_sheds_its_far_hearth_as_a_hub_of_its_own() {
        let mut hubs = Hubs::new((0, 0));
        hubs.sheds = true;
        let town = hubs.founded((1000, 0));
        settle(&mut hubs, (1000, 0), 0, 100);
        assert_eq!(hubs.founded((1080, 0)), town);
        settle(&mut hubs, (1080, 0), 100, 5);
        assert_eq!((hubs.each().count(), hubs.size(town)), (1, 105));
        for n in 0..99 {
            hubs.gone(player(n));
        }
        let shed = hubs.of_hearth((1000, 0)).unwrap();
        let kept = hubs.of_hearth((1080, 0)).unwrap();
        assert_ne!(shed, kept, "the far hearth is its own hub");
        let mut grown = Hubs::new((0, 0));
        grown.founded((1000, 0));
        settle(&mut grown, (1000, 0), 0, 100);
        grown.founded((1080, 0));
        settle(&mut grown, (1080, 0), 100, 5);
        for n in 0..99 {
            grown.gone(player(n));
        }
        assert_eq!(grown.each().count(), 1, "where hubs do not shed, it stays the hub's, with no ground round it");
        assert_eq!((hubs.size(shed), hubs.size(kept)), (1, 5));
        assert_eq!(hubs.core(shed), OUTPOST_CORE, "holding what one player's hearth holds");
        settle(&mut hubs, (1080, 0), 110, 100);
        assert_eq!(hubs.of_hearth((1000, 0)), hubs.of_hearth((1080, 0)), "grown back to it, the hub takes it in again");
    }

    /// A hub's influence is the same over all its core, falls from the
    /// core's edge to nothing at its reach, and is stronger and wider the
    /// more live there; hubs near each other add, and never past whole.
    #[test]
    fn influence_falls_with_distance_and_grows_with_the_hub() {
        let mut hubs = Hubs::new((0, 0));
        let camp = hubs.founded((5000, 0));
        let at = |hubs: &Hubs, far: i32| hubs.influence((5000 + far, 0));
        assert_eq!(at(&hubs, 0), at(&hubs, hubs.core(camp) as i32), "the same to the core's edge");
        assert!(at(&hubs, 0) > at(&hubs, 30) && at(&hubs, 30) > at(&hubs, 60));
        assert_eq!(at(&hubs, (hubs.core(camp) + hubs.reach(camp)) as i32 + 1), 0.0);
        let small = (at(&hubs, 0), at(&hubs, 60), hubs.reach(camp));
        settle(&mut hubs, (5000, 0), 0, 200);
        assert!(at(&hubs, 0) > small.0 && at(&hubs, 60) > small.1 && hubs.reach(camp) > small.2);
        assert_eq!((hubs.influence((0, 0)), hubs.influence((HAVEN_GROUND as i32, 0))), (1.0, 1.0), "the haven's is whole over its ground");
        assert!(hubs.influence((1000, 0)) > 0.0 && hubs.influence((1000, 0)) < 1.0);
    }

    /// What is kept of the hubs lays them again as they were.
    #[test]
    fn hubs_are_recalled_as_they_were_kept() {
        let mut hubs = Hubs::new((0, 0));
        hubs.founded((300, 0));
        hubs.founded((300, 20));
        hubs.founded((700, 0));
        let kept = hubs.kept();
        assert_eq!(kept, vec![vec![(300, 0), (300, 20)], vec![(700, 0)]]);
        let mut back = Hubs::new((0, 0));
        back.recall(kept, vec![(player(1), Some((300, 20))), (player(2), Some((9, 9)))]);
        assert_eq!(back.hubs, hubs.hubs);
        assert_eq!((back.size(HubId(300, 0)), back.home_of(player(2))), (1, None), "a home that stands no more is the haven");
    }
}
