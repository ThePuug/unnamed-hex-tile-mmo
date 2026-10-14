//! The terrain warmed ahead of the players: before the server serves
//! anyone, its site's ladder built and the chunks a player at the spawn
//! streams generated; everything else is built as it is asked for, and as a
//! player crosses into a new chunk, the chunks just past its stream are
//! queued ahead of need, so streaming sends what is kept.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use qrz::Qrz;

use common_bevy::chunk::{calculate_visible_chunks, chunk_tiles, loc_to_chunk, ChunkId, PlayerDiscoveryState, WorldDiscoveryCache, FIXED_STREAM_RADIUS};
use common_bevy::message::{Component, Do, Event};

use crate::resources::event_registry::EventRegistry;
use crate::systems::actor::{generate_chunk, ChunkTaskQueue};
use crate::systems::dens::Dens;

/// How many rings of chunks past its stream a player's ground is warmed.
const AHEAD: u8 = 2;

/// The terrain warmed around a tile: its site's ladder, built or read from
/// where it is kept, and every chunk a player there streams, its tiles
/// requested together, then generated, kept, and their dens found.
pub fn around(registry: &EventRegistry, centre: (i32, i32)) -> (WorldDiscoveryCache, Dens) {
    let started = std::time::Instant::now();
    registry.warm(&[centre]);
    info!("warming the terrain: the ladder and the spawn's tile in {:.1} s", started.elapsed().as_secs_f64());
    let chunk = loc_to_chunk(Qrz { q: centre.0, r: centre.1, z: 0 });
    let chunks = calculate_visible_chunks(chunk, FIXED_STREAM_RADIUS);
    let tiles: Vec<(i32, i32)> = chunks.iter().flat_map(|&c| chunk_tiles(c)).collect();
    registry.warm(&tiles);
    info!("warming the terrain: {} tiles round ({}, {}) at {:.1} s", tiles.len(), centre.0, centre.1, started.elapsed().as_secs_f64());
    let mut cache = WorldDiscoveryCache::default();
    let mut dens = Dens::default();
    for chunk_id in chunks {
        let (terrain, found) = generate_chunk(chunk_id, registry);
        cache.chunks.insert(chunk_id, Arc::new(terrain));
        cache.access_order.get_or_insert(chunk_id, || ());
        for (site, habitat) in found {
            dens.found(site, habitat);
        }
    }
    info!("warming the terrain: {} chunks generated at {:.1} s", cache.chunks.len(), started.elapsed().as_secs_f64());
    (cache, dens)
}

/// The chunks queued ahead of the players, and the chunk each player was
/// warmed round last.
#[derive(Resource, Default)]
pub struct Warming {
    queued: HashSet<ChunkId>,
    at: HashMap<Entity, ChunkId>,
}

/// As a player crosses into a new chunk, its stream and the next [`AHEAD`]
/// rings of chunks queued ahead of need where not generated yet.
pub fn warm_ahead(
    mut reader: MessageReader<Do>,
    world_cache: Res<WorldDiscoveryCache>,
    mut task_queue: ResMut<ChunkTaskQueue>,
    mut warming: ResMut<Warming>,
    players: Query<(), With<PlayerDiscoveryState>>,
) {
    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component: Component::Loc(loc) } } = message else { continue };
        if players.get(*ent).is_err() {
            continue;
        }
        let chunk = loc_to_chunk(**loc);
        if warming.at.insert(*ent, chunk) == Some(chunk) {
            continue;
        }
        for chunk_id in calculate_visible_chunks(chunk, FIXED_STREAM_RADIUS + AHEAD) {
            if !world_cache.chunks.contains_key(&chunk_id) && warming.queued.insert(chunk_id) {
                task_queue.prefetch(chunk_id);
            }
        }
    }
}
