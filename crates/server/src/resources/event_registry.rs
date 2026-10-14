use bevy::prelude::*;
use world::events::Composite;
use world::Cover;

/// Server-side registry of world events: the composite every terrain
/// query goes through (INV-009), Arc-wrapped so async chunk generation
/// tasks share it.
#[derive(Resource, Clone)]
pub struct EventRegistry {
    composite: std::sync::Arc<Composite>,
}

impl EventRegistry {
    /// The world's stack; with `ladder_at`, its heights and water from the
    /// terrain ladder over the landmass around that tile.
    pub fn new(seed: u64, ladder_at: Option<(i32, i32)>) -> Self {
        let composite = match ladder_at {
            Some(centre) => Composite::standard_ladder(seed, centre),
            None => Composite::standard(seed),
        };
        Self { composite: std::sync::Arc::new(composite) }
    }

    /// Whether the world's heights are the terrain ladder's.
    pub fn is_laddered(&self) -> bool {
        self.composite.is_laddered()
    }

    /// The tiles of `coords` made ready together, as a chunk reads them.
    pub fn warm(&self, coords: &[(i32, i32)]) {
        self.composite.warm(coords);
    }

    /// The summaries of the level `w` wide at `cells` made ready together.
    pub fn warm_level(&self, w: i64, cells: &[(i64, i64)]) {
        self.composite.warm_level(w, cells);
    }

    /// Get elevation at a hex tile position (discretized to z-level).
    pub fn elevation_at(&self, q: i32, r: i32) -> i32 {
        self.composite.elevation_at(q, r)
    }

    /// The surface water stands at over a tile as a z-level, or None where
    /// the tile is dry.
    pub fn water_at(&self, q: i32, r: i32) -> Option<i32> {
        self.composite.water_at(q, r)
    }

    /// What stands in a tile's slots.
    pub fn cover_at(&self, q: i32, r: i32) -> Cover {
        self.composite.cover_at(q, r)
    }

    /// The habitat of the den whose site is this tile, where one is.
    pub fn den_at(&self, q: i32, r: i32) -> Option<common::den::Habitat> {
        self.composite.den_at(q, r)
    }

    /// Drain event metrics (reads gauges, resets interval counters).
    pub fn drain_metrics(&self) -> world::events::EventMetricsSnapshot {
        self.composite.drain_metrics()
    }
}

impl common::summary::SummarySource for EventRegistry {
    fn sample(&self, q: i32, r: i32) -> Option<common::summary::TileSample> {
        self.composite.sample(q, r)
    }

    fn cover(&self, q: i32, r: i32) -> Option<Cover> {
        self.composite.cover(q, r)
    }

    fn cell(&self, r: u32, sq: i32, sr: i32) -> Option<(i32, u16)> {
        self.composite.cell(r, sq, sr)
    }
}
