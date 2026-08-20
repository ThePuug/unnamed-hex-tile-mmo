//! SpineEvent — Event #1: continental spine elevation and tags.

//! Scale = SPINE_INFLUENCE (15,225 tiles). Cells contain the full influence
//! radius of any spine epicenter within them. Query checks the cell + 1 neighbor
//! ring in the SpineInstanceIndex — no wider search needed.

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
use super::index::{CellId, CellIndex, EventIndex, IndexRegistry};
use super::faces::{BasinIndex, ErosionalFaceIndex};
use super::plates::PlateCentroidIndex;
use crate::slope_form;
use super::{CellScope, Survey, TileOutput, TileView, WorldEvent};

/// Cell radius in tiles = SPINE_INFLUENCE. A cell contains the full influence
/// extent of any epicenter within it. Query searches cell + 1 neighbor.
pub const SPINE_CELL_SCALE: u32 = SPINE_INFLUENCE as u32;

/// Minimum hex distance between spine epicenters.
/// SPINE_EXCLUSION_DIST (10,000 world units) ≈ 10,000 hex tiles (1 tile ≈ 1 world unit).
const SPINE_EXCLUSION_TILES: u32 = 10_000;

// ── SpineInstanceIndex ──────────────────────────────────────────────────────

/// Index of generated spine instances, keyed by framework cell ID.
/// Populated by SpineEvent::deform, read by SpineEvent::query.
pub struct SpineInstanceIndex {
    /// Instances are `Arc`d so a reader can take the set it needs and drop the
    /// index lock, instead of holding it for the length of a query.
    pub cells: HashMap<CellId, Vec<Arc<SpineInstance>>>,
}

impl Default for SpineInstanceIndex {
    fn default() -> Self { Self { cells: HashMap::new() } }
}

impl SpineInstanceIndex {
    pub fn instances_in(&self, cell_ids: &[CellId]) -> Vec<Arc<SpineInstance>> {
        cell_ids.iter()
            .filter_map(|id| self.cells.get(id))
            .flat_map(|v| v.iter().cloned())
            .collect()
    }
}

impl CellIndex for SpineInstanceIndex {
    type Cell = Vec<Arc<SpineInstance>>;

    fn set(&mut self, cell: CellId, entry: Self::Cell) {
        self.cells.insert(cell, entry);
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
        Self {
            plate_cache,
            seed,
            lattice: HexLattice::new(SPINE_CELL_SCALE),
        }
    }

}

impl WorldEvent for SpineEvent {
    fn name(&self) -> &str { "spines" }
    fn scale(&self) -> u32 { SPINE_CELL_SCALE }

    /// A cell is one SPINE_INFLUENCE across, so an epicenter sitting near a
    /// cell edge still reaches tiles in the adjacent cell. Query therefore
    /// reads one ring out, and the framework must deform it first.
    fn query_reach(&self) -> u32 { 1 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<SpineInstanceIndex>();
        registry.pre_register::<ErosionalFaceIndex>();
        registry.pre_register::<BasinIndex>();
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

    fn deform(&self, scope: &CellScope, matched: &[(i32, i32)]) {
        // Collect centroid data under read lock, then drop it before write lock.
        let centroid_data: Vec<(f64, f64, u64)> = {
            let centroid_index = match scope.read::<PlateCentroidIndex>() {
                Some(idx) => idx,
                None => {
                    scope.publish::<SpineInstanceIndex>(Vec::new());
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

        let mut instances: Vec<Arc<SpineInstance>> = Vec::new();
        for (wx, wy, plate_id) in centroid_data {
            let inst = grow_spine(
                wx, wy, plate_id,
                &mut empty_plates.clone(), &empty_map,
                &self.plate_cache, self.seed,
            );
            if !inst.peaks.is_empty() {
                instances.push(Arc::new(inst));
            }
        }

        // Instances are keyed by where they were seeded, not by the ground they
        // cover — a spine reaches a full SPINE_INFLUENCE past its own cell,
        // which is what `query_reach` declares so readers gather cell plus ring.
        //
        // The geometry those instances leave — faces, basins — is published in
        // `prepare` instead, keyed by the ground it lies on. Here the ring is
        // not yet guaranteed, so this cell can neither see every spine that
        // reaches its own ground nor settle what it laid on someone else's.
        scope.publish::<SpineInstanceIndex>(instances);
    }

    /// The instances any tile in this cell can be reached by: its own cell plus
    /// the rings `query_reach` declares, which is the set the framework has
    /// deformed by the time this runs.
    ///
    /// This is also the first moment the published faces can be made true. A
    /// carve is only real if it survives compositing, and the spine that cut it
    /// cannot see the neighbouring spine whose mountain buries it — that is a
    /// fold over the ring, and folding it at deform time would make the answer
    /// depend on the order cells were visited in. Here the ring is complete by
    /// contract, so the floors are recomposed against every instance that
    /// stands on the same ground.
    fn prepare(&self, scope: &CellScope) -> Box<dyn std::any::Any + Send + Sync> {
        let cells = scope.lattice().cells_within_distance(scope.cell(), self.query_reach());
        let instances = match scope.read::<SpineInstanceIndex>() {
            Some(idx) => idx.instances_in(&cells),
            None => Vec::new(),
        };

        // Everything below is laid on *this cell's* ground, from every spine
        // that reaches it — including spines seeded in the ring, which is why
        // it cannot happen until here. A neighbouring cell lays the geometry
        // these same spines leave on its own ground, from its own ring, so
        // every feature is published exactly once by whoever owns the ground
        // under it and settled against exactly what acts on it.
        let composed = |wx: f64, wy: f64| {
            instances.iter().fold(0.0f64, |acc, i| acc.max(i.sample_at(wx, wy).0))
        };
        let reach = slope_form::MASS_WASTING_REACH;
        let min_height = crate::ELEVATION_PER_Z / crate::TILE_SPACING * reach;

        let mut faces = crate::faces::FaceIndex::new(reach);
        for inst in &instances {
            inst.each_face(&mut |face: crate::ErosionalFace| {
                // A face just outside the cell still acts on tiles inside it,
                // so the cell keeps anything within reach of its own ground.
                // That is still this cell's entry — the halo describes where a
                // face lies, not which cell may record it.
                if !scope.within_reach(face.wx, face.wy, reach) {
                    return;
                }
                let floor = composed(face.wx, face.wy);
                let top = face.floor + face.height;
                faces.insert(
                    crate::ErosionalFace { floor, height: top - floor, ..face },
                    min_height,
                );
            });
        }
        scope.publish::<ErosionalFaceIndex>(Arc::new(faces));

        // A bowl is an extent, not a point, so it belongs to every cell its
        // footprint touches — at a few hundred tiles against a cell thousands
        // across, that is one cell or two.
        scope.publish::<BasinIndex>(BasinIndex::grid_of(
            instances
                .iter()
                .flat_map(|i| i.cirques.iter())
                .filter(|c| scope.within_reach(c.cx, c.cy, c.radius))
                .cloned(),
        ));

        Box::new(instances)
    }

    fn query(
        &self,
        q: i32, r: i32,
        _below: &TileView,
        cell: &(dyn std::any::Any + Send + Sync),
        _seed: u64,
    ) -> Option<TileOutput> {
        let (wx, wy) = hex_to_world(q, r);
        let instances = cell.downcast_ref::<Vec<Arc<SpineInstance>>>()?;

        let mut max_elev = 0.0f64;
        let mut best_tag: Option<PlateTag> = None;

        for inst in instances.iter() {
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

