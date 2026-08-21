//! SpineEvent — Event #1: continental spine elevation and tags.

//! Scale = SPINE_INFLUENCE (15,225 tiles). Cells contain the full influence
//! radius of any spine epicenter within them. Query checks the cell + 1 neighbor
//! ring in the SpineInstanceIndex — no wider search needed.

//! Deform: reads PlateCentroidIndex for qualifying epicentres, thins them to a
//! minimum spacing, grows a spine at each, registers SpineInstanceIndex.
//! Query: evaluates a single tile's elevation + tag from indexed instances.

use std::collections::HashMap;
use std::sync::Arc;



use common::PlateTag;

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
use super::{CellScope, TileOutput, TileView, WorldEvent};

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
}

impl SpineEvent {
    pub fn new(seed: u64) -> Self {
        Self::with_cache(Arc::new(PlateCache::new(seed)), seed)
    }

    pub fn with_cache(plate_cache: Arc<PlateCache>, seed: u64) -> Self {
        Self {
            plate_cache,
            seed,
        }
    }

}

impl WorldEvent for SpineEvent {
    fn name(&self) -> &str { "spines" }
    fn scale(&self) -> u32 { SPINE_CELL_SCALE }

    /// An epicentre reaches SPINE_INFLUENCE from wherever it sits, which is why
    /// the cell is scaled to it: one ring of cells this size clears 1.268x the
    /// radius, so a spine near a cell edge is still folded by every cell it
    /// touches.
    fn max_influence(&self) -> u32 { SPINE_INFLUENCE as u32 }

    fn register_indexes(&self, registry: &mut IndexRegistry) {
        registry.pre_register::<SpineInstanceIndex>();
        registry.pre_register::<ErosionalFaceIndex>();
        registry.pre_register::<BasinIndex>();
    }


    fn deform(&self, scope: &CellScope) {
        // Collect centroid data under read lock, then drop it before write lock.
        let centroid_data: Vec<(f64, f64, u64)> = {
            let centroid_index = match scope.read::<PlateCentroidIndex>() {
                Some(idx) => idx,
                None => {
                    scope.publish::<SpineInstanceIndex>(Vec::new());
                    return;
                }
            };

            // Epicentre candidates: every plate centroid in the plate cells this
            // cell may read. A spine sits deep inside a landmass, so a candidate
            // qualifies only if it *and* every one of its Voronoi neighbours is
            // Inland — a centroid whose neighbour is missing from the index
            // fails, because a neighbour that cannot be seen cannot be vouched
            // for.
            let cells = scope.source_cells::<PlateCentroidIndex>();
            let mut candidates = centroid_index.tiles(&cells);
            candidates.retain(|&(q, r)| {
                centroid_index.neighbors(q, r).iter().all(|&(nq, nr)| {
                    centroid_index.tile_view_at(nq, nr)
                        .map_or(false, |tv| tv.tags.has(PlateTag::Inland))
                })
            });
            candidates.retain(|&(q, r)| {
                centroid_index.tile_view_at(q, r)
                    .map_or(false, |tv| tv.tags.has(PlateTag::Inland))
            });
            let matched = thin_by_spacing(&candidates, SPINE_EXCLUSION_TILES, scope.seed());

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

        // Everything here is keyed by where it was seeded, not by the ground it
        // covers — a spine reaches a full SPINE_INFLUENCE past its own cell,
        // so readers gather cell plus ring.
        //
        // Floors are this cell's own view: deform sees the layers below over its
        // footprint, never its own ring, so it cannot know which of these carves
        // a neighbouring spine buries. A reader composites them against the ring
        // it can see, which is what `prepare` is for.
        let reach = slope_form::MASS_WASTING_REACH;
        let min_height = crate::ELEVATION_PER_Z / crate::TILE_SPACING * reach;
        let mut faces = crate::faces::FaceIndex::new(reach);
        for inst in &instances {
            inst.each_face(&mut |face| faces.insert(face, min_height));
        }
        scope.publish::<ErosionalFaceIndex>(Arc::new(faces));

        scope.publish::<BasinIndex>(BasinIndex::grid_of(
            instances.iter().flat_map(|i| i.cirques.iter()).cloned(),
        ));

        scope.publish::<SpineInstanceIndex>(instances);
    }

    /// The instances any tile in this cell can be reached by: its own cell plus
    /// its one ring, which is the set the framework has
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
        let cells = scope.lattice().cells_within_distance(scope.cell(), 1);
        let instances = match scope.read::<SpineInstanceIndex>() {
            Some(idx) => idx.instances_in(&cells),
            None => Vec::new(),
        };

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
        let mut curvature = 0.0f64;
        let mut best_tag: Option<PlateTag> = None;

        for inst in instances.iter() {
            // Single pass per instance: elevation + tag share one peak scan.
            let s = inst.sample_at(wx, wy);
            let (e, tag) = (s.elevation, s.tag);
            if e > max_elev { max_elev = e; curvature = s.curvature; }

            if let Some(tag) = tag {
                let dominated = best_tag.as_ref()
                    .map_or(true, |b| spine_tag_priority(&tag) > spine_tag_priority(b));
                if dominated { best_tag = Some(tag); }
            }
        }

        if max_elev <= 0.0 { return None; }

        let mut out = TileOutput::default();
        out.elevation_delta = max_elev;
        out.curvature = curvature;
        if let Some(t) = best_tag {
            out.tags_added.add(t);
        }
        Some(out)
    }
}


/// Deterministic priority-ordered greedy exclusion.
///
/// Each candidate takes a priority from `hash(q, r, seed)`; highest first, and
/// a candidate is kept only if it clears every already-kept one by `distance`.
/// Priority comes from the candidate rather than its position in the list, so
/// the result does not depend on the order the index yielded them.
///
/// Private, and not a shared helper: one caller does not justify one.
fn thin_by_spacing(candidates: &[(i32, i32)], distance: u32, seed: u64) -> Vec<(i32, i32)> {
    fn hex_distance(q1: i32, r1: i32, q2: i32, r2: i32) -> i32 {
        let dq = q1 - q2;
        let dr = r1 - r2;
        dq.abs().max(dr.abs()).max((dq + dr).abs())
    }

    let dist = distance as i32;
    let mut prioritized: Vec<((i32, i32), u64)> = candidates.iter()
        .map(|&(q, r)| ((q, r), crate::noise::hash_u64(q as i64, r as i64, seed)))
        .collect();
    prioritized.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    let mut selected: Vec<(i32, i32)> = Vec::new();
    for ((q, r), _) in prioritized {
        let too_close = selected.iter()
            .any(|&(sq, sr)| hex_distance(q, r, sq, sr) < dist);
        if !too_close {
            selected.push((q, r));
        }
    }
    selected
}

#[cfg(test)]
mod spacing_tests {
    use super::thin_by_spacing;

    #[test]
    fn spacing_is_deterministic() {
        let candidates: Vec<(i32, i32)> = (0..50).map(|i| (i % 10, i / 10)).collect();
        assert_eq!(
            thin_by_spacing(&candidates, 10, 42),
            thin_by_spacing(&candidates, 10, 42),
        );
    }

    /// Priority comes from the candidate, so shuffling the input cannot move a
    /// spine. The index yields cells in HashMap order, which is not stable.
    #[test]
    fn spacing_is_order_independent() {
        let candidates: Vec<(i32, i32)> = (0..40).map(|i| (i * 3 % 37, i * 7 % 41)).collect();
        let mut reversed = candidates.clone();
        reversed.reverse();

        let mut a = thin_by_spacing(&candidates, 5, 42);
        let mut b = thin_by_spacing(&reversed, 5, 42);
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }

    #[test]
    fn spacing_enforces_distance() {
        let candidates: Vec<(i32, i32)> = (0..50).map(|i| (i % 10, i / 10)).collect();
        let selected = thin_by_spacing(&candidates, 10, 42);
        for (i, &(q1, r1)) in selected.iter().enumerate() {
            for &(q2, r2) in &selected[i + 1..] {
                let (dq, dr) = (q1 - q2, r1 - r2);
                let d = dq.abs().max(dr.abs()).max((dq + dr).abs());
                assert!(d >= 10, "({q1},{r1}) and ({q2},{r2}) are {d} apart");
            }
        }
    }

    /// Priority is seeded, so two worlds thin the same candidates differently.
    #[test]
    fn spacing_varies_with_seed() {
        let candidates: Vec<(i32, i32)> = (0..50).map(|i| (i % 10, i / 10)).collect();
        assert_ne!(
            thin_by_spacing(&candidates, 10, 42),
            thin_by_spacing(&candidates, 10, 99),
        );
    }

    #[test]
    fn spacing_handles_empty_and_single() {
        assert!(thin_by_spacing(&[], 10, 42).is_empty());
        assert_eq!(thin_by_spacing(&[(5, 3)], 10, 42), vec![(5, 3)]);
    }
}
