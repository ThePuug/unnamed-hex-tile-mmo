use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::message::SummaryKey;
use common_bevy::summary::SummaryCell;

/// Global shared cache of computed summaries.
///
/// Summaries are computed asynchronously from EventRegistry's procedural
/// terrain with the players' changes laid over their samples. A change to a
/// sample revises its summary here, so an entry is never older than the
/// changes it was read with.
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

    /// The entry at `key`, or `cell` stored there where none is: a task's
    /// result never displaces a revision made while the task was out, which
    /// may have read its samples before the change.
    pub fn get_or_insert(&mut self, key: SummaryKey, cell: SummaryCell) -> SummaryCell {
        *self.entries.entry(key).or_insert(cell)
    }
}
