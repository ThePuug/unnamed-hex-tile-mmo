use bevy::prelude::*;
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
use crate::resources::terrain::*;




/// Cached chunk set, recomputed on chunk boundary crossings.
/// Tracks which chunks have been sent to this player so we only send new ones.
#[derive(bevy::prelude::Component)]
pub struct VisibleChunkCache {
    /// All chunks currently sent to this player
    pub sent: std::collections::HashSet<ChunkId>,
    /// Chunk this was computed for (detect boundary crossings)
    pub chunk_id: ChunkId,
}

/// Discover initial chunks when a player first spawns
pub fn do_spawn_discover(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_states: Query<&mut PlayerDiscoveryState>,
    query: Query<&Loc>,
    terrain: Res<Terrain>,
) {
    for message in reader.read() {
        let Do { event: Event::Spawn { ent, .. } } = message else { continue };
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

        // Send radius matches client eviction: terrain_chunk_radius + 1
        let max_z = chunk_max_z(current_chunk, |q, r| terrain.get(q, r));
        let send_radius = terrain_chunk_radius(max_z) as i32 + 1;

        let chunks = calculate_visible_chunks(current_chunk, send_radius as u8);

        for &chunk_id in &chunks {
            writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
            player_state.seen_chunks.insert(chunk_id);
        }

        player_state.last_chunk = Some(current_chunk);

        commands.entity(ent).insert(VisibleChunkCache {
            sent: chunks.into_iter().collect(),
            chunk_id: current_chunk,
        });
    }
}

/// Server-side system: Generates Try::DiscoverChunk events when the server authoritatively changes an entity's Loc.
/// Uses chunk-based boundary detection to reduce discovery events dramatically.
pub fn do_incremental(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut do_writer: MessageWriter<Do>,
    mut player_queries: Query<(&mut PlayerDiscoveryState, &mut VisibleChunkCache)>,
    terrain: Res<Terrain>,
) {
    for message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };
        let ent = *ent;
        let component = *component;

        // Only process Loc changes for chunk-based discovery
        let Component::Loc(loc) = component else { continue };

        let Ok((mut player_state, mut cache)) = player_queries.get_mut(ent) else { continue };

        let new_chunk = loc_to_chunk(*loc);

        // Skip if still in same chunk (cache is still valid)
        if cache.chunk_id == new_chunk {
            continue;
        }

        // Boundary crossing — recompute send radius (matches client eviction)
        let max_z = chunk_max_z(new_chunk, |q, r| terrain.get(q, r));
        let send_radius = terrain_chunk_radius(max_z) as i32 + 1;

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
                do_writer.write(Do { event: Event::EvictChunks { ent, chunks } });
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

/// Generate a chunk of terrain tiles
fn generate_chunk(chunk_id: ChunkId, terrain: &Terrain, map: &Map) -> TerrainChunk {
    let mut tiles = tinyvec::ArrayVec::new();

    for (q, r) in chunk::chunk_tiles(chunk_id) {
        // Check if tile already exists in map (player-modified or pre-placed)
        let (qrz, typ) = if let Some((qrz, typ)) = map.get_by_qr(q, r) {
            (qrz, typ)
        } else {
            // Generate new procedural tile with actual terrain height
            let z = terrain.get(q, r);
            let qrz = Qrz { q, r, z };
            let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
            (qrz, typ)
        };

        tiles.push((qrz, typ));
    }

    TerrainChunk::new(tiles)
}

/// Generates terrain for discovered chunks, inserts into Map, and sends to clients.
/// Actor discovery is handled by the AOI system (aoi.rs) via LoadedBy tracking.
pub fn try_discover_chunk(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    terrain: Res<Terrain>,
    mut map: ResMut<Map>,
) {
    for message in reader.read() {
        if let Try { event: Event::DiscoverChunk { ent, chunk_id } } = message {
            let ent = *ent;
            let chunk_id = *chunk_id;

            // Generate/cache chunk
            let chunk = if world_cache.chunks.contains_key(&chunk_id) {
                world_cache.access_order.get_or_insert(chunk_id, || ());
                Arc::clone(world_cache.chunks.get(&chunk_id).unwrap())
            } else {
                let generated = Arc::new(generate_chunk(chunk_id, &terrain, &map));

                if world_cache.chunks.len() >= world_cache.max_chunks {
                    if let Some((evicted_id, _)) = world_cache.access_order.pop_lru() {
                        world_cache.chunks.remove(&evicted_id);
                    }
                }

                world_cache.chunks.insert(chunk_id, Arc::clone(&generated));
                world_cache.access_order.get_or_insert(chunk_id, || ());

                generated
            };

            // Insert tiles into server Map for physics/collision/AI
            for &(qrz, typ) in &chunk.tiles {
                if map.get(qrz).is_none() {
                    map.insert(qrz, typ);
                }
            }

            // Send ChunkData to client
            writer.write(Do {
                event: Event::ChunkData {
                    ent,
                    chunk_id,
                    tiles: chunk.tiles.clone(),
                }
            });
        }
    }
}

/// Legacy tile-based discovery system (kept for compatibility, may be removed later)
pub fn try_discover(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut map: ResMut<Map>,
    terrain: Res<Terrain>,
    query: Query<(&Loc, &EntityType)>,
) {
    for message in reader.read() {
        if let Try { event: Event::Discover { ent, qrz } } = message {
            let ent = *ent;
            let qrz = *qrz;
            let (&loc, _) = query.get(ent).unwrap();
            if loc.flat_distance(&qrz) > 25 { continue; }
            if let Some((qrz, typ)) = map.get_by_qr(qrz.q, qrz.r) {
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            } else {
                let qrz = Qrz { q:qrz.q, r:qrz.r, z:terrain.get(qrz.q, qrz.r)};
                let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
                map.insert(qrz, typ);
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            }
        }
    }
}

pub fn update(
    mut writer: MessageWriter<Try>,
    mut query: Query<(Entity, &mut Loc, &mut Position), Changed<Position>>,
    map: Res<Map>,
) {
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
