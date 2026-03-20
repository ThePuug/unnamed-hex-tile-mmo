//! Index registry — typed, cross-event spatial indexes.
//!
//! Events register index entries during deform. Other events query them during
//! survey evaluation. The framework manages lifecycle (LRU eviction calls
//! `remove_cell` on all indexes).

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// Cell identifier: lattice coordinates in the source event's hex ball grid.
pub type CellId = (i32, i32);

// ── EventIndex trait ────────────────────────────────────────────────────────

/// Spatial index populated by one event, queryable by others.
/// Entries are partitioned by source cell ID for LRU cleanup.
pub trait EventIndex: Send + Sync + Default + 'static {
    /// Scale (radius) of the event that populates this index.
    fn source_scale(&self) -> u32;

    /// Return tile coordinates from the specified cells.
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)>;

    /// Return graph-connected neighbor tiles for spatial predicates.
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)>;

    /// Build a TileView from this index's metadata at the given position.
    /// Used by survey evaluation to check predicates without tile materialization.
    fn tile_view_at(&self, _q: i32, _r: i32) -> Option<super::TileView> { None }

    /// Remove all entries for a cell. Called on LRU eviction.
    fn remove_cell(&mut self, cell_id: CellId);
}

// ── AnyIndex (type-erased wrapper) ──────────────────────────────────────────

trait AnyIndex: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn source_scale(&self) -> u32;
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)>;
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)>;
    fn tile_view_at(&self, q: i32, r: i32) -> Option<super::TileView>;
    fn remove_cell(&mut self, cell_id: CellId);
}

impl<T: EventIndex + 'static> AnyIndex for T {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn source_scale(&self) -> u32 { EventIndex::source_scale(self) }
    fn tiles(&self, cell_ids: &[CellId]) -> Vec<(i32, i32)> { EventIndex::tiles(self, cell_ids) }
    fn neighbors(&self, q: i32, r: i32) -> Vec<(i32, i32)> { EventIndex::neighbors(self, q, r) }
    fn tile_view_at(&self, q: i32, r: i32) -> Option<super::TileView> { EventIndex::tile_view_at(self, q, r) }
    fn remove_cell(&mut self, cell_id: CellId) { EventIndex::remove_cell(self, cell_id) }
}

// ── IndexRegistry ───────────────────────────────────────────────────────────

/// Shared across all events. Keyed by TypeId. Accumulates across cell evaluations.
pub struct IndexRegistry {
    entries: HashMap<TypeId, Box<dyn AnyIndex>>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn get_or_create<T: EventIndex>(&mut self) -> &mut T {
        let type_id = TypeId::of::<T>();
        self.entries.entry(type_id)
            .or_insert_with(|| Box::new(T::default()));
        self.entries.get_mut(&type_id).unwrap()
            .as_any_mut()
            .downcast_mut::<T>()
            .expect("TypeId mismatch in IndexRegistry")
    }

    pub fn get<T: EventIndex>(&self) -> Option<&T> {
        self.entries.get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.as_any().downcast_ref::<T>())
    }

    pub fn source_scale_of(&self, type_id: TypeId) -> Option<u32> {
        self.entries.get(&type_id).map(|boxed| boxed.source_scale())
    }

    pub fn tiles_by_type_id(&self, type_id: TypeId, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        self.entries.get(&type_id)
            .map(|boxed| boxed.tiles(cell_ids))
            .unwrap_or_default()
    }

    pub fn neighbors_by_type_id(&self, type_id: TypeId, q: i32, r: i32) -> Vec<(i32, i32)> {
        self.entries.get(&type_id)
            .map(|boxed| boxed.neighbors(q, r))
            .unwrap_or_default()
    }

    /// Get a TileView from index metadata at (q, r) via type-erased TypeId.
    pub fn tile_view_at_by_type_id(&self, type_id: TypeId, q: i32, r: i32) -> Option<super::TileView> {
        self.entries.get(&type_id)
            .and_then(|boxed| boxed.tile_view_at(q, r))
    }

    pub fn remove_cell(&mut self, cell_id: CellId) {
        for boxed in self.entries.values_mut() {
            boxed.remove_cell(cell_id);
        }
    }
}
