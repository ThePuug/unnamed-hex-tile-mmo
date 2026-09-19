use bevy::prelude::*;
use world::events::Composite;
use world::TagSet;

/// Server-side registry of world events.

/// Owns the Composite with PlateEvent + TiltEvent + MotionEvent + ThrustingEvent + ThickeningEvent +
/// DrainageEvent. All terrain queries route through here.
/// Arc-wrapped so async chunk generation tasks can share it.
#[derive(Resource, Clone)]
pub struct EventRegistry {
    composite: std::sync::Arc<Composite>,
}

impl EventRegistry {
    pub fn new(seed: u64) -> Self {
        Self { composite: std::sync::Arc::new(Composite::standard(seed)) }
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

    /// Get tags at a hex tile position.
    #[allow(dead_code)]
    pub fn tags_at(&self, q: i32, r: i32) -> TagSet {
        self.composite.tags_at(q, r)
    }

    /// Drain event metrics (reads gauges, resets interval counters).
    pub fn drain_metrics(&self) -> world::events::EventMetricsSnapshot {
        self.composite.drain_metrics()
    }
}
