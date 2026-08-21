//! # World Event System
//!
//! Two independent cascades:
//! - **Deform cascade** (index → index): structural work. Deform reads the
//!   indexes below and populates its own. Cheap — no tile materialization.
//! - **Query cascade** (tile → tile): single (q, r) resolves vertically through the
//!   stack. Each layer's `query()` computes one tile on demand. Framework caches result.
//!
//! # Writing an event
//!
//! `query` runs per tile, on every tile the server streams and every tile a
//! summary samples. Everything below is a way of putting work there that does
//! not belong there, and every one of them has shipped at least once.
//!
//! The first two are enforced by the signature rather than by discipline —
//! `query` takes `below: &TileView` and no `IndexRegistry`, so neither is
//! expressible. They are stated anyway, because the reasons are what generalise.
//!
//! **Read the composite only at your own tile.** Resolving a neighbour costs
//! the whole stack beneath you, per tile. A layer needing a neighbourhood costs
//! N tiles per answer, which a dense reader amortises and a sparse one — an LoD
//! summary reads 7 tiles per hexagon — pays in full. If you need to know what
//! is around a tile, the layer that put it there publishes an index; read that.
//!
//! **Resolve per-cell work once per cell**, in `prepare`. Every tile in a cell
//! shares its cell, its ring, and the set of features reaching it. Walking the
//! ring, taking an index read lock, or building a `Vec` of candidates per tile
//! multiplies all of it by the tiles in a cell.
//!
//! **Deform writes your cell; prepare reads your ring.** `deform(L, C)` builds
//! L's indexes for C from layers below L, cascading them over C and its ring,
//! and writes entries lying in C alone — `CellScope::publish` takes no cell id,
//! so that half is enforced. It cannot see its own layer's ring: the cells
//! there may not be deformed yet, and folding whatever happens to be warm makes
//! the answer depend on visit order.
//!
//! `prepare(L, C)` reads L's own indexes over C plus exactly one ring, which
//! the framework guarantees is deformed by then. One ring is the entire budget
//! — `max_influence` asserts at setup that the cell scale is large enough for
//! one ring to suffice, and cannot buy a second. A layer needing wider reach
//! needs a larger cell scale. It cascades nothing and writes nothing. Anything
//! that must see its own layer's neighbours settled belongs here — that is the
//! whole reason the two phases are separate.
//!
//! **Publish what you know; never make a consumer infer it.** A headwall, a
//! channel wall, a closed basin — the layer that cut it knows where it is, and
//! a consumer that has to recover it by reading elevations around a tile is
//! doing far more work to get a worse answer. Publish an `EventIndex` and let
//! the framework own its lifecycle. Do not hang it off your own instance type
//! and make consumers reach through you for it.
//!
//! **Publish what you leave, and let a reader settle it.** A carve's intended
//! floor and the ground it actually leaves part company wherever a clamp or an
//! overlapping feature got there first, and a face that overstates its depth
//! tells the layer above to cut down to reach ground that was never taken. Your
//! `deform` can only know your own chain, so publish that and let whoever reads
//! the index composite it against the ring — the phase that is allowed to see
//! one.
//!
//! **Index published geometry spatially; never scan it.** A linear walk over
//! one instance's bowls or steps is fine at build time and is a per-tile cost
//! when a consumer does it. `HexSpatialGrid` with `insert_radius` over the
//! reach a consumer may ask about turns it into one lookup.
//!
//! **Measure before and after, against the same stack without your layer.**
//! `crates/world/tests/slope_probe.rs` shows the shape: contiguous chunk,
//! sparse sample, summary region, each read either side of the layer. A ratio
//! that only looks reasonable on the dense pattern is not a result.

pub mod faces;
pub mod index;
pub mod motion;
pub mod plates;
pub mod slope_form;
pub mod spines;

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use dashmap::DashMap;
use parking_lot::{MappedRwLockReadGuard, Mutex};

use common::{HexLattice, TagSet};

use crate::hex_to_world;

pub use index::{CellId, CellIndex, EventIndex, IndexRegistry};

// ══════════════════════════════════════════════════════════════════════════════
// World Event Composite Framework
// ══════════════════════════════════════════════════════════════════════════════

/// Per-tile output from a single event's query.
#[derive(Default, Clone, Copy)]
pub struct TileOutput {
    pub tags_added: TagSet,
    pub tags_removed: TagSet,
    pub elevation_delta: f64,
    /// Laplacian of the surface this layer adds, in z per world unit squared.
    ///
    /// Curvature cannot be recovered from a composed elevation without reading
    /// neighbouring tiles, which is the one thing a query may not do. The layer
    /// that emits a feature knows its second derivative in closed form, so it
    /// states it here and a consumer reads it at its own tile — the same
    /// contract elevation follows, for the same reason.
    pub curvature: f64,
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
    /// Laplacian of the composed surface, summed over the layers below. Sums
    /// because elevation does: the derivative of a sum is the sum of the
    /// derivatives.
    pub curvature: f64,
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

    /// The cells of `T`'s own lattice this cell may read — its footprint plus
    /// one ring, which is exactly the set the cascade deformed.
    ///
    /// Every read of a lower layer's index goes through here. Working the set
    /// out per event is how the framework ended up with two copies of the same
    /// arithmetic that disagreed about which cells were in scope, and an index
    /// read outside the deformed set returns empty rather than failing.
    pub fn source_cells<T: EventIndex>(&self) -> Vec<CellId> {
        let scale = self.indexes
            .source_scale_of(std::any::TypeId::of::<T>())
            .unwrap_or(self.lattice.radius);
        footprint_plus_ring(self.lattice, self.cell, &HexLattice::new(scale))
    }

    /// Record this cell's entry. There is deliberately no way to say which
    /// cell: it is always the one being evaluated.
    pub fn publish<T: CellIndex>(&self, entry: T::Cell) {
        self.indexes.get_or_create::<T>().set(self.cell, entry);
    }
}

// ── WorldEvent trait ────────────────────────────────────────────────────────

/// A world event with separate structural (deform) and tile (query) passes.

/// **Deform**: structural work. Reads the indexes below, places this layer's
/// features, populates its own indexes. Never materializes tiles.

/// **Query**: resolves a single tile on demand. Uses own indexes + composed tile
/// from all layers below. Framework caches the result.
pub trait WorldEvent: Send + Sync {
    fn name(&self) -> &str;
    fn scale(&self) -> u32;

    /// The furthest a feature originating at a point in this event's cell can
    /// affect terrain, in world units.

    /// **This does not decide what the framework deforms, and nothing reads it
    /// but one assertion.** A cell always reads itself plus one ring; there is
    /// no way to ask for more. What this declares is whether the cell scale is
    /// large enough for that ring to be sufficient — the containment proof for
    /// the event, checked at `add_event` and never consulted again.

    /// If it does not fit, the answer is a larger cell scale. Reaching further
    /// is not on the menu: a layer that folds over origins it cannot see caches
    /// terrain that is wrong for the life of the composite, and an index read
    /// against an undeformed cell returns empty rather than failing, so nothing
    /// reports it.
    fn max_influence(&self) -> u32 { 0 }

    /// Pre-register index types this event writes during deform.
    /// Called once during `Composite::add_event()`. Events call
    /// `registry.pre_register::<MyIndex>()` for each index they create.
    fn register_indexes(&self, _registry: &mut IndexRegistry) {}

    /// Structural work: read the indexes below through [`CellScope`], place
    /// this layer's features, publish them. No tile materialization — indexes
    /// only, and there is no route from here to the tile cascade.

    /// Runs before this cell's neighbours are guaranteed to exist, so anything
    /// needing to see them belongs in [`WorldEvent::prepare`], not here.
    fn deform(&self, scope: &CellScope);

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

/// Tiles in a hex ball of the given radius.
fn hex_ball_tiles(radius: u32) -> usize {
    let r = radius as usize;
    3 * r * r + 3 * r + 1
}

/// Hex distance between two tile coordinates.
fn hex_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dq, dr) = (a.0 - b.0, a.1 - b.1);
    (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
}

/// The `other`-lattice cells a cell of `lattice` may read: everything
/// overlapping its footprint, plus exactly one ring.
///
/// This is the one-ring rule in code, and it is the only implementation. The
/// deform cascade and `CellScope::source_cells` both call it — they used to carry
/// separate copies of the same arithmetic, and when one was tightened the other
/// went on asking for cells nobody deformed, which is a silent-empty read and
/// moved 913 tiles of terrain before it was caught.
///
/// Two hex balls share a tile exactly when their centres are within the sum of
/// their radii in hex distance. That test is exact and costs a subtraction,
/// where walking the footprint costs one visit per tile — 9.7M of them in a
/// radius-1800 cell.
///
/// The search ball only has to be wide enough to hold every candidate: a cell
/// centre at lattice distance k sits at least `k·step − r_other` away in hex
/// distance, `step` being the nearest-neighbour centre spacing, so nothing past
/// that k can touch. The exact test discards the rest. Bounding the *search*
/// that way is not the same as charging both circumradii to the *answer* —
/// doing that is what turned equal-scale hops into two rings where the geometry
/// needs none.
pub(super) fn footprint_plus_ring(
    lattice: &HexLattice,
    cell_id: CellId,
    other: &HexLattice,
) -> Vec<CellId> {
    let centre = lattice.cell_center(cell_id);
    let touch = (lattice.radius + other.radius) as i32;

    let r_other = other.radius as f64;
    let step = (3.0 * r_other * r_other + 3.0 * r_other + 1.0).sqrt();
    let search = ((touch as f64 + r_other) / step).floor() as u32 + 1;

    let mut out: HashSet<CellId> = HashSet::new();
    for id in other.cells_within_distance(other.cell_id(centre.0, centre.1), search) {
        if hex_distance(other.cell_center(id), centre) <= touch {
            out.extend(other.cells_within_distance(id, 1));
        }
    }
    out.into_iter().collect()
}

/// The furthest a feature may reach from its origin and still be folded by
/// every cell it touches, given that a cell reads exactly one ring.
///
/// A feature originating in cell C is offered to `prepare(D)` only when D is C
/// or one of C's six neighbours. So the reach one ring buys is the clearance
/// between C and the nearest cell at lattice distance 2 — a feature that
/// crosses that gap lands in ground whose cell will never see it.
///
/// A hex ball of radius R is a regular hexagon of circumradius R with corners
/// on the hex axes, so its support radius in direction θ is `R·cos(δ)` for δ
/// the angle to the nearest corner. Two cells offset by `v` are separated by
/// `|v| − 2·h(θ_v)`. Measured, this settles at ≈ 1.268·R for any R above a
/// handful of tiles.
fn one_ring_clearance(lattice: &HexLattice) -> f64 {
    fn support(r: f64, theta: f64) -> f64 {
        let sixty = std::f64::consts::PI / 3.0;
        let delta = (theta.rem_euclid(sixty) - sixty * 0.5).abs();
        r * delta.cos()
    }

    let origin = (0, 0);
    let centre = lattice.cell_center(origin);
    let (cx, cy) = hex_to_world(centre.0, centre.1);
    let ring1 = lattice.cells_within_distance(origin, 1);
    let r = lattice.radius as f64;

    let mut gap = f64::MAX;
    for id in lattice.cells_within_distance(origin, 2) {
        if ring1.contains(&id) { continue; }
        let c = lattice.cell_center(id);
        let (dx, dy) = hex_to_world(c.0, c.1);
        let (vx, vy) = (dx - cx, dy - cy);
        gap = gap.min(vx.hypot(vy) - 2.0 * support(r, vy.atan2(vx)));
    }
    gap
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
    /// Cells whose ring has been deformed. Lets the hot path settle for a
    /// single lookup instead of re-walking the ring on every tile in the cell.
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

        // Containment: one ring has to cover everything this event's features
        // can reach. Nothing else in the framework reads `max_influence` — if
        // the scale is too small the answer is a bigger cell, because reading
        // a second ring is not expressible.
        let clearance = one_ring_clearance(&lattice);
        let influence = event.max_influence() as f64;
        assert!(
            influence <= clearance,
            "event '{}' declares max_influence {} at cell scale {}, but one ring of \
             that scale only covers {:.0}. Raise the scale to at least {} — the ring \
             does not move.",
            event.name(),
            event.max_influence(),
            lattice.radius,
            clearance,
            (influence * lattice.radius as f64 / clearance).ceil() as u32,
        );

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
        {
            let _s = tracing::debug_span!("deform").entered();
            for layer in 0..self.events.len() {
                let cell_id = self.lattices[layer].cell_id(q, r);
                self.ensure_query_neighbourhood(layer, cell_id);
            }
        }

        // Phase 2: Query cascade — resolve tile bottom-up
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0, curvature: 0.0 };

        {
            let _s = tracing::debug_span!("query").entered();
            for layer in 0..self.events.len() {
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
                view.curvature += tile_out.curvature;
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

    /// Deform every cell `layer` is allowed to read for a tile in `cell_id` —
    /// its own cell and one ring. Never more: an event needing wider reach
    /// needs a larger cell scale, which `max_influence` checks at setup.

    /// Without this, a query reading a neighbouring cell's index sees whatever
    /// happens to be warm. The result is cached, so a tile materialized while
    /// a neighbour was cold stays wrong for the life of the composite, and two
    /// processes that touched tiles in a different order disagree about the
    /// terrain for the same seed.
    fn ensure_query_neighbourhood(&self, layer: usize, cell_id: CellId) {
        // Every tile in a cell shares one neighbourhood, and cells are never
        // evicted, so walking the ring once per cell is enough. Without this
        // gate the ring walk lands on the per-tile hot path and costs a Vec
        // allocation plus seven lookups on every query.
        if self.cell_caches[layer].neighbourhood_ready.contains_key(&cell_id) {
            return;
        }
        for cell in self.lattices[layer].cells_within_distance(cell_id, 1) {
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

        // Cascade: this cell's footprint over every lower layer, plus one ring.
        for sub_layer in 0..layer {
            let sub_cells = footprint_plus_ring(
                &self.lattices[layer], cell_id, &self.lattices[sub_layer],
            );
            for sub_cell in sub_cells {
                self.ensure_deformed(sub_layer, sub_cell);
            }
        }

        let _s = tracing::debug_span!("event_deform", event = self.events[layer].name()).entered();

        // Deform: populate indexes only
        let scope = CellScope {
            cell: cell_id,
            lattice: &self.lattices[layer],
            indexes: &self.indexes,
            seed: self.seed,
        };
        self.events[layer].deform(&scope);

        // Mark cell as deformed
        self.cell_caches[layer].insert_empty(cell_id);
    }

    /// Resolve the composite TileView from layers 0..up_to.
    /// Used by the `below` closure passed to query. Writes computed results
    /// through to the cell caches so the work is never recomputed (insert is a
    /// no-op for cells not yet deformed).
    fn resolve_below(&self, up_to: usize, q: i32, r: i32) -> TileView {
        let (wx, wy) = hex_to_world(q, r);
        let mut view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0, curvature: 0.0 };

        for li in 0..up_to {
            let cell_id = self.lattices[li].cell_id(q, r);
            let tile_out = if let Some(cached) = self.cell_caches[li].get_tile(cell_id, q, r) {
                cached
            } else {
                // Same guarantee tile_at makes: a query must not run against a
                // half-deformed neighbourhood, or it caches a wrong tile. This
                // path is reached from `below` closures,
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
            view.curvature += tile_out.curvature;
        }

        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records which cells the framework deforms, so the one-ring contract can
    /// be checked without paying for real terrain generation.
    struct ReachProbe {
        /// What the event would *like* to read. The framework has no way to
        /// honour it, and that is the property under test.
        reach: u32,
        deformed: Arc<parking_lot::Mutex<Vec<CellId>>>,
    }

    impl WorldEvent for ReachProbe {
        fn name(&self) -> &str { "reach-probe" }
        fn scale(&self) -> u32 { 32 }

        fn deform(&self, scope: &CellScope) {
            let cell_id = scope.cell();
            self.deformed.lock().push(cell_id);
        }

        fn query(
            &self, _q: i32, _r: i32, _b: &TileView,
            _c: &(dyn Any + Send + Sync), _s: u64,
        ) -> Option<TileOutput> { None }
    }

    /// Every event gets its own cell and one ring — seven cells — whatever it
    /// would prefer. There is no declaration that widens it, which is the point:
    /// a layer folding over origins it cannot see caches the wrong tile forever,
    /// and that is how whole mountains went missing depending on access order.
    #[test]
    fn every_event_gets_exactly_one_ring() {
        for reach in 0..=2u32 {
            let deformed = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let mut c = Composite::new(1);
            c.add_event(Box::new(ReachProbe { reach, deformed: deformed.clone() }));
            c.tile_at(0, 0);

            let cells = deformed.lock();
            assert_eq!(
                cells.len(), 7,
                "an event wanting {reach} rings still got {} cells, not 7",
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
        // Chunk radius 9 — the scale the streaming layers are cut to.
        assert_eq!(hex_ball_tiles(9), 271);
    }

    /// Two adjacent layers at the same cell scale must not walk tiles to find
    /// their overlap — at radius 1800 that is 9.7M tiles per cell, and it once
    /// turned a ~100ms first touch into minutes. Now trivial: same scale means
    /// same lattice, so the footprint is one cell and the answer is its ring.
    #[test]
    fn equal_scale_layers_do_not_enumerate_whole_cells() {
        use crate::plates::PlateCache;
        use motion::MotionEvent;
        use plates::PlateEvent;

        let seed = 0x9E3779B97F4A7C15;
        let plate_cache = Arc::new(PlateCache::new(seed));
        let mut c = Composite::new(seed);
        c.add_event(Box::new(PlateEvent::with_cache(plate_cache.clone())));
        c.add_event(Box::new(MotionEvent::with_cache(plate_cache, seed)));

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
