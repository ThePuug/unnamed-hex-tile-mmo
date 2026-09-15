use bevy::prelude::*;
use world::events::Composite;
use world::events::plates::PlateEvent;
use world::events::tilt::TiltEvent;
use world::events::motion::MotionEvent;
use world::events::thickening::ThickeningEvent;
use world::events::thrusting::ThrustingEvent;
use world::events::dissection::DissectionEvent;
use world::events::drainage::DrainageEvent;
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
        let mut composite = Composite::new(seed);
        composite.add_event(Box::new(PlateEvent::new()));
        composite.add_event(Box::new(TiltEvent::new()));
        composite.add_event(Box::new(MotionEvent::new()));
        composite.add_event(Box::new(ThrustingEvent::new()));
        composite.add_event(Box::new(ThickeningEvent::new()));
        composite.add_event(Box::new(DrainageEvent::new()));
        composite.add_event(Box::new(DissectionEvent::new()));

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
