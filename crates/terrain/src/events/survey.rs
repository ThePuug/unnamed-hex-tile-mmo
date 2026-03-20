//! Survey — declarative tile selection for world events.
//!
//! Survey evaluation uses index metadata (tile_view_at) for index-scoped predicates.
//! For `Survey::all()` with filter, the framework provides a `resolve_tile` callback
//! that triggers the query cascade for layers below — acceptable for small-scale events.

use std::any::TypeId;

use common::HexLattice;

use super::index::{CellId, EventIndex, IndexRegistry};
use super::TileView;

// ── Internal types ──────────────────────────────────────────────────────────

enum TileSource {
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
}

impl Survey {
    /// Every tile in the cell.
    pub fn all() -> Self {
        Self {
            tile_source: TileSource::All,
            spatial_predicates: Vec::new(),
            filter: None,
        }
    }

    /// Tiles from a typed index.
    pub fn from_index<T: EventIndex>() -> Self {
        Self {
            tile_source: TileSource::FromIndex(TypeId::of::<T>()),
            spatial_predicates: Vec::new(),
            filter: None,
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

    /// Per-tile filter (runs last, author-owned).
    pub fn filter(
        mut self,
        f: impl Fn(&TileView, u64) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.filter = Some(Box::new(f));
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
    match &survey.tile_source {
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
            let reach = (source_lat.radius + lattice.radius) as i32;
            let lattice_reach = (reach as f64 / (2.0 * source_lat.radius as f64 + 1.0)).ceil() as u32 + 1;
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
    }
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
}
