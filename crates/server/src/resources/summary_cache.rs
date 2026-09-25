use std::collections::HashMap;

use bevy::prelude::*;
use common::summary::PartStats;
use common_bevy::message::SummaryKey;
use common_bevy::summary::SummaryCell;

/// Global shared cache of computed summaries, and the stats of each
/// summary players have changed.
///
/// A summary is computed from EventRegistry's procedural terrain, its
/// canopy from the world as generated and the rest with the players'
/// changes laid over its samples. A change to a tile moves the stats of
/// the part holding it at every level, which set that part's canopy here,
/// so an entry is never older than the changes it was read with.
#[derive(Resource, Default)]
pub struct SummaryCache {
    entries: HashMap<SummaryKey, SummaryCell>,
    /// Only a summary a change has touched holds stats.
    stats: HashMap<SummaryKey, PartStats>,
}

impl SummaryCache {
    /// Cache lookup only — returns None on miss.
    pub fn get(&self, key: &SummaryKey) -> Option<SummaryCell> {
        self.entries.get(key).copied()
    }

    /// The entry at `key`, or `cell` stored there where none is: a task's
    /// result never displaces a revision made while the task was out, which
    /// may have read its samples before the change.
    pub fn get_or_insert(&mut self, key: SummaryKey, cell: SummaryCell) -> SummaryCell {
        *self.entries.entry(key).or_insert(cell)
    }

    /// The summary at `key` and its parts' stats, for a change to move:
    /// `generate` computes the summary where the cache holds none.
    pub fn touch(&mut self, key: SummaryKey, generate: impl FnOnce() -> SummaryCell) -> (&mut SummaryCell, &mut PartStats) {
        (self.entries.entry(key).or_insert_with(generate), self.stats.entry(key).or_default())
    }

    /// The summary at `key`, where the cache holds one.
    pub fn get_mut(&mut self, key: &SummaryKey) -> Option<&mut SummaryCell> {
        self.entries.get_mut(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{Canopy, Content, Cover};

    /// A touch computes a summary only where none is cached, and a task's
    /// result arriving after never displaces what the change moved.
    #[test]
    fn a_touch_computes_once_and_a_late_task_keeps_out() {
        let key = SummaryKey { r: 4, sq: 1, sr: -2 };
        let pine = Cover::NONE.with(0, Content::Pine);
        let generated = SummaryCell { canopy: [Canopy::of(pine); common::summary::PARTS], ..Default::default() };
        let mut cache = SummaryCache::default();

        let (cell, stats) = cache.touch(key, || generated);
        for _ in 0..9 {
            cell.canopy[0] = stats.change(key.r, 0, pine, Cover::NONE, || [9, 0, 0]);
        }
        assert!(cache.get(&key).unwrap().canopy[0].is_empty(), "the part felled whole");

        let (cell, _) = cache.touch(key, || unreachable!("the summary is cached"));
        assert!(cell.canopy[0].is_empty());
        assert!(cache.get_or_insert(key, generated).canopy[0].is_empty(), "the task's reading stays out");
    }
}
