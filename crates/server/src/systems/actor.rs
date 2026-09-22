use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use qrz::Qrz;
use std::sync::Arc;

use common_bevy::{
    chunk::{self, *},
    components::{
        behaviour::PlayerControlled,
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
pub const MAX_CHUNK_TASKS: usize = 16;

/// In-flight async chunk generation tasks.
/// Task returns (chunk, duration_ms) so we can report async metrics.
#[derive(Resource, Default)]
pub struct ChunkTaskQueue {
    tasks: Vec<(ChunkId, Task<(TerrainChunk, f32)>)>,
    /// Chunks currently being generated (avoid duplicate tasks).
    pub in_flight: std::collections::HashSet<ChunkId>,
    /// Cache-missed chunks awaiting a task slot, drained nearest-first.
    pending: Vec<ChunkId>,
    /// Every player waiting on a chunk queued or in flight. Each is sent it
    /// when it lands: a chunk is generated once, but a second player's
    /// request for it is never lost to the first's.
    waiters: std::collections::HashMap<ChunkId, Vec<Entity>>,
}

impl ChunkTaskQueue {
    /// Records `ent` as waiting on `chunk_id`. True when the chunk is newly
    /// queued; false when it was queued or in flight already, so the request
    /// joins the waiters instead of raising a second task.
    fn enqueue(&mut self, chunk_id: ChunkId, ent: Entity) -> bool {
        match self.waiters.entry(chunk_id) {
            std::collections::hash_map::Entry::Occupied(mut waiting) => {
                if !waiting.get().contains(&ent) {
                    waiting.get_mut().push(ent);
                }
                false
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(vec![ent]);
                self.pending.push(chunk_id);
                true
            }
        }
    }

    /// Everyone waiting on `chunk_id`, released to be sent it.
    fn release(&mut self, chunk_id: ChunkId) -> Vec<Entity> {
        self.waiters.remove(&chunk_id).unwrap_or_default()
    }

    /// Chunks waiting on a task slot. What the queue is behind by: the
    /// in-flight count only says the budget is spent.
    pub fn pending_len(&self) -> usize { self.pending.len() }
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
    let mut tiles: tinyvec::ArrayVec<[(Qrz, EntityType, Option<i32>); 272]> = tinyvec::ArrayVec::new();
    let coords: Vec<(i32, i32)> = chunk::chunk_tiles(chunk_id).collect();

    for &(q, r) in &coords {
        let z = registry.elevation_at(q, r);
        let water = registry.water_at(q, r);
        let cover = registry.cover_at(q, r);
        let qrz = Qrz { q, r, z };
        let typ = EntityType::Decorator(Decorator { cover, is_solid: true });
        tiles.push((qrz, typ, water));
    }

    TerrainChunk::new(tiles)
}

/// Merge a chunk's tiles into the server Map for physics, collision and AI,
/// leaving ground already there alone, and build the tiles as sent.
fn merge_and_pack(chunk: &TerrainChunk, map: &Map) -> tinyvec::ArrayVec<[(i32, EntityType, Option<i32>); 272]> {
    for &(qrz, typ, water) in &chunk.tiles {
        if map.get(qrz).is_none() {
            map.insert(qrz, typ);
            map.set_water(qrz.q, qrz.r, water);
        }
    }
    chunk.tiles.iter().map(|&(qrz, typ, water)| (qrz.z, typ, water)).collect()
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
                send_cached_chunk(&[ent], chunk_id, &mut world_cache, &*map, &mut writer);
                continue;
            }

            // Queued or in flight already: joins its waiters, served on completion.
            task_queue.enqueue(chunk_id, ent);
        }
    }

    // Drain pending nearest-first under the task budget. Runs every frame so
    // the backlog keeps flowing as tasks complete.
    if !task_queue.pending.is_empty() && task_queue.in_flight.len() < MAX_CHUNK_TASKS {
        // Farthest-first sort, by each chunk's nearest waiter, so the nearest
        // pops from the back.
        let mut pending = std::mem::take(&mut task_queue.pending);
        pending.sort_by_key(|&chunk_id| {
            let dist = task_queue.waiters.get(&chunk_id).into_iter().flatten()
                .filter_map(|&ent| locs.get(ent).ok())
                .map(|loc| chunk_hex_distance(loc_to_chunk(**loc), chunk_id))
                .min()
                .unwrap_or(i32::MAX);
            std::cmp::Reverse(dist)
        });

        while task_queue.in_flight.len() < MAX_CHUNK_TASKS {
            let Some(chunk_id) = pending.pop() else { break };

            // Generated for another request while queued: send now.
            if world_cache.chunks.contains_key(&chunk_id) {
                let waiting = task_queue.release(chunk_id);
                send_cached_chunk(&waiting, chunk_id, &mut world_cache, &*map, &mut writer);
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
            task_queue.tasks.push((chunk_id, task));
        }

        task_queue.pending = pending;
    }
}

/// Send a cached chunk to every client in `ents` and merge its tiles into
/// the server Map.
fn send_cached_chunk(
    ents: &[Entity],
    chunk_id: ChunkId,
    world_cache: &mut WorldDiscoveryCache,
    map: &Map,
    writer: &mut MessageWriter<Do>,
) {
    world_cache.access_order.get_or_insert(chunk_id, || ());
    let chunk = Arc::clone(world_cache.chunks.get(&chunk_id).unwrap());
    let wire_tiles = merge_and_pack(&chunk, map);
    for &ent in ents {
        writer.write(Do {
            event: Event::ChunkData { ent, chunk_id, tiles: wire_tiles.clone() }
        });
    }
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

    for (chunk_id, mut task) in current {
        if let Some((chunk, duration_ms)) = block_on(poll_once(&mut task)) {
            _t.get_or_insert_with(|| timings.scope("chunk_poll"));
            snapshot.record(&[("chunk.dur_ms", duration_ms)]);
            let chunk = Arc::new(chunk);

            // Insert into world cache
            if world_cache.chunks.len() >= world_cache.max_chunks {
                if let Some((evicted_id, _)) = world_cache.access_order.pop_lru() {
                    world_cache.chunks.remove(&evicted_id);
                }
            }
            world_cache.chunks.insert(chunk_id, Arc::clone(&chunk));
            world_cache.access_order.get_or_insert(chunk_id, || ());

            let wire_tiles = merge_and_pack(&chunk, &map);
            for ent in task_queue.release(chunk_id) {
                writer.write(Do {
                    event: Event::ChunkData { ent, chunk_id, tiles: wire_tiles.clone() }
                });
            }

            task_queue.in_flight.remove(&chunk_id);
        } else {
            pending.push((chunk_id, task));
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
        let qrz = position.reached(&map);
        if **loc0 != qrz {
            position.rebase(qrz, &map);
            **loc0 = qrz;

            // Send Loc update to client
            writer.write(Try { event: Event::Incremental { ent, component: Component::Loc(Loc::new(qrz)) } });
        }
    }
}

/// Puts a player on the ground at the tile it asks for, as a spawn does:
/// standing on the terrain there, whatever the map holds yet, and the tile
/// broadcast so the stream, the area of interest and every client follow.
/// Any connection may move its own entity; there is no admin identity on
/// the wire.
pub fn try_teleport(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut query: Query<(&mut Loc, &mut Position, &mut AirTime), With<PlayerControlled>>,
    registry: Res<EventRegistry>,
) {
    for message in reader.read() {
        let Try { event: Event::Teleport { ent, q, r } } = message else { continue };
        let (ent, q, r) = (*ent, *q, *r);
        let Ok((mut loc, mut position, mut airtime)) = query.get_mut(ent) else { continue };
        let qrz = Qrz { q, r, z: registry.elevation_at(q, r) + 1 };
        *loc = Loc::new(qrz);
        *position = Position::at_tile(qrz);
        airtime.state = None;
        info!("teleport: {ent} to {qrz:?}");
        writer.write(Do { event: Event::Incremental { ent, component: Component::Loc(*loc) } });
    }
}

/// Broadcast heading changes to clients

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

#[cfg(test)]
mod chunk_queue_tests {
    use super::*;

    #[test]
    fn a_second_request_for_a_queued_chunk_joins_its_waiters() {
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let mut queue = ChunkTaskQueue::default();
        let chunk = ChunkId(3, -2);

        assert!(queue.enqueue(chunk, a));
        assert!(!queue.enqueue(chunk, b));
        assert!(!queue.enqueue(chunk, a));
        assert_eq!(queue.pending, vec![chunk]);

        assert_eq!(queue.release(chunk), vec![a, b]);
        assert!(queue.release(chunk).is_empty());
    }

    #[test]
    fn a_chunk_in_flight_still_collects_waiters() {
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let mut queue = ChunkTaskQueue::default();
        let chunk = ChunkId(0, 0);

        assert!(queue.enqueue(chunk, a));
        queue.pending.clear();
        queue.in_flight.insert(chunk);
        assert!(!queue.enqueue(chunk, b));
        assert!(queue.pending.is_empty());
        assert_eq!(queue.release(chunk), vec![a, b]);
    }
}
