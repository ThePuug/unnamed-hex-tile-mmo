//! Index registry — typed, cross-event spatial indexes.
//!
//! Events register index entries during deform. Other events query them during
//! survey evaluation. The framework manages lifecycle (LRU eviction calls
//! `remove_cell` on all indexes).
//!
//! The HashMap is immutable after initialization — all index types are
//! pre-registered during `Composite::add_event()`. Each index has its own
//! `RwLock` so independent indexes don't contend. No outer lock needed.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{
    RwLock, RwLockReadGuard, RwLockWriteGuard,
    MappedRwLockReadGuard, MappedRwLockWriteGuard,
};

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
///
/// The HashMap is immutable after initialization. Each index has its own
/// `RwLock` — independent indexes don't contend. Deform writes to one index
/// while concurrent queries read from another without blocking.
pub struct IndexRegistry {
    /// Immutable after init. Each value has an independent RwLock.
    entries: HashMap<TypeId, Arc<RwLock<Box<dyn AnyIndex>>>>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Pre-register a typed index. Called during `Composite::add_event()`.
    /// Must be called before any concurrent access begins.
    pub fn pre_register<T: EventIndex>(&mut self) {
        self.entries.entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(RwLock::new(Box::new(T::default()))));
    }


    /// Typed read access. Returns a mapped guard implementing `Deref<Target = T>`.
    /// Returns `None` if the index type has not been registered.
    pub fn get<T: EventIndex>(&self) -> Option<MappedRwLockReadGuard<'_, T>> {
        let arc = self.entries.get(&TypeId::of::<T>())?;
        let guard = arc.read();
        Some(RwLockReadGuard::map(guard, |boxed| {
            boxed.as_any().downcast_ref::<T>().expect("TypeId mismatch in IndexRegistry")
        }))
    }

    /// Typed write access. Panics if the index type was not pre-registered.
    /// Returns a mapped guard implementing `DerefMut<Target = T>`.
    pub fn get_or_create<T: EventIndex>(&self) -> MappedRwLockWriteGuard<'_, T> {
        let arc = self.entries.get(&TypeId::of::<T>())
            .expect("Index type not pre-registered — add registers_indexes() to your WorldEvent");
        let guard = arc.write();
        RwLockWriteGuard::map(guard, |boxed| {
            boxed.as_any_mut().downcast_mut::<T>().expect("TypeId mismatch in IndexRegistry")
        })
    }

    // ── Type-erased access (used by survey evaluation) ──

    pub fn source_scale_of(&self, type_id: TypeId) -> Option<u32> {
        self.entries.get(&type_id).map(|arc| arc.read().source_scale())
    }

    pub fn tiles_by_type_id(&self, type_id: TypeId, cell_ids: &[CellId]) -> Vec<(i32, i32)> {
        self.entries.get(&type_id)
            .map(|arc| arc.read().tiles(cell_ids))
            .unwrap_or_default()
    }

    pub fn neighbors_by_type_id(&self, type_id: TypeId, q: i32, r: i32) -> Vec<(i32, i32)> {
        self.entries.get(&type_id)
            .map(|arc| arc.read().neighbors(q, r))
            .unwrap_or_default()
    }

    pub fn tile_view_at_by_type_id(&self, type_id: TypeId, q: i32, r: i32) -> Option<super::TileView> {
        self.entries.get(&type_id)
            .and_then(|arc| arc.read().tile_view_at(q, r))
    }

    /// Remove cell entries from all indexes. Called on LRU eviction.
    pub fn remove_cell(&self, cell_id: CellId) {
        for arc in self.entries.values() {
            arc.write().remove_cell(cell_id);
        }
    }
}
