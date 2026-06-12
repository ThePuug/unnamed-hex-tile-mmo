//! SpineEvent — Event #1: continental spine elevation and tags.
//!
//! Scale = SPINE_INFLUENCE (15,225 tiles). Cells contain the full influence
//! radius of any spine epicenter within them. Query checks the cell + 1 neighbor
//! ring in the SpineInstanceIndex — no wider search needed.
//!
//! Deform: reads PlateCentroidIndex for qualifying epicenters (survey-driven,
//! spaced by min_spacing), generates spine instances, registers SpineInstanceIndex.
//! Query: evaluates a single tile's elevation + tag from indexed instances.

use std::collections::HashMap;
use std::sync::Arc;

use common::{HexLattice, PlateTag};

use crate::hex_to_world;
use crate::plates::PlateCache;
use crate::spine::{
    SpineInstance, SPINE_INFLUENCE,
    grow_spine, spine_tag_priority,
};
use super::index::{CellId, EventIndex, IndexRegistry};
use super::plates::PlateCentroidIndex;
use super::{Survey, TileOutput, TileView, WorldEvent};

/// Cell radius in tiles = SPINE_INFLUENCE. A cell contains the full influence
/// extent of any epicenter within it. Query searches cell + 1 neighbor.
const SPINE_CELL_SCALE: u32 = SPINE_INFLUENCE as u32;

/// Minimum hex distance between spine epicenters.
/// SPINE_EXCLUSION_DIST (10,000 world units) ≈ 10,000 hex tiles (1 tile ≈ 1 world unit).
const SPINE_EXCLUSION_TILES: u32 = 10_000;

// ── SpineInstanceIndex ──────────────────────────────────────────────────────

/// Index of generated spine instances, keyed by framework cell ID.
/// Populated by SpineEvent::deform, read by SpineEvent::query.
pub struct SpineInstanceIndex {
    pub cells: HashMap<CellId, Vec<SpineInstance>>,
}

impl Default for SpineInstanceIndex {
    fn default() -> Self { Self { cells: HashMap::new() } }
}

impl SpineInstanceIndex {
    pub fn instances_in(&self, cell_ids: &[CellId]) -> Vec<&SpineInstance> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter())
            .collect()
    }
}

impl EventIndex for SpineInstanceIndex {
    fn source_scale(&self) -> u32 { SPINE_CELL_SCALE }

    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        // Return epicenter positions (as hex tiles)
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|insts| insts.iter().map(|i| {
                crate::world_to_hex(i.bounding_center.0, i.bounding_center.1)
            }))
            .collect()
    }

    fn neighbors(&self, _q: i32, _r: i32) -> Vec<(i32, i32)> { vec![] }

    fn remove_cell(&mut self, cell_id: CellId) {
        self.cells.remove(&cell_id);
    }
}

// ── SpineEvent ──────────────────────────────────────────────────────────────

pub struct SpineEvent {
    plate_cache: Arc<PlateCache>,
    seed: u64,
    /// Cell lattice at SPINE_CELL_SCALE — hoisted out of the per-tile query.
    lattice: HexLattice,
}

impl SpineEvent {
    pub fn new(seed: u64) -> Self {
        Self::with_cache(Arc::new(PlateCache::new(seed)), seed)
    }

    pub fn with_cache(plate_cache: Arc<PlateCache>, seed: u64) -> Self {
        Self { plate_cache, seed, lattice: HexLattice::new(SPINE_CELL_SCALE) }
    }
}

impl WorldEvent for SpineEvent {
    fn name(&self) -> &str { "spines" }
    fn scale(&self) -> u32 { SPINE_CELL_SCALE }
    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<SpineInstanceIndex>();
    }

    fn survey(&self) -> Survey {
        Survey::from_index::<PlateCentroidIndex>()
            .all_neighbors_in::<PlateCentroidIndex>(
                |tile| tile.tags.has(PlateTag::Inland),
                1,
            )
            .filter(|tile, _seed| tile.tags.has(PlateTag::Inland))
            .min_spacing(SPINE_EXCLUSION_TILES)
    }

    fn deform(
        &self,
        cell_id: CellId,
        matched: &[(i32, i32)],
        indexes: &IndexRegistry,
        _seed: u64,
    ) {
        // Collect centroid data under read lock, then drop it before write lock.
        let centroid_data: Vec<(f64, f64, u64)> = {
            let centroid_index = match indexes.get::<PlateCentroidIndex>() {
                Some(idx) => idx,
                None => {
                    indexes.get_or_create::<SpineInstanceIndex>()
                        .cells.insert(cell_id, Vec::new());
                    return;
                }
            };
            matched.iter().filter_map(|&(q, r)| {
                centroid_index.cells.values()
                    .flat_map(|entries| entries.iter())
                    .find(|e| e.q == q && e.r == r)
                    .map(|e| (e.wx, e.wy, e.plate_id))
            }).collect()
        }; // read guard dropped

        let empty_plates: Vec<crate::PlateCenter> = Vec::new();
        let empty_map: HashMap<u64, usize> = HashMap::new();

        let mut instances: Vec<SpineInstance> = Vec::new();
        for (wx, wy, plate_id) in centroid_data {
            let inst = grow_spine(
                wx, wy, plate_id,
                &mut empty_plates.clone(), &empty_map,
                &self.plate_cache, self.seed,
            );
            if !inst.peaks.is_empty() {
                instances.push(inst);
            }
        }

        // Brief write lock for the insert
        indexes.get_or_create::<SpineInstanceIndex>()
            .cells.insert(cell_id, instances);
    }

    fn query(
        &self,
        q: i32, r: i32,
        cell_id: CellId,
        indexes: &IndexRegistry,
        _below: &dyn Fn(i32, i32) -> TileView,
        _seed: u64,
    ) -> Option<TileOutput> {
        let spine_index = indexes.get::<SpineInstanceIndex>()?;
        let (wx, wy) = hex_to_world(q, r);

        // Search this cell + 1 neighbor ring for instances
        let nearby_cells = self.lattice.cells_within_distance(cell_id, 1);
        let instances = spine_index.instances_in(&nearby_cells);

        let mut max_elev = 0.0f64;
        let mut best_tag: Option<PlateTag> = None;

        for inst in &instances {
            // Single pass per instance: elevation + tag share one peak scan.
            let (e, tag) = inst.sample_at(wx, wy);
            if e > max_elev { max_elev = e; }

            if let Some(tag) = tag {
                let dominated = best_tag.as_ref()
                    .map_or(true, |b| spine_tag_priority(&tag) > spine_tag_priority(b));
                if dominated { best_tag = Some(tag); }
            }
        }

        if max_elev <= 0.0 { return None; }

        let mut out = TileOutput::default();
        out.elevation_delta = max_elev;
        if let Some(t) = best_tag {
            out.tags_added.add(t);
        }
        Some(out)
    }
}

