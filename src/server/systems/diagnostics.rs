use bevy::prelude::*;
use std::collections::HashMap;

use crate::common::resources::map::Map;

#[derive(Resource)]
pub struct TerrainTracker {
    seen_tiles: HashMap<(i16, i16), Vec<i16>>,
    last_tile_count: usize,
}

impl Default for TerrainTracker {
    fn default() -> Self {
        Self {
            seen_tiles: HashMap::new(),
            last_tile_count: 0,
        }
    }
}

/// Continuously monitor for duplicate terrain tiles at the same q,r coordinates
pub fn check_duplicate_tiles(
    map: Res<Map>,
    mut tracker: ResMut<TerrainTracker>,
) {
    // Count current tiles
    let mut current_tiles = 0;
    let mut qr_map: HashMap<(i16, i16), Vec<i16>> = HashMap::new();
    
    for (qrz, _typ) in map.iter_tiles() {
        current_tiles += 1;
        qr_map.entry((qrz.q, qrz.r))
            .or_insert_with(Vec::new)
            .push(qrz.z);
    }
    
    // Only check if new tiles were added
    if current_tiles == tracker.last_tile_count {
        return;
    }
    
    tracker.last_tile_count = current_tiles;
    
    // Check for new duplicates (silently)
    let mut new_duplicates = Vec::new();
    for ((q, r), z_values) in &qr_map {
        if z_values.len() > 1 {
            // Check if this is a new duplicate or got worse
            let previous_count = tracker.seen_tiles.get(&(*q, *r)).map(|v| v.len()).unwrap_or(0);
            if z_values.len() > previous_count {
                new_duplicates.push((*q, *r, z_values.clone(), previous_count));
            }
        }
    }
    
    // Update tracker
    tracker.seen_tiles = qr_map.clone();
}
