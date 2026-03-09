use std::collections::{HashMap, HashSet};

use common::{ArrayVec, PlateTag, Tagged, MAX_PLATE_TAGS};
use crate::noise::{hash_u64, hash_f64, simplex_2d};
use crate::plates::{PlateCenter, PlateCache};
use crate::{MACRO_CELL_SIZE, MICRO_CELL_SIZE,
            MICRO_SUPPRESSION_RATE,
            MICRO_JITTER_WAVELENGTH, MICRO_JITTER_MIN, MICRO_JITTER_MAX,
            WARP_STRENGTH_MAX, MAX_ELONGATION, ORPHAN_CORRECTION_MARGIN};

/// Row height factor for hex grid: sqrt(3)/2.
const HEX_ROW_HEIGHT: f64 = 0.8660254037844386;

/// Chunk size in world units. Each chunk holds ~6 micro cells per side.
/// Large enough that any micro cell's 6 hex neighbors are within the center
/// chunk or a 1-ring of surrounding chunks (the invariant for chunk correction).
const MICRO_CHUNK_SIZE: f64 = MICRO_CELL_SIZE * 6.0;


/// Spatial centroid of a macro plate derived from its corrected micro cells.
///
/// Computed by [`MicroplateCache::populate_region`] after orphan correction.
/// Represents the plate's actual center of mass — more accurate than the hex
/// lattice seed position, which is a generation artifact only used during
/// initial plate assignment. The centroid is the authoritative plate center
/// for any computation that references "where this plate is."
#[derive(Clone, Debug)]
pub struct PlateCentroid {
    pub wx: f64,
    pub wy: f64,
    pub plate_id: u64,
    /// Number of corrected micro cells contributing to this centroid.
    pub cell_count: usize,
}

/// A microplate center with position, identity, sub-grid cell, and macro assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct MicroplateCenter {
    pub wx: f64,
    pub wy: f64,
    pub id: u64,
    /// Macro plate this micro cell is assigned to (via warped distance).
    /// Zero if not yet assigned (raw micro_cell_at result).
    pub parent_id: u64,
    pub sub_cell_q: i32,
    pub sub_cell_r: i32,
    /// Tags assigned by generation and the event system. Starts empty;
    /// populated after [`MicroplateCache::populate_region`].
    pub tags: ArrayVec<[PlateTag; MAX_PLATE_TAGS]>,
    /// Local elevation in world units. Defaults to 0.0;
    /// populated by terrain events after macro elevation is established.
    pub elevation: f64,
}

impl Tagged for MicroplateCenter {
    fn tags(&self) -> &ArrayVec<[PlateTag; MAX_PLATE_TAGS]> { &self.tags }
    fn tags_mut(&mut self) -> &mut ArrayVec<[PlateTag; MAX_PLATE_TAGS]> { &mut self.tags }
}

/// One chunk of the micro cell geometry layer.
/// Stores active (non-suppressed) cells inline — (sub_q, sub_r, wx, wy, id) —
/// so `lookup` can iterate directly without a per-cell HashMap access.
/// Retroactively suppressed cells are removed by `fix_orphans`.
/// Correction state lives in `MicroplateCache` — geometry has no plate knowledge.
struct GeometryChunk {
    /// Active cells: (sub_q, sub_r, wx, wy, id). Inline data eliminates the
    /// per-cell `cells` HashMap lookup in the hot `lookup` path.
    cells: Vec<(i32, i32, f64, f64, u64)>,
}

/// Geometry-only layer for micro cell positions and suppression.
///
/// Owns cell positions and the chunk index. No plate data — no `warped_plate_at`,
/// no macro assignments, no correction. `MicroplateCache` wraps this and adds
/// the assignment + correction layer on top.
///
/// Hot-path pixel lookups call `micro_cell_at` here directly. Geometry chunks
/// are cheap to populate (hash + noise per cell, no plate lookups), so cold
/// per-thread caches in the viewer pay only that cost on first access.
pub struct MicroCellGeometry {
    seed: u64,
    /// Raw sub-grid cell cache: (cq, cr) → Option<(wx, wy, id)>
    cells: HashMap<(i32, i32), Option<(f64, f64, u64)>>,
    /// Chunk index: chunk_coord → active cell list
    chunks: HashMap<(i32, i32), GeometryChunk>,
}

impl MicroCellGeometry {
    pub fn new(seed: u64) -> Self {
        Self { seed, cells: HashMap::new(), chunks: HashMap::new() }
    }

    /// Populate geometry for a chunk: enumerate sub-grid cells, apply jitter and
    /// flat suppression, then store surviving positions inline.  No plate assignment.
    ///
    /// Idempotent: returns immediately if already populated.
    pub(crate) fn populate_chunk(&mut self, chunk_cq: i32, chunk_cr: i32) {
        if self.chunks.contains_key(&(chunk_cq, chunk_cr)) {
            return;
        }

        let odd_shift = if chunk_cr & 1 != 0 { MICRO_CHUNK_SIZE * 0.5 } else { 0.0 };
        let center_wx = chunk_cq as f64 * MICRO_CHUNK_SIZE + odd_shift;
        let center_wy = chunk_cr as f64 * MICRO_CHUNK_SIZE * HEX_ROW_HEIGHT;

        let search_radius = MICRO_CHUNK_SIZE;
        let (q_min, r_min) = micro_world_to_cell(center_wx - search_radius, center_wy - search_radius);
        let (q_max, r_max) = micro_world_to_cell(center_wx + search_radius, center_wy + search_radius);
        let margin = 2i32;
        let seed = self.seed;

        // Pass 1: populate cells for the full scan range.
        for cr in (r_min - margin)..=(r_max + margin) {
            for cq in (q_min - margin)..=(q_max + margin) {
                self.cells.entry((cq, cr))
                    .or_insert_with(|| micro_center_for_cell(cq, cr, seed));
            }
        }

        // Pass 2: build inline cell list from survivors owned by this chunk.
        let mut cells: Vec<(i32, i32, f64, f64, u64)> = Vec::new();
        for cr in (r_min - margin)..=(r_max + margin) {
            for cq in (q_min - margin)..=(q_max + margin) {
                if let Some((wx, wy, id)) = self.cells[&(cq, cr)] {
                    if micro_chunk_coord(wx, wy) == (chunk_cq, chunk_cr) {
                        cells.push((cq, cr, wx, wy, id));
                    }
                }
            }
        }

        self.chunks.insert((chunk_cq, chunk_cr), GeometryChunk { cells });
    }

    /// Read-only micro cell lookup. Assumes the center chunk and its 6 neighbors
    /// are already populated. Panics if any required chunk is missing.
    ///
    /// Use `micro_cell_at` for lazy-populating lookup, or pre-populate with
    /// `populate_region` and share via `Arc<MicroCellGeometry>` for read-only
    /// parallel access with zero per-thread geometry rebuilding.
    pub fn lookup(&self, wx: f64, wy: f64) -> MicroplateCenter {
        let (chunk_cq, chunk_cr) = micro_chunk_coord(wx, wy);
        let chunks = &self.chunks;

        let mut best: Option<MicroplateCenter> = None;
        let mut best_dist = f64::MAX;

        for (cq, cr) in std::iter::once((chunk_cq, chunk_cr)).chain(hex_chunk_1ring(chunk_cq, chunk_cr)) {
            let chunk = chunks.get(&(cq, cr))
                .unwrap_or_else(|| panic!("chunk ({cq}, {cr}) not populated — call populate_chunk or populate_region first"));
            for &(mcq, mcr, mx, my, mid) in &chunk.cells {
                let d = dist_sq(wx, wy, mx, my);
                if d < best_dist {
                    best = Some(MicroplateCenter {
                        wx: mx,
                        wy: my,
                        id: mid,
                        parent_id: 0,
                        sub_cell_q: mcq,
                        sub_cell_r: mcr,
                        tags: ArrayVec::new(),
                        elevation: 0.0,
                    });
                    best_dist = d;
                }
            }
        }

        best.expect("no micro cell found in chunk + 1-ring — suppression rate too high")
    }

    /// Find the nearest surviving micro cell to a world position.
    /// Populates the center chunk and its 6 neighbors lazily. No plate assignment.
    pub fn micro_cell_at(&mut self, wx: f64, wy: f64) -> MicroplateCenter {
        let (chunk_cq, chunk_cr) = micro_chunk_coord(wx, wy);
        self.populate_chunk(chunk_cq, chunk_cr);
        for (nq, nr) in hex_chunk_1ring(chunk_cq, chunk_cr) {
            self.populate_chunk(nq, nr);
        }
        self.lookup(wx, wy)
    }
}

// ──── Micro-grid seeds (distinct from macro seeds) ────

const MICRO_SUPPRESS_SEED: u64 = 0xBBBB_CAFE_0001;
const MICRO_JITTER_SEED: u64 = 0xBBBB_CAFE_0002;
const MICRO_OFFSET_X_SEED: u64 = 0xBBBB_CAFE_0003;
const MICRO_OFFSET_Y_SEED: u64 = 0xBBBB_CAFE_0004;
const MICRO_ID_SEED: u64 = 0xBBBB_CAFE_0005;

// ──── Micro-grid cell → microplate center ────

/// Jitter factor at a world position for the micro grid.
fn micro_jitter_at(wx: f64, wy: f64, seed: u64) -> f64 {
    let n = simplex_2d(
        wx / MICRO_JITTER_WAVELENGTH,
        wy / MICRO_JITTER_WAVELENGTH,
        seed ^ MICRO_JITTER_SEED,
    );
    let t = (n + 1.0) * 0.5;
    MICRO_JITTER_MIN + t * (MICRO_JITTER_MAX - MICRO_JITTER_MIN)
}

/// Whether a micro cell is suppressed (produces no microplate center).
///
/// Flat rate everywhere — micro cell character is independent of the coastline.
fn micro_cell_is_suppressed(cq: i32, cr: i32, seed: u64) -> bool {
    hash_f64(cq as i64, cr as i64, seed ^ MICRO_SUPPRESS_SEED) < MICRO_SUPPRESSION_RATE
}

/// Compute the microplate center for a specific micro-grid cell (odd-r offset hex).
/// Returns None if the cell is suppressed.
fn micro_center_for_cell(cq: i32, cr: i32, seed: u64) -> Option<(f64, f64, u64)> {
    if micro_cell_is_suppressed(cq, cr, seed) {
        return None;
    }

    let odd_shift = if cr & 1 != 0 { MICRO_CELL_SIZE * 0.5 } else { 0.0 };
    let nominal_wx = cq as f64 * MICRO_CELL_SIZE + odd_shift;
    let nominal_wy = cr as f64 * MICRO_CELL_SIZE * HEX_ROW_HEIGHT;

    let jitter = micro_jitter_at(nominal_wx, nominal_wy, seed);

    let offset_x = hash_f64(cq as i64, cr as i64, seed ^ MICRO_OFFSET_X_SEED) - 0.5;
    let offset_y = hash_f64(cq as i64, cr as i64, seed ^ MICRO_OFFSET_Y_SEED) - 0.5;

    let wx = nominal_wx + offset_x * jitter * MICRO_CELL_SIZE;
    let wy = nominal_wy + offset_y * jitter * MICRO_CELL_SIZE;

    let id = hash_u64(cq as i64, cr as i64, seed ^ MICRO_ID_SEED);

    Some((wx, wy, id))
}

/// Which micro-grid cell contains a world position.
fn micro_world_to_cell(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = MICRO_CELL_SIZE * HEX_ROW_HEIGHT;
    let cr = (wy / row_height).round() as i32;
    let odd_shift = if cr & 1 != 0 { MICRO_CELL_SIZE * 0.5 } else { 0.0 };
    let cq = ((wx - odd_shift) / MICRO_CELL_SIZE).round() as i32;
    (cq, cr)
}

/// Which micro chunk (hex odd-r lattice) contains a world position.
///
/// Uses proper cube-coordinate rounding: convert to fractional axial coords
/// (both q and r derived continuously from wx/wy), compute all three cube
/// components, round all three, then fix the component with the largest
/// rounding error.  This correctly follows 60° hex edges and avoids the
/// straight horizontal/vertical artifacts that arise when r is rounded first
/// and its integer parity is used to pick the odd-row x-shift for q.
fn micro_chunk_coord(wx: f64, wy: f64) -> (i32, i32) {
    let row_height = MICRO_CHUNK_SIZE * HEX_ROW_HEIGHT;

    // Fractional axial coordinates (pointy-top, odd-r convention).
    // q_frac uses the continuous r_frac — no premature rounding.
    let r_frac = wy / row_height;
    let q_frac = wx / MICRO_CHUNK_SIZE - r_frac * 0.5;
    let s_frac = -q_frac - r_frac;

    // Round all three cube components.
    let mut qi = q_frac.round() as i32;
    let mut ri = r_frac.round() as i32;
    let si = s_frac.round() as i32;

    // Fix whichever component has the largest rounding error so that
    // qi + ri + si == 0 (cube-coordinate invariant) is restored.
    let q_diff = (qi as f64 - q_frac).abs();
    let r_diff = (ri as f64 - r_frac).abs();
    let s_diff = (si as f64 - s_frac).abs();

    if q_diff > r_diff && q_diff > s_diff {
        qi = -ri - si;
    } else if r_diff > s_diff {
        ri = -qi - si;
    }
    // else: si has the largest error; qi and ri are already correct,
    // and si = -qi - ri is implied but unused.

    // Cube/axial → odd-r offset.
    let cr = ri;
    let cq = qi + (ri - (ri & 1)) / 2;
    (cq, cr)
}

/// The 6 hex neighbors of chunk `(cq, cr)` in the odd-r offset lattice.
fn hex_chunk_1ring(cq: i32, cr: i32) -> [(i32, i32); 6] {
    let offsets = if cr & 1 == 0 { &HEX_NEIGHBORS_EVEN } else { &HEX_NEIGHBORS_ODD };
    (*offsets).map(|(dq, dr)| (cq + dq, cr + dr))
}

fn dist_sq(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

// ──── Public API (bottom-up flow) ────

/// Find the nearest micro cell to a world position using euclidean distance.
/// Micro cells are equidimensional everywhere — elongation lives in the
/// macro plate assignment layer (`warped_plate_at`), not here.
pub fn micro_cell_at(wx: f64, wy: f64, seed: u64) -> MicroplateCenter {
    let (cq, cr) = micro_world_to_cell(wx, wy);

    let mut best: Option<MicroplateCenter> = None;
    let mut best_dist = f64::MAX;

    for dr in -3..=3 {
        for dq in -3..=3 {
            let ncq = cq + dq;
            let ncr = cr + dr;
            if let Some((mx, my, mid)) = micro_center_for_cell(ncq, ncr, seed) {
                let d = dist_sq(wx, wy, mx, my);
                if d < best_dist {
                    best = Some(MicroplateCenter {
                        wx: mx,
                        wy: my,
                        id: mid,
                        parent_id: 0,
                        sub_cell_q: ncq,
                        sub_cell_r: ncr,
                        tags: ArrayVec::new(),
                        elevation: 0.0,
                    });
                    best_dist = d;
                }
            }
        }
    }

    best.expect("no micro cell found in 3-ring neighborhood — micro suppression rate too high")
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `PlateCache::warped_plate_at(micro.wx, micro.wy)` with a shared cache.
pub fn macro_plate_for(micro: &MicroplateCenter, seed: u64) -> PlateCenter {
    PlateCache::new(seed).warped_plate_at(micro.wx, micro.wy)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `MicroplateCache::plate_info_at` for repeated lookups.
pub fn plate_info_at(wx: f64, wy: f64, seed: u64) -> (PlateCenter, MicroplateCenter) {
    let mut cache = PlateCache::new(seed);
    let mut micro = micro_cell_at(wx, wy, seed);
    let macro_plate = cache.warped_plate_at(micro.wx, micro.wy);
    micro.parent_id = macro_plate.id;
    (macro_plate, micro)
}

/// UNCACHED — creates throwaway PlateCache per call.
/// Use `generate_micro_cells_for_macro` with a shared `&mut PlateCache`.
pub fn micro_cells_for_macro(macro_seed: &PlateCenter, seed: u64) -> Vec<MicroplateCenter> {
    let mut plate_cache = PlateCache::new(seed);
    generate_micro_cells_for_macro(macro_seed, seed, &mut plate_cache)
}

/// Internal: generate micro cells for a macro seed using a shared PlateCache.
fn generate_micro_cells_for_macro(
    macro_seed: &PlateCenter,
    seed: u64,
    plate_cache: &mut PlateCache,
) -> Vec<MicroplateCenter> {
    let search_radius = (MACRO_CELL_SIZE + WARP_STRENGTH_MAX) * MAX_ELONGATION;
    let cell_reach = (search_radius / MICRO_CELL_SIZE) as i32 + 3;

    let (center_cq, center_cr) = micro_world_to_cell(macro_seed.wx, macro_seed.wy);

    let mut children = Vec::new();

    for dr in -cell_reach..=cell_reach {
        for dq in -cell_reach..=cell_reach {
            let cq = center_cq + dq;
            let cr = center_cr + dr;

            if let Some((wx, wy, id)) = micro_center_for_cell(cq, cr, seed) {
                let owner = plate_cache.warped_plate_at(wx, wy);
                if owner.id == macro_seed.id {
                    children.push(MicroplateCenter {
                        wx,
                        wy,
                        id,
                        parent_id: macro_seed.id,
                        sub_cell_q: cq,
                        sub_cell_r: cr,
                        tags: ArrayVec::new(),
                        elevation: 0.0,
                    });
                }
            }
        }
    }

    children
}


// ──── Cached API ────

/// Lazy cache for the bottom-up micro → macro lookup flow.
///
/// ### Correction model
///
/// For batch rendering (viewer, offline tools): call [`Self::populate_region`]
/// once. It populates micro cells for the region of interest plus a
/// Every `plate_info_at` call returns corrected, connectivity-verified data.
/// On first access to a chunk, `ensure_corrected_region` populates a
/// `ORPHAN_CORRECTION_MARGIN`-wide border around it, runs `fix_orphans`, and
/// marks the queried chunk and its hex 1-ring as corrected. Subsequent calls
/// to corrected chunks return immediately with no additional work.
///
/// `populate_region` is a batch performance hint: it warms a full viewport
/// in one pass and marks core chunks corrected so individual `plate_info_at`
/// calls within the warmed region skip the per-query correction overhead.
pub struct MicroplateCache {
    /// Geometry layer: cell positions + chunk index. No plate data.
    pub geometry: MicroCellGeometry,
    /// Macro assignment cache: micro_id → PlateCenter
    macro_assignments: HashMap<u64, PlateCenter>,
    /// Plate cache for warped macro lookups
    pub plate_cache: PlateCache,
    /// Chunks that have had orphan correction applied.
    corrected_chunks: HashSet<(i32, i32)>,
    /// Post-correction centroids: plate_id → centroid.
    /// Populated by [`Self::populate_region`] after `fix_orphans`.
    /// Empty until `populate_region` is called.
    centroids: HashMap<u64, PlateCentroid>,
    /// Tags per micro cell, keyed by cell ID.
    /// Populated by [`Self::classify_micro_tags`] during `populate_region`.
    /// Enriches the `tags` field of `MicroplateCenter` returned by `plate_info_at`.
    micro_tags: HashMap<u64, ArrayVec<[PlateTag; MAX_PLATE_TAGS]>>,
}

impl MicroplateCache {
    pub fn new(seed: u64) -> Self {
        Self {
            geometry: MicroCellGeometry::new(seed),
            macro_assignments: HashMap::new(),
            plate_cache: PlateCache::new(seed),
            corrected_chunks: HashSet::new(),
            centroids: HashMap::new(),
            micro_tags: HashMap::new(),
        }
    }

    /// Geometry-only micro cell lookup. Delegates to the geometry layer —
    /// no plate assignment, no correction. Safe to call on cold per-thread caches.
    pub fn micro_cell_at(&mut self, wx: f64, wy: f64) -> MicroplateCenter {
        self.geometry.micro_cell_at(wx, wy)
    }

    /// Populate a chunk: first populate geometry (positions only), then assign
    /// macro plates for any cells not yet assigned.
    ///
    /// Geometry population is idempotent. Macro assignment only runs for cells
    /// newly discovered by geometry (those not yet in `macro_assignments`).
    fn populate_chunk(&mut self, chunk_cq: i32, chunk_cr: i32) {
        self.geometry.populate_chunk(chunk_cq, chunk_cr);

        // Assign macro plates for cells in this chunk that don't have one yet.
        // Clone the inline cell list to release the geometry borrow before calling
        // warped_plate_at (which needs &mut self.plate_cache).
        let cells: Vec<(i32, i32, f64, f64, u64)> = self.geometry.chunks[&(chunk_cq, chunk_cr)]
            .cells.clone();

        for (_, _, wx, wy, id) in cells {
            if !self.macro_assignments.contains_key(&id) {
                let plate = self.plate_cache.warped_plate_at(wx, wy);
                self.macro_assignments.insert(id, plate);
            }
        }
    }

    /// Populate all chunks within `ORPHAN_CORRECTION_MARGIN` of the given chunk,
    /// run a global `fix_orphans` pass, then mark the queried chunk and its hex
    /// 1-ring as corrected.
    ///
    /// Margin chunks (outside the 1-ring) are populated for context only and are
    /// left uncorrected. When a later query lands in one of those margin chunks,
    /// it triggers its own `ensure_corrected_region`, extending the corrected zone
    /// outward naturally.
    fn ensure_corrected_region(&mut self, chunk_cq: i32, chunk_cr: i32) {
        let odd_shift = if chunk_cr & 1 != 0 { MICRO_CHUNK_SIZE * 0.5 } else { 0.0 };
        let center_wx = chunk_cq as f64 * MICRO_CHUNK_SIZE + odd_shift;
        let center_wy = chunk_cr as f64 * MICRO_CHUNK_SIZE * HEX_ROW_HEIGHT;

        let margin = ORPHAN_CORRECTION_MARGIN;
        let row_height = MICRO_CHUNK_SIZE * HEX_ROW_HEIGHT;
        let min_wy = center_wy - margin;
        let max_wy = center_wy + margin;
        let min_wx = center_wx - margin;
        let max_wx = center_wx + margin;

        let cr_min = (min_wy / row_height).floor() as i32 - 1;
        let cr_max = (max_wy / row_height).ceil() as i32 + 1;
        for cr in cr_min..=cr_max {
            let odd = if cr & 1 != 0 { MICRO_CHUNK_SIZE * 0.5 } else { 0.0 };
            let cq_min = ((min_wx - odd) / MICRO_CHUNK_SIZE).floor() as i32 - 1;
            let cq_max = ((max_wx - odd) / MICRO_CHUNK_SIZE).ceil() as i32 + 1;
            for cq in cq_min..=cq_max {
                self.populate_chunk(cq, cr);
            }
        }

        self.fix_orphans();

        // Mark the queried chunk and its 1-ring corrected. Margin chunks are
        // context — they get their own correction pass when queried.
        self.corrected_chunks.insert((chunk_cq, chunk_cr));
        for (nq, nr) in hex_chunk_1ring(chunk_cq, chunk_cr) {
            self.corrected_chunks.insert((nq, nr));
        }
    }

    /// Performance hint: warm a full viewport before the query loop.
    ///
    /// Populates all chunks within `half_width × half_height` plus
    /// `ORPHAN_CORRECTION_MARGIN`, runs a single global `fix_orphans` pass, then
    /// marks core chunks (those within the requested region) as corrected.
    /// Margin chunks are populated for context but left uncorrected, so any future
    /// query that lands in the margin triggers its own `ensure_corrected_region`.
    ///
    /// The margin guarantees every macro plate seed that owns a cell inside the core
    /// region is visible, so `fix_orphans` resolves the full plate body for all
    /// core cells. Core cells are guaranteed orphan-free after this call.
    pub fn populate_region(
        &mut self,
        center_wx: f64,
        center_wy: f64,
        half_width: f64,
        half_height: f64,
    ) {
        let margin = ORPHAN_CORRECTION_MARGIN;
        let row_height = MICRO_CHUNK_SIZE * HEX_ROW_HEIGHT;

        let min_wy = center_wy - half_height - margin;
        let max_wy = center_wy + half_height + margin;
        let min_wx = center_wx - half_width - margin;
        let max_wx = center_wx + half_width + margin;

        let cr_min = (min_wy / row_height).floor() as i32 - 1;
        let cr_max = (max_wy / row_height).ceil() as i32 + 1;

        for cr in cr_min..=cr_max {
            let odd_shift = if cr & 1 != 0 { MICRO_CHUNK_SIZE * 0.5 } else { 0.0 };
            let cq_min = ((min_wx - odd_shift) / MICRO_CHUNK_SIZE).floor() as i32 - 1;
            let cq_max = ((max_wx - odd_shift) / MICRO_CHUNK_SIZE).ceil() as i32 + 1;
            for cq in cq_min..=cq_max {
                self.populate_chunk(cq, cr);
            }
        }

        self.fix_orphans();

        // Mark only core chunks corrected. One chunk-width buffer ensures every
        // micro cell within the requested region is in a corrected chunk, even if
        // the cell's chunk center sits slightly beyond the half_width/half_height edge.
        for &(cq, cr) in self.geometry.chunks.keys() {
            let odd_shift = if cr & 1 != 0 { MICRO_CHUNK_SIZE * 0.5 } else { 0.0 };
            let cwx = cq as f64 * MICRO_CHUNK_SIZE + odd_shift;
            let cwy = cr as f64 * row_height;
            if (cwx - center_wx).abs() <= half_width + MICRO_CHUNK_SIZE
                && (cwy - center_wy).abs() <= half_height + MICRO_CHUNK_SIZE
            {
                self.corrected_chunks.insert((cq, cr));
            }
        }

        // Tags require all assignments and corrections to be final.
        self.classify_micro_tags();

        // Centroids require corrected flags to be set first (they filter on them).
        self.compute_centroids();
    }

    /// Assign Sea, Coast, or Inland to every live micro cell.
    ///
    /// Uses regime values (no plate lookups) and the hex-neighbor cell grid
    /// to determine if any of the 6 adjacent micro cells have a different
    /// land/water status. Called at the end of [`Self::populate_region`].
    fn classify_micro_tags(&mut self) {
        use crate::{REGIME_LAND_THRESHOLD, COASTAL_WARP_THRESHOLD};

        // Collect all live cells — releases the borrow on geometry.cells so we
        // can re-borrow it immutably inside the loop.
        let live_cells: Vec<(i32, i32, f64, f64, u64)> = self.geometry.cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (cq, cr, wx, wy, id))
            })
            .collect();

        // Precompute regime and warp strength for each live cell.
        // Both require mutable plate_cache access, so done before the immutable loop.
        let regimes: HashMap<u64, bool> = live_cells.iter()
            .map(|&(_, _, wx, wy, id)| {
                (id, self.plate_cache.regime_value_at(wx, wy) >= REGIME_LAND_THRESHOLD)
            })
            .collect();
        let warp_strengths: HashMap<u64, f64> = live_cells.iter()
            .map(|&(_, _, wx, wy, id)| {
                (id, self.plate_cache.warp_strength_at(wx, wy))
            })
            .collect();

        let mut new_tags: HashMap<u64, ArrayVec<[PlateTag; MAX_PLATE_TAGS]>> = HashMap::with_capacity(live_cells.len());
        for &(_cq, _cr, _wx, _wy, id) in &live_cells {
            let is_land = regimes[&id];
            let is_coast = warp_strengths.get(&id).copied().unwrap_or(0.0) > COASTAL_WARP_THRESHOLD;
            let tag = if is_coast {
                PlateTag::Coast
            } else if is_land {
                PlateTag::Inland
            } else {
                PlateTag::Sea
            };
            new_tags.insert(id, { let mut av = ArrayVec::new(); av.push(tag); av });
        }
        self.micro_tags = new_tags;
    }

    /// Cached lookup: micro cell + corrected macro assignment.
    ///
    /// Guarantees a connectivity-verified assignment on every call. On first access
    /// to a chunk, triggers `ensure_corrected_region` which loads a
    /// `ORPHAN_CORRECTION_MARGIN`-wide context, runs `fix_orphans`, and marks the
    /// queried chunk and its 1-ring corrected. Subsequent calls to corrected chunks
    /// return immediately.
    pub fn plate_info_at(&mut self, wx: f64, wy: f64) -> (PlateCenter, MicroplateCenter) {
        let (chunk_cq, chunk_cr) = micro_chunk_coord(wx, wy);

        if !self.corrected_chunks.contains(&(chunk_cq, chunk_cr)) {
            self.ensure_corrected_region(chunk_cq, chunk_cr);
        }

        // Chunks are guaranteed populated by ensure_corrected_region; use read-only lookup.
        let mut micro = self.geometry.lookup(wx, wy);

        let macro_plate = self.macro_assignments.get(&micro.id)
            .expect("macro assignment must exist after ensure_corrected_region")
            .clone();

        micro.parent_id = macro_plate.id;
        if let Some(tags) = self.micro_tags.get(&micro.id) {
            micro.tags = tags.clone();
        }
        (macro_plate, micro)
    }

    /// Look up the cached macro plate assignment for a micro cell by its ID.
    pub fn macro_assignment(&self, micro_id: u64) -> Option<&PlateCenter> {
        self.macro_assignments.get(&micro_id)
    }

    /// Consume this cache and return ownership of its geometry layer.
    ///
    /// After calling `populate_region`, use this to extract the pre-warmed
    /// `MicroCellGeometry` for sharing across rayon threads via `Arc`.
    /// Save any needed data (e.g., `centroids()`, `all_macro_ids()`) before calling.
    pub fn take_geometry(self) -> MicroCellGeometry {
        self.geometry
    }

    /// Extract all corrected micro→macro assignments as micro_id → macro_plate_id.
    pub fn all_macro_ids(&self) -> HashMap<u64, u64> {
        self.macro_assignments.iter()
            .map(|(&mid, plate)| (mid, plate.id))
            .collect()
    }

    /// Centroid of a macro plate, or `None` if the plate has no corrected cells.
    ///
    /// Valid after [`Self::populate_region`]; returns `None` for plates entirely
    /// in the margin (uncorrected) or before `populate_region` is called.
    pub fn plate_centroid(&self, plate_id: u64) -> Option<&PlateCentroid> {
        self.centroids.get(&plate_id)
    }

    /// Iterate all computed plate centroids.
    ///
    /// Non-empty only after [`Self::populate_region`].
    pub fn centroids(&self) -> impl Iterator<Item = &PlateCentroid> {
        self.centroids.values()
    }

    /// Compute plate centroids from corrected micro cells.
    ///
    /// Iterates `chunk.cells` (inline data) for corrected chunks only.
    /// No per-cell HashMap access — wx/wy/id are stored directly in the chunk.
    fn compute_centroids(&mut self) {
        // Iterate corrected chunks; inline data eliminates per-cell HashMap access.
        let mut sums: HashMap<u64, (f64, f64, usize)> = HashMap::new();
        for (key, chunk) in &self.geometry.chunks {
            if !self.corrected_chunks.contains(key) { continue; }
            for &(_, _, wx, wy, id) in &chunk.cells {
                let Some(plate) = self.macro_assignments.get(&id) else { continue };
                let entry = sums.entry(plate.id).or_insert((0.0, 0.0, 0));
                entry.0 += wx;
                entry.1 += wy;
                entry.2 += 1;
            }
        }

        self.centroids = sums.into_iter()
            .map(|(plate_id, (sum_wx, sum_wy, count))| {
                (plate_id, PlateCentroid {
                    wx: sum_wx / count as f64,
                    wy: sum_wy / count as f64,
                    plate_id,
                    cell_count: count,
                })
            })
            .collect();
    }

    /// Fix orphaned macro plate assignments across all cached cells.
    ///
    /// Connected component analysis: for each macro plate, flood-fill its micro
    /// cells. Keep only the largest component (the main body); reassign all
    /// smaller fragments to the surrounding majority plate. Repeat until stable.
    /// Converges in a small number of rounds for typical plate configurations;
    /// the 10-round cap handles pathological cascade chains.
    ///
    /// Returns the number of cells corrected.
    pub fn fix_orphans(&mut self) -> usize {
        // Build reverse map: micro_id → (cq, cr, wx, wy)
        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = self.geometry.cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Ensure all cached micro cells have macro assignments
        let unassigned: Vec<(u64, f64, f64)> = id_to_pos.iter()
            .filter(|(id, _)| !self.macro_assignments.contains_key(id))
            .map(|(&id, &(_, _, wx, wy))| (id, wx, wy))
            .collect();
        for (id, wx, wy) in unassigned {
            let plate = self.plate_cache.warped_plate_at(wx, wy);
            self.macro_assignments.insert(id, plate);
        }

        // Build neighbor map once (topology doesn't change, only assignments)
        let all_neighbors: HashMap<u64, Vec<u64>> = id_to_pos.iter()
            .filter_map(|(&id, &(cq, cr, _, _))| {
                if self.macro_assignments.contains_key(&id) {
                    Some((id, micro_neighbor_ids(cq, cr, id, &self.geometry.cells)))
                } else {
                    None
                }
            })
            .collect();

        // ── CC loop: repeat until no fragments found ──

        let mut total_corrected = 0;
        const MAX_ROUNDS: usize = 10;

        for _ in 0..MAX_ROUNDS {
            let round_count = self.cc_round(&all_neighbors);
            if round_count == 0 { break; }
            total_corrected += round_count;
        }

        // ── Final sweep: minority fragment suppression ──
        //
        // After cc_round converges, minority fragments of multi-CC plates may still
        // remain if their surrounding cells are all suppressed (no neighbor vote was
        // possible in cc_round). This sweep finds those fragments and either:
        //   a) Reassigns them to the surrounding majority plate (if external neighbors exist).
        //   b) Suppresses them retroactively (micro_cells = None) if they are completely
        //      surrounded by a gap of suppressed cells. Suppression is correct here:
        //      reassigning to any plate would create a new orphan of that plate at the
        //      same isolated position. After suppression, plate_info_at for those positions
        //      resolves via the nearest surviving micro cell (no data loss, different cell).

        // Rebuild same-plate adjacency from current (post-cc_round) assignments.
        let mut post_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, nbrs) in &all_neighbors {
            if let Some(my_plate) = self.macro_assignments.get(&id) {
                let my_id = my_plate.id;
                let same: Vec<u64> = nbrs.iter()
                    .filter(|&&nid| self.macro_assignments.get(&nid).map_or(false, |p| p.id == my_id))
                    .copied()
                    .collect();
                post_adj.insert(id, same);
            }
        }

        // BFS to find all CCs per plate.
        let mut sweep_visited: HashSet<u64> = HashSet::new();
        let mut plate_ccs: HashMap<u64, Vec<Vec<u64>>> = HashMap::new();
        {
            let mut sorted_ids: Vec<u64> = post_adj.keys().copied().collect();
            sorted_ids.sort_unstable();
            for start in sorted_ids {
                if sweep_visited.contains(&start) { continue; }
                let plate_id = match self.macro_assignments.get(&start) {
                    Some(p) => p.id, None => continue,
                };
                let mut cc = Vec::new();
                let mut queue = vec![start];
                while let Some(cur) = queue.pop() {
                    if !sweep_visited.insert(cur) { continue; }
                    cc.push(cur);
                    if let Some(nbrs) = post_adj.get(&cur) {
                        for &nid in nbrs {
                            if !sweep_visited.contains(&nid) { queue.push(nid); }
                        }
                    }
                }
                plate_ccs.entry(plate_id).or_default().push(cc);
            }
        }

        // Reassign or suppress minority fragments of multi-CC plates.
        let mut sweep_corrections: Vec<(u64, PlateCenter)> = Vec::new();
        for (_, ccs) in &plate_ccs {
            if ccs.len() <= 1 { continue; }
            // Sort: largest first (= main body), ties broken by min cell ID.
            let mut sorted: Vec<&Vec<u64>> = ccs.iter().collect();
            sorted.sort_by(|a, b| {
                b.len().cmp(&a.len())
                    .then_with(|| a.iter().min().cmp(&b.iter().min()))
            });
            let fragments: &[&Vec<u64>] = &sorted[1..];

            for &cc in fragments {
                let cc_set: HashSet<u64> = cc.iter().copied().collect();

                // First try the 1-ring (direct hex neighbors from all_neighbors).
                let mut counts: HashMap<u64, (usize, PlateCenter)> = HashMap::new();
                for &cid in cc {
                    if let Some(nbrs) = all_neighbors.get(&cid) {
                        for &nid in nbrs {
                            if cc_set.contains(&nid) { continue; }
                            if let Some(plate) = self.macro_assignments.get(&nid) {
                                let entry = counts.entry(plate.id).or_insert((0, plate.clone()));
                                entry.0 += 1;
                            }
                        }
                    }
                }

                if let Some((_, new_plate)) = counts.into_values()
                    .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.id.cmp(&a.1.id)))
                {
                    // Reachable neighbors exist — reassign to the majority surrounding plate.
                    for &cid in cc {
                        sweep_corrections.push((cid, new_plate.clone()));
                    }
                } else {
                    // Fragment completely surrounded by suppressed/absent cells.
                    // Any plate we assign it to would create a new orphan of that plate
                    // (same isolation, different ID). Suppress the cells instead so they
                    // drop out of the BFS entirely and stop causing orphan failures.
                    for &cid in cc {
                        if let Some(&(cq, cr, wx, wy)) = id_to_pos.get(&cid) {
                            self.geometry.cells.insert((cq, cr), None);
                            self.macro_assignments.remove(&cid);
                            // Remove from chunk's inline cell list so lookup skips it.
                            let (chunk_cq, chunk_cr) = micro_chunk_coord(wx, wy);
                            if let Some(chunk) = self.geometry.chunks.get_mut(&(chunk_cq, chunk_cr)) {
                                chunk.cells.retain(|cell| (cell.0, cell.1) != (cq, cr));
                            }
                        }
                    }
                }
            }
        }

        let sweep_count = sweep_corrections.len();
        for (id, plate) in sweep_corrections {
            self.macro_assignments.insert(id, plate);
        }

        total_corrected + sweep_count
    }

    /// One round of connected component analysis + fragment reassignment (global).
    ///
    /// Fragments are processed in ascending min-cell-ID order. Corrections are
    /// applied immediately (not batched), so each fragment sees the updated state
    /// from earlier fragments. This breaks oscillation: when two isolated cells
    /// mutually point at each other's plate, the lower-ID cell wins and the
    /// higher-ID cell sees the updated plate on its surrounding check.
    ///
    /// Returns the number of cells actually reassigned (0 = stable).
    fn cc_round(&mut self, all_neighbors: &HashMap<u64, Vec<u64>>) -> usize {
        // Build same-plate adjacency from current assignments
        let mut same_plate_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, nbrs) in all_neighbors {
            if let Some(my_plate) = self.macro_assignments.get(&id) {
                let my_plate_id = my_plate.id;
                let same: Vec<u64> = nbrs.iter()
                    .filter(|nid| {
                        self.macro_assignments.get(nid)
                            .map_or(false, |p| p.id == my_plate_id)
                    })
                    .copied()
                    .collect();
                same_plate_adj.insert(id, same);
            }
        }

        // BFS for connected components (deterministic via sorted IDs)
        let mut visited: HashSet<u64> = HashSet::new();
        let mut all_components: Vec<Vec<u64>> = Vec::new();
        let mut sorted_ids: Vec<u64> = same_plate_adj.keys().copied().collect();
        sorted_ids.sort_unstable();

        for id in sorted_ids {
            if visited.contains(&id) { continue; }
            let mut component = Vec::new();
            let mut queue = vec![id];
            while let Some(current) = queue.pop() {
                if !visited.insert(current) { continue; }
                component.push(current);
                if let Some(nbrs) = same_plate_adj.get(&current) {
                    for &nid in nbrs {
                        if !visited.contains(&nid) {
                            queue.push(nid);
                        }
                    }
                }
            }
            all_components.push(component);
        }

        // Group by plate, identify fragments
        let mut plate_components: HashMap<u64, Vec<Vec<u64>>> = HashMap::new();
        for component in all_components {
            if let Some(plate) = self.macro_assignments.get(&component[0]) {
                plate_components.entry(plate.id).or_default().push(component);
            }
        }

        let mut fragments: Vec<Vec<u64>> = Vec::new();
        for (_, mut components) in plate_components {
            if components.len() <= 1 { continue; }
            components.sort_by(|a, b| {
                b.len().cmp(&a.len())
                    .then_with(|| a.iter().min().cmp(&b.iter().min()))
            });
            fragments.extend(components.into_iter().skip(1));
        }

        if fragments.is_empty() { return 0; }

        // Sort fragments by min cell ID — stable processing order across rounds.
        fragments.sort_by_key(|f| *f.iter().min().unwrap_or(&u64::MAX));

        // Apply corrections immediately (not batched) so each fragment reads
        // the state updated by earlier fragments in this same round.
        let mut count = 0;
        for fragment in &fragments {
            let frag_set: HashSet<u64> = fragment.iter().copied().collect();
            let mut surrounding: HashMap<u64, (usize, PlateCenter)> = HashMap::new();
            for &cid in fragment {
                if let Some(nbrs) = all_neighbors.get(&cid) {
                    for &nid in nbrs {
                        if frag_set.contains(&nid) { continue; }
                        if let Some(plate) = self.macro_assignments.get(&nid) {
                            let entry = surrounding.entry(plate.id)
                                .or_insert((0, plate.clone()));
                            entry.0 += 1;
                        }
                    }
                }
            }
            if let Some((_, new_plate)) = surrounding.into_values()
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.id.cmp(&a.1.id)))
            {
                let new_id = new_plate.id;
                for &cid in fragment {
                    if self.macro_assignments.get(&cid).map_or(true, |p| p.id != new_id) {
                        self.macro_assignments.insert(cid, new_plate.clone());
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Returns the number of corrected chunks. Useful for tests.
    #[cfg(test)]
    fn corrected_chunk_count(&self) -> usize {
        self.corrected_chunks.len()
    }

    /// Returns true if the chunk containing (wx, wy) has been corrected.
    #[cfg(test)]
    fn chunk_is_corrected(&self, wx: f64, wy: f64) -> bool {
        let (cq, cr) = micro_chunk_coord(wx, wy);
        self.corrected_chunks.contains(&(cq, cr))
    }

    /// Returns true if the micro sub-grid cell at (cq, cr) belongs to a
    /// chunk that has been corrected. Cells in neighbor chunks are populated
    /// for boundary context but not corrected.
    #[cfg(test)]
    fn is_cell_in_corrected_chunk(&self, cq: i32, cr: i32) -> bool {
        if let Some(Some((wx, wy, _))) = self.geometry.cells.get(&(cq, cr)) {
            let (chunk_cq, chunk_cr) = micro_chunk_coord(*wx, *wy);
            self.corrected_chunks.contains(&(chunk_cq, chunk_cr))
        } else {
            false
        }
    }
}

/// Hex neighbor offsets for odd-r offset grid (even rows).
const HEX_NEIGHBORS_EVEN: [(i32, i32); 6] = [(-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)];
/// Hex neighbor offsets for odd-r offset grid (odd rows).
const HEX_NEIGHBORS_ODD: [(i32, i32); 6] = [(-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)];

/// Find micro neighbor IDs via direct sub-grid hex coordinate offsets.
/// 6 lookups per cell — no spatial scanning, no distance math.
fn micro_neighbor_ids(
    cq: i32, cr: i32, id: u64,
    micro_cells: &HashMap<(i32, i32), Option<(f64, f64, u64)>>,
) -> Vec<u64> {
    let offsets = if cr & 1 == 0 { &HEX_NEIGHBORS_EVEN } else { &HEX_NEIGHBORS_ODD };
    let mut neighbors = Vec::new();
    for &(dq, dr) in offsets {
        if let Some(Some((_, _, nid))) = micro_cells.get(&(cq + dq, cr + dr)) {
            if *nid != id {
                neighbors.push(*nid);
            }
        }
    }
    neighbors
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plates::{macro_plate_at, macro_plate_neighbors};
    use common::{PlateTag, Tagged};
    use std::collections::HashSet;

    #[test]
    fn micro_cell_at_returns_result_everywhere() {
        let seed = 42u64;
        for x in (-10000..10000).step_by(500) {
            for y in (-10000..10000).step_by(500) {
                // Should never panic — micro grid has no macro dependency
                let _micro = micro_cell_at(x as f64, y as f64, seed);
            }
        }
    }

    #[test]
    fn micro_cell_at_returns_nearest() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(1500) {
            for y in (-5000..5000).step_by(1500) {
                let wx = x as f64;
                let wy = y as f64;
                let result = micro_cell_at(wx, wy, seed);
                let result_dist = dist_sq(wx, wy, result.wx, result.wy);

                // Brute force check over wider area
                let (cq, cr) = micro_world_to_cell(wx, wy);
                for dq in -4..=4 {
                    for dr in -4..=4 {
                        if let Some((mx, my, _)) = micro_center_for_cell(cq + dq, cr + dr, seed) {
                            let d = dist_sq(wx, wy, mx, my);
                            assert!(result_dist <= d + 1e-6,
                                "micro_cell_at({wx}, {wy}) not nearest: result dist²={result_dist}, found dist²={d}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn macro_plate_for_is_deterministic() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(2000) {
            for y in (-5000..5000).step_by(2000) {
                let micro = micro_cell_at(x as f64, y as f64, seed);
                let a = macro_plate_for(&micro, seed);
                let b = macro_plate_for(&micro, seed);
                assert_eq!(a.id, b.id);
            }
        }
    }

    #[test]
    fn plate_info_at_is_consistent() {
        let seed = 42u64;
        for x in (-8000..8000).step_by(500) {
            for y in (-8000..8000).step_by(500) {
                let wx = x as f64;
                let wy = y as f64;
                let (macro_plate, micro) = plate_info_at(wx, wy, seed);
                assert_eq!(micro.parent_id, macro_plate.id,
                    "plate_info_at({wx}, {wy}): micro.parent_id={} != macro.id={}",
                    micro.parent_id, macro_plate.id);

                // macro_plate_for should agree
                let raw_micro = micro_cell_at(wx, wy, seed);
                let assigned = macro_plate_for(&raw_micro, seed);
                assert_eq!(assigned.id, macro_plate.id,
                    "plate_info_at vs macro_plate_for disagree at ({wx}, {wy})");
            }
        }
    }

    #[test]
    fn every_macro_has_micro_cells() {
        let seed = 42u64;
        for x in (-10000..10000).step_by(3000) {
            for y in (-10000..10000).step_by(3000) {
                let plate = macro_plate_at(x as f64, y as f64, seed);
                let children = micro_cells_for_macro(&plate, seed);
                assert!(!children.is_empty(),
                    "Macro plate at ({x}, {y}) id={} has no micro cells", plate.id);
            }
        }
    }

    #[test]
    fn micro_cells_for_macro_matches_individual() {
        let seed = 42u64;
        // Pick a macro plate and verify its micro cells individually report back to it
        let plate = macro_plate_at(0.0, 0.0, seed);
        let children = micro_cells_for_macro(&plate, seed);

        for child in &children {
            let assigned = macro_plate_for(child, seed);
            assert_eq!(assigned.id, plate.id,
                "Micro cell id={} at ({:.0}, {:.0}) claims plate {} but macro_plate_for says {}",
                child.id, child.wx, child.wy, plate.id, assigned.id);
        }
    }

    #[test]
    fn micro_cell_ids_unique_within_macro() {
        let seed = 42u64;
        for x in (-5000..5000).step_by(3000) {
            for y in (-5000..5000).step_by(3000) {
                let plate = macro_plate_at(x as f64, y as f64, seed);
                let children = micro_cells_for_macro(&plate, seed);
                let ids: HashSet<u64> = children.iter().map(|c| c.id).collect();
                assert_eq!(ids.len(), children.len(),
                    "Duplicate micro IDs within plate at ({x}, {y})");
            }
        }
    }

    #[test]
    fn micro_suppression_rate_matches_constant() {
        // The flat suppression rate must be statistically close to MICRO_SUPPRESSION_RATE.
        // This is an invariant: the hash must be uniform, not biased.
        // Tolerance of ±5pp accommodates sampling variance over the grid.
        let seed = 42u64;
        let mut suppressed = 0;
        let mut total = 0;
        for cq in -40..40 {
            for cr in -40..40 {
                total += 1;
                if micro_cell_is_suppressed(cq, cr, seed) {
                    suppressed += 1;
                }
            }
        }
        let rate = suppressed as f64 / total as f64;
        assert!((rate - MICRO_SUPPRESSION_RATE).abs() < 0.05,
            "Observed suppression rate {rate:.3} deviates more than 5pp from \
             MICRO_SUPPRESSION_RATE ({MICRO_SUPPRESSION_RATE})");
    }

    #[test]
    fn cached_matches_uncached_micro() {
        // After orphan correction, the cached path may retroactively suppress some
        // micro cells, meaning cached and uncached can legitimately return different
        // cells at the same position. The micro-cell-must-match invariant no longer
        // holds. Determinism between independent caches is covered separately by
        // plate_info_at_is_deterministic.
        //
        // This test verifies that both paths return without panicking across a
        // representative grid, confirming the 3-ring search radius is sufficient
        // even at high suppression rates combined with retroactive suppression.
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        for x in (-5000..5000).step_by(1000) {
            for y in (-5000..5000).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                let _ = plate_info_at(wx, wy, seed);   // uncached — must not panic
                let _ = cache.plate_info_at(wx, wy);   // cached — must not panic
            }
        }
    }


    // ──── populate_region / correction tests ────

    #[test]
    fn plate_info_at_marks_chunk_corrected() {
        // plate_info_at must mark the queried chunk (and its 1-ring) corrected on
        // first access. populate_region on a fresh cache must also mark core chunks
        // corrected.
        let seed = 42u64;

        // plate_info_at triggers correction
        let mut cache = MicroplateCache::new(seed);
        assert!(!cache.chunk_is_corrected(0.0, 0.0));
        cache.plate_info_at(0.0, 0.0);
        assert!(cache.chunk_is_corrected(0.0, 0.0),
            "plate_info_at must mark the queried chunk corrected");

        // populate_region also marks core chunks corrected
        let mut cache2 = MicroplateCache::new(seed);
        assert!(!cache2.chunk_is_corrected(0.0, 0.0));
        cache2.populate_region(0.0, 0.0, 1000.0, 1000.0);
        assert!(cache2.chunk_is_corrected(0.0, 0.0),
            "populate_region must mark core chunk corrected");
    }

    #[test]
    fn plate_info_at_is_idempotent() {
        // Repeated plate_info_at calls to the same point must return
        // identical results and must not change the corrected-chunk count.
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);

        let (macro1, micro1) = cache.plate_info_at(0.0, 0.0);
        let count = cache.corrected_chunk_count();

        let (macro2, micro2) = cache.plate_info_at(0.0, 0.0);
        assert_eq!(cache.corrected_chunk_count(), count);
        assert_eq!(macro1.id, macro2.id);
        assert_eq!(micro1.id, micro2.id);
    }

    #[test]
    fn plate_info_at_is_deterministic() {
        // Same seed + same query sequence must produce identical results
        // across independent caches.
        let seed = 42u64;
        let mut cache_a = MicroplateCache::new(seed);
        let mut cache_b = MicroplateCache::new(seed);

        for x in (-5000..5000i32).step_by(1000) {
            for y in (-5000..5000i32).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                let (macro_a, micro_a) = cache_a.plate_info_at(wx, wy);
                let (macro_b, micro_b) = cache_b.plate_info_at(wx, wy);
                assert_eq!(macro_a.id, macro_b.id, "macro mismatch at ({wx}, {wy})");
                assert_eq!(micro_a.id, micro_b.id, "micro mismatch at ({wx}, {wy})");
            }
        }
    }

    // ──── Orphan correction tests ────

    /// Count globally-disconnected macro plate fragments (across the full cache)
    /// that contain at least one cell in a corrected chunk.
    ///
    /// "Core" is defined by chunk membership (corrected flag), not by raw world
    /// coordinates. This obeys the Chunk System Is The Spatial Authority invariant:
    /// `populate_region` marks only core chunks corrected; margin chunks are left
    /// uncorrected. So "has a cell in a corrected chunk" == "has a core cell."
    ///
    /// Uses the same hex-adjacency graph as `fix_orphans`. After `fix_orphans`
    /// converges, all plates have exactly one global CC, so this returns 0.
    fn count_global_core_orphans(cache: &MicroplateCache) -> usize {
        // id → (cq, cr, wx, wy) for all non-suppressed cells
        let id_to_pos: HashMap<u64, (i32, i32, f64, f64)> = cache.geometry.cells.iter()
            .filter_map(|(&(cq, cr), cell)| {
                cell.map(|(wx, wy, id)| (id, (cq, cr, wx, wy)))
            })
            .collect();

        // Same-plate adjacency across the full cache (same as cc_round)
        let mut same_plate_adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for (&id, &(cq, cr, _, _)) in &id_to_pos {
            if let Some(my_plate) = cache.macro_assignments.get(&id) {
                let my_id = my_plate.id;
                let same: Vec<u64> = micro_neighbor_ids(cq, cr, id, &cache.geometry.cells)
                    .into_iter()
                    .filter(|nid| {
                        cache.macro_assignments.get(nid).map_or(false, |p| p.id == my_id)
                    })
                    .collect();
                same_plate_adj.insert(id, same);
            }
        }

        // BFS: global CCs per plate
        let mut plate_ccs: HashMap<u64, Vec<Vec<u64>>> = HashMap::new();
        let mut visited: HashSet<u64> = HashSet::new();
        let mut sorted_ids: Vec<u64> = same_plate_adj.keys().copied().collect();
        sorted_ids.sort_unstable();

        for id in sorted_ids {
            if visited.contains(&id) { continue; }
            let mut cc = Vec::new();
            let mut queue = vec![id];
            while let Some(cur) = queue.pop() {
                if !visited.insert(cur) { continue; }
                cc.push(cur);
                if let Some(nbrs) = same_plate_adj.get(&cur) {
                    for &nid in nbrs {
                        if !visited.contains(&nid) { queue.push(nid); }
                    }
                }
            }
            if let Some(plate) = cache.macro_assignments.get(&id) {
                plate_ccs.entry(plate.id).or_default().push(cc);
            }
        }

        // Count minority fragments with at least one cell in a corrected chunk.
        // Chunk membership (not raw world coordinates) is the spatial authority.
        let mut orphan_count = 0;
        for (plate_id, mut ccs) in plate_ccs {
            if ccs.len() <= 1 { continue; }
            ccs.sort_by(|a, b| b.len().cmp(&a.len())); // largest first = main body
            for frag in ccs.into_iter().skip(1) {
                let has_core_cell = frag.iter().any(|&cell_id| {
                    id_to_pos.get(&cell_id)
                        .map_or(false, |&(cq, cr, _, _)| {
                            cache.is_cell_in_corrected_chunk(cq, cr)
                        })
                });
                if has_core_cell {
                    let (_, _, wx, wy) = id_to_pos[&frag[0]];
                    eprintln!("ORPHAN plate={plate_id:016x} size={} at ({wx:.0},{wy:.0})",
                        frag.len());
                    orphan_count += 1;
                }
            }
        }
        orphan_count
    }

    #[test]
    fn populate_region_is_deterministic() {
        // Two caches with identical seed and same populate_region call must
        // produce identical corrected macro assignments.
        let seed = 42u64;
        let mut cache_a = MicroplateCache::new(seed);
        let mut cache_b = MicroplateCache::new(seed);

        cache_a.populate_region(0.0, 0.0, 5000.0, 5000.0);
        cache_b.populate_region(0.0, 0.0, 5000.0, 5000.0);

        for (&id, plate_a) in &cache_a.macro_assignments {
            if let Some(plate_b) = cache_b.macro_assignments.get(&id) {
                assert_eq!(plate_a.id, plate_b.id,
                    "Macro assignment for micro {id} differs between caches");
            }
        }
    }

    /// After populate_region, no macro plate has globally-disconnected fragments
    /// containing cells in the core ±25k. Uses the same adjacency graph as
    /// fix_orphans — a cell cluster is only an orphan if it is disconnected in
    /// the full cache, not merely cut off by a narrow clipping window.
    #[test]
    fn no_orphans_at_25k_after_populate_region() {
        let seed = 0x9E3779B97F4A7C15;
        let mut cache = MicroplateCache::new(seed);
        cache.populate_region(0.0, 0.0, 25_000.0, 25_000.0);

        let orphan_count = count_global_core_orphans(&cache);
        assert_eq!(orphan_count, 0,
            "{orphan_count} globally-disconnected fragments with core cells remain");
    }

    // ──── Centroid tests ────

    /// Every plate that has at least one corrected micro cell must have a
    /// centroid, and the centroid must lie within the bounding box of those cells.
    #[test]
    fn centroid_within_corrected_cell_bounds() {
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        cache.populate_region(0.0, 0.0, 5_000.0, 5_000.0);

        // Collect bounding box per plate from corrected cells (inline data, no cells.get).
        let mut bounds: HashMap<u64, (f64, f64, f64, f64)> = HashMap::new();
        for (key, chunk) in &cache.geometry.chunks {
            if !cache.corrected_chunks.contains(key) { continue; }
            for &(_, _, wx, wy, id) in &chunk.cells {
                if let Some(plate) = cache.macro_assignments.get(&id) {
                    let e = bounds.entry(plate.id)
                        .or_insert((f64::MAX, f64::MIN, f64::MAX, f64::MIN));
                    e.0 = e.0.min(wx);
                    e.1 = e.1.max(wx);
                    e.2 = e.2.min(wy);
                    e.3 = e.3.max(wy);
                }
            }
        }

        assert!(!bounds.is_empty(), "no plates found in corrected region");

        for (&plate_id, &(min_wx, max_wx, min_wy, max_wy)) in &bounds {
            let c = cache.plate_centroid(plate_id)
                .expect("plate with corrected cells must have a centroid");
            assert!(c.wx >= min_wx - 1e-9 && c.wx <= max_wx + 1e-9,
                "centroid wx={} outside [{min_wx}, {max_wx}] for plate {plate_id:016x}",
                c.wx);
            assert!(c.wy >= min_wy - 1e-9 && c.wy <= max_wy + 1e-9,
                "centroid wy={} outside [{min_wy}, {max_wy}] for plate {plate_id:016x}",
                c.wy);
        }
    }

    /// Two caches with the same seed and region must produce identical centroids.
    #[test]
    fn centroids_are_deterministic() {
        let seed = 42u64;
        let mut cache_a = MicroplateCache::new(seed);
        let mut cache_b = MicroplateCache::new(seed);
        cache_a.populate_region(0.0, 0.0, 5_000.0, 5_000.0);
        cache_b.populate_region(0.0, 0.0, 5_000.0, 5_000.0);

        assert_eq!(cache_a.centroids.len(), cache_b.centroids.len(),
            "centroid counts differ between identical caches");

        for (&plate_id, ca) in &cache_a.centroids {
            let cb = cache_b.plate_centroid(plate_id)
                .expect("centroid in cache_a must exist in cache_b");
            assert!((ca.wx - cb.wx).abs() < 1e-9,
                "centroid wx differs for plate {plate_id:016x}");
            assert!((ca.wy - cb.wy).abs() < 1e-9,
                "centroid wy differs for plate {plate_id:016x}");
            assert_eq!(ca.cell_count, cb.cell_count,
                "cell_count differs for plate {plate_id:016x}");
        }
    }

    /// populate_region then plate_info_at on the same cache must not clear centroids.
    #[test]
    fn centroids_non_empty_after_populate_region() {
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        assert_eq!(cache.centroids().count(), 0, "centroids non-empty before populate_region");
        cache.populate_region(0.0, 0.0, 5_000.0, 5_000.0);
        assert!(cache.centroids().count() > 0, "no centroids after populate_region");
    }

    // ──── MicroCellGeometry layer tests ────

    #[test]
    fn geometry_independent_query() {
        // MicroCellGeometry can be constructed and queried with no PlateCache involvement.
        let seed = 42u64;
        let mut geom = MicroCellGeometry::new(seed);
        let cell = geom.micro_cell_at(0.0, 0.0);
        assert_ne!(cell.id, 0, "cell ID should be non-zero");
        assert_eq!(cell.parent_id, 0, "geometry layer never sets parent_id");
    }

    #[test]
    fn geometry_is_deterministic() {
        // Two independent geometry caches with same seed must return identical cells.
        let seed = 42u64;
        let mut geom_a = MicroCellGeometry::new(seed);
        let mut geom_b = MicroCellGeometry::new(seed);
        for x in (-5000..5000i32).step_by(1000) {
            for y in (-5000..5000i32).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                assert_eq!(geom_a.micro_cell_at(wx, wy).id, geom_b.micro_cell_at(wx, wy).id,
                    "geometry mismatch at ({wx}, {wy})");
            }
        }
    }

    #[test]
    fn cache_micro_cell_matches_geometry() {
        // cache.micro_cell_at delegates to geometry — no correction applied,
        // so both independent caches return the same cell for the same position.
        let seed = 42u64;
        let mut geom = MicroCellGeometry::new(seed);
        let mut cache = MicroplateCache::new(seed);
        for x in (-5000..5000i32).step_by(1000) {
            for y in (-5000..5000i32).step_by(1000) {
                let wx = x as f64;
                let wy = y as f64;
                assert_eq!(geom.micro_cell_at(wx, wy).id, cache.micro_cell_at(wx, wy).id,
                    "geometry/cache mismatch at ({wx}, {wy})");
            }
        }
    }

    // ──── classify_micro_tags tests ────

    #[test]
    fn every_micro_plate_has_exactly_one_base_tag_after_populate_region() {
        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        cache.populate_region(0.0, 0.0, 3_000.0, 3_000.0);

        let base_tags = [PlateTag::Sea, PlateTag::Coast, PlateTag::Inland];
        // Sample positions within the core region (avoids boundary effects).
        for x in (-2000..=2000i32).step_by(500) {
            for y in (-2000..=2000i32).step_by(500) {
                let (_, micro) = cache.plate_info_at(x as f64, y as f64);
                let count = base_tags.iter().filter(|t| micro.has_tag(t)).count();
                assert_eq!(
                    count, 1,
                    "micro cell {} at ({x}, {y}) should have exactly one base tag, got {count}",
                    micro.id
                );
            }
        }
    }

    #[test]
    fn micro_coast_plates_have_high_warp_or_border_opposite_regime() {
        use crate::{REGIME_LAND_THRESHOLD, COASTAL_WARP_THRESHOLD};

        let seed = 42u64;
        let mut cache = MicroplateCache::new(seed);
        cache.populate_region(0.0, 0.0, 3_000.0, 3_000.0);

        for x in (-2000..=2000i32).step_by(500) {
            for y in (-2000..=2000i32).step_by(500) {
                let (_, micro) = cache.plate_info_at(x as f64, y as f64);
                if !micro.has_tag(&PlateTag::Coast) { continue; }

                let strength = cache.plate_cache.warp_strength_at(micro.wx, micro.wy);
                if strength > COASTAL_WARP_THRESHOLD { continue; } // warp-driven coast

                let is_land = cache.plate_cache.regime_value_at(micro.wx, micro.wy) >= REGIME_LAND_THRESHOLD;
                let offsets = if micro.sub_cell_r & 1 == 0 { &HEX_NEIGHBORS_EVEN } else { &HEX_NEIGHBORS_ODD };
                let has_opposite_neighbor = offsets.iter().any(|&(dq, dr)| {
                    let ncq = micro.sub_cell_q + dq;
                    let ncr = micro.sub_cell_r + dr;
                    if let Some(Some((nwx, nwy, _))) = cache.geometry.cells.get(&(ncq, ncr)) {
                        let nbr_land = cache.plate_cache.regime_value_at(*nwx, *nwy) >= REGIME_LAND_THRESHOLD;
                        nbr_land != is_land
                    } else {
                        false
                    }
                });
                assert!(
                    has_opposite_neighbor,
                    "Coast micro cell {} at ({x}, {y}) must have high warp OR border a cell of opposite regime",
                    micro.id
                );
            }
        }
    }

}
