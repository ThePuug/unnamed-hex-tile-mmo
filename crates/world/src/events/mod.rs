//! # World Event System
//!
//! Two independent cascades:
//! - **Deform cascade** (index → index): structural work. Survey evaluates against
//!   indexes, deform populates indexes. Cheap — no tile materialization.
//! - **Query cascade** (tile → tile): single (q, r) resolves vertically through the
//!   stack. Each layer's `query()` computes one tile on demand. Framework caches result.

pub mod index;
pub mod plates;
pub mod spawner;
pub mod spines;
pub mod survey;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use dashmap::DashMap;
use parking_lot::Mutex;

use common::{HexLattice, TagSet};

use crate::hex_to_world;

pub use index::{CellId, EventIndex, IndexRegistry};
pub use survey::Survey;

// ══════════════════════════════════════════════════════════════════════════════
// World Event Composite Framework
// ══════════════════════════════════════════════════════════════════════════════

/// Per-tile output from a single event's query.
#[derive(Default, Clone, Copy)]
pub struct TileOutput {
    pub tags_added: TagSet,
    pub tags_removed: TagSet,
    pub elevation_delta: f64,
}

/// Read-only composite view at a single tile.
#[derive(Clone)]
pub struct TileView {
    pub q: i32,
    pub r: i32,
    pub wx: f64,
    pub wy: f64,
    pub tags: TagSet,
    pub elevation: f64,
}

// ── WorldEvent trait ────────────────────────────────────────────────────────

/// A world event with separate structural (deform) and tile (query) passes.
///
/// **Deform**: structural work. Reads indexes from below, runs survey, populates
/// own indexes. Never materializes tiles. Cheap even for large cells.
///
/// **Query**: resolves a single tile on demand. Uses own indexes + composed tile
/// from all layers below. Framework caches the result.
pub trait WorldEvent: Send + Sync {
    fn name(&self) -> &str;
    fn scale(&self) -> u32;
    fn survey(&self) -> Survey;

    /// Pre-register index types this event writes during deform.
    /// Called once during `Composite::add_event()`. Events call
    /// `registry.pre_register::<MyIndex>()` for each index they create.
    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Structural work. Build indexes from survey results.
    /// No tile materialization — indexes only.
    fn deform(
        &self,
        cell_id: CellId,
        matched: &[(i32, i32)],
        indexes: &IndexRegistry,
        seed: u64,
    );

    /// Resolve a single tile on demand. `below` lazily resolves the composite
    /// tile from all layers below at any (q, r). Returns None if this event
    /// contributes nothing at this position.
    fn query(
        &self,
        q: i32, r: i32,
        cell_id: CellId,
        indexes: &IndexRegistry,
        below: &dyn Fn(i32, i32) -> TileView,
        seed: u64,
    ) -> Option<TileOutput>;
}

// ── Metrics ─────────────────────────────────────────────────────────────────

/// Atomic hit/miss counters. Lock-free, safe for concurrent access.
/// Counters are cumulative lifetime totals; console computes rates.
struct HitCounters {
    hits: AtomicU64,
    misses: AtomicU64,
}

impl HitCounters {
    fn new() -> Self {
        Self { hits: AtomicU64::new(0), misses: AtomicU64::new(0) }
    }
    fn record(&self, hit: bool) {
        if hit {
            self.hits.fetch_add(1, Relaxed);
        } else {
            self.misses.fetch_add(1, Relaxed);
        }
    }
    fn hits(&self) -> u64 { self.hits.load(Relaxed) }
    fn misses(&self) -> u64 { self.misses.load(Relaxed) }
}

/// Per-event layer metrics (lock-free).
struct LayerMetrics {
    cell_counters: HitCounters,
}

impl Default for LayerMetrics {
    fn default() -> Self {
        Self { cell_counters: HitCounters::new() }
    }
}

/// Composite-level metrics (lock-free).
struct CompositeMetrics {
    tile_counters: HitCounters,
}

impl Default for CompositeMetrics {
    fn default() -> Self {
        Self { tile_counters: HitCounters::new() }
    }
}

/// Snapshot of event metrics for external consumption.
pub struct EventMetricsSnapshot {
    pub visible: usize,
    pub tile_hits: u64,
    pub tile_misses: u64,
    pub layers: Vec<LayerMetricsSnapshot>,
}

pub struct LayerMetricsSnapshot {
    pub name: String,
    /// Cells currently in LRU cache with index entries (gauge — current coverage).
    pub indexed: usize,
    pub cell_hits: u64,
    pub cell_misses: u64,
}

// ── Cell Cache (concurrent) ─────────────────────────────────────────────────

const DEFAULT_MAX_CELLS: usize = 2000;

struct CellEntry {
    tiles: parking_lot::RwLock<HashMap<(i32, i32), TileOutput>>,
    last_accessed: AtomicU64,
}

struct CellCache {
    /// Cell entries. Presence = cell has been deformed (Warm).
    /// Arc-wrapped so readers clone the Arc and release the DashMap shard lock
    /// immediately. Eviction removes the DashMap entry; the Arc keeps the data
    /// alive until all readers finish.
    cells: DashMap<CellId, Arc<CellEntry>>,
    /// Per-cell deform serialization locks (double-checked locking).
    deform_locks: DashMap<CellId, Arc<Mutex<()>>>,
    /// Monotonic counter for LRU ordering (lock-free touch).
    access_counter: AtomicU64,
    max_cells: usize,
    metrics: LayerMetrics,
}

impl CellCache {
    fn new(max_cells: usize) -> Self {
        Self {
            cells: DashMap::new(),
            deform_locks: DashMap::new(),
            access_counter: AtomicU64::new(0),
            max_cells,
            metrics: LayerMetrics::default(),
        }
    }

    fn has(&self, cell_id: CellId) -> bool {
        self.cells.contains_key(&cell_id)
    }

    /// Get the per-cell deform lock (create if needed). Returns cloned Arc
    /// so the DashMap ref is released before locking.
    fn deform_lock(&self, cell_id: CellId) -> Arc<Mutex<()>> {
        self.deform_locks
            .entry(cell_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn get_tile(&self, cell_id: CellId, q: i32, r: i32) -> Option<TileOutput> {
        self.cells.get(&cell_id)
            .and_then(|entry| entry.tiles.read().get(&(q, r)).copied())
    }

    fn insert_empty(&self, cell_id: CellId) {
        let stamp = self.access_counter.fetch_add(1, Relaxed) + 1;
        self.cells.entry(cell_id).or_insert_with(|| Arc::new(CellEntry {
            tiles: parking_lot::RwLock::new(HashMap::new()),
            last_accessed: AtomicU64::new(stamp),
        }));
        self.evict_if_over_budget();
    }

    fn insert_tile(&self, cell_id: CellId, q: i32, r: i32, tile: TileOutput) {
        if let Some(entry) = self.cells.get(&cell_id) {
            entry.tiles.write().insert((q, r), tile);
        }
    }

    fn touch(&self, cell_id: CellId) {
        let stamp = self.access_counter.fetch_add(1, Relaxed) + 1;
        if let Some(entry) = self.cells.get(&cell_id) {
            entry.last_accessed.store(stamp, Relaxed);
        }
    }

    fn tile_count(&self) -> usize {
        self.cells.iter().map(|e| e.tiles.read().len()).sum()
    }

    fn evict_if_over_budget(&self) {
        // Eviction intentionally disabled — caches grow unbounded.
        // `max_cells` / `last_accessed` / `access_counter` are retained for when
        // LRU is reinstated. See docs/design/world-events.md (Implementation Gaps).
    }
}

// ── Composite ───────────────────────────────────────────────────────────────

/// No global Mutex. Cell caches use DashMap + per-cell deform locks.
/// IndexRegistry uses interior RwLock. All methods take `&self`.
pub struct Composite {
    events: Vec<Box<dyn WorldEvent>>,
    lattices: Vec<HexLattice>,
    cell_caches: Vec<CellCache>,
    indexes: IndexRegistry,
    metrics: CompositeMetrics,
    seed: u64,
}

impl Composite {
    pub fn new(seed: u64) -> Self {
        Self {
            events: Vec::new(),
            lattices: Vec::new(),
            cell_caches: Vec::new(),
            indexes: IndexRegistry::new(),
            metrics: CompositeMetrics::default(),
            seed,
        }
    }

    pub fn add_event(&mut self, event: Box<dyn WorldEvent>) {
        let lattice = HexLattice::new(event.scale());
        // Pre-register indexes declared by this event (HashMap frozen after init)
        event.register_indexes(&mut self.indexes);
        self.cell_caches.push(CellCache::new(DEFAULT_MAX_CELLS));
        self.lattices.push(lattice);
        self.events.push(event);
    }

    /// Get the final tile state at (q, r). Lazily triggers deform + query cascades.
    /// Thread-safe: no global lock. Per-cell deform locks serialize cold cells.
    pub fn tile_at(&self, q: i32, r: i32) -> TileView {
        // Phase 1: Deform cascade — ensure all cells containing this tile are deformed
        for layer in 0..self.events.len() {
            let cell_id = self.lattices[layer].cell_id(q, r);
            self.ensure_deformed(layer, cell_id);
        }

        // Phase 2: Query cascade — resolve tile bottom-up
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

        for layer in 0..self.events.len() {
            let cell_id = self.lattices[layer].cell_id(q, r);
            self.cell_caches[layer].touch(cell_id);

            let cached = self.cell_caches[layer].get_tile(cell_id, q, r);
            let tile_out = if let Some(to) = cached {
                self.metrics.tile_counters.record(true);
                to
            } else {
                self.metrics.tile_counters.record(false);
                let below_fn = |bq: i32, br: i32| -> TileView {
                    self.resolve_below(layer, bq, br)
                };
                let result = self.events[layer].query(q, r, cell_id, &self.indexes, &below_fn, self.seed);
                match result {
                    Some(to) => { self.cell_caches[layer].insert_tile(cell_id, q, r, to); to }
                    None => TileOutput::default(),
                }
            };

            for t in tile_out.tags_added.iter() { view.tags.add(t); }
            for t in tile_out.tags_removed.iter() { view.tags.remove(t); }
            view.elevation += tile_out.elevation_delta;
        }
        view
    }

    /// Batch-materialize tiles. No global lock — each tile_at is independent.
    pub fn tiles_at(&self, coords: &[(i32, i32)]) -> HashMap<(i32, i32), TileView> {
        coords.iter().map(|&(q, r)| ((q, r), self.tile_at(q, r))).collect()
    }

    pub fn elevation_at(&self, q: i32, r: i32) -> i32 {
        crate::discretize_elevation(self.tile_at(q, r).elevation)
    }

    pub fn tags_at(&self, q: i32, r: i32) -> TagSet {
        self.tile_at(q, r).tags
    }

    /// Access the IndexRegistry directly (no lock needed — interior mutability).
    pub fn with_indexes<R>(&self, f: impl FnOnce(&IndexRegistry) -> R) -> R {
        f(&self.indexes)
    }

    /// Read gauges and drain interval counters. Returns a snapshot for external reporting.
    pub fn drain_metrics(&self) -> EventMetricsSnapshot {
        let visible: usize = self.cell_caches.iter().map(|c| c.tile_count()).sum();

        let tile_hits = self.metrics.tile_counters.hits();
        let tile_misses = self.metrics.tile_counters.misses();

        let layers: Vec<LayerMetricsSnapshot> = self.cell_caches.iter().enumerate().map(|(i, cache)| {
            let name = if i < self.events.len() {
                self.events[i].name().to_string()
            } else {
                format!("layer_{i}")
            };
            LayerMetricsSnapshot {
                name,
                indexed: cache.cells.len(),
                cell_hits: cache.metrics.cell_counters.hits(),
                cell_misses: cache.metrics.cell_counters.misses(),
            }
        }).collect();

        EventMetricsSnapshot { visible, tile_hits, tile_misses, layers }
    }

    // ── Deform cascade (per-cell double-checked locking) ────────────────────

    fn ensure_deformed(&self, layer: usize, cell_id: CellId) {
        // Fast path: already deformed
        if self.cell_caches[layer].has(cell_id) {
            self.cell_caches[layer].metrics.cell_counters.record(true);
            return;
        }

        // Slow path: acquire per-cell deform lock
        let lock = self.cell_caches[layer].deform_lock(cell_id);
        let _guard = lock.lock();

        // Recheck after acquiring lock (another task may have deformed it)
        if self.cell_caches[layer].has(cell_id) {
            self.cell_caches[layer].metrics.cell_counters.record(true);
            return;
        }
        self.cell_caches[layer].metrics.cell_counters.record(false);

        let lattice = &self.lattices[layer];
        let (cq, cr) = lattice.cell_center(cell_id);

        // Cascade: ensure lower layers' overlapping cells are deformed.
        for sub_layer in 0..layer {
            let sub_lat = &self.lattices[sub_layer];

            let sub_cells: Vec<CellId> = if lattice.radius <= sub_lat.radius {
                let mut needed: HashSet<CellId> = HashSet::new();
                for (tq, tr) in lattice.tiles_in_cell(cell_id) {
                    needed.insert(sub_lat.cell_id(tq, tr));
                }
                needed.into_iter().collect()
            } else {
                let reach = (sub_lat.radius + lattice.radius) as f64;
                let min_step = (3.0 * sub_lat.radius as f64 + 2.0) / 2.0;
                let lattice_reach = (reach / min_step).ceil() as u32;
                let center_sub_cell = sub_lat.cell_id(cq, cr);
                sub_lat.cells_within_distance(center_sub_cell, lattice_reach)
            };

            for sub_cell in sub_cells {
                self.ensure_deformed(sub_layer, sub_cell);
            }
        }

        // Evaluate survey
        let surv = self.events[layer].survey();
        let resolve_tile = |q: i32, r: i32| -> TileView {
            self.resolve_below(layer, q, r)
        };
        let matched = survey::evaluate_survey(
            &surv, cell_id, lattice, &self.indexes, Some(&resolve_tile), self.seed,
        );

        // Deform: populate indexes only
        self.events[layer].deform(cell_id, &matched, &self.indexes, self.seed);

        // Mark cell as deformed
        self.cell_caches[layer].insert_empty(cell_id);
    }

    /// Resolve the composite TileView from layers 0..up_to (read-only cache access).
    /// Used by the `below` closure passed to query. Does NOT cache intermediate results.
    fn resolve_below(&self, up_to: usize, q: i32, r: i32) -> TileView {
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

        for li in 0..up_to {
            let cell_id = self.lattices[li].cell_id(q, r);
            let tile_out = if let Some(cached) = self.cell_caches[li].get_tile(cell_id, q, r) {
                cached
            } else {
                let sub_below = |bq: i32, br: i32| -> TileView {
                    self.resolve_below(li, bq, br)
                };
                match self.events[li].query(q, r, cell_id, &self.indexes, &sub_below, self.seed) {
                    Some(to) => to,
                    None => TileOutput::default(),
                }
            };

            for t in tile_out.tags_added.iter() { view.tags.add(t); }
            for t in tile_out.tags_removed.iter() { view.tags.remove(t); }
            view.elevation += tile_out.elevation_delta;
        }

        view
    }
}
