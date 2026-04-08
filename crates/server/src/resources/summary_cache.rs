use std::collections::HashMap;

use bevy::prelude::*;
use common_bevy::message::{SummaryData, SummaryKey};
use common_bevy::summary::{summary_lattice, select_center_z};

use crate::resources::event_registry::EventRegistry;

/// Global shared cache of computed summary center_z values.
///
/// Summaries are computed lazily from the EventRegistry's procedural terrain.
/// Since terrain is deterministic from the seed, cached values never change
/// (no invalidation needed until runtime deformation is added).
#[derive(Resource)]
pub struct SummaryCache {
    entries: HashMap<SummaryKey, i32>,
    registry: EventRegistry,
}

impl SummaryCache {
    pub fn new(registry: EventRegistry) -> Self {
        Self {
            entries: HashMap::new(),
            registry,
        }
    }

    /// Get or compute the center_z for a summary hex.
    pub fn get_or_compute(&mut self, key: SummaryKey) -> i32 {
        if let Some(&z) = self.entries.get(&key) {
            return z;
        }

        let lat = summary_lattice(key.r);
        let tile_zs: Vec<i32> = lat
            .tiles_in_cell((key.sq, key.sr))
            .map(|(tq, tr)| self.registry.elevation_at(tq, tr))
            .collect();

        let center_z = select_center_z(&tile_zs);
        self.entries.insert(key, center_z);
        center_z
    }

    /// Compute a batch of summaries, returning SummaryData for each.
    pub fn batch_compute(&mut self, keys: &[SummaryKey]) -> Vec<SummaryData> {
        keys.iter()
            .map(|key| {
                let center_z = self.get_or_compute(*key);
                SummaryData {
                    r: key.r,
                    sq: key.sq,
                    sr: key.sr,
                    center_z,
                }
            })
            .collect()
    }
}
