use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::message::SummaryKey;
use common_bevy::summary::SummaryCell;

/// Global shared cache of computed summaries.

/// Summaries are computed asynchronously from EventRegistry's procedural
/// terrain. Since terrain is deterministic from the seed, cached values
/// never change (no invalidation needed until runtime deformation).
#[derive(Resource, Default)]
pub struct SummaryCache {
    entries: HashMap<SummaryKey, SummaryCell>,
}

impl SummaryCache {
    /// Cache lookup only — returns None on miss.
    pub fn get(&self, key: &SummaryKey) -> Option<SummaryCell> {
        self.entries.get(key).copied()
    }

    /// Store a computed result.
    pub fn insert(&mut self, key: SummaryKey, cell: SummaryCell) {
        self.entries.insert(key, cell);
    }
}
