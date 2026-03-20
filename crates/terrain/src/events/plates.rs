//! PlateEvent — Event #0: macro plate classification + centroid index.
//!
//! Deform: discovers plate centroids at plate granularity (not per-tile),
//! classifies them, registers in PlateCentroidIndex with tag metadata.
//! Query: resolves a single tile's plate classification via warped Voronoi.

use std::collections::HashMap;
use std::sync::Mutex;

use common::{HexLattice, PlateTag, TagSet};

use crate::hex_to_world;
use crate::plates::PlateCache;
use crate::world_to_hex;
use super::index::{CellId, EventIndex, IndexRegistry};
use super::{Survey, TileOutput, TileView, WorldEvent};

const PLATE_CELL_SCALE: u32 = 128;

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
            TileView { q, r, wx, wy, tags, elevation: 0.0 }
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
    plate_cache: Mutex<PlateCache>,
}

impl PlateEvent {
    pub fn new(seed: u64) -> Self {
        Self { plate_cache: Mutex::new(PlateCache::new(seed)) }
    }
}

impl WorldEvent for PlateEvent {
    fn name(&self) -> &str { "plates" }
    fn scale(&self) -> u32 { PLATE_CELL_SCALE }
    fn survey(&self) -> Survey { Survey::all() }

    fn deform(
        &self,
        cell_id: CellId,
        _matched: &[(i32, i32)],
        indexes: &mut IndexRegistry,
        _seed: u64,
    ) {
        let mut plate_cache = self.plate_cache.lock().unwrap();
        let lattice = HexLattice::new(self.scale());
        let (center_q, center_r) = lattice.cell_center(cell_id);
        let (center_wx, center_wy) = hex_to_world(center_q, center_r);

        // Discover plates at centroid granularity (not per-tile).
        // Radius in world units: hex ball radius in tiles × ~1.5 world units/tile + margin.
        let cell_world_radius = self.scale() as f64 * 1.5 + crate::MACRO_CELL_SIZE;
        let mut plates = plate_cache.plates_in_radius(center_wx, center_wy, cell_world_radius);
        plate_cache.classify_tags(&mut plates);

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

            let nbrs = plate_cache.plate_neighbors(plate.wx, plate.wy);
            let nbr_coords: Vec<(i32, i32)> = nbrs.iter()
                .map(|n| world_to_hex(n.wx, n.wy))
                .collect();
            neighbor_edges.push(((pq, pr), nbr_coords));
            tags_at_entries.push(((pq, pr), tag_set));
        }

        let centroid_index = indexes.get_or_create::<PlateCentroidIndex>();
        centroid_index.cells.insert(cell_id, centroids);
        for ((cq, cr), nbrs) in neighbor_edges {
            centroid_index.neighbor_graph.insert((cq, cr), nbrs);
        }
        for ((q, r), tags) in tags_at_entries {
            centroid_index.tags_at.insert((q, r), tags);
        }
    }

    fn query(
        &self,
        q: i32, r: i32,
        _cell_id: CellId,
        _indexes: &IndexRegistry,
        _below: &dyn Fn(i32, i32) -> TileView,
        _seed: u64,
    ) -> Option<TileOutput> {
        let mut plate_cache = self.plate_cache.lock().unwrap();
        let (wx, wy) = hex_to_world(q, r);
        let mut plate = plate_cache.warped_plate_at(wx, wy);
        plate_cache.classify_tags(std::slice::from_mut(&mut plate));
        let tag = plate.tags.first().copied().unwrap_or(PlateTag::Sea);

        let mut out = TileOutput::default();
        out.tags_added.add(tag);
        Some(out)
    }
}
