//! # World Event System

//! Two independent cascades:
//! - **Deform cascade** (index → index): structural work. Survey evaluates against
//!   indexes, deform populates indexes. Cheap — no tile materialization.
//! - **Query cascade** (tile → tile): single (q, r) resolves vertically through the
//!   stack. Each layer's `query()` computes one tile on demand. Framework caches result.

pub mod faces;
pub mod index;
pub mod plates;
pub mod sea;
pub mod slope_form;
pub mod spawner;
pub mod spines;
pub mod survey;

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use dashmap::DashMap;
use parking_lot::{MappedRwLockReadGuard, Mutex};

use common::{HexLattice, TagSet};

use crate::hex_to_world;

pub use index::{CellId, CellIndex, EventIndex, IndexRegistry};
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

// ── CellScope ───────────────────────────────────────────────────────────────

/// The cell a layer is being evaluated for, and the only cell it may write.
///
/// Reads are unrestricted — seeing a neighbour is how a layer composites the
/// features reaching into its own ground. Writes are not: [`CellScope::publish`]
/// takes no cell id, so a layer physically cannot record an entry against
/// another cell. It has no way to name one.
///
/// That matters because an index keyed by which feature *produced* an entry,
/// rather than which ground it lies on, cannot be settled by anyone: the cell
/// that produced it may not be able to see everything that acts on it, and the
/// cell that owns the ground never knew it existed. Keeping writes inside the
/// footprint keeps those two the same cell.
pub struct CellScope<'a> {
    cell: CellId,
    lattice: &'a HexLattice,
    indexes: &'a IndexRegistry,
    seed: u64,
}

impl<'a> CellScope<'a> {
    pub fn cell(&self) -> CellId { self.cell }
    pub fn seed(&self) -> u64 { self.seed }

    /// This layer's cell lattice, for reaching a neighbourhood of cells.
    pub fn lattice(&self) -> &HexLattice { self.lattice }

    /// Read any index. A layer settles its own ground against everything that
    /// reaches it, which means reading past its own cell.
    pub fn read<T: EventIndex>(&self) -> Option<MappedRwLockReadGuard<'_, T>> {
        self.indexes.get::<T>()
    }

    /// Record this cell's entry. There is deliberately no way to say which
    /// cell: it is always the one being evaluated.
    pub fn publish<T: CellIndex>(&self, entry: T::Cell) {
        self.indexes.get_or_create::<T>().set(self.cell, entry);
    }
}

// ── WorldEvent trait ────────────────────────────────────────────────────────

/// A world event with separate structural (deform) and tile (query) passes.

/// **Deform**: structural work. Reads indexes from below, runs survey, populates
/// own indexes. Never materializes tiles. Cheap even for large cells.

/// **Query**: resolves a single tile on demand. Uses own indexes + composed tile
/// from all layers below. Framework caches the result.
pub trait WorldEvent: Send + Sync {
    fn name(&self) -> &str;
    fn scale(&self) -> u32;
    fn survey(&self) -> Survey;

    /// Whether this event's query can contribute a non-empty TileOutput.

    /// Index-only events (deform populates indexes; query never modifies the
    /// tile) return false. The framework then skips their deform + query
    /// during tile materialization entirely — their cells are deformed on
    /// demand via `Composite::ensure_indexed` when their indexes are read.
    fn contributes_tiles(&self) -> bool { true }

    /// How many rings of neighbouring cells this event's `query` reads from
    /// its own indexes, beyond the cell containing the tile.

    /// The framework deforms this whole neighbourhood before calling `query`.
    /// An event whose features can reach past their own cell boundary MUST
    /// declare it: `deform` only populates the index for cells it is called
    /// on, so a query reading an undeformed neighbour silently sees nothing
    /// there and caches the wrong answer permanently. Default 0 — query reads
    /// only its own cell.
    fn query_reach(&self) -> u32 { 0 }

    /// Pre-register index types this event writes during deform.
    /// Called once during `Composite::add_event()`. Events call
    /// `registry.pre_register::<MyIndex>()` for each index they create.
    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Structural work. Build indexes from survey results.
    /// No tile materialization — indexes only.

    /// Runs before this cell's neighbours are guaranteed to exist, so anything
    /// needing to see them belongs in [`WorldEvent::prepare`], not here.
    fn deform(&self, scope: &CellScope, matched: &[(i32, i32)]);

    /// Everything a query may read beyond its own tile, resolved once for the
    /// cell that contains it.

    /// Run on the first query into a cell, after the framework has deformed the
    /// whole neighbourhood this layer is scoped to — so an index read here sees
    /// the complete cell-plus-ring set whatever order cells were visited in,
    /// which is the only place a fold over that neighbourhood is both complete
    /// and order-independent.

    /// Nothing resolved here may be resolved per tile instead. Every tile in
    /// the cell shares it, so a ring walk, an index read lock or an allocation
    /// left in `query` is multiplied by the tiles in a cell.
    fn prepare(&self, _scope: &CellScope) -> Box<dyn Any + Send + Sync> {
        Box::new(())
    }

    /// Resolve a single tile. Returns None if this event contributes nothing
    /// at this position.

    /// `below` is the composed tile **at this position and nowhere else**. A
    /// layer that needs to know what surrounds a tile reads it from an index
    /// the layer that put it there published: resolving a neighbour through
    /// the composite costs the whole stack beneath, per tile, and a consumer
    /// that samples sparsely pays it in full.

    /// `cell` is what [`WorldEvent::prepare`] built, downcast to the event's
    /// own type.
    fn query(
        &self,
        q: i32, r: i32,
        below: &TileView,
        cell: &(dyn Any + Send + Sync),
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

/// Ceiling on the exact sub-cell footprint walk in [`Composite::ensure_deformed`].

/// Exact enumeration visits every tile in a cell to collect the sub-cells it
/// overlaps, so its cost is O(cell area) — a radius-1800 cell is 9.7M tiles.
/// Above this size the geometric bound is used instead: it can pull in one
/// extra ring of sub-cells, but its cost does not scale with cell area.
/// Radius 9 (the spawner cell, 271 tiles) stays well inside the exact path,
/// where it deforms the minimum set of plate cells.
const MAX_EXACT_FOOTPRINT_TILES: usize = 4096;

/// Tiles in a hex ball of the given radius.
fn hex_ball_tiles(radius: u32) -> usize {
    let r = radius as usize;
    3 * r * r + 3 * r + 1
}

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
    /// What `prepare` built for a cell, kept for every tile in it.
    contexts: DashMap<CellId, Arc<dyn Any + Send + Sync>>,
    /// Cells whose full query neighbourhood has been deformed. Only used by
    /// layers with `query_reach() > 0`; lets the hot path settle for a single
    /// lookup instead of re-walking the ring on every tile in the cell.
    neighbourhood_ready: DashMap<CellId, ()>,
    /// Monotonic counter for LRU ordering (lock-free touch).
    access_counter: AtomicU64,
    max_cells: usize,
    metrics: LayerMetrics,
}

impl CellCache {
    fn new(max_cells: usize) -> Self {
        Self {
            cells: DashMap::new(),
            contexts: DashMap::new(),
            deform_locks: DashMap::new(),
            neighbourhood_ready: DashMap::new(),
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
        // LRU is reinstated. See world-events.md (unnamed-indie-studio-internal/projects/unnamed-hex-tile-mmo/design/, Implementation Gaps).
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
        let _span = tracing::info_span!("tile_at").entered();

        // Phase 1: Deform cascade — ensure all cells containing this tile are deformed.
        // Index-only events (contributes_tiles() == false) are skipped: their
        // deform cannot affect this tile's output.
        {
            let _s = tracing::debug_span!("deform").entered();
            for layer in 0..self.events.len() {
                if !self.events[layer].contributes_tiles() { continue; }
                let cell_id = self.lattices[layer].cell_id(q, r);
                self.ensure_query_neighbourhood(layer, cell_id);
            }
        }

        // Phase 2: Query cascade — resolve tile bottom-up
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

        {
            let _s = tracing::debug_span!("query").entered();
            for layer in 0..self.events.len() {
                if !self.events[layer].contributes_tiles() { continue; }
                let cell_id = self.lattices[layer].cell_id(q, r);
                self.cell_caches[layer].touch(cell_id);

                let cached = self.cell_caches[layer].get_tile(cell_id, q, r);
                let tile_out = if let Some(to) = cached {
                    self.metrics.tile_counters.record(true);
                    to
                } else {
                    self.metrics.tile_counters.record(false);
                    // `view` already holds the composite of every layer below,
                    // which is the whole of what a query may read besides its
                    // own cell context.
                    let ctx = self.cell_context(layer, cell_id);
                    let _s = tracing::debug_span!("event_query", event = self.events[layer].name()).entered();
                    let result = self.events[layer].query(q, r, &view, &*ctx, self.seed);
                    // None is cached as an empty output: query is deterministic,
                    // so "contributes nothing here" is as cacheable as a result.
                    // Without this, sea/flat tiles re-run every layer's query on
                    // every repeat access.
                    let to = result.unwrap_or_default();
                    self.cell_caches[layer].insert_tile(cell_id, q, r, to);
                    to
                };

                for t in tile_out.tags_added.iter() { view.tags.add(t); }
                for t in tile_out.tags_removed.iter() { view.tags.remove(t); }
                view.elevation += tile_out.elevation_delta;
            }
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

    /// Ensure index-only events (`contributes_tiles() == false`) have deformed
    /// every cell containing one of `coords`. Tile materialization skips those
    /// layers, so callers that read their indexes must declare the tiles they
    /// are interested in first. Idempotent and cheap for already-warm cells.
    pub fn ensure_indexed(&self, coords: &[(i32, i32)]) {
        for layer in 0..self.events.len() {
            if self.events[layer].contributes_tiles() { continue; }
            let lattice = &self.lattices[layer];
            let needed: HashSet<CellId> = coords.iter()
                .map(|&(q, r)| lattice.cell_id(q, r))
                .collect();
            for cell in needed {
                self.ensure_deformed(layer, cell);
            }
        }
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

    /// Deform every cell `layer`'s `query` is allowed to read for a tile in
    /// `cell_id` — its own cell plus `query_reach()` rings around it.

    /// Without this, a query reading a neighbouring cell's index sees whatever
    /// happens to be warm. The result is cached, so a tile materialized while
    /// a neighbour was cold stays wrong for the life of the composite, and two
    /// processes that touched tiles in a different order disagree about the
    /// terrain for the same seed.
    fn ensure_query_neighbourhood(&self, layer: usize, cell_id: CellId) {
        let reach = self.events[layer].query_reach();
        if reach == 0 {
            self.ensure_deformed(layer, cell_id);
            return;
        }
        // Every tile in a cell shares one neighbourhood, and cells are never
        // evicted, so walking the ring once per cell is enough. Without this
        // gate the ring walk lands on the per-tile hot path and costs a Vec
        // allocation plus `3·reach·(reach+1)+1` lookups on every query.
        if self.cell_caches[layer].neighbourhood_ready.contains_key(&cell_id) {
            return;
        }
        for cell in self.lattices[layer].cells_within_distance(cell_id, reach) {
            self.ensure_deformed(layer, cell);
        }
        self.cell_caches[layer].neighbourhood_ready.insert(cell_id, ());
    }

    /// What this layer resolved once for the cell, building it if this is the
    /// first tile to ask.
    ///
    /// Only ever called after [`Composite::ensure_query_neighbourhood`], which
    /// is what lets `prepare` fold over the whole cell-plus-ring set: at deform
    /// time the ring may be cold, so a fold there would depend on the order
    /// cells were visited in.
    fn cell_context(&self, layer: usize, cell_id: CellId) -> Arc<dyn Any + Send + Sync> {
        if let Some(c) = self.cell_caches[layer].contexts.get(&cell_id) {
            return c.clone();
        }
        let scope = CellScope {
            cell: cell_id,
            lattice: &self.lattices[layer],
            indexes: &self.indexes,
            seed: self.seed,
        };
        let built: Arc<dyn Any + Send + Sync> = Arc::from(self.events[layer].prepare(&scope));
        self.cell_caches[layer].contexts.insert(cell_id, built.clone());
        built
    }

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

            // Exact enumeration deforms the minimum set of sub-cells, but costs
            // O(cell area). Only take it when this cell is both smaller than the
            // sub-cell and small in absolute terms — at equal radii it degenerates
            // to walking millions of tiles per deform.
            let exact_affordable = lattice.radius <= sub_lat.radius
                && hex_ball_tiles(lattice.radius) <= MAX_EXACT_FOOTPRINT_TILES;

            let sub_cells: Vec<CellId> = if exact_affordable {
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

        let _s = tracing::debug_span!("event_deform", event = self.events[layer].name()).entered();
        let matched = {
            let _ss = tracing::debug_span!("survey").entered();
            survey::evaluate_survey(
                &surv, cell_id, lattice, &self.indexes, Some(&resolve_tile), self.seed,
            )
        };

        // Deform: populate indexes only
        let scope = CellScope {
            cell: cell_id,
            lattice: &self.lattices[layer],
            indexes: &self.indexes,
            seed: self.seed,
        };
        self.events[layer].deform(&scope, &matched);

        // Mark cell as deformed
        self.cell_caches[layer].insert_empty(cell_id);
    }

    /// Resolve the composite TileView from layers 0..up_to.
    /// Used by the `below` closure passed to query, and by survey evaluation.
    /// Writes computed results through to the cell caches so survey/below work
    /// is never recomputed (insert is a no-op for cells not yet deformed).
    fn resolve_below(&self, up_to: usize, q: i32, r: i32) -> TileView {
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

        for li in 0..up_to {
            if !self.events[li].contributes_tiles() { continue; }
            let cell_id = self.lattices[li].cell_id(q, r);
            let tile_out = if let Some(cached) = self.cell_caches[li].get_tile(cell_id, q, r) {
                cached
            } else {
                // Same guarantee tile_at makes: a query must not run against a
                // half-deformed neighbourhood, or it caches a wrong tile. This
                // path is reached from survey evaluation and `below` closures,
                // which tile_at's phase 1 does not cover.
                self.ensure_query_neighbourhood(li, cell_id);
                let ctx = self.cell_context(li, cell_id);
                let _s = tracing::debug_span!("event_query", event = self.events[li].name()).entered();
                let to = self.events[li]
                    .query(q, r, &view, &*ctx, self.seed)
                    .unwrap_or_default();
                self.cell_caches[li].insert_tile(cell_id, q, r, to);
                to
            };

            for t in tile_out.tags_added.iter() { view.tags.add(t); }
            for t in tile_out.tags_removed.iter() { view.tags.remove(t); }
            view.elevation += tile_out.elevation_delta;
        }

        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records which cells the framework deforms, so the `query_reach`
    /// contract can be checked without paying for real terrain generation.
    struct ReachProbe {
        reach: u32,
        deformed: Arc<parking_lot::Mutex<Vec<CellId>>>,
    }

    impl WorldEvent for ReachProbe {
        fn name(&self) -> &str { "reach-probe" }
        fn scale(&self) -> u32 { 32 }
        fn survey(&self) -> Survey { Survey::none() }
        fn query_reach(&self) -> u32 { self.reach }

        fn deform(&self, scope: &CellScope, _m: &[(i32, i32)]) {
            let cell_id = scope.cell();
            self.deformed.lock().push(cell_id);
        }

        fn query(
            &self, _q: i32, _r: i32, _b: &TileView,
            _c: &(dyn Any + Send + Sync), _s: u64,
        ) -> Option<TileOutput> { None }
    }

    /// An event that reads N rings of neighbours must have all of them
    /// deformed before its query runs. A query that reads an undeformed
    /// neighbour sees an empty index and caches the wrong tile forever —
    /// that is how whole mountains went missing depending on access order.
    #[test]
    fn query_reach_deforms_the_whole_neighbourhood() {
        for reach in 0..=2u32 {
            let deformed = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let mut c = Composite::new(1);
            c.add_event(Box::new(ReachProbe { reach, deformed: deformed.clone() }));
            c.tile_at(0, 0);

            let cells = deformed.lock();
            assert_eq!(
                cells.len(),
                hex_ball_tiles(reach),
                "reach {reach} should deform a hex ball of {} cells, got {}",
                hex_ball_tiles(reach),
                cells.len(),
            );
        }
    }

    /// The ring walk must not land on the per-tile hot path: every tile in a
    /// cell shares one neighbourhood, so it is deformed once, not per tile.
    #[test]
    fn query_neighbourhood_is_walked_once_per_cell() {
        let deformed = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mut c = Composite::new(1);
        c.add_event(Box::new(ReachProbe { reach: 1, deformed: deformed.clone() }));

        // Many tiles inside one radius-32 cell.
        for q in -5..=5 {
            for r in -5..=5 {
                c.tile_at(q, r);
            }
        }

        assert_eq!(
            deformed.lock().len(),
            hex_ball_tiles(1),
            "neighbourhood re-walked per tile instead of once per cell"
        );
    }

    #[test]
    fn hex_ball_tiles_matches_known_sizes() {
        assert_eq!(hex_ball_tiles(0), 1);
        assert_eq!(hex_ball_tiles(1), 7);
        // Chunk radius 9 — must stay under the exact-footprint ceiling so the
        // spawner layer keeps deforming the minimum set of plate cells.
        assert_eq!(hex_ball_tiles(9), 271);
        assert!(hex_ball_tiles(9) <= MAX_EXACT_FOOTPRINT_TILES);
    }

    /// Two adjacent layers at the same cell scale must not send the deform
    /// cascade down the exact-enumeration path — at radius 1800 that walks
    /// 9.7M tiles per cell and turns a ~100ms first touch into minutes.
    #[test]
    fn equal_scale_layers_do_not_enumerate_whole_cells() {
        use crate::plates::PlateCache;
        use plates::PlateEvent;
        use sea::SeaEvent;

        assert!(
            hex_ball_tiles(1800) > MAX_EXACT_FOOTPRINT_TILES,
            "a radius-1800 cell must exceed the exact-footprint ceiling"
        );

        let seed = 0x9E3779B97F4A7C15;
        let plate_cache = Arc::new(PlateCache::new(seed));
        let mut c = Composite::new(seed);
        c.add_event(Box::new(PlateEvent::with_cache(plate_cache)));
        c.add_event(Box::new(SeaEvent::new()));

        // Both layers are scale 1800. A cold first touch is dominated by the
        // plate deform (~100ms in release); the bug this guards made it minutes.
        let t = std::time::Instant::now();
        c.tile_at(3000, 2000);
        let dt = t.elapsed();
        assert!(
            dt < std::time::Duration::from_secs(20),
            "same-scale deform cascade took {dt:?} — footprint walk regressed"
        );
    }
}
