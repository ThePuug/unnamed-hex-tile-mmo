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

/// Noise wavelength for spawner density field (world units).
/// Controls cluster size — larger = bigger spawner-dense/sparse regions.
const SPAWNER_NOISE_WAVELENGTH: f64 = 800.0;

/// Noise threshold for spawner eligibility. Higher = fewer spawners.
/// At 0.3, roughly 30% of the noise field is above threshold.
const SPAWNER_NOISE_THRESHOLD: f64 = 0.1;

/// Seed offset to avoid correlation with other noise fields.
const SPAWNER_NOISE_SEED_OFFSET: u64 = 0x5370_6177_6E65_72; // "Spawner"

/// Spawner chunk scale in world units. Each spawner chunk evaluates one
/// potential spawner at its center. Uses the same hex chunk layout as
/// tile chunks but at a coarser spacing to control density.
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
            }, |data| data.is_some());
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

    /// Access cache metrics for snapshot.
    pub fn cache_metrics(&self) -> &crate::events::EventCacheMetrics {
        &self.cache.metrics
    }
}

/// Evaluate a single spawner chunk: deterministic from seed + chunk coords.
fn evaluate_spawner_chunk(
    cq: i32, cr: i32,
    seed: u64,
    terrain: &Terrain,
) -> SpawnerChunkData {
    let (wx, wy) = crate::events::chunk_center(cq, cr, SPAWNER_CHUNK_SCALE);

    if !spawner_eligible(wx, wy, seed) {
        return None;
    }

    let (q, r) = crate::world_to_hex(wx, wy);
    let tags = terrain.tags_at(q, r);

    archetype_for_tags(tags.as_slice()).map(|a| SpawnerPlacement { wx, wy, archetype: a })
}

/// Noise-based spawner eligibility. Spatially coherent — nearby chunks
/// have similar density, creating spawner-dense and sparse regions.
fn spawner_eligible(wx: f64, wy: f64, seed: u64) -> bool {
    let noise = crate::noise::simplex_2d(
        wx / SPAWNER_NOISE_WAVELENGTH,
        wy / SPAWNER_NOISE_WAVELENGTH,
        seed.wrapping_add(SPAWNER_NOISE_SEED_OFFSET),
    );
    noise > SPAWNER_NOISE_THRESHOLD
}

// ── Tag predicate matching ──

/// Combinators for matching against a tile's tag set.
#[derive(Clone)]
pub enum TagPredicate {
    /// Tag set must contain this tag.
    With(PlateTag),
    /// Tag set must NOT contain this tag.
    Without(PlateTag),
    /// Tag set must contain at least one of these.
    AnyOf(Vec<PlateTag>),
    /// All sub-predicates must match.
    AllOf(Vec<TagPredicate>),
}

impl TagPredicate {
    fn has(tags: &[PlateTag], target: &PlateTag) -> bool {
        tags.iter().any(|t| std::mem::discriminant(t) == std::mem::discriminant(target))
    }

    pub fn matches(&self, tags: &[PlateTag]) -> bool {
        match self {
            TagPredicate::With(t) => Self::has(tags, t),
            TagPredicate::Without(t) => !Self::has(tags, t),
            TagPredicate::AnyOf(ts) => ts.iter().any(|t| Self::has(tags, t)),
            TagPredicate::AllOf(preds) => preds.iter().all(|p| p.matches(tags)),
        }
    }
}

/// Archetype rule: predicate + archetype. First matching rule wins.
type ArchetypeRule = (TagPredicate, SpawnerArchetype);

fn archetype_rules() -> Vec<ArchetypeRule> {
    use PlateTag::*;
    use SpawnerArchetype::*;
    use TagPredicate::*;

    vec![
        // Highland terrain → Berserker
        (AllOf(vec![With(Highland)]), Berserker),
        // Foothills terrain → Juggernaut
        (AllOf(vec![With(Foothills)]), Juggernaut),
        // Ridge terrain → Defender
        (AllOf(vec![With(Ridge)]), Defender),
        // Flat inland (no spine tags) → Kiter
        (AllOf(vec![
            With(Inland),
            Without(Highland),
            Without(Foothills),
            Without(Ridge),
        ]), Kiter),
    ]
}

/// Map terrain tags to spawner archetype via predicate rules. First match wins.
fn archetype_for_tags(tags: &[PlateTag]) -> Option<SpawnerArchetype> {
    archetype_rules().iter()
        .find(|(pred, _)| pred.matches(tags))
        .map(|(_, arch)| *arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawner_eligible_deterministic() {
        let a = spawner_eligible(500.0, 300.0, 42);
        let b = spawner_eligible(500.0, 300.0, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn spawner_eligible_varies_spatially() {
        let mut trues = 0;
        for x in 0..100 {
            if spawner_eligible(x as f64 * SPAWNER_CHUNK_SCALE, 0.0, 42) { trues += 1; }
        }
        assert!(trues > 5 && trues < 95, "expected spatial variation, got {trues}/100 eligible");
    }

    #[test]
    fn archetype_sea_excluded() {
        assert_eq!(archetype_for_tags(&[PlateTag::Sea]), None);
    }

    #[test]
    fn archetype_ridge_is_defender() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland, PlateTag::Ridge]), Some(SpawnerArchetype::Defender));
    }

    #[test]
    fn archetype_highland_is_berserker() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland, PlateTag::Highland]), Some(SpawnerArchetype::Berserker));
    }

    #[test]
    fn archetype_coast_no_match() {
        assert_eq!(archetype_for_tags(&[PlateTag::Coast]), None);
    }

    #[test]
    fn archetype_inland_flat_is_kiter() {
        assert_eq!(archetype_for_tags(&[PlateTag::Inland]), Some(SpawnerArchetype::Kiter));
    }
}
