//! Dens: where creatures live, as the world puts them and players leave
//! them.
//!
//! The world says where a den may stand and on what ground: a tile carries
//! a den site's habitat and its place in its colony
//! (`EventRegistry::den_at`), and a chunk's sites are found as it is
//! generated. A nest stands on every site unless it was broken: the
//! country is full where no one has been, and only what players changed is
//! kept. A den is a nest and the pack that lives in it. The nest is an
//! actor of its own, standing where the den is drawn, that a player
//! targets and strikes; its pack stands round it in groups, each an
//! engagement at a post of its own that notices a player from [`NOTICE`]
//! tiles, nearer than the posts stand apart, so a pack is pulled a group
//! at a time and never comes whole. While the nest stands it brings back
//! one of its pack's dead every [`RESPAWN`].
//!
//! Breaking the nest leaves the den broken, drawn as its wreck, whatever
//! of its pack still lives the last of it. A broken nest is raised again
//! while any nest of its colony stands, one each [`REGROW`]; a colony
//! with none standing is dead until a colony beside it has stood for
//! [`SPREAD`], and then one of its nests is raised from there
//! ([`Dens::grow`]). Both go by the calendar, seen or not, so cleared
//! country heals inward from its edge.
//!
//! A nest's creatures are at the level of the country where it stands
//! (`Hubs::level`), read as they are made: a hub's push lowers them as
//! the country gives way to it. On quiet ground (`Hubs::quiet`) no nest
//! stands, and none is raised there.
//!
//! A den outlives the server where it stands (`at`), and a colony which
//! of its nests are broken: whatever keeps the world is told of each that
//! changes ([`Dens::take_changed`], [`Dens::take_changed_colonies`]) and
//! recalls those it kept. A nest and its pack do not, and come back whole
//! as a player nears; the ground a den clears is laid again when it next
//! stands.
//!
//! A den is drawn about the tile its pack first stood on, as its
//! archetype's model, standing or wrecked, its seed and turn its site's:
//! its pieces' circles stand on the map for every walker to go round, the
//! ground it stands on and [`OPEN`] tiles past it is cleared of all that
//! grew or lay there as long as it stands, thinning back into what stood
//! there over [`THIN`] more, so the den is seen over the wood round it,
//! and every player within `Setting::DenSight` is told of it as it is
//! (`show_dens`). A den is drawn once the map holds all the ground it
//! clears, which is cleared from the map's tiles, never read from the
//! world's generator: that may be busy building elsewhere for longer than
//! a frame.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use qrz::{Qrz, DIRECTIONS};
use serde::{Deserialize, Serialize};

use combat::engagement::{Engagement, EngagementMember, Members};
use common_bevy::{
    archetype::{calculate_enemy_attributes, EnemyArchetype},
    den::DenLook,
    hubs::Hubs,
    message::{Do, Event},
    moment::Moment,
    settings::{Setting, Settings},
    systems::When,
    chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId},
    components::{
        behaviour::Side,
        entity_type::{actor::{ActorIdentity, ActorImpl, Origin}, EntityType},
        Loc,
    },
    plugins::nntree::NearestNeighbor,
    resources::map::Map,
    systems::combat::resources::Fighter,
    tuning::Tuning,
};

use crate::{
    resources::Lobby,
    systems::gathering::{roll, Ground},
};

/// How near a player a den's nest and pack are out in the world, in tiles
pub const PLACE_RANGE: i32 = 80;

/// Chunks round a player's that hold every site within [`PLACE_RANGE`]
const PLACE_CHUNKS: u8 = 5;

/// How often dens are tended: packs brought back, broken nests raised
const TEND: Duration = Duration::from_secs(1);

/// How long a standing nest takes to bring back one of its pack's dead
pub const RESPAWN: Duration = Duration::from_secs(20);

/// How long a colony with a nest standing takes to raise one of its
/// broken nests again, from when it last changed
pub const REGROW: Duration = Duration::from_secs(600);

/// How long a colony with no nest standing lies dead beside one that
/// stands before a nest of its own is raised from there
pub const SPREAD: Duration = Duration::from_secs(3 * 86_400);

/// The fewest and the most groups a den's pack stands in; a site's roll
/// says which
pub const GROUPS: (u8, u8) = (2, 4);

/// How many creatures a group holds when whole
pub const GROUP: u8 = 2;

/// How far from its nest a group past the first stands, in tiles
pub const GROUP_REACH: i32 = 9;

/// How far a den's creature takes a target from, in tiles: nearer than
/// its groups stand apart, so each is pulled alone
pub const NOTICE: u32 = 7;

/// How many levels below its nest's a den's pack stands
pub const PACK_BELOW: u8 = 3;

/// How many tiles a den's search for dry ground round its site may look at
const SEARCH: usize = 400;

/// The fewest tiles between two dens: the sites of a clump stand further
/// apart, and two under one water would else take the same shore.
pub const APART: i32 = 24;

/// Tiles past a den's furthest piece its ground stays open: from the
/// camera's height and pitch, a tree nearer than this to its edge hides it.
pub const OPEN: u32 = 4;

/// Tiles past the open ground over which the wood thins back to the
/// world's.
pub const THIN: u32 = 3;

// A hub's quiet ground is as wide as a den's creature follows a target
// from its post, or a nest just past it still sends its pack to the wall.
const _: () = assert!(common_bevy::hubs::QUIET as i32 == combat::behaviour::LEASH_DISTANCE);
const _: () = assert!((NOTICE as i32) < GROUP_REACH, "a group is noticed alone only from nearer than the next stands");

/// A den site the server has generated, and the den on it, if one has
/// come out there.
#[derive(Clone, Debug)]
pub struct Site {
    pub tile: Qrz,
    pub place: common::den::Site,
    pub den: Option<Den>,
}

/// A den: the nest on a site and the pack that lives in it, and where it
/// stands. Whether its nest stands or is broken is its colony's to say
/// ([`Dens::broken`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Den {
    pub archetype: EnemyArchetype,
    /// How many groups its pack stands in when whole
    pub groups: u8,
    /// Where its nest stands, on dry ground near the site; none until it
    /// has first stood
    pub at: Option<Qrz>,
    /// What of it is out in the world: nothing while no player is near,
    /// or its nest is broken
    pub out: Option<Out>,
}

/// A den's nest and pack out in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Out {
    pub nest: Entity,
    /// Each group's engagement, the nest a member of the first; none for
    /// a group with no one left of it
    pub groups: [Option<Entity>; GROUPS.1 as usize],
    /// When the nest next brings back one of its dead, while it is short
    /// of any
    pub refill: Option<Moment>,
}

/// A colony some of whose nests are broken, or were of late: the nests of
/// a den lattice cell, as the world names it on each of their sites. One
/// with every nest standing since the world was made is kept nowhere.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Colony {
    /// Its sites as the server has found them: which of its colony's
    /// each is, and its tile. The world names a colony's candidates and
    /// each one's own ground says whether it is a site, so how many it
    /// has is how many were found: all of them once the ground round any
    /// one is generated, which it is before a nest there is struck.
    pub sites: Vec<(u8, i32, i32)>,
    /// Which of its sites hold a broken nest
    pub broken: Vec<u8>,
    /// When it last changed, a nest broken or raised, on the calendar
    pub since: When,
    /// When it last had a nest raised with none standing; none where it
    /// never lay dead
    #[serde(default)]
    pub revived: Option<When>,
}

impl Colony {
    fn whole(sites: Vec<(u8, i32, i32)>, now: When) -> Self {
        Colony { sites, broken: Vec::new(), since: now, revived: None }
    }

    /// Whether any of its nests stands.
    pub fn alive(&self) -> bool {
        self.broken.len() < self.sites.len()
    }

    fn is_broken(&self, nth: u8) -> bool {
        self.broken.contains(&nth)
    }

    /// Notes its `nth` site, on `tile`; whether it was not known.
    fn finds(&mut self, nth: u8, tile: (i32, i32)) -> bool {
        let new = self.sites.iter().all(|&(known, ..)| known != nth);
        if new {
            self.sites.push((nth, tile.0, tile.1));
        }
        new
    }

    /// Breaks its `nth` nest; whether it stood.
    fn breaks(&mut self, nth: u8) -> bool {
        let stood = !self.is_broken(nth);
        if stood {
            self.broken.push(nth);
        }
        stood
    }

    /// Raises every nest due by `now`, where `beside` is the moment the
    /// first of the colonies beside it to stand has stood from, none
    /// where none stands, and `quiet` says whether a tile takes no nest.
    /// Each is raised at the moment it fell due, so a colony found again
    /// after any while is where it would have been. Whether any was.
    fn grows(&mut self, now: When, beside: Option<When>, quiet: impl Fn((i32, i32)) -> bool) -> bool {
        let mut grew = false;
        loop {
            let due = match (self.alive(), beside) {
                (true, _) => self.since + REGROW,
                (false, Some(beside)) => self.since.max(beside) + SPREAD,
                (false, None) => break,
            };
            let takes = |nth: u8| self.sites.iter().any(|&(site, q, r)| site == nth && !quiet((q, r)));
            let Some(next) = self.broken.iter().position(|&nth| takes(nth)).filter(|_| now >= due) else { break };
            if !self.alive() {
                self.revived = Some(due);
            }
            self.broken.remove(next);
            self.since = due;
            grew = true;
        }
        grew
    }

    /// Whether nothing of it need be kept at `now`: every nest stands,
    /// and it has stood long enough to be to the colonies beside it as
    /// one that never fell.
    fn settled(&self, now: When) -> bool {
        self.broken.is_empty() && self.revived.is_none_or(|revived| now >= revived + SPREAD)
    }
}

/// Every den site the server has generated, by the chunk it lies in, with
/// the dens on them; and every colony that is not whole, wherever it is.
#[derive(Resource, Default)]
pub struct Dens {
    sites: HashMap<ChunkId, Vec<Site>>,
    /// Dens recalled for sites not yet found, by the site's `(q, r)`.
    recalled: HashMap<(i32, i32), Den>,
    /// Every site found, by its colony: which of it each is, and its tile.
    found: HashMap<(i32, i32), Vec<(u8, i32, i32)>>,
    colonies: HashMap<(i32, i32), Colony>,
    /// Sites whose den came to stand somewhere or went since the last
    /// [`Dens::take_changed`].
    changed: Vec<Qrz>,
    /// Colonies that changed since the last [`Dens::take_changed_colonies`].
    changed_colonies: Vec<(i32, i32)>,
}

impl Dens {
    /// Records a site a generated chunk holds, with the den recalled for
    /// it, if any; one already known is kept as it stands, den and all.
    pub fn found(&mut self, tile: Qrz, place: common::den::Site) {
        let sites = self.sites.entry(loc_to_chunk(tile)).or_default();
        if !sites.iter().any(|site| site.tile == tile) {
            sites.push(Site { tile, place, den: self.recalled.remove(&(tile.q, tile.r)) });
            self.found.entry(place.colony).or_default().push((place.nth, tile.q, tile.r));
            if self.colonies.get_mut(&place.colony).is_some_and(|colony| colony.finds(place.nth, (tile.q, tile.r))) {
                self.changed_colonies.push(place.colony);
            }
        }
    }

    /// Lays `den`, kept from an earlier run, on the site at `(q, r)`,
    /// where that site has none: on one already found, or when it is.
    pub fn recall(&mut self, (q, r): (i32, i32), den: Den) {
        match self.sites.values_mut().flatten().find(|site| (site.tile.q, site.tile.r) == (q, r)) {
            Some(site) => {
                site.den.get_or_insert(den);
            }
            None => {
                self.recalled.insert((q, r), den);
            }
        }
    }

    /// Lays `colony`, kept from an earlier run, as colony `id`.
    pub fn recall_colony(&mut self, id: (i32, i32), colony: Colony) {
        self.colonies.insert(id, colony);
    }

    /// The sites whose den changed since the last call: came to stand
    /// somewhere, or went.
    pub fn take_changed(&mut self) -> Vec<Qrz> {
        std::mem::take(&mut self.changed)
    }

    /// The colonies that changed since the last call.
    pub fn take_changed_colonies(&mut self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.changed_colonies)
    }

    /// Colony `id` as it is kept; none where every nest of it stands as
    /// the world made it.
    pub fn colony(&self, id: (i32, i32)) -> Option<&Colony> {
        self.colonies.get(&id)
    }

    /// Whether the nest on the site at `place` is broken.
    pub fn broken(&self, place: common::den::Site) -> bool {
        self.colonies.get(&place.colony).is_some_and(|colony| colony.is_broken(place.nth))
    }

    /// Whether a den's nest stands within `reach` tiles of `tile`.
    pub fn stands_within(&self, tile: Qrz, reach: i32) -> bool {
        self.sites.values().flatten().any(|site| {
            site.den.is_some_and(|den| den.at.unwrap_or(site.tile).flat_distance(&tile) <= reach) && !self.broken(site.place)
        })
    }

    /// The den on the site at `tile` that has stood, as it is.
    pub fn den_on(&self, tile: Qrz) -> Option<Den> {
        self.sites.get(&loc_to_chunk(tile))?.iter().find(|site| site.tile == tile)?.den.filter(|den| den.at.is_some())
    }

    /// Notes that `engagement` is gone from the world. The one a nest
    /// stands with goes only abandoned, so its den has nothing out, to
    /// come back whole; any other is a group with no one left of it.
    pub fn ended(&mut self, engagement: Entity) {
        for den in self.sites.values_mut().flatten().filter_map(|site| site.den.as_mut()) {
            match den.out.and_then(|out| out.groups.iter().position(|group| *group == Some(engagement))) {
                Some(0) => den.out = None,
                Some(group) => den.out.as_mut().expect("it has a group out").groups[group] = None,
                None => {}
            }
        }
    }

    /// Notes that `actor` died at `now`: the nest of a den, that nest is
    /// broken from then, and what lives of its pack is the last of it.
    pub fn died(&mut self, actor: Entity, now: When) {
        let Some(site) = self.sites.values_mut().flatten().find(|site| site.den.is_some_and(|den| den.out.is_some_and(|out| out.nest == actor))) else { return };
        let place = site.place;
        if let Some(den) = site.den.as_mut() {
            den.out = None;
        }
        let colony = self.colonies.entry(place.colony).or_insert_with(|| Colony::whole(self.found.get(&place.colony).cloned().unwrap_or_default(), now));
        colony.breaks(place.nth);
        colony.since = now;
        self.changed_colonies.push(place.colony);
    }

    /// Raises every broken nest due by `now`, and forgets each colony
    /// whole again for long enough: a colony's own by its standing nests,
    /// a dead one's from the first colony beside it to stand, the six a
    /// step from its own number, one never broken having stood for ever.
    /// No nest is raised on a tile `hubs` keeps quiet.
    pub fn grow(&mut self, now: When, hubs: &Hubs) {
        let mut ids: Vec<(i32, i32)> = self.colonies.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let beside = DIRECTIONS
                .iter()
                .filter_map(|step| match self.colonies.get(&(id.0 + step.q, id.1 + step.r)) {
                    None => Some(When(0)),
                    Some(colony) => colony.alive().then(|| colony.revived.unwrap_or(When(0))),
                })
                .min();
            let colony = self.colonies.get_mut(&id).expect("it is one of the colonies kept");
            let grew = colony.grows(now, beside, |tile| hubs.quiet(tile));
            if colony.settled(now) {
                self.colonies.remove(&id);
            }
            if grew || !self.colonies.contains_key(&id) {
                self.changed_colonies.push(id);
            }
        }
    }
}

/// An engagement gone from the world: every member dead, or abandoned
/// unwatched.
#[derive(Message, Clone, Copy, Debug)]
pub struct EngagementEnded {
    pub engagement: Entity,
}

/// The nearest dry ground the server has materialized to `site` and
/// [`APART`] from every den of `taken`, walking out by neighbour offsets,
/// and the tile its pack stands on there: the floor's, a level up. None
/// where none lies within [`SEARCH`] tiles.
pub fn dry_ground_near(map: &Map, site: Qrz, taken: &[Qrz]) -> Option<Qrz> {
    let mut seen = std::collections::HashSet::from([(site.q, site.r)]);
    let mut frontier = std::collections::VecDeque::from([(site.q, site.r)]);
    while let Some((q, r)) = frontier.pop_front() {
        if let Some((floor, _)) = map.get_by_qr(q, r) {
            if map.water_at(q, r).is_none() && taken.iter().all(|den| den.flat_distance(&floor) >= APART) {
                return Some(floor + Qrz::Z);
            }
        }
        if seen.len() >= SEARCH {
            continue;
        }
        for d in DIRECTIONS {
            let next = (q + d.q, r + d.r);
            if seen.insert(next) {
                frontier.push_back(next);
            }
        }
    }
    None
}

/// Where the `group`th of the `groups` a den on `site` holds stands, its
/// nest on `at`: the first with the nest, the others [`GROUP_REACH`] out,
/// set evenly round it at the site's own turn, each on dry ground the map
/// holds there and with the nest where it holds none.
fn post(map: &Map, site: Qrz, at: Qrz, group: usize, groups: u8) -> Qrz {
    if group == 0 {
        return at;
    }
    let outer = (groups as usize).saturating_sub(1).max(1);
    let turn = common::sway(site.q, site.r, common::TILE_SLOTS as usize).variation as usize;
    let step = DIRECTIONS[(turn + (group - 1) * DIRECTIONS.len() / outer) % DIRECTIONS.len()];
    let (q, r) = (at.q + step.q * GROUP_REACH, at.r + step.r * GROUP_REACH);
    match map.get_by_qr(q, r) {
        Some((floor, _)) if map.water_at(q, r).is_none() => floor + Qrz::Z,
        _ => at,
    }
}

/// Ages every hub to the calendar's moment, the country giving way to
/// each a level in `Setting::LevelHours`.
pub fn age_hubs(mut hubs: ResMut<Hubs>, settings: Res<Settings>, runtime: Res<combat::RunTime>, time: Res<Time>) {
    hubs.age(runtime.when(&time), settings.get(Setting::LevelHours));
}

/// Hears every frame which engagements left the world and which nests
/// were broken, and each [`TEND`] raises the broken nests that are due,
/// takes the den off every site near a player that has come to lie on
/// quiet ground, brings out the nest and pack of a den that stands with
/// nothing out, and brings back one of a short pack's dead when its time
/// comes. A pack stands on the ground the map holds, never read from the
/// world's generator, which may be busy building elsewhere for longer
/// than a frame.
#[allow(clippy::too_many_arguments)]
pub fn tend_dens(
    mut dens: ResMut<Dens>,
    mut ended: MessageReader<EngagementEnded>,
    mut messages: ParamSet<(MessageReader<Do>, MessageWriter<Do>)>,
    lobby: Res<Lobby>,
    players: Query<&Loc>,
    engagements: Query<Option<&Members>, With<Engagement>>,
    map: Res<Map>,
    tuning: Res<Tuning>,
    time: Res<Time>,
    runtime: Res<combat::RunTime>,
    dice: Res<combat::dice::Dice>,
    hubs: Res<Hubs>,
    mut next: Local<Moment>,
    mut commands: Commands,
) {
    let now = Moment::ZERO + time.elapsed();
    // What is kept of a colony, when it last changed, is the calendar's
    let when = runtime.when(&time);
    for message in messages.p0().read() {
        if let Do { event: Event::Despawn { ent, died: true } } = message {
            dens.died(*ent, when);
        }
    }
    for ending in ended.read() {
        dens.ended(ending.engagement);
    }
    if now < *next {
        return;
    }
    *next = now + TEND;
    dens.grow(when, &hubs);

    let Dens { sites, found, colonies, changed, changed_colonies, .. } = &mut *dens;
    let mut taken: Vec<Qrz> = sites.values().flatten().filter_map(|site| site.den?.at).collect();
    let mut quieted: Vec<Entity> = Vec::new();
    let map = &*map;

    for &player in lobby.right_values() {
        let Ok(loc) = players.get(player) else { continue };
        for chunk in calculate_visible_chunks(loc_to_chunk(**loc), PLACE_CHUNKS) {
            let Some(sites) = sites.get_mut(&chunk) else { continue };
            for site in sites.iter_mut().filter(|site| site.tile.flat_distance(loc) <= PLACE_RANGE) {
                let (tile, place) = ((site.tile.q, site.tile.r), site.place);
                if hubs.quiet(tile) {
                    if let Some(den) = site.den.take() {
                        info!("den gone from quiet ground at ({}, {})", tile.0, tile.1);
                        quieted.extend(den.out.iter().flat_map(|out| out.groups).flatten());
                        changed.push(site.tile);
                    }
                    if colonies.entry(place.colony).or_insert_with(|| Colony::whole(found.get(&place.colony).cloned().unwrap_or_default(), when)).breaks(place.nth) {
                        changed_colonies.push(place.colony);
                    }
                    continue;
                }
                if colonies.get(&place.colony).is_some_and(|colony| colony.is_broken(place.nth)) {
                    continue;
                }
                let den = site.den.get_or_insert_with(|| Den {
                    archetype: EnemyArchetype::denning_on(place.habitat),
                    // Rolled by the site, so a seed packs the same dens
                    groups: GROUPS.0 + dice.roll(("pack", site.tile)).pick((GROUPS.1 - GROUPS.0 + 1) as usize) as u8,
                    at: None,
                    out: None,
                });
                let level = hubs.level(tile);
                let below = level.saturating_sub(PACK_BELOW);
                // A member on a tile the map does not hold yet stands level
                // with its post.
                let elevation = |post: Qrz| move |q: i32, r: i32| map.get_by_qr(q, r).map_or(post.z - 1, |(floor, _)| floor.z);
                if let (Some(mut out), Some(at)) = (den.out, den.at) {
                    let nest = out.nest;
                    let living = |group: Option<Entity>| {
                        group.and_then(|group| engagements.get(group).ok()).map_or(0, |members| members.map_or(0, |members| members.all().filter(|&member| member != nest).count()))
                    };
                    let short = (0..den.groups as usize).find(|&group| living(out.groups[group]) < GROUP as usize);
                    out.refill = match (short, out.refill) {
                        (None, _) => None,
                        (Some(_), None) => Some(now + RESPAWN),
                        (Some(group), Some(due)) if now >= due => {
                            let post = post(map, site.tile, at, group, den.groups);
                            match out.groups[group].filter(|&held| engagements.get(held).is_ok()) {
                                Some(held) => {
                                    combat::engagement::spawn_member(&tuning, held, post, living(Some(held)), den.archetype, Side::WILD, below, NOTICE, elevation(post), &mut commands, &time);
                                }
                                None => {
                                    out.groups[group] = Some(combat::engagement::spawn_engagement(&tuning, post, den.archetype, Side::WILD, below, NOTICE, 1, elevation(post), &mut commands, &time));
                                }
                            }
                            None
                        }
                        (_, waiting) => waiting,
                    };
                    den.out = Some(out);
                    continue;
                }
                let Some(at) = den.at.or_else(|| dry_ground_near(map, site.tile, &taken)) else { continue };
                if den.at.is_none() {
                    taken.push(at);
                    changed.push(site.tile);
                }
                info!("den of {} groups of {:?} at level {level} on {:?} ground at ({}, {})", den.groups, den.archetype, place.habitat, at.q, at.r);
                den.at = Some(at);
                let mut groups = [None; GROUPS.1 as usize];
                for (group, held) in groups.iter_mut().enumerate().take(den.groups as usize) {
                    let post = post(map, site.tile, at, group, den.groups);
                    *held = Some(combat::engagement::spawn_engagement(&tuning, post, den.archetype, Side::WILD, below, NOTICE, GROUP, elevation(post), &mut commands, &time));
                }
                let nest = spawn_nest(&tuning, groups[0].expect("a den has a first group"), at, den.archetype, level, &mut commands, &time);
                den.out = Some(Out { nest, groups, refill: None });
            }
        }
    }

    // A den gone from quiet ground takes what it had out with it, as an
    // engagement abandoned goes
    let mut writer = messages.p1();
    for group in quieted {
        for member in engagements.get(group).ok().flatten().into_iter().flat_map(|members| members.all()) {
            writer.write(Do { event: Event::Despawn { ent: member, died: false } });
        }
        commands.entity(group).try_despawn();
    }
}

/// Spawns the nest of a den of `archetype` at `level` on `at`, a member of
/// its `pack`'s engagement, so it leaves the world when that does. It is an
/// actor with health and a side and nothing to act with: no heading, so it
/// picks no target, is never flanked and is never dragged.
fn spawn_nest(tuning: &Tuning, pack: Entity, at: Qrz, archetype: EnemyArchetype, level: u8, commands: &mut Commands, time: &Time) -> Entity {
    let profile = archetype.profile();
    let typ = EntityType::Actor(ActorImpl {
        origin: Origin::Evolved,
        approach: profile.approach,
        resilience: profile.resilience,
        identity: ActorIdentity::Nest(archetype),
    });
    let loc = Loc::new(at);
    let fighter = Fighter::new(tuning, calculate_enemy_attributes(level, archetype), Moment::ZERO + time.elapsed());
    let nest = commands.spawn((typ, loc, fighter, Side::WILD, EngagementMember(pack))).id();
    commands.entity(nest).insert(NearestNeighbor::new(nest, loc));
    nest
}

/// The den on `site` as drawn, about the tile its nest stands on, once
/// it has first stood, as its wreck where its nest is `broken`: its seed
/// and turn the site's own, from a slot past the tile's, so no tree's
/// sway is its.
pub fn shown(site: &Site, broken: bool) -> Option<(Qrz, DenLook)> {
    let den = site.den?;
    let at = den.at?;
    let sway = common::sway(site.tile.q, site.tile.r, common::TILE_SLOTS as usize);
    let look = DenLook {
        archetype: den.archetype,
        seed: (sway.variation % 256) as u8,
        yaw: sway.yaw as f32,
        cleared: broken,
    };
    Some((at, look))
}

/// Every tile of the ground the den on `at` clears or thins, each with
/// the ring it lies on round the den's own tile.
fn cleared_ground(at: Qrz, look: &DenLook) -> impl Iterator<Item = (Qrz, u32)> {
    let home = Qrz { z: 0, ..at };
    (0..=look.clearing() + OPEN + THIN).flat_map(move |k| home.ring(k).into_iter().map(move |tile| (tile, k)))
}

/// What a den leaves of `stood`, tile `(q, r)`'s cover before it, `ring`
/// tiles out from the den's own where its ground stays open out to
/// `open`: nothing there, then each growth and boulder kept by its own
/// roll, the more the further out, all of it past the thinning.
fn cleared_cover(stood: common::Cover, (q, r): (i32, i32), ring: u32, open: u32) -> common::Cover {
    let cut = 1.0 - ring.saturating_sub(open) as f64 / (THIN + 1) as f64;
    let kept = common::Cover::NONE.with_rock(stood.rock());
    let kept = stood.filled().filter(|&(site, _)| roll(q, r, site) >= cut).fold(kept, |cover, (site, growth)| cover.with(site, growth));
    let sites = common::SITES.len();
    stood.boulders().filter(|&k| roll(q, r, sites + k) >= cut).fold(kept, |cover, k| cover.with_boulder(k))
}

/// A den drawn on the map: how, and the cover it took from each tile of
/// its ground it cleared, laid back when it goes.
pub struct Drawn {
    look: DenLook,
    took: Vec<((i32, i32), common::Cover)>,
}

/// Lays back what each den gone from the map took and takes its circles
/// off, stands every den's circles on the map as it now is, clears the
/// ground of each that has come to stand once the map holds all of it,
/// and tells each player of every den drawn within `Setting::DenSight`
/// it has not been told of as it is, and of each it was told of that is
/// gone or out of sight.
pub fn show_dens(
    dens: Res<Dens>,
    map: Res<Map>,
    mut ground: Ground,
    lobby: Res<Lobby>,
    settings: Res<Settings>,
    players: Query<&Loc>,
    mut drawn: Local<HashMap<Qrz, Drawn>>,
    mut told: Local<HashMap<Entity, HashMap<Qrz, DenLook>>>,
    mut writer: MessageWriter<Do>,
) {
    let now: HashMap<Qrz, DenLook> = dens.sites.values().flatten().filter_map(|site| shown(site, dens.broken(site.place))).collect();
    drawn.retain(|at, den| {
        if now.contains_key(at) {
            return true;
        }
        map.set_solids((at.q, at.r), &[]);
        for &((q, r), cover) in &den.took {
            ground.put(&mut writer, q, r, cover);
        }
        false
    });
    for (&at, &look) in &now {
        if let Some(den) = drawn.get_mut(&at) {
            if den.look != look {
                map.set_solids((at.q, at.r), &look.circles());
                den.look = look;
            }
            continue;
        }
        let Some(standing) = cleared_ground(at, &look)
            .map(|(tile, ring)| Some((tile, ring, ground.cover(tile.q, tile.r)?)))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        map.set_solids((at.q, at.r), &look.circles());
        let open = look.clearing() + OPEN;
        let mut took = Vec::new();
        for (tile, ring, stood) in standing {
            let cleared = cleared_cover(stood, (tile.q, tile.r), ring, open);
            if cleared != stood {
                ground.put(&mut writer, tile.q, tile.r, cleared);
                took.push(((tile.q, tile.r), stood));
            }
        }
        drawn.insert(at, Drawn { look, took });
    }

    let sight = settings.get(Setting::DenSight);
    told.retain(|player, _| lobby.get_by_right(player).is_some());
    for &player in lobby.right_values() {
        let Ok(loc) = players.get(player) else { continue };
        let known = told.entry(player).or_default();
        for (&at, look) in drawn.iter().map(|(at, den)| (at, den.look)).filter(|(at, _)| at.flat_distance(loc) <= sight) {
            if known.insert(at, look) != Some(look) {
                writer.write(Do { event: Event::Den { ent: player, at, den: Some(look) } });
            }
        }
        known.retain(|at, _| {
            let still = drawn.contains_key(at) && at.flat_distance(loc) <= sight;
            if !still {
                writer.write(Do { event: Event::Den { ent: player, at: *at, den: None } });
            }
            still
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::den::Habitat;

    const DAY: Duration = Duration::from_secs(86_400);

    fn place(colony: (i32, i32), nth: u8, _of: u8) -> common::den::Site {
        common::den::Site { habitat: Habitat::Open, colony, nth }
    }

    /// A den out in the world on the `nth` of the `of` sites found of
    /// `colony`, its nest and the engagements of its two groups.
    fn out(dens: &mut Dens, colony: (i32, i32), nth: u8, of: u8, from: u32) -> (Qrz, Entity, [Entity; 2]) {
        let tile = |nth: u8| Qrz { q: colony.0 * 500 + nth as i32 * 30, r: colony.1 * 500, z: 0 };
        let site = tile(nth);
        let (nest, groups) = (Entity::from_raw_u32(from).unwrap(), [Entity::from_raw_u32(from + 1).unwrap(), Entity::from_raw_u32(from + 2).unwrap()]);
        for other in 0..of {
            dens.found(tile(other), place(colony, other, of));
        }
        dens.sites.get_mut(&loc_to_chunk(site)).unwrap().iter_mut().find(|held| held.tile == site).unwrap().den = Some(Den {
            archetype: EnemyArchetype::Berserker,
            groups: 2,
            at: Some(site),
            out: Some(Out { nest, groups: [Some(groups[0]), Some(groups[1]), None, None], refill: None }),
        });
        (site, nest, groups)
    }

    #[test]
    fn a_group_wiped_is_one_group_gone_and_a_nest_abandoned_waits_whole() {
        let mut dens = Dens::default();
        let (site, nest, groups) = out(&mut dens, (0, 0), 0, 2, 7);
        let den = |dens: &Dens| dens.den_on(site).unwrap();

        dens.ended(groups[1]);
        assert_eq!(den(&dens).out.unwrap().groups, [Some(groups[0]), None, None, None], "its nest stands, and brings the group back");
        dens.died(groups[1], When(5));
        assert!(!dens.broken(place((0, 0), 0, 2)), "a creature dead, its nest stands");

        dens.ended(groups[0]);
        assert_eq!(den(&dens).out, None, "abandoned, it stands with nothing out");
        assert!(!dens.broken(place((0, 0), 0, 2)));

        dens.sites.get_mut(&loc_to_chunk(site)).unwrap().iter_mut().find(|held| held.tile == site).unwrap().den.as_mut().unwrap().out = Some(Out { nest, groups: [Some(groups[0]), None, None, None], refill: None });
        dens.died(nest, When(10_000));
        assert!(dens.broken(place((0, 0), 0, 2)), "its nest broken, it is broken");
        assert_eq!(den(&dens).out, None);
        assert!(!dens.stands_within(site, 5), "and stands no more");
        assert_eq!(dens.take_changed_colonies(), vec![(0, 0)]);
    }

    /// A colony raises its broken nests again while one stands, a dead
    /// one only from a colony standing beside it and far more slowly, and
    /// cleared country heals from its edge inward.
    #[test]
    fn a_colony_regrows_from_its_own_and_a_dead_one_from_beside_it() {
        let hubs = Hubs::new((100_000, 0));
        let broke = When(1_000_000);
        let mut dens = Dens::default();
        // One nest of two broken at (0, 0); (1, 0) and (2, 0) dead, each
        // of one nest; every colony round them whole
        for (colony, nth, of, from) in [((0, 0), 0, 2, 10), ((1, 0), 0, 1, 20), ((2, 0), 0, 1, 30)] {
            let (_, nest, _) = out(&mut dens, colony, nth, of, from);
            dens.died(nest, broke);
        }
        // Walled in: the colonies beside the dead two are dead as well, but for each other and (0, 0)
        for (colony, from) in [((2, -1), 40), ((3, -1), 50), ((3, 0), 60), ((2, 1), 70), ((1, 1), 80), ((0, 1), 90), ((1, -1), 100)] {
            let (_, nest, _) = out(&mut dens, colony, 0, 1, from);
            dens.died(nest, broke);
        }
        let broken = |dens: &Dens, colony| dens.broken(place(colony, 0, 1));

        dens.grow(broke + REGROW - Duration::from_secs(1), &hubs);
        assert!(dens.broken(place((0, 0), 0, 2)), "not before its time");
        dens.grow(broke + REGROW, &hubs);
        assert!(!dens.broken(place((0, 0), 0, 2)), "a colony with a nest standing raises its broken one");
        assert!(broken(&dens, (1, 0)) && broken(&dens, (2, 0)), "a dead one waits far longer");

        dens.grow(broke + SPREAD, &hubs);
        assert!(!broken(&dens, (1, 0)), "then a nest is raised from the colony standing beside it");
        assert!(broken(&dens, (2, 0)), "and the one past it waits on that one");
        dens.grow(broke + SPREAD + SPREAD - Duration::from_secs(1), &hubs);
        assert!(broken(&dens, (2, 0)));
        // Found again long after, it is where it would have been
        for _ in 0..4 {
            dens.grow(broke + SPREAD * 40, &hubs);
        }
        assert!(!broken(&dens, (2, 0)) && !broken(&dens, (3, 0)));
        assert_eq!(dens.colony((2, 0)), None, "whole for long enough, nothing of it is kept");
    }

    /// No nest is raised on quiet ground, however long it waits.
    #[test]
    fn no_nest_is_raised_on_quiet_ground() {
        let mut hubs = Hubs::new((100_000, 0));
        let mut dens = Dens::default();
        let (site, nest, _) = out(&mut dens, (0, 0), 0, 2, 10);
        let (far, far_nest, _) = out(&mut dens, (9, 9), 0, 2, 20);
        hubs.founded((site.q, site.r));
        assert!(hubs.quiet((site.q, site.r)) && !hubs.quiet((far.q, far.r)));
        dens.died(nest, When(0));
        dens.died(far_nest, When(0));
        dens.grow(When(0) + DAY * 400, &hubs);
        assert!(dens.broken(place((0, 0), 0, 2)), "a hub holds its ground");
        assert!(!dens.broken(place((9, 9), 0, 2)));
    }

    /// Every piece of every den, at every seed and turn, stands on ground
    /// its den clears.
    #[test]
    fn a_dens_clearing_holds_every_piece() {
        use qrz::Convert;
        let map = Map::new(qrz::Map::new(common::grid::HEX_RADIUS, 0.8));
        let at = Qrz { q: 5, r: -3, z: 2 };
        for archetype in EnemyArchetype::ALL {
            for seed in 0..3 {
                for yaw in [0.0, 1.3, 4.0] {
                    let look = DenLook { archetype, seed, yaw, cleared: false };
                    let cleared: std::collections::HashSet<(i32, i32)> =
                        cleared_ground(at, &look).filter(|&(_, ring)| ring <= look.clearing()).map(|(t, _)| (t.q, t.r)).collect();
                    let centre = map.convert(Qrz { z: 0, ..at });
                    for piece in look.pieces() {
                        let [x, z] = piece.placed(yaw);
                        let tile = map.convert(centre + Vec3::new(x, 0.0, z));
                        assert!(cleared.contains(&(tile.q, tile.r)), "{archetype:?} {seed} {yaw}: a piece stands outside its clearing");
                    }
                }
            }
        }
    }

    /// A den's open ground holds nothing, the wood past its thinning is the
    /// world's own, and between them it thins out the further it lies.
    #[test]
    fn a_dens_ground_opens_then_thins_back_into_the_wood() {
        let wood = common::Cover::NONE.with(0, common::Content::Pine).with(1, common::Content::Deciduous).with(2, common::Content::Brush).with_rock(common::Rock::Limestone);
        let open = 6;
        let kept = |ring: u32| -> usize {
            (0..400).map(|q| cleared_cover(wood, (q, -q / 2), ring, open).filled().count()).sum()
        };
        assert_eq!(cleared_cover(wood, (3, 4), open, open), common::Cover::NONE.with_rock(common::Rock::Limestone));
        assert_eq!(cleared_cover(wood, (3, 4), open + THIN + 1, open), wood);
        let thinning: Vec<usize> = (open..=open + THIN + 1).map(kept).collect();
        assert!(thinning.windows(2).all(|w| w[0] <= w[1]), "{thinning:?}");
    }

    #[test]
    fn a_den_stands_on_the_nearest_dry_ground_to_its_site() {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for q in -3..=3 {
            map.insert(Qrz { q, r: 0, z: 0 }, Default::default());
        }
        for q in -3..=1 {
            map.set_water(q, 0, Some(1));
        }
        assert_eq!(dry_ground_near(&map, Qrz { q: -2, r: 0, z: 0 }, &[]), Some(Qrz { q: 2, r: 0, z: 1 }), "a site under water stands its den on the shore");
        assert_eq!(dry_ground_near(&map, Qrz { q: 3, r: 0, z: 0 }, &[]), Some(Qrz { q: 3, r: 0, z: 1 }), "a dry site stands it on the site");
        assert_eq!(dry_ground_near(&map, Qrz { q: 40, r: 40, z: 0 }, &[]), None, "where nothing is materialized, nowhere");
        assert_eq!(dry_ground_near(&map, Qrz { q: 3, r: 0, z: 0 }, &[Qrz { q: 2, r: 0, z: 1 }]), None, "and never on another den");
    }
}
