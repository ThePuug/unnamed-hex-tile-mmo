//! Survey — declarative tile selection for world events.
//!
//! Survey evaluation uses index metadata (tile_view_at) for index-scoped predicates.
//! For `Survey::all()` with filter, the framework provides a `resolve_tile` callback
//! that triggers the query cascade for layers below — acceptable for small-scale events.

use std::any::TypeId;

use common::HexLattice;

use crate::noise::hash_u64;
use super::index::{CellId, EventIndex, IndexRegistry};
use super::TileView;

// ── Internal types ──────────────────────────────────────────────────────────

enum TileSource {
    None,
    All,
    FromIndex(TypeId),
}

#[allow(dead_code)]
enum SpatialPredicate {
    AllNeighbors {
        check: Box<dyn Fn(&TileView) -> bool + Send + Sync>,
        radius: u32,
        index: Option<TypeId>,
    },
    AnyNeighbor {
        check: Box<dyn Fn(&TileView) -> bool + Send + Sync>,
        radius: u32,
        index: Option<TypeId>,
    },
}

// ── Survey ──────────────────────────────────────────────────────────────────

/// Declarative, framework-evaluated tile selection.
pub struct Survey {
    tile_source: TileSource,
    spatial_predicates: Vec<SpatialPredicate>,
    filter: Option<Box<dyn Fn(&TileView, u64) -> bool + Send + Sync>>,
    min_spacing: Option<u32>,
}

impl Survey {
    /// No tile enumeration — deform runs with empty matched.
    /// Use when the event discovers its own data internally.
    pub fn none() -> Self {
        Self {
            tile_source: TileSource::None,
            spatial_predicates: Vec::new(),
            filter: None,
            min_spacing: None,
        }
    }

    /// Every tile in the cell.
    pub fn all() -> Self {
        Self {
            tile_source: TileSource::All,
            spatial_predicates: Vec::new(),
            filter: None,
            min_spacing: None,
        }
    }

    /// Tiles from a typed index.
    pub fn from_index<T: EventIndex>() -> Self {
        Self {
            tile_source: TileSource::FromIndex(TypeId::of::<T>()),
            spatial_predicates: Vec::new(),
            filter: None,
            min_spacing: None,
        }
    }

    /// All graph-connected neighbors (via index neighbor graph) must pass check.
    pub fn all_neighbors_in<T: EventIndex>(
        mut self,
        check: impl Fn(&TileView) -> bool + Send + Sync + 'static,
        radius: u32,
    ) -> Self {
        self.spatial_predicates.push(SpatialPredicate::AllNeighbors {
            check: Box::new(check),
            radius,
            index: Some(TypeId::of::<T>()),
        });
        self
    }

    /// Per-tile filter (author-owned predicate).
    pub fn filter(
        mut self,
        f: impl Fn(&TileView, u64) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.filter = Some(Box::new(f));
        self
    }

    /// Enforce minimum hex distance between surviving candidates.
    /// Uses deterministic priority-ordered greedy exclusion.
    pub fn min_spacing(mut self, distance: u32) -> Self {
        self.min_spacing = Some(distance);
        self
    }
}

// ── Survey evaluation ───────────────────────────────────────────────────────

/// Evaluate a survey against the IndexRegistry.
///
/// `resolve_tile`: optional callback that resolves a TileView from layers below
/// via the query cascade. Required for `Survey::all()` with filter (small-scale
/// events where materializing the full cell is acceptable). For index-scoped
/// surveys, predicates use index metadata instead.
pub fn evaluate_survey(
    survey: &Survey,
    cell_id: CellId,
    lattice: &HexLattice,
    indexes: &IndexRegistry,
    resolve_tile: Option<&dyn Fn(i32, i32) -> TileView>,
    seed: u64,
) -> Vec<(i32, i32)> {
    let mut tiles = match &survey.tile_source {
        TileSource::None => return Vec::new(),
        TileSource::All => {
            let mut tiles: Vec<(i32, i32)> = lattice.tiles_in_cell(cell_id).collect();

            // If there's a filter and a resolve_tile callback, apply it
            if let (Some(filter), Some(resolve)) = (&survey.filter, resolve_tile) {
                tiles.retain(|&(q, r)| {
                    let tv = resolve(q, r);
                    filter(&tv, seed)
                });
            }
            tiles
        }
        TileSource::FromIndex(source_type_id) => {
            let source_type_id = *source_type_id;

            // Find source-event cells overlapping this cell's hex ball
            let source_scale = indexes.source_scale_of(source_type_id).unwrap_or(lattice.radius);
            let source_lat = HexLattice::new(source_scale);
            let (cq, cr) = lattice.cell_center(cell_id);
            let reach = (source_lat.radius + lattice.radius) as f64;
            let min_step = (3.0 * source_lat.radius as f64 + 2.0) / 2.0;
            let lattice_reach = (reach / min_step).ceil() as u32;
            let source_center = source_lat.cell_id(cq, cr);
            let overlapping = source_lat.cells_within_distance(source_center, lattice_reach);
            let mut tiles = indexes.tiles_by_type_id(source_type_id, &overlapping);

            // Spatial predicates — use tile_view_at from index
            for pred in &survey.spatial_predicates {
                match pred {
                    SpatialPredicate::AllNeighbors { check, radius: _, index } => {
                        let view_source = index.unwrap_or(source_type_id);
                        tiles.retain(|&(q, r)| {
                            let neighbors = match index {
                                Some(type_id) => indexes.neighbors_by_type_id(*type_id, q, r),
                                None => hex_neighbors(q, r),
                            };
                            neighbors.iter().all(|&(nq, nr)| {
                                indexes.tile_view_at_by_type_id(view_source, nq, nr)
                                    .map_or(false, |tv| check(&tv))
                            })
                        });
                    }
                    SpatialPredicate::AnyNeighbor { check, radius: _, index } => {
                        let view_source = index.unwrap_or(source_type_id);
                        tiles.retain(|&(q, r)| {
                            let neighbors = match index {
                                Some(type_id) => indexes.neighbors_by_type_id(*type_id, q, r),
                                None => hex_neighbors(q, r),
                            };
                            neighbors.iter().any(|&(nq, nr)| {
                                indexes.tile_view_at_by_type_id(view_source, nq, nr)
                                    .map_or(false, |tv| check(&tv))
                            })
                        });
                    }
                }
            }

            // Per-tile filter — use tile_view_at from source index
            if let Some(filter) = &survey.filter {
                tiles.retain(|&(q, r)| {
                    indexes.tile_view_at_by_type_id(source_type_id, q, r)
                        .map_or(false, |tv| filter(&tv, seed))
                });
            }

            tiles
        }
    };

    // min_spacing — greedy exclusion with deterministic priority
    if let Some(distance) = survey.min_spacing {
        tiles = apply_min_spacing(&tiles, distance, seed);
    }

    tiles
}

/// Deterministic priority-ordered greedy exclusion.
/// Assigns each candidate a priority from hash(q, r, seed), sorts highest-first,
/// then selects candidates that are at least `distance` hex tiles from all
/// previously selected candidates.
fn apply_min_spacing(
    candidates: &[(i32, i32)],
    distance: u32,
    seed: u64,
) -> Vec<(i32, i32)> {
    let dist = distance as i32;

    // Priority assignment + sort (highest first)
    let mut prioritized: Vec<((i32, i32), u64)> = candidates.iter()
        .map(|&(q, r)| ((q, r), hash_u64(q as i64, r as i64, seed)))
        .collect();
    prioritized.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    // Greedy exclusion
    let mut selected: Vec<(i32, i32)> = Vec::new();
    for ((q, r), _) in prioritized {
        let too_close = selected.iter().any(|&(sq, sr)| {
            hex_distance(q, r, sq, sr) < dist
        });
        if !too_close {
            selected.push((q, r));
        }
    }

    selected
}

fn hex_distance(q1: i32, r1: i32, q2: i32, r2: i32) -> i32 {
    let dq = q1 - q2;
    let dr = r1 - r2;
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

fn hex_neighbors(q: i32, r: i32) -> Vec<(i32, i32)> {
    vec![
        (q + 1, r), (q - 1, r),
        (q, r + 1), (q, r - 1),
        (q + 1, r - 1), (q - 1, r + 1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_neighbors_count() {
        assert_eq!(hex_neighbors(0, 0).len(), 6);
    }

    #[test]
    fn min_spacing_deterministic() {
        let candidates: Vec<(i32, i32)> = (0..100).map(|i| (i, 0)).collect();
        let a = apply_min_spacing(&candidates, 10, 42);
        let b = apply_min_spacing(&candidates, 10, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn min_spacing_enforces_distance() {
        let candidates: Vec<(i32, i32)> = (0..100).map(|i| (i, 0)).collect();
        let selected = apply_min_spacing(&candidates, 10, 42);

        // Every pair must be >= 10 apart
        for i in 0..selected.len() {
            for j in (i + 1)..selected.len() {
                let d = hex_distance(selected[i].0, selected[i].1, selected[j].0, selected[j].1);
                assert!(d >= 10, "pair {:?} {:?} distance {} < 10", selected[i], selected[j], d);
            }
        }
        // Should select some candidates (100 tiles, distance 10 → ~10 survivors)
        assert!(selected.len() >= 5);
        assert!(selected.len() <= 15);
    }

    #[test]
    fn min_spacing_different_seed_different_selection() {
        let candidates: Vec<(i32, i32)> = (0..100).map(|i| (i, 0)).collect();
        let a = apply_min_spacing(&candidates, 10, 42);
        let b = apply_min_spacing(&candidates, 10, 99);
        // Different seeds should usually produce different selections
        // (not guaranteed but extremely likely with 100 candidates)
        assert_ne!(a, b);
    }

    #[test]
    fn min_spacing_empty_input() {
        let selected = apply_min_spacing(&[], 10, 42);
        assert!(selected.is_empty());
    }

    #[test]
    fn min_spacing_single_candidate() {
        let selected = apply_min_spacing(&[(5, 3)], 10, 42);
        assert_eq!(selected, vec![(5, 3)]);
    }
}
