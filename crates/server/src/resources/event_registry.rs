use bevy::prelude::*;
use common::HexLattice;
use terrain::events::Composite;
use terrain::events::plates::PlateEvent;
use terrain::events::spawner::{SpawnerEvent, SpawnerPlacementIndex, SpawnerPlacement};
use terrain::events::spines::SpineEvent;
use terrain::TagSet;

/// Server-side registry of world events.
///
/// Owns the Composite with PlateEvent + SpineEvent + SpawnerEvent.
/// All terrain queries and spawner placement queries route through here.
#[derive(Resource)]
pub struct EventRegistry {
    composite: Composite,
}

impl EventRegistry {
    pub fn new(seed: u64) -> Self {
        let mut composite = Composite::new(seed);
        composite.add_event(Box::new(PlateEvent::new(seed)));
        composite.add_event(Box::new(SpineEvent::new(seed)));
        composite.add_event(Box::new(SpawnerEvent::new(seed)));

        Self { composite }
    }

    /// Get elevation at a hex tile position (discretized to z-level).
    pub fn elevation_at(&self, q: i32, r: i32) -> i32 {
        self.composite.elevation_at(q, r)
    }

    /// Get tags at a hex tile position.
    #[allow(dead_code)]
    pub fn tags_at(&self, q: i32, r: i32) -> TagSet {
        self.composite.tags_at(q, r)
    }

    /// Query spawner placements near a hex tile position.
    /// Triggers deform cascade to populate SpawnerPlacementIndex, then
    /// queries the index for placements within the search radius.
    pub fn spawners_near(&self, q: i32, r: i32) -> Vec<SpawnerPlacement> {
        // Trigger deform cascade so spawner index is populated for this region
        let _ = self.composite.tile_at(q, r);

        // Query SpawnerPlacementIndex for nearby cells
        let spawner_lattice = HexLattice::new(9); // SPAWNER_CELL_SCALE
        let center_cell = spawner_lattice.cell_id(q, r);
        // Search 1-ring of spawner cells (covers ~19 tile radius)
        let search_cells = spawner_lattice.cells_within_distance(center_cell, 5);

        self.composite.with_indexes(|indexes| {
            match indexes.get::<SpawnerPlacementIndex>() {
                Some(idx) => idx.placements_in(&search_cells).into_iter().cloned().collect(),
                None => Vec::new(),
            }
        })
    }

    /// Drain event metrics (reads gauges, resets interval counters).
    pub fn drain_metrics(&self) -> terrain::events::EventMetricsSnapshot {
        self.composite.drain_metrics()
    }
}
