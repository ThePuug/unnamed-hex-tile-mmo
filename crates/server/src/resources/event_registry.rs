use std::collections::HashMap;
use bevy::prelude::*;
use terrain::spawners::{SpawnerCache, SpawnerPlacement};
use terrain::events::EventCacheMetrics;

/// Identifies event types for metrics tracking.
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub enum EventTypeId {
    Spawner,
}

/// Per-event-type metrics for application-specific gating and materialization.
#[derive(Default)]
pub struct EventGateMetrics {
    pub candidates: u64,
    pub accepted: u64,
    pub rejections: HashMap<&'static str, u64>,
    pub materializations: u64,
}

impl EventGateMetrics {
    pub fn candidate(&mut self) { self.candidates += 1; }
    pub fn accept(&mut self) { self.accepted += 1; }
    pub fn reject(&mut self, gate: &'static str) {
        *self.rejections.entry(gate).or_insert(0) += 1;
    }
    pub fn materialized(&mut self) { self.materializations += 1; }

    /// Snapshot and reset all counters.
    pub fn drain(&mut self) -> EventGateMetrics {
        EventGateMetrics {
            candidates: std::mem::take(&mut self.candidates),
            accepted: std::mem::take(&mut self.accepted),
            rejections: std::mem::take(&mut self.rejections),
            materializations: std::mem::take(&mut self.materializations),
        }
    }
}

/// Server-side registry of world event caches.
/// Owns gameplay event layers (spawners, future fauna/water).
/// Queries the Terrain substrate for plate/spine tag access.
#[derive(Resource)]
pub struct EventRegistry {
    spawner_cache: SpawnerCache,
    gate_metrics: HashMap<EventTypeId, EventGateMetrics>,
}

impl EventRegistry {
    pub fn new(seed: u64) -> Self {
        let mut gate_metrics = HashMap::new();
        gate_metrics.insert(EventTypeId::Spawner, EventGateMetrics::default());
        Self {
            spawner_cache: SpawnerCache::new(seed),
            gate_metrics,
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

    /// Access gate metrics for a specific event type.
    pub fn gate_metrics(&mut self, id: EventTypeId) -> &mut EventGateMetrics {
        self.gate_metrics.entry(id).or_default()
    }

    /// Drain all metrics for the drain system.
    pub fn drain_metrics(&mut self) -> DrainedMetrics {
        DrainedMetrics {
            spawner_cache: self.spawner_cache.cache_metrics().drain(),
            spawner_gates: self.gate_metrics.entry(EventTypeId::Spawner).or_default().drain(),
        }
    }
}

/// Snapshot of all event metrics, produced by drain_metrics().
pub struct DrainedMetrics {
    pub spawner_cache: EventCacheMetrics,
    pub spawner_gates: EventGateMetrics,
}

impl DrainedMetrics {
    /// Format a single summary log line for the spawner funnel.
    pub fn spawner_summary(&self) -> String {
        let c = &self.spawner_cache;
        let g = &self.spawner_gates;
        let rejections: Vec<String> = g.rejections.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        let rej_str = if rejections.is_empty() { "none".to_string() } else { rejections.join(", ") };
        format!(
            "eval: {} chunks ({} with output) | cache: {} hits, {} misses, sz={} | queries: {}, found: {} | gates: {} candidates, {} accepted (rejected: {}) | materialized: {}",
            c.chunks_evaluated, c.chunks_with_output,
            c.cache_hits, c.cache_misses, c.cache_size,
            c.queries, c.results_returned,
            g.candidates, g.accepted, rej_str,
            g.materializations,
        )
    }
}
