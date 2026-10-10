//! Index registry — typed, cross-event spatial indexes.
//!
//! A layer names its index types in `register_indexes`, fills them in
//! `deform`, and the layers above read them through `CellScope`, in
//! `deform` and in `prepare`. Nothing is ever evicted: an entry, once
//! published, lives for the life of the composite.
//!
//! The map is immutable after initialization — every index type is
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

// ── CellIndex trait ─────────────────────────────────────────────────────────

/// An index holding exactly one entry per cell of the layer that fills it.
///
/// Implementing this is what lets a layer publish through [`CellScope`], which
/// never lets it name a cell other than the one being evaluated. An index whose
/// entries belong to the ground they describe wants this; one that is keyed by
/// which feature produced an entry, and expects readers to gather over the ring
/// instead, does not.
///
/// [`CellScope`]: super::CellScope
pub trait CellIndex: Send + Sync + Default + 'static {
    /// What one cell contributes.
    type Cell: Send + Sync;

    /// Record a cell's entry, replacing whatever it held.
    fn set(&mut self, cell: CellId, entry: Self::Cell);

    /// A cell's entry, where it has one.
    fn get(&self, cell: CellId) -> Option<&Self::Cell>;
}

// ── IndexRegistry ───────────────────────────────────────────────────────────

/// Shared across all events. Keyed by TypeId. Accumulates across cell evaluations.
///
/// The HashMap is immutable after initialization. Each index has its own
/// `RwLock` — independent indexes don't contend. Deform writes to one index
/// while concurrent queries read from another without blocking.
pub struct IndexRegistry {
    /// Immutable after init. Each value has an independent RwLock.
    entries: HashMap<TypeId, Arc<RwLock<Box<dyn Any + Send + Sync>>>>,
    /// The layer that fills each index, so a read can deform exactly that
    /// layer under the reader's footprint and nothing else.
    layers: HashMap<TypeId, usize>,
    registering: usize,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), layers: HashMap::new(), registering: 0 }
    }

    /// Pre-register a typed index. Called during `Composite::add_event()`.
    /// Must be called before any concurrent access begins.
    pub fn pre_register<T: CellIndex>(&mut self) {
        self.entries.entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(RwLock::new(Box::new(T::default()))));
        self.layers.entry(TypeId::of::<T>()).or_insert(self.registering);
    }

    /// The layer whose `register_indexes` is running, recorded against every
    /// index it registers.
    pub fn set_registering_layer(&mut self, layer: usize) {
        self.registering = layer;
    }

    /// The layer that fills `T`, or None if nothing registered it.
    pub fn layer_of<T: CellIndex>(&self) -> Option<usize> {
        self.layers.get(&TypeId::of::<T>()).copied()
    }

    /// Typed read access. Returns a mapped guard implementing `Deref<Target = T>`.
    /// Returns `None` if the index type has not been registered.
    pub fn get<T: CellIndex>(&self) -> Option<MappedRwLockReadGuard<'_, T>> {
        let arc = self.entries.get(&TypeId::of::<T>())?;
        let guard = arc.read();
        Some(RwLockReadGuard::map(guard, |boxed| {
            boxed.downcast_ref::<T>().expect("TypeId mismatch in IndexRegistry")
        }))
    }

    /// Typed write access. Panics if the index type was not pre-registered.
    /// Returns a mapped guard implementing `DerefMut<Target = T>`.
    pub fn get_or_create<T: CellIndex>(&self) -> MappedRwLockWriteGuard<'_, T> {
        let arc = self.entries.get(&TypeId::of::<T>())
            .expect("Index type not pre-registered — add registers_indexes() to your WorldEvent");
        let guard = arc.write();
        RwLockWriteGuard::map(guard, |boxed| {
            boxed.downcast_mut::<T>().expect("TypeId mismatch in IndexRegistry")
        })
    }
}
