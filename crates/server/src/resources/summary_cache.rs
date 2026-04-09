use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::message::SummaryKey;

/// Global shared cache of computed summary center_z values.
///
/// Summaries are computed asynchronously from EventRegistry's procedural
/// terrain. Since terrain is deterministic from the seed, cached values
/// never change (no invalidation needed until runtime deformation).
#[derive(Resource, Default)]
pub struct SummaryCache {
    entries: HashMap<SummaryKey, i32>,
}

impl SummaryCache {
    /// Cache lookup only — returns None on miss.
    pub fn get(&self, key: &SummaryKey) -> Option<i32> {
        self.entries.get(key).copied()
    }

    /// Store a computed result.
    pub fn insert(&mut self, key: SummaryKey, center_z: i32) {
        self.entries.insert(key, center_z);
    }
}
