//! # Spawner Event Layer
//!
//! Lazily evaluates spawner placements based on terrain tags from prior layers
//! (plate classification + spine cross-section tags). Uses the generic EventCache
//! for LRU-evicted, deterministic chunk caching.
//!
//! Spawners do NOT exist as Map tiles. They live only in this cache, queried
//! directly by the engagement activation system.

use common::PlateTag;
use crate::Terrain;
use crate::events::EventCache;

/// Probability that an eligible chunk center gets a spawner.
const SPAWNER_PROBABILITY: f64 = 0.3;

/// Spawner chunk scale in world units. Each spawner chunk evaluates one
/// potential spawner at its center. Uses the same hex chunk layout as
/// tile chunks but at a coarser spacing to control density.
///
/// At CHUNK_SPACING=19 tiles, this means one potential spawner per tile chunk.
const SPAWNER_CHUNK_SCALE: f64 = 19.0;

/// Maximum cached spawner chunks before LRU eviction.
const SPAWNER_CACHE_MAX_CHUNKS: usize = 10_000;

/// Enemy archetype — re-exported from common-bevy for terrain-layer use.
/// The terrain crate can't depend on common-bevy, so we define a local
/// mirror that maps 1:1. Server converts at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnerArchetype {
    Berserker,
    Juggernaut,
    Kiter,
    Defender,
}

/// A spawner placement within a chunk.
#[derive(Debug, Clone)]
pub struct SpawnerPlacement {
    /// World position of the spawner
    pub wx: f64,
    pub wy: f64,
    /// Archetype determined by terrain tags
    pub archetype: SpawnerArchetype,
}

/// Per-chunk spawner data: zero or one spawner placement.
pub type SpawnerChunkData = Option<SpawnerPlacement>;

/// Lazily-evaluated spawner cache. Queries terrain tags from the Terrain
/// struct (which provides plate + spine tag access) to determine placements.
pub struct SpawnerCache {
    cache: EventCache<SpawnerChunkData>,
    seed: u64,
}

impl SpawnerCache {
    pub fn new(seed: u64) -> Self {
        Self {
            cache: EventCache::new(SPAWNER_CACHE_MAX_CHUNKS),
            seed,
        }
    }

    /// Query spawners near a world position. Populates the 1-ring and returns
    /// all spawner placements in the neighborhood.
    pub fn spawners_near(
        &mut self,
        wx: f64, wy: f64,
        terrain: &Terrain,
    ) -> Vec<SpawnerPlacement> {
        let (cq, cr) = crate::events::chunk_coord(wx, wy, SPAWNER_CHUNK_SCALE);
        let seed = self.seed;

        for (dq, dr) in crate::events::chunk_1ring(cr) {
            self.cache.ensure(cq + dq, cr + dr, &mut |eq, er| {
                evaluate_spawner_chunk(eq, er, seed, terrain)
            });
        }

        let mut result = Vec::new();
        for (dq, dr) in crate::events::chunk_1ring(cr) {
            self.cache.touch(cq + dq, cr + dr);
            if let Some(Some(placement)) = self.cache.get(cq + dq, cr + dr) {
                result.push(placement.clone());
            }
        }
        result
    }
}

/// Evaluate a single spawner chunk: deterministic from seed + chunk coords.
fn evaluate_spawner_chunk(
    cq: i32, cr: i32,
    seed: u64,
    terrain: &Terrain,
) -> SpawnerChunkData {
    if !spawner_roll(cq, cr, seed) {
        return None;
    }

    let (wx, wy) = crate::events::chunk_center(cq, cr, SPAWNER_CHUNK_SCALE);
    let (q, r) = crate::world_to_hex(wx, wy);
    let tags = terrain.tags_at(q, r);

    archetype_for_tags(tags.as_slice()).map(|archetype| {
        SpawnerPlacement { wx, wy, archetype }
    })
}

/// Deterministic spawner eligibility check for a chunk.
fn spawner_roll(cq: i32, cr: i32, seed: u64) -> bool {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    cq.hash(&mut hasher);
    cr.hash(&mut hasher);
    0x5370_6177_6E65_72u64.hash(&mut hasher); // "Spawner" salt
    let h = hasher.finish();
    (h % 1000) < (SPAWNER_PROBABILITY * 1000.0) as u64
}

/// Map terrain tags to spawner archetype. Returns None if terrain is ineligible.
fn archetype_for_tags(tags: &[PlateTag]) -> Option<SpawnerArchetype> {
    let has = |t: PlateTag| tags.iter().any(|tag| std::mem::discriminant(tag) == std::mem::discriminant(&t));

    if has(PlateTag::Sea) { return None; }
    if has(PlateTag::Ridge) { return None; }

    if has(PlateTag::Highland) { return Some(SpawnerArchetype::Berserker); }
    if has(PlateTag::Foothills) { return Some(SpawnerArchetype::Juggernaut); }
    if has(PlateTag::Coast) { return Some(SpawnerArchetype::Defender); }
    if has(PlateTag::Inland) { return Some(SpawnerArchetype::Kiter); }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawner_roll_deterministic() {
        let a = spawner_roll(5, 3, 42);
        let b = spawner_roll(5, 3, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn spawner_roll_varies_by_position() {
        // Different chunks should get different rolls (not all same)
        let mut trues = 0;
        for cq in 0..100 {
            if spawner_roll(cq, 0, 42) { trues += 1; }
        }
        assert!(trues > 10 && trues < 90, "expected ~30% spawners, got {trues}/100");
    }

    #[test]
    fn archetype_sea_excluded() {
        assert_eq!(archetype_for_tags(&[PlateTag::Sea]), None);
    }

    #[test]
    fn archetype_ridge_excluded() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland, PlateTag::Ridge]), None);
    }

    #[test]
    fn archetype_highland_is_berserker() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland, PlateTag::Highland]), Some(SpawnerArchetype::Berserker));
    }

    #[test]
    fn archetype_coast_is_defender() {
        assert_eq!(archetype_for_tags(&[PlateTag::Coast]), Some(SpawnerArchetype::Defender));
    }

    #[test]
    fn archetype_inland_flat_is_kiter() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland]), Some(SpawnerArchetype::Kiter));
    }
}
