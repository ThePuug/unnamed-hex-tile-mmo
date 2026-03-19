use bevy::prelude::*;
use terrain::spawners::{SpawnerCache, SpawnerPlacement};
use terrain::events::EventCacheMetrics;

/// Server-side registry of world event caches.
/// Owns gameplay event layers (spawners, future fauna/water).
/// Queries the Terrain substrate for plate/spine tag access.
#[derive(Resource)]
pub struct EventRegistry {
    spawner_cache: SpawnerCache,
}

impl EventRegistry {
    pub fn new(seed: u64) -> Self {
        Self {
            spawner_cache: SpawnerCache::new(seed),
        }
    }

    /// Query spawner placements near a hex tile position.
    pub fn spawners_near(
        &mut self,
        terrain: &terrain::Terrain,
        q: i32, r: i32,
    ) -> Vec<SpawnerPlacement> {
        let (wx, wy) = terrain::hex_to_world(q, r);
        self.spawner_cache.spawners_near(wx, wy, terrain)
    }

    /// Snapshot spawner cache metrics (cumulative).
    pub fn spawner_metrics(&self) -> EventCacheMetrics {
        self.spawner_cache.cache_metrics().clone()
    }
}
