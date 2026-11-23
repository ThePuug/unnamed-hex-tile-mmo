//! Dens: where creatures live, as the world puts them and players leave
//! them.
//!
//! The world says where a den may stand and on what ground: a tile carries
//! a den site's habitat (`EventRegistry::den_at`), and a chunk's sites are
//! found as it is generated. A den on a site is a change to the world, the
//! server's to make, and it ages on world time. A site with none, a player
//! near, gets one of the archetype that dens on its ground, at the level its
//! distance from the haven gives. A den is a nest and the pack that lives in it. The nest is
//! an actor of its own, standing where the den is drawn, that a player
//! targets and strikes; while it stands, the den brings back one of its
//! pack's dead every [`RESPAWN`]. A standing den keeps both while players
//! come and go: one abandoned unwatched comes back whole when a player
//! returns. Breaking the nest leaves the den cleared, whatever of its pack
//! still lives the last of it: a change that stands for [`DECAY`] and then
//! is gone, leaving the site as the world made it. A standing den's growth
//! with age is unbuilt. A den outlives the server where it stands and as
//! it is, standing or cleared: whatever keeps the world is told of each
//! that changes ([`Dens::take_changed`]) and recalls those it kept
//! ([`Dens::recall`]) before their chunk's sites are found. Its nest and
//! pack do not, and come back whole as a player nears; the ground it
//! clears is laid again when it next stands.
//!
//! A den is drawn about the tile its pack first stood on, as its
//! archetype's model, active or cleared, its seed and turn its site's:
//! its pieces' circles stand on the map for every walker to go round, the
//! ground it stands on and [`OPEN`] tiles past it is cleared of all that
//! grew or lay there as long as it stands, thinning back into what stood
//! there over [`THIN`] more, so the den is seen over the wood round it,
//! and every player within [`sight`] is told of it as it is
//! (`show_dens`). A den is drawn once the map holds all the ground it
//! clears, which is cleared from the map's tiles, never read from the
//! world's generator: that may be busy building elsewhere for longer than
//! a frame.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use qrz::{Qrz, DIRECTIONS};

use combat::engagement::{Engagement, EngagementMember, Members};
use common::den::Habitat;
use common_bevy::{
    archetype::{calculate_enemy_attributes, EnemyArchetype},
    den::DenLook,
    message::{Do, Event},
    moment::Moment,
    chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId},
    components::{
        behaviour::Side,
        entity_type::{actor::{ActorIdentity, ActorImpl, Origin}, EntityType},
        resources::SpawnPoint,
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

/// How near a player a site gets its den, or a den its pack back, in tiles
pub const PLACE_RANGE: i32 = 80;

/// Chunks round a player's that hold every site within [`PLACE_RANGE`]
const PLACE_CHUNKS: u8 = 5;

/// How long a cleared den stands before the site is as the world made it
pub const DECAY: Duration = Duration::from_secs(600);

/// How often dens are raised and packs brought back
const TEND: Duration = Duration::from_secs(1);

/// How long a standing nest takes to bring back one of its pack's dead
pub const RESPAWN: Duration = Duration::from_secs(20);

/// The fewest a den's pack holds; a site's roll may add one
pub const PACK: u8 = 2;

/// How many levels below its nest's a den's pack stands
pub const PACK_BELOW: u8 = 3;

/// Tiles from the haven each level of a den lies further out
pub const STRETCH: i32 = 400;

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

/// A den site the server has generated, and the den on it, if any.
#[derive(Clone, Debug)]
pub struct Site {
    pub tile: Qrz,
    pub habitat: Habitat,
    pub den: Option<Den>,
}

/// A den: the nest on a site and the pack that lives in it, where it
/// stands, and how it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Den {
    pub archetype: EnemyArchetype,
    /// Its nest's level; its pack stands [`PACK_BELOW`] under it
    pub level: u8,
    /// How many its pack holds when whole
    pub size: u8,
    /// Where its pack stands, on dry ground near the site; none until it
    /// has first stood
    pub at: Option<Qrz>,
    pub state: DenState,
    /// When it came to be as it is, on the server's clock
    pub since: Moment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenState {
    /// Its nest stands: the nest and its pack's engagement out in the
    /// world, neither while no player is near, and when it next brings
    /// back one of its dead, while it is short of any
    Standing { pack: Option<Entity>, nest: Option<Entity>, refill: Option<Moment> },
    /// Its nest was broken
    Cleared,
}

impl DenState {
    /// A den standing with nothing of it out in the world.
    pub const UNWATCHED: DenState = DenState::Standing { pack: None, nest: None, refill: None };
}

impl Den {
    /// Whether the den has decayed by `now`, and the site is as the world
    /// made it.
    pub fn decayed(&self, now: Moment) -> bool {
        self.state == DenState::Cleared && now >= self.since + DECAY
    }
}

/// Every den site the server has generated, by the chunk it lies in, with
/// the dens on them.
#[derive(Resource, Default)]
pub struct Dens {
    sites: HashMap<ChunkId, Vec<Site>>,
    /// Dens recalled for sites not yet found, by the site's `(q, r)`.
    recalled: HashMap<(i32, i32), Den>,
    /// Sites whose den came to stand, was cleared or went since the last
    /// [`Dens::take_changed`].
    changed: Vec<Qrz>,
}

impl Dens {
    /// Records a site a generated chunk holds, with the den recalled for
    /// it, if any; one already known is kept as it stands, den and all.
    pub fn found(&mut self, tile: Qrz, habitat: Habitat) {
        let sites = self.sites.entry(loc_to_chunk(tile)).or_default();
        if !sites.iter().any(|site| site.tile == tile) {
            sites.push(Site { tile, habitat, den: self.recalled.remove(&(tile.q, tile.r)) });
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

    /// The sites whose den changed since the last call: came to stand, was
    /// cleared, or went.
    pub fn take_changed(&mut self) -> Vec<Qrz> {
        std::mem::take(&mut self.changed)
    }

    /// The den on the site at `tile` that has stood, as it is.
    pub fn den_on(&self, tile: Qrz) -> Option<Den> {
        self.sites.get(&loc_to_chunk(tile))?.iter().find(|site| site.tile == tile)?.den.filter(|den| den.at.is_some())
    }

    fn standing(&mut self, holds: impl Fn(Option<Entity>, Option<Entity>) -> bool) -> Option<&mut Site> {
        self.sites.values_mut().flatten()
            .find(|site| site.den.is_some_and(|den| matches!(den.state, DenState::Standing { pack, nest, .. } if holds(pack, nest))))
    }

    /// Notes that `pack` is gone from the world, its nest with it: its den
    /// stands unwatched, to come back whole.
    pub fn ended(&mut self, pack: Entity) {
        if let Some(den) = self.standing(|held, _| held == Some(pack)).and_then(|site| site.den.as_mut()) {
            den.state = DenState::UNWATCHED;
        }
    }

    /// Notes that `actor` died at `now`: the nest of a standing den, that
    /// den is cleared from then.
    pub fn died(&mut self, actor: Entity, now: Moment) {
        let Some(site) = self.standing(|_, nest| nest == Some(actor)) else { return };
        let tile = site.tile;
        if let Some(den) = site.den.as_mut() {
            *den = Den { state: DenState::Cleared, since: now, ..*den };
        }
        self.changed.push(tile);
    }
}

/// An engagement gone from the world: every member dead, or abandoned
/// unwatched.
#[derive(Message, Clone, Copy, Debug)]
pub struct EngagementEnded {
    pub engagement: Entity,
}

/// The level of a den on a site `tile`: its distance from the `haven`
/// over [`STRETCH`], whoever finds it.
pub fn den_level(tile: Qrz, haven: Qrz) -> u8 {
    (tile.flat_distance(&haven) / STRETCH).clamp(0, u8::MAX as i32) as u8
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

/// Hears every frame which packs left the world and which nests were
/// broken, and each [`TEND`] raises a den on every site near a player that
/// has none, brings out the nest and pack of a standing den that has
/// neither, brings back one of a short pack's dead when its time comes,
/// and lets a decayed den go. A pack stands on the ground the map holds,
/// never read from the world's generator, which may be busy building
/// elsewhere for longer than a frame.
#[allow(clippy::too_many_arguments)]
pub fn tend_dens(
    mut dens: ResMut<Dens>,
    mut ended: MessageReader<EngagementEnded>,
    mut done: MessageReader<Do>,
    lobby: Res<Lobby>,
    players: Query<&Loc>,
    engagements: Query<Option<&Members>, With<Engagement>>,
    map: Res<Map>,
    tuning: Res<Tuning>,
    time: Res<Time>,
    dice: Res<combat::dice::Dice>,
    haven: Option<Res<SpawnPoint>>,
    mut next: Local<Moment>,
    mut commands: Commands,
) {
    let now = Moment::ZERO + time.elapsed();
    for message in done.read() {
        if let Do { event: Event::Despawn { ent, died: true } } = message {
            dens.died(*ent, now);
        }
    }
    for ending in ended.read() {
        dens.ended(ending.engagement);
    }
    let Some(haven) = haven else { return };
    if now < *next {
        return;
    }
    *next = now + TEND;
    let mut taken: Vec<Qrz> = dens.sites.values().flatten().filter_map(|site| site.den?.at).collect();
    let mut changed = Vec::new();

    for &player in lobby.right_values() {
        let Ok(loc) = players.get(player) else { continue };
        for chunk in calculate_visible_chunks(loc_to_chunk(**loc), PLACE_CHUNKS) {
            let Some(sites) = dens.sites.get_mut(&chunk) else { continue };
            for site in sites.iter_mut().filter(|site| site.tile.flat_distance(loc) <= PLACE_RANGE) {
                if site.den.is_some_and(|den| den.decayed(now)) {
                    info!("den decayed at ({}, {})", site.tile.q, site.tile.r);
                    site.den = None;
                    changed.push(site.tile);
                }
                let den = site.den.get_or_insert_with(|| Den {
                    archetype: EnemyArchetype::denning_on(site.habitat),
                    level: den_level(site.tile, haven.0),
                    // Rolled by the site, so a seed packs the same dens
                    size: PACK + dice.roll(("pack", site.tile)).pick(2) as u8,
                    at: None,
                    state: DenState::UNWATCHED,
                    since: now,
                });
                let DenState::Standing { pack, nest, refill } = den.state else { continue };
                let map = &*map;
                let elevation = |at: Qrz| move |q: i32, r: i32| map.get_by_qr(q, r).map_or(at.z - 1, |(floor, _)| floor.z);
                let below = den.level.saturating_sub(PACK_BELOW);
                if let (Some(pack), Some(at)) = (pack, den.at) {
                    if let Ok(members) = engagements.get(pack) {
                        let living = members.map_or(0, |members| members.all().filter(|&member| Some(member) != nest).count());
                        let refill = match refill {
                            _ if living >= den.size as usize => None,
                            None => Some(now + RESPAWN),
                            Some(due) if now >= due => {
                                combat::engagement::spawn_member(&tuning, pack, at, living, den.archetype, Side::WILD, below, elevation(at), &mut commands, &time);
                                None
                            }
                            waiting => waiting,
                        };
                        den.state = DenState::Standing { pack: Some(pack), nest, refill };
                        continue;
                    }
                }
                let Some(at) = den.at.or_else(|| dry_ground_near(&map, site.tile, &taken)) else { continue };
                if den.at.is_none() {
                    taken.push(at);
                    changed.push(site.tile);
                    info!("den of {} {:?} at level {} on {:?} ground at ({}, {})", den.size, den.archetype, den.level, site.habitat, at.q, at.r);
                }
                den.at = Some(at);
                // A member on a tile the map does not hold yet stands level
                // with the den's own.
                let pack = combat::engagement::spawn_engagement(
                    &tuning, at, den.archetype, Side::WILD, below, den.size, elevation(at), &mut commands, &time,
                );
                let nest = spawn_nest(&tuning, pack, at, den.archetype, den.level, &mut commands, &time);
                den.state = DenState::Standing { pack: Some(pack), nest: Some(nest), refill: None };
            }
        }
    }
    dens.changed.extend(changed);
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

/// How far a player sees a den from, in tiles: the outer edge of the
/// level past the first summary level, the furthest a den is drawn.
pub fn sight() -> i32 {
    let edge = common_bevy::summary::threshold_horiz(common_bevy::summary::LOD_LEVELS[2]);
    (edge / (common::grid::HEX_RADIUS * 3f32.sqrt())).ceil() as i32
}

/// The den on `site` as drawn, about the tile its pack stands on, once
/// it has first stood: its seed and turn the site's own, from a slot past
/// the tile's, so no tree's sway is its.
pub fn shown(site: &Site) -> Option<(Qrz, DenLook)> {
    let den = site.den?;
    let at = den.at?;
    let sway = common::sway(site.tile.q, site.tile.r, common::TILE_SLOTS as usize);
    let look = DenLook {
        archetype: den.archetype,
        seed: (sway.variation % 256) as u8,
        yaw: sway.yaw as f32,
        cleared: den.state == DenState::Cleared,
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
/// and tells each player of every den drawn within its [`sight`] it has
/// not been told of as it is, and of each it was told of that is gone or
/// out of sight.
pub fn show_dens(
    dens: Res<Dens>,
    map: Res<Map>,
    mut ground: Ground,
    lobby: Res<Lobby>,
    players: Query<&Loc>,
    mut drawn: Local<HashMap<Qrz, Drawn>>,
    mut told: Local<HashMap<Entity, HashMap<Qrz, DenLook>>>,
    mut writer: MessageWriter<Do>,
) {
    let now: HashMap<Qrz, DenLook> = dens.sites.values().flatten().filter_map(shown).collect();
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

    let sight = sight();
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

    #[test]
    fn a_den_is_deeper_the_further_from_the_haven() {
        let haven = Qrz { q: 107289, r: -7677, z: 0 };
        let at = |tiles: i32| Qrz { q: haven.q + tiles, r: haven.r, z: 0 };
        assert_eq!(den_level(at(0), haven), 0, "at the haven, the weakest");
        assert!(den_level(at(STRETCH * 4), haven) > den_level(at(STRETCH), haven), "further out, deeper");
        assert_eq!(den_level(at(STRETCH * 100), haven), 100, "past any player's level");
    }

    #[test]
    fn a_den_is_cleared_when_its_nest_is_broken_and_an_abandoned_one_waits_whole() {
        let site = Qrz { q: 0, r: 0, z: 0 };
        let (pack, nest, member) = (Entity::from_raw_u32(7).unwrap(), Entity::from_raw_u32(8).unwrap(), Entity::from_raw_u32(9).unwrap());
        let out = DenState::Standing { pack: Some(pack), nest: Some(nest), refill: None };
        let mut dens = Dens::default();
        dens.found(site, Habitat::Open);
        let den = |dens: &Dens| dens.sites[&loc_to_chunk(site)][0].den.unwrap();
        dens.sites.get_mut(&loc_to_chunk(site)).unwrap()[0].den = Some(Den {
            archetype: EnemyArchetype::Berserker, level: 0, size: 2, at: Some(site),
            state: out, since: Moment::ZERO,
        });

        dens.ended(pack);
        assert_eq!(den(&dens).state, DenState::UNWATCHED, "abandoned, it stands with nothing out");

        dens.sites.get_mut(&loc_to_chunk(site)).unwrap()[0].den.as_mut().unwrap().state = out;
        let killed = Moment::from_millis(10_000);
        dens.died(member, killed);
        assert_eq!(den(&dens).state, out, "one of its pack dead, it stands");
        dens.died(nest, killed);
        assert_eq!(den(&dens).state, DenState::Cleared);
        dens.ended(pack);
        assert_eq!(den(&dens).state, DenState::Cleared, "and stays cleared when the last of its pack goes");
        assert!(!den(&dens).decayed(killed + DECAY - Duration::from_secs(1)), "cleared, it stands a while");
        assert!(den(&dens).decayed(killed + DECAY), "then it is gone");
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
