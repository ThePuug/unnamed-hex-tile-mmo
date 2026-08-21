//! PlateEvent — Event #0: macro plate centroids, and the crustal substrate
//! every layer above stands on.

//! Deform: discovers plate centroids at plate granularity (not per-tile) and
//! registers them in PlateCentroidIndex with the substrate elevation each one
//! stands at.
//! Query: places the substrate — the elevation the regime field gives this
//! position, above or below the sea-level datum.

use std::collections::HashMap;
use std::sync::Arc;

use common::{HexLattice, TagSet};

use crate::hex_to_world;
use crate::plates::{PlateCache, substrate_from_raw, raw_regime_noise};
use crate::world_to_hex;
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::{CellScope, TileOutput, TileView, WorldEvent};

const PLATE_CELL_SCALE: u32 = 1800;

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
    /// Substrate elevation at the centroid. Above 0 is continental crust.
    pub elevation: f64,
}

/// Index of macro plate centroids and their Voronoi neighbor graph.
/// Populated by PlateEvent, queried by SpineEvent.
pub struct PlateCentroidIndex {
    pub cells: HashMap<CellId, Vec<CentroidEntry>>,
    pub neighbor_graph: HashMap<(i32, i32), Vec<(i32, i32)>>,
    /// Fast (q, r) → substrate elevation lookup for tile_view_at.
    elevation_at: HashMap<(i32, i32), f64>,
}

impl Default for PlateCentroidIndex {
    fn default() -> Self {
        Self { cells: HashMap::new(), neighbor_graph: HashMap::new(), elevation_at: HashMap::new() }
    }
}

/// What one cell contributes: its own centroids, and the graph and elevation
/// entries for those centroids. All three are keyed to ground inside the cell, so they
/// travel together and are written together.
pub struct PlateCentroidCell {
    pub centroids: Vec<CentroidEntry>,
    pub neighbor_edges: Vec<((i32, i32), Vec<(i32, i32)>)>,
    pub elevation_at: Vec<((i32, i32), f64)>,
}

impl CellIndex for PlateCentroidIndex {
    type Cell = PlateCentroidCell;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry.centroids);
        for (at, nbrs) in entry.neighbor_edges {
            self.neighbor_graph.insert(at, nbrs);
        }
        for (at, elevation) in entry.elevation_at {
            self.elevation_at.insert(at, elevation);
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
        self.elevation_at.get(&(q, r)).map(|&elevation| {
            let (wx, wy) = hex_to_world(q, r);
            TileView { q, r, wx, wy, tags: TagSet::new(), elevation, curvature: 0.0 }
        })
    }

    fn remove_cell(&mut self, cell_id: CellId) {
        if let Some(entries) = self.cells.remove(&cell_id) {
            for entry in &entries {
                self.neighbor_graph.remove(&(entry.q, entry.r));
                self.elevation_at.remove(&(entry.q, entry.r));
            }
        }
    }
}

// ── PlateEvent ──────────────────────────────────────────────────────────────

pub struct PlateEvent {
    plate_cache: Arc<PlateCache>,
}

impl PlateEvent {
    pub fn new(seed: u64) -> Self {
        Self::with_cache(Arc::new(PlateCache::new(seed)))
    }

    pub fn with_cache(plate_cache: Arc<PlateCache>) -> Self {
        Self { plate_cache }
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
        let plates = self.plate_cache.plates_in_radius(center_wx, center_wy, cell_world_radius);

        let mut centroids: Vec<CentroidEntry> = Vec::new();
        let mut neighbor_edges: Vec<((i32, i32), Vec<(i32, i32)>)> = Vec::new();
        let mut elevation_at_entries: Vec<((i32, i32), f64)> = Vec::new();

        for plate in &plates {
            let (pq, pr) = world_to_hex(plate.wx, plate.wy);
            // Only register centroids whose position falls in this cell
            if lattice.cell_id(pq, pr) != cell_id { continue; }

            let elevation = self.plate_cache.plate_elevation(plate);
            centroids.push(CentroidEntry {
                q: pq, r: pr,
                wx: plate.wx, wy: plate.wy,
                plate_id: plate.id,
                cell_q: plate.cell_q, cell_r: plate.cell_r,
                elevation,
            });

            let nbrs = self.plate_cache.plate_neighbors(plate.wx, plate.wy);
            let nbr_coords: Vec<(i32, i32)> = nbrs.iter()
                .map(|n| world_to_hex(n.wx, n.wy))
                .collect();
            neighbor_edges.push(((pq, pr), nbr_coords));
            elevation_at_entries.push(((pq, pr), elevation));
        }

        scope.publish::<PlateCentroidIndex>(PlateCentroidCell {
            centroids,
            neighbor_edges,
            elevation_at: elevation_at_entries,
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

        // The substrate lives here because sea level has no structure: it is a
        // pure function of (position, seed), so the layer that would carry it
        // could never read another cell and its correct margin is zero. The
        // framework derives margin from cell scales, not from what a layer
        // reads, so a layer of its own costs a full cascade for a function
        // call. If sea level ever becomes dynamic — glacial eustasy, or a base
        // level driven by drainage — it has real structure and earns its layer
        // back.
        //
        // No tag accompanies it. Whether this tile is land is not a
        // classification any layer stores; it is `elevation >= 0.0`, read off
        // the composite by whoever needs to know.
        Some(TileOutput {
            elevation_delta: substrate_from_raw(raw_regime_noise(wx, wy, seed)),
            ..TileOutput::default()
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CONTINENT_MAX_RISE, SEA_MAX_DEPTH, SHORE_RAW};

    const SEED: u64 = 0x9E3779B97F4A7C15;

    fn event() -> PlateEvent {
        PlateEvent::new(SEED)
    }

    fn sample(mut f: impl FnMut(i32, i32, f64, f64)) {
        for i in 0..200 {
            for j in 0..200 {
                let (q, r) = (i * 150 - 15000, j * 150 - 15000);
                let (wx, wy) = hex_to_world(q, r);
                f(q, r, wx, wy);
            }
        }
    }

    /// The datum is the only thing that decides land, and the curve agrees
    /// with it on both sides. Nothing else may.
    #[test]
    fn substrate_sign_follows_the_datum() {
        let mut land = 0;
        let mut sea = 0;
        sample(|_, _, wx, wy| {
            let raw = raw_regime_noise(wx, wy, SEED);
            let e = crate::substrate_elevation_at(wx, wy, SEED);
            if raw >= SHORE_RAW {
                assert!(e >= 0.0, "raw {raw} above the datum gave elevation {e}");
                land += 1;
            } else {
                assert!(e < 0.0, "raw {raw} below the datum gave elevation {e}");
                sea += 1;
            }
        });
        assert!(land > 1000 && sea > 1000, "unbalanced sample: {land} land, {sea} sea");
    }

    /// Both branches stay inside the range their constants declare, and both
    /// actually reach for it — a branch that never approaches its extreme is a
    /// branch whose scale constant is not doing anything.
    #[test]
    fn substrate_is_bounded_and_uses_its_range() {
        let mut deepest: f64 = 0.0;
        let mut highest: f64 = 0.0;
        sample(|_, _, wx, wy| {
            let e = crate::substrate_elevation_at(wx, wy, SEED);
            assert!(
                (-SEA_MAX_DEPTH..=CONTINENT_MAX_RISE).contains(&e),
                "elevation {e} out of range at ({wx}, {wy})"
            );
            deepest = deepest.min(e);
            highest = highest.max(e);
        });
        assert!(deepest < -SEA_MAX_DEPTH * 0.9, "open ocean stayed shallow: {deepest}");
        assert!(highest > CONTINENT_MAX_RISE * 0.7, "interiors stayed low: {highest}");
    }

    /// The two branches are one curve, and what that has to buy is a coastline
    /// a player can walk off. Continuity in the abstract is not the claim — the
    /// claim is that adjacent tiles across a shoreline sit a step apart rather
    /// than a ledge, and that neither side is a flat band, which would be the
    /// land mask rebuilt out of the curve.
    #[test]
    fn shoreline_is_a_step_not_a_ledge() {
        let mut steps: Vec<f64> = Vec::new();
        for r in (-6000..6000).step_by(37) {
            let mut prev: Option<f64> = None;
            for q in -400..400 {
                let (wx, wy) = hex_to_world(q, r);
                let e = crate::substrate_elevation_at(wx, wy, SEED);
                if let Some(p) = prev {
                    // Only the band either side of the datum is under test.
                    if p.abs() < 8.0 || e.abs() < 8.0 {
                        steps.push((e - p).abs());
                    }
                }
                prev = Some(e);
            }
        }
        assert!(steps.len() > 1000, "too few near-shore tiles sampled: {}", steps.len());
        steps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = steps[steps.len() / 2];
        let p999 = steps[steps.len() * 999 / 1000];
        let max = steps[steps.len() - 1];
        assert!(median < 1.0, "median coastal step {median} z — that is a staircase");
        assert!(p999 < 4.0, "p99.9 coastal step {p999} z reads as a ledge");
        assert!(max < 8.0, "worst coastal step {max} z is a cliff");
        // Not flat either: the shore band has to actually climb.
        assert!(steps.iter().any(|&s| s > 0.1),
            "no tile pair near the shore moved — that band is a mask");
    }

    /// The substrate is monotone in the field it reads: a position with more
    /// crust under it is never lower than one with less.
    #[test]
    fn substrate_is_monotone_in_raw() {
        let mut prev = f64::NEG_INFINITY;
        for i in 0..=1000 {
            let raw = i as f64 / 1000.0 * 1.15;
            let e = substrate_from_raw(raw);
            assert!(e >= prev, "substrate fell from {prev} to {e} at raw {raw}");
            prev = e;
        }
    }

    /// PlateEvent emits elevation and nothing else. A tag here would be a land
    /// mask by another name.
    #[test]
    fn query_emits_no_tags() {
        let e = event();
        let mut checked = 0;
        sample(|q, r, wx, wy| {
            let view = TileView { q, r, wx, wy, tags: TagSet::new(), elevation: 0.0, curvature: 0.0 };
            let out = e.query(q, r, &view, &(), SEED).unwrap();
            assert!(out.tags_added.is_empty(), "plates emitted tag(s) at ({q}, {r})");
            assert!(out.tags_removed.is_empty(), "plates removed tag(s) at ({q}, {r})");
            assert_eq!(out.elevation_delta, crate::substrate_elevation_at(wx, wy, SEED));
            checked += 1;
        });
        assert!(checked > 1000);
    }
}
