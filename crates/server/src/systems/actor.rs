use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use qrz::{Convert, Qrz};
use std::sync::Arc;

use common_bevy::{
    chunk::{self, *},
    components::{
        entity_type::{ decorator::*, *},
        heading::Heading,
        position::Position,
        *
    },
    message::{Component, Event, *},
    resources::map::*,
};
use crate::resources::event_registry::EventRegistry;
use crate::plugins::metrics::SystemTimings;




/// Cached chunk set, recomputed on chunk boundary crossings.
/// Tracks which chunks have been sent to this player so we only send new ones.
#[derive(bevy::prelude::Component)]
pub struct VisibleChunkCache {
    /// All chunks currently sent to this player
    pub sent: std::collections::HashSet<ChunkId>,
    /// Chunk this was computed for (detect boundary crossings)
    pub chunk_id: ChunkId,
}

/// Maximum concurrent async chunk-generation tasks. A spawn queues 1,387
/// chunks at once; unbounded task spawning saturates the async compute pool
/// in raster order, generating the radius-21 frontier before the chunks the
/// player is standing on. Pending work drains nearest-first instead.
const MAX_CHUNK_TASKS: usize = 16;

/// In-flight async chunk generation tasks.
/// Task returns (chunk, duration_ms) so we can report async metrics.
#[derive(Resource, Default)]
pub struct ChunkTaskQueue {
    tasks: Vec<(Entity, ChunkId, Task<(TerrainChunk, f32)>)>,
    /// Chunks currently being generated (avoid duplicate tasks).
    pub in_flight: std::collections::HashSet<ChunkId>,
    /// Cache-missed requests awaiting a task slot, drained nearest-first.
    pending: Vec<(Entity, ChunkId)>,
    /// Mirror of `pending` chunk ids for O(1) duplicate suppression.
    pending_set: std::collections::HashSet<ChunkId>,
}

/// Hex distance between two chunks (in tiles, via their center tiles).
fn chunk_hex_distance(a: ChunkId, b: ChunkId) -> i32 {
    let ca = a.center();
    let cb = b.center();
    let dq = ca.q - cb.q;
    let dr = ca.r - cb.r;
    dq.abs().max(dr.abs()).max((dq + dr).abs())
}

/// Discover initial chunks when a player first spawns
pub fn do_spawn_discover(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_states: Query<&mut PlayerDiscoveryState>,
    query: Query<&Loc>,
    timings: Res<SystemTimings>,
) {
    let mut _t = None;
    for message in reader.read() {
        let Do { event: Event::Spawn { ent, .. } } = message else { continue };
        _t.get_or_insert_with(|| timings.scope("spawn_disc"));
        let ent = *ent;

        // Only process entities with PlayerDiscoveryState (players)
        let Ok(mut player_state) = player_states.get_mut(ent) else { continue };

        // CRITICAL: Only discover chunks for initial spawns (when last_chunk is None).
        // This prevents infinite loops when try_discover_chunk sends Do::Spawn events
        // for remote players - we don't want to re-discover chunks for them.
        if player_state.last_chunk.is_some() {
            continue;
        }

        // Get player's location
        let Ok(loc) = query.get(ent) else { continue };

        let current_chunk = loc_to_chunk(**loc);

        // Fixed streaming radius — covers gameplay area (AOI, physics, r=0–r=2).
        // Visual frontier beyond this is handled by server-sent summaries.
        let send_radius = FIXED_STREAM_RADIUS as i32;

        let chunks = calculate_visible_chunks(current_chunk, send_radius as u8);

        for &chunk_id in &chunks {
            writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
            player_state.seen_chunks.insert(chunk_id);
        }

        player_state.last_chunk = Some(current_chunk);

        commands.entity(ent).insert((
            VisibleChunkCache {
                sent: chunks.into_iter().collect(),
                chunk_id: current_chunk,
            },
            crate::systems::summary::VisibleSummaryCache::default(),
        ));
    }
}

/// Server-side system: Generates Try::DiscoverChunk events when the server authoritatively changes an entity's Loc.
/// Uses chunk-based boundary detection to reduce discovery events dramatically.
pub fn do_incremental(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_queries: Query<(&mut PlayerDiscoveryState, &mut VisibleChunkCache)>,
    timings: Res<SystemTimings>,
) {
    let mut _t = None;
    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };
        let ent = *ent;
        let component = *component;

        // Only process Loc changes for chunk-based discovery
        let Component::Loc(loc) = component else { continue };
        _t.get_or_insert_with(|| timings.scope("incremental"));

        let Ok((mut player_state, mut cache)) = player_queries.get_mut(ent) else { continue };

        let new_chunk = loc_to_chunk(*loc);

        // Skip if still in same chunk (cache is still valid)
        if cache.chunk_id == new_chunk {
            continue;
        }

        // Fixed streaming radius (same as do_spawn_discover)
        let send_radius = FIXED_STREAM_RADIUS as i32;

        let new_chunks = calculate_visible_chunks(new_chunk, send_radius as u8);
        let new_set: std::collections::HashSet<ChunkId> = new_chunks.iter().copied().collect();

        // Capture evicted chunks before retaining
        let evicted: Vec<ChunkId> = cache.sent.iter()
            .filter(|id| !new_set.contains(id))
            .copied()
            .collect();

        cache.sent.retain(|id| new_set.contains(id));
        player_state.seen_chunks.retain(|id| new_set.contains(id));

        // Send eviction message to client
        if !evicted.is_empty() {
            use tinyvec::ArrayVec;
            for batch in evicted.chunks(64) {
                let mut chunks = ArrayVec::new();
                for &cid in batch { chunks.push(cid); }
                writer.write(Try { event: Event::EvictChunks { ent, chunks } });
            }
        }

        // Send newly visible chunks
        for &chunk_id in &new_chunks {
            if !cache.sent.contains(&chunk_id) {
                writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
                player_state.seen_chunks.insert(chunk_id);
                cache.sent.insert(chunk_id);
            }
        }

        player_state.last_chunk = Some(new_chunk);
        cache.chunk_id = new_chunk;
    }
}

/// Generate a chunk of terrain tiles (pure computation, no ECS access).
fn generate_chunk(chunk_id: ChunkId, registry: &EventRegistry) -> TerrainChunk {
    let mut tiles: tinyvec::ArrayVec<[(Qrz, EntityType); 272]> = tinyvec::ArrayVec::new();
    let coords: Vec<(i32, i32)> = chunk::chunk_tiles(chunk_id).collect();

    for &(q, r) in &coords {
        let z = registry.elevation_at(q, r);
        let qrz = Qrz { q, r, z };
        let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
        tiles.push((qrz, typ));
    }

    // Spawner placements are index-only: tile materialization no longer
    // deforms their cells. Warm them here, where every survey resolve is a
    // cache hit on the tiles materialized above.
    registry.warm_indexes(&coords);

    TerrainChunk::new(tiles)
}

/// Dispatch chunk generation: cache hits → immediate Do, cache misses →
/// pending queue, drained nearest-first under MAX_CHUNK_TASKS async tasks.
/// EvictChunks passthrough is handled here too.
pub fn try_discover_chunk(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    registry: Res<EventRegistry>,
    map: ResMut<Map>,
    mut task_queue: ResMut<ChunkTaskQueue>,
    locs: Query<&Loc>,
) {
    for message in reader.read() {
        // Passthrough: EvictChunks Try → Do (server-authoritative eviction)
        if let Try { event: Event::EvictChunks { ent, chunks } } = message {
            writer.write(Do { event: Event::EvictChunks { ent: *ent, chunks: chunks.clone() } });
            continue;
        }
        if let Try { event: Event::DiscoverChunk { ent, chunk_id } } = message {
            let ent = *ent;
            let chunk_id = *chunk_id;

            // Cache hit → immediate send
            if world_cache.chunks.contains_key(&chunk_id) {
                send_cached_chunk(ent, chunk_id, &mut world_cache, &*map, &mut writer);
                continue;
            }

            // Already in flight or queued → skip (delivered on completion)
            if task_queue.in_flight.contains(&chunk_id) { continue; }
            if !task_queue.pending_set.insert(chunk_id) { continue; }
            task_queue.pending.push((ent, chunk_id));
        }
    }

    // Drain pending nearest-first under the task budget. Runs every frame so
    // the backlog keeps flowing as tasks complete.
    if !task_queue.pending.is_empty() && task_queue.in_flight.len() < MAX_CHUNK_TASKS {
        // Farthest-first sort so we can pop the nearest from the back.
        let mut pending = std::mem::take(&mut task_queue.pending);
        pending.sort_by_key(|&(ent, chunk_id)| {
            let dist = locs.get(ent)
                .map(|loc| chunk_hex_distance(loc_to_chunk(**loc), chunk_id))
                .unwrap_or(i32::MAX);
            std::cmp::Reverse(dist)
        });

        while task_queue.in_flight.len() < MAX_CHUNK_TASKS {
            let Some((ent, chunk_id)) = pending.pop() else { break };
            task_queue.pending_set.remove(&chunk_id);

            // Generated by another player's request while queued → send now.
            if world_cache.chunks.contains_key(&chunk_id) {
                send_cached_chunk(ent, chunk_id, &mut world_cache, &*map, &mut writer);
                continue;
            }

            task_queue.in_flight.insert(chunk_id);
            let reg = registry.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let start = std::time::Instant::now();
                let chunk = generate_chunk(chunk_id, &reg);
                let duration_ms = start.elapsed().as_secs_f64() as f32 * 1000.0;
                (chunk, duration_ms)
            });
            task_queue.tasks.push((ent, chunk_id, task));
        }

        task_queue.pending = pending;
    }
}

/// Send a cached chunk to a client and merge its tiles into the server Map.
fn send_cached_chunk(
    ent: Entity,
    chunk_id: ChunkId,
    world_cache: &mut WorldDiscoveryCache,
    map: &Map,
    writer: &mut MessageWriter<Do>,
) {
    world_cache.access_order.get_or_insert(chunk_id, || ());
    let chunk = Arc::clone(world_cache.chunks.get(&chunk_id).unwrap());

    // Insert tiles into server Map for physics/collision/AI
    for &(qrz, typ) in &chunk.tiles {
        if map.get(qrz).is_none() {
            map.insert(qrz, typ);
        }
    }

    let wire_tiles: tinyvec::ArrayVec<[(i32, EntityType); 272]> = chunk.tiles.iter()
        .map(|&(qrz, typ)| (qrz.z, typ))
        .collect();
    writer.write(Do {
        event: Event::ChunkData { ent, chunk_id, tiles: wire_tiles }
    });
}

/// Poll completed async chunk tasks. Inserts into Map + cache, sends ChunkData.
pub fn poll_chunk_tasks(
    mut writer: MessageWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    map: ResMut<Map>,
    mut task_queue: ResMut<ChunkTaskQueue>,
    snapshot: Res<crate::plugins::metrics::MetricSnapshot>,
    timings: Res<SystemTimings>,
) {
    let mut _t = None;
    let mut pending = Vec::new();
    let current = std::mem::take(&mut task_queue.tasks);

    for (ent, chunk_id, mut task) in current {
        if let Some((chunk, duration_ms)) = block_on(poll_once(&mut task)) {
            _t.get_or_insert_with(|| timings.scope("chunk_poll"));
            snapshot.record(&[("async.task_duration_ms", duration_ms)]);
            let chunk = Arc::new(chunk);

            // Insert into world cache
            if world_cache.chunks.len() >= world_cache.max_chunks {
                if let Some((evicted_id, _)) = world_cache.access_order.pop_lru() {
                    world_cache.chunks.remove(&evicted_id);
                }
            }
            world_cache.chunks.insert(chunk_id, Arc::clone(&chunk));
            world_cache.access_order.get_or_insert(chunk_id, || ());

            // Insert tiles into server Map
            for &(qrz, typ) in &chunk.tiles {
                if map.get(qrz).is_none() {
                    map.insert(qrz, typ);
                }
            }

            // Send to client
            let wire_tiles: tinyvec::ArrayVec<[(i32, EntityType); 272]> = chunk.tiles.iter()
                .map(|&(qrz, typ)| (qrz.z, typ))
                .collect();
            writer.write(Do {
                event: Event::ChunkData { ent, chunk_id, tiles: wire_tiles }
            });

            task_queue.in_flight.remove(&chunk_id);
        } else {
            pending.push((ent, chunk_id, task));
        }
    }

    task_queue.tasks = pending;
}

pub fn update(
    mut writer: MessageWriter<Try>,
    mut query: Query<(Entity, &mut Loc, &mut Position), Changed<Position>>,
    map: Res<Map>,
    timings: Res<SystemTimings>,
) {
    if query.is_empty() { return; }
    let _t = timings.scope("actor_update");
    for (ent, mut loc0, mut position) in &mut query {
        let px = map.convert(**loc0);
        let qrz = map.convert(px + position.offset);
        if **loc0 != qrz {
            // Adjust offset to be relative to new tile center
            let world_pos = px + position.offset;
            let new_tile_center = map.convert(qrz);
            position.offset = world_pos - new_tile_center;
            position.tile = qrz;

            // Update Loc component directly
            **loc0 = qrz;

            // Send Loc update to client
            writer.write(Try { event: Event::Incremental { ent, component: Component::Loc(Loc::new(qrz)) } });
        }
    }
}

/// Broadcast heading changes to clients (ADR-011)
///
/// Detects when Heading components change and broadcasts them as Incremental events.
/// This ensures clients see NPCs facing the correct direction and can calculate proper
/// interpolation targets for remote players.
pub fn broadcast_heading_changes(
    mut writer: MessageWriter<Try>,
    query: Query<(Entity, &Heading), Changed<Heading>>,
) {
    for (ent, &heading) in &query {
        writer.write(Try {
            event: Event::Incremental {
                ent,
                component: Component::Heading(heading),
            },
        });
    }
}
