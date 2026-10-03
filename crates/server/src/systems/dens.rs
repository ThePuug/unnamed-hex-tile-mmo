//! Dens: where creatures live, as the world puts them and players leave
//! them.
//!
//! The world says where a den may stand and on what ground: a tile carries
//! a den site's habitat (`EventRegistry::den_at`), and a chunk's sites are
//! found as it is generated. A den on a site is a change to the world, the
//! server's to make, and it ages on world time. A site with none, a player
//! near, gets one of the archetype that dens on its ground, at the level its
//! distance from the haven gives, never nearer the player's own than
//! [`MARGIN`]. A standing den keeps its pack while players come and go: one
//! abandoned unwatched comes back whole when a player returns. Killing the
//! pack leaves the den cleared, a change that stands for [`DECAY`] and then
//! is gone, leaving the site as the world made it. A standing den's growth
//! with age is unbuilt. Dens are held in memory, as every change to the
//! world is.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use qrz::{Qrz, DIRECTIONS};

use common::den::Habitat;
use common_bevy::{
    archetype::EnemyArchetype,
    chunk::{calculate_visible_chunks, loc_to_chunk, ChunkId},
    components::{behaviour::Side, engagement::Engagement, ActorAttributes, Loc},
    haven::HAVEN_LOCATION,
    resources::map::Map,
    tuning::Tuning,
};

use crate::resources::{event_registry::EventRegistry, Lobby};

/// How near a player a site gets its den, or a den its pack back, in tiles
pub const PLACE_RANGE: i32 = 80;

/// Chunks round a player's that hold every site within [`PLACE_RANGE`]
const PLACE_CHUNKS: u8 = 5;

/// How long a cleared den stands before the site is as the world made it
pub const DECAY: Duration = Duration::from_secs(600);

/// How often dens are raised and packs brought back
const TEND: Duration = Duration::from_secs(1);

/// Tiles from the haven each level of a den lies further out
pub const STRETCH: i32 = 400;

/// How many levels below the nearest player's a den always stands
pub const MARGIN: u32 = 5;

/// How many tiles a den's search for dry ground round its site may look at
const SEARCH: usize = 400;

/// A den site the server has generated, and the den on it, if any.
#[derive(Clone, Debug)]
pub struct Site {
    pub tile: Qrz,
    pub habitat: Habitat,
    pub den: Option<Den>,
}

/// A den: the pack that lives on a site, where it stands, and how it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Den {
    pub archetype: EnemyArchetype,
    pub level: u8,
    pub size: u8,
    /// Where its pack stands, on dry ground near the site; none until it
    /// has first stood
    pub at: Option<Qrz>,
    pub state: DenState,
    /// When it came to be as it is
    pub since: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenState {
    /// Its pack lives: the engagement out in the world, none while no
    /// player is near
    Standing { pack: Option<Entity> },
    /// Its pack was killed
    Cleared,
}

impl Den {
    /// Whether the den has decayed by `now`, and the site is as the world
    /// made it.
    pub fn decayed(&self, now: Duration) -> bool {
        self.state == DenState::Cleared && now >= self.since + DECAY
    }
}

/// Every den site the server has generated, by the chunk it lies in, with
/// the dens on them.
#[derive(Resource, Default)]
pub struct Dens {
    sites: HashMap<ChunkId, Vec<Site>>,
}

impl Dens {
    /// Records a site a generated chunk holds; one already known is kept as
    /// it stands, den and all.
    pub fn found(&mut self, tile: Qrz, habitat: Habitat) {
        let sites = self.sites.entry(loc_to_chunk(tile)).or_default();
        if !sites.iter().any(|site| site.tile == tile) {
            sites.push(Site { tile, habitat, den: None });
        }
    }

    /// Notes how `pack` ended at `now`: killed, its den is cleared from
    /// then; abandoned, its den stands without it.
    pub fn ended(&mut self, pack: Entity, cleared: bool, now: Duration) {
        let Some(den) = self.sites.values_mut().flatten().filter_map(|site| site.den.as_mut())
            .find(|den| den.state == DenState::Standing { pack: Some(pack) })
        else {
            return;
        };
        *den = if cleared {
            Den { state: DenState::Cleared, since: now, ..*den }
        } else {
            Den { state: DenState::Standing { pack: None }, ..*den }
        };
    }
}

/// An engagement gone: `cleared` when every member died, else abandoned
/// unwatched.
#[derive(Message, Clone, Copy, Debug)]
pub struct EngagementEnded {
    pub engagement: Entity,
    pub cleared: bool,
}

/// The level of a den on a site `tile`, for a player of `level`: its
/// distance from the haven over [`STRETCH`], never within [`MARGIN`] of the
/// player's.
pub fn den_level(tile: Qrz, level: u32) -> u8 {
    let far = (tile.flat_distance(&HAVEN_LOCATION) / STRETCH).max(0) as u32;
    far.min(level.saturating_sub(MARGIN)).min(u8::MAX as u32) as u8
}

/// The nearest dry ground the server has materialized to `site`, walking
/// out by neighbour offsets, and the tile its pack stands on there: the
/// floor's, a level up. None where none lies within [`SEARCH`] tiles.
pub fn dry_ground_near(map: &Map, site: Qrz) -> Option<Qrz> {
    let mut seen = std::collections::HashSet::from([(site.q, site.r)]);
    let mut frontier = std::collections::VecDeque::from([(site.q, site.r)]);
    while let Some((q, r)) = frontier.pop_front() {
        if let Some((floor, _)) = map.get_by_qr(q, r) {
            if map.water_at(q, r).is_none() {
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

/// Hears every frame how packs ended, and each [`TEND`] raises a den on
/// every site near a player that has none, brings back the pack of a
/// standing den that has none, and lets a decayed den go.
#[allow(clippy::too_many_arguments)]
pub fn tend_dens(
    mut dens: ResMut<Dens>,
    mut ended: MessageReader<EngagementEnded>,
    lobby: Res<Lobby>,
    players: Query<(&Loc, Option<&ActorAttributes>)>,
    engagements: Query<(), With<Engagement>>,
    map: Res<Map>,
    registry: Res<EventRegistry>,
    tuning: Res<Tuning>,
    time: Res<Time>,
    mut next: Local<Duration>,
    mut commands: Commands,
) {
    let now = time.elapsed();
    for ending in ended.read() {
        dens.ended(ending.engagement, ending.cleared, now);
    }
    if now < *next {
        return;
    }
    *next = now + TEND;

    for &player in lobby.right_values() {
        let Ok((loc, attrs)) = players.get(player) else { continue };
        let level = attrs.map_or(0, ActorAttributes::total_level);
        for chunk in calculate_visible_chunks(loc_to_chunk(**loc), PLACE_CHUNKS) {
            let Some(sites) = dens.sites.get_mut(&chunk) else { continue };
            for site in sites.iter_mut().filter(|site| site.tile.flat_distance(loc) <= PLACE_RANGE) {
                if site.den.is_some_and(|den| den.decayed(now)) {
                    info!("den decayed at ({}, {})", site.tile.q, site.tile.r);
                    site.den = None;
                }
                let den = site.den.get_or_insert_with(|| {
                    // The spec's den: one at its level, or two three below it
                    let level = den_level(site.tile, level);
                    let pair = rand::random::<bool>() && level >= 3;
                    Den {
                        archetype: EnemyArchetype::denning_on(site.habitat),
                        level: if pair { level - 3 } else { level },
                        size: if pair { 2 } else { 1 },
                        at: None,
                        state: DenState::Standing { pack: None },
                        since: now,
                    }
                });
                let DenState::Standing { pack } = den.state else { continue };
                if pack.is_some_and(|pack| engagements.contains(pack)) {
                    continue;
                }
                let Some(at) = den.at.or_else(|| dry_ground_near(&map, site.tile)) else { continue };
                if den.at.is_none() {
                    info!("den of {} {:?} at level {} on {:?} ground at ({}, {})", den.size, den.archetype, den.level, site.habitat, at.q, at.r);
                }
                den.at = Some(at);
                let pack = combat::engagement::spawn_engagement(
                    &tuning, at, den.archetype, Side::WILD, den.level, den.size,
                    |q, r| registry.elevation_at(q, r), &mut commands, &time,
                );
                den.state = DenState::Standing { pack: Some(pack) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_den_is_deeper_the_further_from_the_haven_and_never_near_the_players_level() {
        let at = |tiles: i32| Qrz { q: HAVEN_LOCATION.q + tiles, r: HAVEN_LOCATION.r, z: 0 };
        assert_eq!(den_level(at(0), 15), 0, "at the haven, the weakest");
        assert!(den_level(at(STRETCH * 4), 15) > den_level(at(STRETCH), 15), "further out, deeper");
        assert_eq!(den_level(at(STRETCH * 100), 15), 10, "never within the margin of the player's level");
        assert_eq!(den_level(at(STRETCH * 100), 3), 0, "and a player below the margin meets the weakest");
    }

    #[test]
    fn a_cleared_den_stands_until_it_decays_and_an_abandoned_one_waits_for_its_pack() {
        let site = Qrz { q: 0, r: 0, z: 0 };
        let pack = Entity::from_raw_u32(7).unwrap();
        let mut dens = Dens::default();
        dens.found(site, Habitat::Open);
        let den = |dens: &Dens| dens.sites[&loc_to_chunk(site)][0].den.unwrap();
        dens.sites.get_mut(&loc_to_chunk(site)).unwrap()[0].den = Some(Den {
            archetype: EnemyArchetype::Berserker, level: 0, size: 1, at: Some(site),
            state: DenState::Standing { pack: Some(pack) }, since: Duration::ZERO,
        });

        dens.ended(pack, false, Duration::from_secs(5));
        assert_eq!(den(&dens).state, DenState::Standing { pack: None }, "abandoned, it stands without its pack");

        dens.sites.get_mut(&loc_to_chunk(site)).unwrap()[0].den.as_mut().unwrap().state = DenState::Standing { pack: Some(pack) };
        let killed = Duration::from_secs(10);
        dens.ended(pack, true, killed);
        assert_eq!(den(&dens).state, DenState::Cleared);
        assert!(!den(&dens).decayed(killed + DECAY - Duration::from_secs(1)), "cleared, it stands a while");
        assert!(den(&dens).decayed(killed + DECAY), "then it is gone");
    }

    #[test]
    fn a_den_stands_on_the_nearest_dry_ground_to_its_site() {
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        for q in -3..=3 {
            map.insert(Qrz { q, r: 0, z: 0 }, Default::default());
        }
        for q in -3..=1 {
            map.set_water(q, 0, Some(1));
        }
        assert_eq!(dry_ground_near(&map, Qrz { q: -2, r: 0, z: 0 }), Some(Qrz { q: 2, r: 0, z: 1 }), "a site under water stands its den on the shore");
        assert_eq!(dry_ground_near(&map, Qrz { q: 3, r: 0, z: 0 }), Some(Qrz { q: 3, r: 0, z: 1 }), "a dry site stands it on the site");
        assert_eq!(dry_ground_near(&map, Qrz { q: 40, r: 40, z: 0 }), None, "where nothing is materialized, nowhere");
    }
}
