//! PlateEvent — Event #0: macro plate classification, centroid index, and the
//! depth of everything the regime field puts below sea level.

//! Deform: discovers plate centroids at plate granularity (not per-tile),
//! classifies them, registers in PlateCentroidIndex with tag metadata.
//! Query: resolves a single tile's plate classification via warped Voronoi, and
//! its depth if it is under water.

use std::collections::HashMap;
use std::sync::Arc;

use common::{HexLattice, PlateTag, TagSet};

use crate::hex_to_world;
use crate::plates::{PlateCache, inverse_sigmoid, raw_regime_noise};
use crate::world_to_hex;
use crate::{REGIME_LAND_THRESHOLD, REGIME_SIGMOID_MIDPOINT, REGIME_SIGMOID_STEEPNESS};
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const PLATE_CELL_SCALE: u32 = 1800;

/// Depth in z-levels at the abyssal plain (raw regime ≈ 0). At `RISE` 0.8 this
/// is 160 world units below sea level, matching the deepest stop on the
/// terrain shader's elevation ramp.
pub const SEA_MAX_DEPTH: f64 = 200.0;

/// Shelf profile exponent, applied to the normalised shore→abyss fraction.
/// Greater than 1 holds the near-shore band shallow so the coastline is a
/// wadeable beach instead of a drop-off.
const SHELF_EXPONENT: f64 = 2.0;

// ── PlateCentroidIndex ──────────────────────────────────────────────────────

/// Centroid entry registered by PlateEvent.
pub struct CentroidEntry {
    pub q: i32,
    pub r: i32,
    pub wx: f64,
    pub wy: f64,
    pub plate_id: u64,
    pub cell_q: i32,
    pub cell_r: i32,
    pub tags: TagSet,
}

/// Index of macro plate centroids and their Voronoi neighbor graph.
/// Populated by PlateEvent, queried by SpineEvent.
pub struct PlateCentroidIndex {
    pub cells: HashMap<CellId, Vec<CentroidEntry>>,
    pub neighbor_graph: HashMap<(i32, i32), Vec<(i32, i32)>>,
    /// Fast (q, r) → TagSet lookup for tile_view_at.
    tags_at: HashMap<(i32, i32), TagSet>,
}

impl Default for PlateCentroidIndex {
    fn default() -> Self {
        Self { cells: HashMap::new(), neighbor_graph: HashMap::new(), tags_at: HashMap::new() }
    }
}

/// What one cell contributes: its own centroids, and the graph and tag entries
/// for those centroids. All three are keyed to ground inside the cell, so they
/// travel together and are written together.
pub struct PlateCentroidCell {
    pub centroids: Vec<CentroidEntry>,
    pub neighbor_edges: Vec<((i32, i32), Vec<(i32, i32)>)>,
    pub tags_at: Vec<((i32, i32), TagSet)>,
}

impl CellIndex for PlateCentroidIndex {
    type Cell = PlateCentroidCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry.centroids);
        for (at, nbrs) in entry.neighbor_edges {
            self.neighbor_graph.insert(at, nbrs);
        }
        for (at, tags) in entry.tags_at {
            self.tags_at.insert(at, tags);
        }
    }
}

impl EventIndex for PlateCentroidIndex {
    fn source_scale(&self) -> u32 { PLATE_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|entries| entries.iter().map(|e| (e.q, e.r)))
            .collect()
    }

    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)> {
        self.neighbor_graph.get(&(q, r)).cloned().unwrap_or_default()
    }

    fn tile_view_at(&self, q: i32, r: i32) -> Option<TileView> {
        self.tags_at.get(&(q, r)).map(|&tags| {
            let (wx, wy) = hex_to_world(q, r);
            TileView { q, r, wx, wy, tags, elevation: 0.0, curvature: 0.0 }
        })
    }

    fn remove_cell(&mut self, cell_id: CellId) {
        if let Some(entries) = self.cells.remove(&cell_id) {
            for entry in &entries {
                self.neighbor_graph.remove(&(entry.q, entry.r));
                self.tags_at.remove(&(entry.q, entry.r));
            }
        }
    }
}

// ── PlateEvent ──────────────────────────────────────────────────────────────

pub struct PlateEvent {
    plate_cache: Arc<PlateCache>,
    /// Raw (pre-sigmoid) regime value at the shoreline — the `x` where
    /// `sigmoid(x) == REGIME_LAND_THRESHOLD`. Derived from the sigmoid
    /// constants rather than tuned, so retuning the regime field moves the
    /// shoreline here automatically.
    shore_raw: f64,
}

impl PlateEvent {
    pub fn new(seed: u64) -> Self {
        Self::with_cache(Arc::new(PlateCache::new(seed)))
    }

    pub fn with_cache(plate_cache: Arc<PlateCache>) -> Self {
        Self {
            plate_cache,
            shore_raw: inverse_sigmoid(
                REGIME_LAND_THRESHOLD,
                REGIME_SIGMOID_MIDPOINT,
                REGIME_SIGMOID_STEEPNESS,
            ),
        }
    }

    /// Raw regime value at the shoreline. Exposed for probes and tests.
    pub fn shore_raw(&self) -> f64 { self.shore_raw }

    /// Depth in z-levels below sea level at a world position. Zero on land.

    /// Driven by the **pre-sigmoid** regime field. `regime_value_at` applies a
    /// steepness-40 sigmoid that deliberately flattens deep water, and it does
    /// its job too well to reuse here: 93.7% of water tiles land within 17% of
    /// the sigmoid floor, and the median water tile reads 0.0000.
    /// `raw_regime_noise` still grades smoothly — measured continental shelf is
    /// ~230 tiles from shore to a quarter of the raw range, which is a wide
    /// beach at player scale.
    pub fn depth_at(&self, wx: f64, wy: f64, seed: u64) -> f64 {
        Self::depth_from_raw(raw_regime_noise(wx, wy, seed), self.shore_raw)
    }

    /// Depth from an already-evaluated raw regime value, so a caller holding
    /// one does not evaluate the noise field twice for the same tile.
    fn depth_from_raw(raw: f64, shore_raw: f64) -> f64 {
        if raw >= shore_raw {
            return 0.0;
        }
        // 0 at the shoreline, approaching 1 in open ocean.
        let frac = (1.0 - raw / shore_raw).clamp(0.0, 1.0);
        SEA_MAX_DEPTH * frac.powf(SHELF_EXPONENT)
    }
}

impl WorldEvent for PlateEvent {
    fn name(&self) -> &str { "plates" }
    fn scale(&self) -> u32 { PLATE_CELL_SCALE }
    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<PlateCentroidIndex>();
    }

    fn deform(&self, scope: &CellScope) {
        let cell_id = scope.cell();
        let lattice = HexLattice::new(self.scale());
        let (center_q, center_r) = lattice.cell_center(cell_id);
        let (center_wx, center_wy) = hex_to_world(center_q, center_r);

        let cell_world_radius = self.scale() as f64 * 1.5 + crate::MACRO_CELL_SIZE;
        let mut plates = self.plate_cache.plates_in_radius(center_wx, center_wy, cell_world_radius);
        self.plate_cache.classify_tags(&mut plates);

        let mut centroids: Vec<CentroidEntry> = Vec::new();
        let mut neighbor_edges: Vec<((i32, i32), Vec<(i32, i32)>)> = Vec::new();
        let mut tags_at_entries: Vec<((i32, i32), TagSet)> = Vec::new();

        for plate in &plates {
            let (pq, pr) = world_to_hex(plate.wx, plate.wy);
            // Only register centroids whose position falls in this cell
            if lattice.cell_id(pq, pr) != cell_id { continue; }

            let tag = plate.tags.first().copied().unwrap_or(PlateTag::Sea);
            let tag_set = TagSet::from(tag);
            centroids.push(CentroidEntry {
                q: pq, r: pr,
                wx: plate.wx, wy: plate.wy,
                plate_id: plate.id,
                cell_q: plate.cell_q, cell_r: plate.cell_r,
                tags: tag_set,
            });

            let nbrs = self.plate_cache.plate_neighbors(plate.wx, plate.wy);
            let nbr_coords: Vec<(i32, i32)> = nbrs.iter()
                .map(|n| world_to_hex(n.wx, n.wy))
                .collect();
            neighbor_edges.push(((pq, pr), nbr_coords));
            tags_at_entries.push(((pq, pr), tag_set));
        }

        scope.publish::<PlateCentroidIndex>(PlateCentroidCell {
            centroids,
            neighbor_edges,
            tags_at: tags_at_entries,
        });
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        _cell: &(dyn std::any::Any + Send + Sync),
        seed: u64,
    ) -> Option<TileOutput> {
        let (wx, wy) = hex_to_world(q, r);
        let mut plate = self.plate_cache.warped_plate_at(wx, wy);
        self.plate_cache.classify_tags(std::slice::from_mut(&mut plate));
        let tag = plate.tags.first().copied().unwrap_or(PlateTag::Sea);

        let mut out = TileOutput::default();

        // Bathymetry lives here because sea level has no structure: depth is a
        // pure function of (position, seed), so the layer that would carry it
        // could never read another cell and its correct margin is zero. The
        // framework derives margin from cell scales, not from what a layer
        // reads, so a layer of its own costs a full cascade for a function
        // call. If sea level ever becomes dynamic — glacial eustasy, or a base
        // level driven by drainage — it has real structure and earns its layer
        // back.
        let depth = Self::depth_from_raw(raw_regime_noise(wx, wy, seed), self.shore_raw);
        if depth > 0.0 {
            out.elevation_delta = -depth;
            out.tags_added.add(PlateTag::Sea);
            // `Inland` is a verdict about a whole plate, taken at its centroid,
            // and it disagrees with the tile's own regime value ~4% of the
            // time. A submerged tile is not interior land whatever its plate
            // says — so the tag is never emitted, rather than emitted and
            // taken back by a layer above. `Coast` still stands: a submerged
            // tile on a coastal plate is beach, not open ocean.
            if tag != PlateTag::Inland {
                out.tags_added.add(tag);
            }
        } else {
            out.tags_added.add(tag);
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regime_value_at;

    const SEED: u64 = 0x9E3779B97F4A7C15;

    fn event() -> PlateEvent {
        PlateEvent::new(SEED)
    }

    #[test]
    fn shore_raw_matches_land_threshold() {
        let e = event();
        let back = crate::plates::sigmoid(
            e.shore_raw(),
            REGIME_SIGMOID_MIDPOINT,
            REGIME_SIGMOID_STEEPNESS,
        );
        assert!(
            (back - REGIME_LAND_THRESHOLD).abs() < 1e-9,
            "inverse_sigmoid round-trip failed: {back} != {REGIME_LAND_THRESHOLD}"
        );
    }

    #[test]
    fn land_is_never_submerged() {
        let e = event();
        let mut checked = 0;
        for i in 0..200 {
            for j in 0..200 {
                let (wx, wy) = hex_to_world(i * 150 - 15000, j * 150 - 15000);
                if regime_value_at(wx, wy, SEED) >= REGIME_LAND_THRESHOLD {
                    assert_eq!(
                        e.depth_at(wx, wy, SEED), 0.0,
                        "land tile at ({wx}, {wy}) was given depth"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "expected a meaningful land sample, got {checked}");
    }

    #[test]
    fn depth_is_bounded_and_deepens_offshore() {
        let e = event();
        let mut deepest: f64 = 0.0;
        for i in 0..200 {
            for j in 0..200 {
                let (wx, wy) = hex_to_world(i * 150 - 15000, j * 150 - 15000);
                let d = e.depth_at(wx, wy, SEED);
                assert!(
                    (0.0..=SEA_MAX_DEPTH).contains(&d),
                    "depth {d} out of range at ({wx}, {wy})"
                );
                deepest = deepest.max(d);
            }
        }
        assert!(
            deepest > SEA_MAX_DEPTH * 0.9,
            "open ocean never approached max depth: {deepest}"
        );
    }

    /// A submerged tile on a plate its centroid called `Inland` reads as `Sea`
    /// and nothing else. The correction used to be an add-then-remove across
    /// two layers; in one layer the tag is simply never emitted, and this is
    /// the case that proves the two are equivalent.
    #[test]
    fn submerged_inland_plate_tile_reads_as_sea() {
        let e = event();
        let mut checked = 0;
        for i in 0..200 {
            for j in 0..200 {
                let (q, r) = (i * 150 - 15000, j * 150 - 15000);
                let (wx, wy) = hex_to_world(q, r);
                if e.depth_at(wx, wy, SEED) <= 0.0 { continue; }
                let mut plate = e.plate_cache.warped_plate_at(wx, wy);
                e.plate_cache.classify_tags(std::slice::from_mut(&mut plate));
                if plate.tags.first().copied() != Some(PlateTag::Inland) { continue; }

                let view = TileView {
                    q, r, wx, wy,
                    tags: TagSet::new(), elevation: 0.0, curvature: 0.0,
                };
                let out = e.query(q, r, &view, &(), SEED).unwrap();
                assert!(out.tags_added.has(PlateTag::Sea));
                assert!(!out.tags_added.has(PlateTag::Inland));
                assert!(out.tags_removed.is_empty(), "tag correction should not be needed");
                checked += 1;
            }
        }
        assert!(checked > 0, "no submerged Inland-plate tiles in the sample");
    }
}
