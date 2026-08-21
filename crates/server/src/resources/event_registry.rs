use bevy::prelude::*;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::motion::MotionEvent;
use world::events::slope_form::SlopeFormEvent;
use world::events::spines::SpineEvent;
use world::TagSet;

/// Server-side registry of world events.

/// Owns the Composite with PlateEvent + MotionEvent + SpineEvent +
/// SlopeFormEvent. All terrain queries route through here.
/// Arc-wrapped so async chunk generation tasks can share it.
#[derive(Resource, Clone)]
pub struct EventRegistry {
    composite: std::sync::Arc<Composite>,
}

impl EventRegistry {
    pub fn new(seed: u64) -> Self {
        let plate_cache = std::sync::Arc::new(world::PlateCache::new(seed));
        let mut composite = Composite::new(seed);
        composite.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
        composite.add_event(Box::new(MotionEvent::with_cache(plate_cache.clone(), seed)));
        composite.add_event(Box::new(SpineEvent::with_cache(plate_cache, seed)));
        composite.add_event(Box::new(SlopeFormEvent::new()));

        Self { composite: std::sync::Arc::new(composite) }
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

    /// Drain event metrics (reads gauges, resets interval counters).
    pub fn drain_metrics(&self) -> world::events::EventMetricsSnapshot {
        self.composite.drain_metrics()
    }
}
