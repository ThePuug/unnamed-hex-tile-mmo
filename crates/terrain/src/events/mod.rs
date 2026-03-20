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

use std::collections::HashMap;
use std::sync::Mutex;

use common::{HexLattice, TagSet};

use crate::hex_to_world;

pub use index::{CellId, EventIndex, IndexRegistry};
pub use survey::Survey;

// ── Legacy hex chunk grid utilities (used by spawners) ──────────────────────

const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

pub fn chunk_coord(wx: f64, wy: f64, scale: f64) -> (i32, i32) {
    let row_height = scale * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { scale * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / scale).round() as i32;
    (cq, cr)
}

pub fn chunk_center(cq: i32, cr: i32, scale: f64) -> (f64, f64) {
    let odd_shift = if cr & 1 != 0 { scale * 0.5 } else { 0.0 };
    (cq as f64 * scale + odd_shift, cr as f64 * scale * HEX_ROW_HEIGHT)
}

pub fn chunk_1ring(cr: i32) -> [(i32, i32); 7] {
    if cr & 1 == 0 {
        [(0, 0), (-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    } else {
        [(0, 0), (-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    }
}

// ── Legacy Event Cache (used by spawners) ───────────────────────────────────

#[derive(Default, Clone)]
pub struct EventCacheMetrics {
    pub chunks_evaluated: u64,
    pub chunks_with_output: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

pub struct EventCache<T> {
    chunks: HashMap<(i32, i32), LegacyCacheEntry<T>>,
    access_counter: u64,
    max_chunks: usize,
    pub metrics: EventCacheMetrics,
}

struct LegacyCacheEntry<T> { data: T, last_accessed: u64 }

impl<T> EventCache<T> {
    pub fn new(max_chunks: usize) -> Self {
        Self { chunks: HashMap::new(), access_counter: 0, max_chunks, metrics: EventCacheMetrics::default() }
    }
    pub fn get_or_evaluate(&mut self, cq: i32, cr: i32, evaluate_fn: &mut impl FnMut(i32, i32) -> T) -> &T {
        self.access_counter += 1;
        let stamp = self.access_counter;
        if !self.chunks.contains_key(&(cq, cr)) {
            self.metrics.cache_misses += 1;
            let data = evaluate_fn(cq, cr);
            self.metrics.chunks_evaluated += 1;
            self.chunks.insert((cq, cr), LegacyCacheEntry { data, last_accessed: stamp });
            self.evict_if_over_budget();
        } else { self.metrics.cache_hits += 1; }
        let entry = self.chunks.get_mut(&(cq, cr)).unwrap();
        entry.last_accessed = stamp;
        &entry.data
    }
    pub fn ensure(&mut self, cq: i32, cr: i32, evaluate_fn: &mut impl FnMut(i32, i32) -> T, has_output: impl FnOnce(&T) -> bool) {
        if self.chunks.contains_key(&(cq, cr)) { self.metrics.cache_hits += 1; return; }
        self.metrics.cache_misses += 1;
        self.access_counter += 1;
        let data = evaluate_fn(cq, cr);
        self.metrics.chunks_evaluated += 1;
        if has_output(&data) { self.metrics.chunks_with_output += 1; }
        self.chunks.insert((cq, cr), LegacyCacheEntry { data, last_accessed: self.access_counter });
        self.evict_if_over_budget();
    }
    pub fn ensure_1ring(&mut self, cq: i32, cr: i32, evaluate_fn: &mut impl FnMut(i32, i32) -> T, has_output: impl Fn(&T) -> bool) {
        for (dq, dr) in chunk_1ring(cr) { self.ensure(cq + dq, cr + dr, evaluate_fn, &has_output); }
    }
    pub fn get(&self, cq: i32, cr: i32) -> Option<&T> { self.chunks.get(&(cq, cr)).map(|e| &e.data) }
    pub fn touch(&mut self, cq: i32, cr: i32) {
        self.access_counter += 1;
        if let Some(entry) = self.chunks.get_mut(&(cq, cr)) { entry.last_accessed = self.access_counter; }
    }
    fn evict_if_over_budget(&mut self) {
        if self.chunks.len() <= self.max_chunks { return; }
        let lru_key = self.chunks.iter().min_by_key(|(_, e)| e.last_accessed).map(|(&k, _)| k);
        if let Some(key) = lru_key { self.chunks.remove(&key); }
    }
}

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

/// Cached per-tile outputs for one cell. Lazily populated by query.
pub struct CellOutput {
    pub tiles: HashMap<(i32, i32), TileOutput>,
}

impl CellOutput {
    pub fn new() -> Self { Self { tiles: HashMap::new() } }
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

    /// Structural work. Build indexes from survey results.
    /// No tile materialization — indexes only.
    fn deform(
        &self,
        cell_id: CellId,
        matched: &[(i32, i32)],
        indexes: &mut IndexRegistry,
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

/// Per-event layer metrics (gauges + interval counters).
#[derive(Default)]
struct LayerMetrics {
    /// Cells currently in deformed state (gauge).
    cells_deformed: usize,
    /// Cells currently with index entries (gauge).
    cells_indexed: usize,
    /// Cell cache hits since last drain (interval).
    cell_hits: u64,
    /// Cell cache misses since last drain (interval).
    cell_misses: u64,
}

/// Composite-level metrics.
#[derive(Default)]
struct CompositeMetrics {
    /// Tiles currently cached across all layers (gauge).
    visible_tiles: usize,
    /// Tile cache hits since last drain (interval).
    tile_hits: u64,
    /// Tile cache misses since last drain (interval).
    tile_misses: u64,
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
    pub scanned: usize,
    pub indexed: usize,
    pub cell_hits: u64,
    pub cell_misses: u64,
}

// ── Cell Cache ──────────────────────────────────────────────────────────────

const DEFAULT_MAX_CELLS: usize = 2000;

struct CellCache {
    cells: HashMap<CellId, CellCacheEntry>,
    access_counter: u64,
    max_cells: usize,
    metrics: LayerMetrics,
}

struct CellCacheEntry {
    output: CellOutput,
    last_accessed: u64,
}

impl CellCache {
    fn new(max_cells: usize) -> Self {
        Self { cells: HashMap::new(), access_counter: 0, max_cells, metrics: LayerMetrics::default() }
    }
    fn has(&self, cell_id: CellId) -> bool { self.cells.contains_key(&cell_id) }

    fn get_tile(&self, cell_id: CellId, q: i32, r: i32) -> Option<&TileOutput> {
        self.cells.get(&cell_id)
            .and_then(|e| e.output.tiles.get(&(q, r)))
    }

    fn insert_empty(&mut self, cell_id: CellId) {
        self.access_counter += 1;
        self.cells.entry(cell_id).or_insert(CellCacheEntry {
            output: CellOutput::new(),
            last_accessed: self.access_counter,
        });
        self.metrics.cells_deformed = self.cells.len();
        self.evict_if_over_budget();
    }

    fn insert_tile(&mut self, cell_id: CellId, q: i32, r: i32, tile: TileOutput) {
        if let Some(entry) = self.cells.get_mut(&cell_id) {
            entry.output.tiles.insert((q, r), tile);
        }
    }

    fn touch(&mut self, cell_id: CellId) {
        self.access_counter += 1;
        if let Some(e) = self.cells.get_mut(&cell_id) { e.last_accessed = self.access_counter; }
    }

    fn tile_count(&self) -> usize {
        self.cells.values().map(|e| e.output.tiles.len()).sum()
    }

    fn evict_if_over_budget(&mut self) {
        if self.cells.len() <= self.max_cells { return; }
        let lru_key = self.cells.iter().min_by_key(|(_, e)| e.last_accessed).map(|(&k, _)| k);
        if let Some(key) = lru_key {
            self.cells.remove(&key);
            self.metrics.cells_deformed = self.cells.len();
        }
    }
}

// ── Composite ───────────────────────────────────────────────────────────────

struct CompositeState {
    cell_caches: Vec<CellCache>,
    indexes: IndexRegistry,
    metrics: CompositeMetrics,
}

pub struct Composite {
    events: Vec<Box<dyn WorldEvent>>,
    lattices: Vec<HexLattice>,
    state: Mutex<CompositeState>,
    seed: u64,
}

impl Composite {
    pub fn new(seed: u64) -> Self {
        Self {
            events: Vec::new(),
            lattices: Vec::new(),
            state: Mutex::new(CompositeState {
                cell_caches: Vec::new(),
                indexes: IndexRegistry::new(),
                metrics: CompositeMetrics::default(),
            }),
            seed,
        }
    }

    pub fn add_event(&mut self, event: Box<dyn WorldEvent>) {
        let lattice = HexLattice::new(event.scale());
        self.state.lock().unwrap().cell_caches.push(CellCache::new(DEFAULT_MAX_CELLS));
        self.lattices.push(lattice);
        self.events.push(event);
    }

    /// Get the final tile state at (q, r). Lazily triggers deform + query cascades.
    pub fn tile_at(&self, q: i32, r: i32) -> TileView {
        let mut state = self.state.lock().unwrap();
        let CompositeState { cell_caches, indexes, metrics } = &mut *state;

        // Phase 1: Deform cascade — ensure all cells containing this tile are deformed
        for layer in 0..self.events.len() {
            let cell_id = self.lattices[layer].cell_id(q, r);
            ensure_deformed(&self.events, &self.lattices, cell_caches, indexes, layer, cell_id, self.seed);
        }

        // Phase 2: Query cascade — resolve tile bottom-up
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

        for layer in 0..self.events.len() {
            let cell_id = self.lattices[layer].cell_id(q, r);
            cell_caches[layer].touch(cell_id);

            // Check tile cache
            let cached = cell_caches[layer].get_tile(cell_id, q, r).copied();
            let tile_out = if let Some(to) = cached {
                metrics.tile_hits += 1;
                to
            } else {
                metrics.tile_misses += 1;
                // Split caches: lower (for below closure) vs current (for caching)
                let (lower, upper) = cell_caches.split_at_mut(layer);
                let current = &mut upper[0];

                let events = &self.events;
                let lattices = &self.lattices;
                let seed = self.seed;
                let below_fn = |bq: i32, br: i32| -> TileView {
                    resolve_below(&*events, &*lattices, &*lower, &*indexes, bq, br, seed)
                };

                let result = self.events[layer].query(q, r, cell_id, &*indexes, &below_fn, self.seed);
                match result {
                    Some(to) => { current.insert_tile(cell_id, q, r, to); to }
                    None => TileOutput::default(),
                }
            };

            for t in tile_out.tags_added.iter() { view.tags.add(t); }
            for t in tile_out.tags_removed.iter() { view.tags.remove(t); }
            view.elevation += tile_out.elevation_delta;
        }

        view
    }

    pub fn elevation_at(&self, q: i32, r: i32) -> i32 {
        crate::discretize_elevation(self.tile_at(q, r).elevation)
    }

    pub fn tags_at(&self, q: i32, r: i32) -> TagSet {
        self.tile_at(q, r).tags
    }

    /// Access the IndexRegistry under the lock via closure.
    pub fn with_indexes<R>(&self, f: impl FnOnce(&IndexRegistry) -> R) -> R {
        let state = self.state.lock().unwrap();
        f(&state.indexes)
    }

    /// Read gauges and drain interval counters. Returns a snapshot for external reporting.
    pub fn drain_metrics(&self) -> EventMetricsSnapshot {
        let mut state = self.state.lock().unwrap();
        let CompositeState { cell_caches, metrics, .. } = &mut *state;

        // Compute visible tiles gauge from all layers
        let visible: usize = cell_caches.iter().map(|c| c.tile_count()).sum();

        // Drain composite interval counters
        let tile_hits = metrics.tile_hits;
        let tile_misses = metrics.tile_misses;
        metrics.tile_hits = 0;
        metrics.tile_misses = 0;

        // Per-layer snapshots
        let layers: Vec<LayerMetricsSnapshot> = cell_caches.iter_mut().enumerate().map(|(i, cache)| {
            let name = if i < self.events.len() {
                self.events[i].name().to_string()
            } else {
                format!("layer_{i}")
            };
            let snap = LayerMetricsSnapshot {
                name,
                scanned: cache.metrics.cells_deformed,
                indexed: cache.metrics.cells_indexed,
                cell_hits: cache.metrics.cell_hits,
                cell_misses: cache.metrics.cell_misses,
            };
            // Drain interval counters
            cache.metrics.cell_hits = 0;
            cache.metrics.cell_misses = 0;
            snap
        }).collect();

        EventMetricsSnapshot { visible, tile_hits, tile_misses, layers }
    }
}

// ── Deform cascade ──────────────────────────────────────────────────────────

fn ensure_deformed(
    events: &[Box<dyn WorldEvent>],
    lattices: &[HexLattice],
    cell_caches: &mut [CellCache],
    indexes: &mut IndexRegistry,
    layer: usize,
    cell_id: CellId,
    seed: u64,
) {
    if cell_caches[layer].has(cell_id) {
        cell_caches[layer].metrics.cell_hits += 1;
        return;
    }
    cell_caches[layer].metrics.cell_misses += 1;

    let lattice = &lattices[layer];
    let (cq, cr) = lattice.cell_center(cell_id);

    // Cascade: ensure lower layers' overlapping cells are deformed
    for sub_layer in 0..layer {
        let sub_lat = &lattices[sub_layer];
        let reach = (sub_lat.radius + lattice.radius) as i32;
        let lattice_reach = (reach as f64 / (2.0 * sub_lat.radius as f64 + 1.0)).ceil() as u32 + 1;
        let center_sub_cell = sub_lat.cell_id(cq, cr);
        for sub_cell in sub_lat.cells_within_distance(center_sub_cell, lattice_reach) {
            ensure_deformed(events, lattices, cell_caches, indexes, sub_layer, sub_cell, seed);
        }
    }

    // Evaluate survey. For Survey::all() with filter, provide a resolve_tile
    // callback that triggers the query cascade for layers below.
    let surv = events[layer].survey();
    let resolve_tile = |q: i32, r: i32| -> TileView {
        resolve_below(events, lattices, &cell_caches[..layer], indexes, q, r, seed)
    };
    let matched = survey::evaluate_survey(
        &surv, cell_id, lattice, indexes, Some(&resolve_tile), seed,
    );
    // Deform: populate indexes only
    events[layer].deform(cell_id, &matched, indexes, seed);

    // Mark cell as deformed (insert empty CellOutput for tile caching)
    let (_, upper) = cell_caches.split_at_mut(layer);
    upper[0].insert_empty(cell_id);
}

/// Resolve the composite TileView from layers 0..N (read-only cache access).
/// Used by the `below` closure passed to query. Recursively resolves uncached
/// tiles via query but does NOT cache intermediate results (avoids mutable
/// borrow conflicts). The main tile_at loop handles caching at each layer.
fn resolve_below(
    events: &[Box<dyn WorldEvent>],
    lattices: &[HexLattice],
    lower: &[CellCache],
    indexes: &IndexRegistry,
    q: i32, r: i32,
    seed: u64,
) -> TileView {
    let (wx, wy) = hex_to_world(q, r);
    let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0 };

    for (li, cache) in lower.iter().enumerate() {
        let cell_id = lattices[li].cell_id(q, r);
        let tile_out = if let Some(cached) = cache.get_tile(cell_id, q, r) {
            *cached
        } else {
            let sub_below = |bq: i32, br: i32| -> TileView {
                resolve_below(events, lattices, &lower[..li], indexes, bq, br, seed)
            };
            match events[li].query(q, r, cell_id, indexes, &sub_below, seed) {
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
