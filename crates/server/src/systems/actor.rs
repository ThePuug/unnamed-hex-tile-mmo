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

/// Cached per-chunk visibility sets, recomputed on chunk boundary crossings.
///
/// Uses `chunk_max_z` as worst-case player height so the result is valid
/// for any position within the chunk. Tracks inner (full detail) and outer
/// (summary LoD) rings separately for upgrade/downgrade detection.
#[derive(bevy::prelude::Component)]
pub struct VisibleChunkCache {
    /// Chunks sent with full tile data (inner ring, ≤ detail_boundary_radius)
    pub full_detail: std::collections::HashSet<ChunkId>,
    /// Chunks sent as summaries (outer ring, > detail_boundary_radius)
    pub summary: std::collections::HashSet<ChunkId>,
    /// Chunks the player might still have (visibility + 1 buffer)
    pub eviction: std::collections::HashSet<ChunkId>,
    /// Chunk this was computed for (detect boundary crossings)
    pub chunk_id: ChunkId,
}

/// Compute the set of chunks the client would keep (visibility + 1 buffer per chunk).
/// Mirrors the client's per-chunk eviction logic so the server knows when to re-send.
fn compute_eviction_set(
    center: ChunkId,
    player_z: i32,
    base_radius: u8,
    max_radius: u8,
    terrain: &Terrain,
) -> std::collections::HashSet<ChunkId> {
    let r = (max_radius + 1) as i32;
    let base_plus_buffer = base_radius as i32 + 1;
    let mut kept = std::collections::HashSet::new();

    for dq in -r..=r {
        let dr_min = (-r).max(-dq - r);
        let dr_max = r.min(-dq + r);
        for dr in dr_min..=dr_max {
            let hex_dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
            let chunk_id = ChunkId(center.0 + dq, center.1 + dr);

            if hex_dist <= base_plus_buffer {
                kept.insert(chunk_id);
                continue;
            }

            let center_tile = chunk_id.center();
            let chunk_z = terrain.get(center_tile.q, center_tile.r);
            let vis = visibility_radius(player_z, chunk_z, MAX_FOV) as i32 + 1;

            if hex_dist <= vis {
                kept.insert(chunk_id);
            }
        }
    }

    kept
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
    for &message in reader.read() {
        let Do { event: Event::Spawn { ent, .. } } = message else { continue };

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

        // Use max elevation in player's chunk as worst-case height (superset of
        // what any position in this chunk could see)
        let max_z = chunk_max_z(current_chunk, |q, r| terrain.get(q, r));
        // Base radius: old symmetric loading (floor). Max: uncapped for valley extension.
        let base_radius = terrain_chunk_radius(max_z);
        let max_radius = elevation_chunk_radius_raw(max_z);

        // Detail boundary at MAX_FOV: server sends full data for everything
        // within the worst-case (widest zoom) detail radius.
        let detail_radius = chunk::detail_boundary_radius(max_z, MAX_FOV);

        // Discover initial visible chunks (adaptive per-chunk filtering)
        let (inner, outer) = calculate_visible_chunks_adaptive(
            current_chunk, max_z, base_radius, max_radius, detail_radius, MAX_FOV,
            |q, r| terrain.get(q, r),
        );

        // Eviction set: per-chunk visibility + 1 buffer (mirrors client eviction)
        let eviction = compute_eviction_set(current_chunk, max_z, base_radius, max_radius, &terrain);

        // Inner ring: send full chunk data
        for &chunk_id in &inner {
            writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id, summary_only: false } });
            player_state.seen_chunks.insert(chunk_id);
        }

        // Outer ring: send summaries (lightweight)
        for &chunk_id in &outer {
            writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id, summary_only: true } });
            player_state.seen_chunks.insert(chunk_id);
        }

        player_state.last_chunk = Some(current_chunk);

        // Cache visibility sets for boundary-crossing detection
        commands.entity(ent).insert(VisibleChunkCache {
            full_detail: inner.into_iter().collect(),
            summary: outer.into_iter().collect(),
            eviction,
            chunk_id: current_chunk,
        });
    }
}

/// Server-side system: Generates Try::DiscoverChunk events when the server authoritatively changes an entity's Loc
/// Uses chunk-based boundary detection to reduce discovery events dramatically.
/// Adaptive per-chunk visibility extends loading toward valleys and tightens toward ridges.
pub fn do_incremental(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_queries: Query<(&mut PlayerDiscoveryState, &mut VisibleChunkCache)>,
    terrain: Res<Terrain>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        // Only process Loc changes for chunk-based discovery
        let Component::Loc(loc) = component else { continue };

        let Ok((mut player_state, mut cache)) = player_queries.get_mut(ent) else { continue };

        let new_chunk = loc_to_chunk(*loc);

        // Skip if still in same chunk (cache is still valid)
        if cache.chunk_id == new_chunk {
            continue;
        }

        // Boundary crossing — recompute adaptive visibility
        let max_z = chunk_max_z(new_chunk, |q, r| terrain.get(q, r));
        let base_radius = terrain_chunk_radius(max_z);
        let max_radius = elevation_chunk_radius_raw(max_z);

        let detail_radius = chunk::detail_boundary_radius(max_z, MAX_FOV);

        let (new_inner, new_outer) = calculate_visible_chunks_adaptive(
            new_chunk, max_z, base_radius, max_radius, detail_radius, MAX_FOV,
            |q, r| terrain.get(q, r),
        );
        let new_eviction = compute_eviction_set(new_chunk, max_z, base_radius, max_radius, &terrain);

        // Mirror client eviction: full-detail uses detail_radius + 2 (inner ring
        // extends to detail+1, buffer adds 1 more for boundary crossing).
        let detail_buffer = detail_radius as i32 + 2;
        cache.full_detail.retain(|id| {
            chunk_hex_distance(*id, new_chunk) <= detail_buffer
        });
        cache.summary.retain(|id| new_eviction.contains(id));
        player_state.seen_chunks.retain(|id| cache.full_detail.contains(id) || cache.summary.contains(id));

        // Inner ring: send full data for new chunks or upgrades (was summary → now inner)
        for &chunk_id in &new_inner {
            if !cache.full_detail.contains(&chunk_id) {
                writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id, summary_only: false } });
                player_state.seen_chunks.insert(chunk_id);
                cache.full_detail.insert(chunk_id);
                cache.summary.remove(&chunk_id);
            }
        }

        // Outer ring: send summaries for new chunks and boundary ring chunks
        // that have full detail but no summary yet (zone 2 needs both).
        for &chunk_id in &new_outer {
            if !cache.summary.contains(&chunk_id) {
                writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id, summary_only: true } });
                player_state.seen_chunks.insert(chunk_id);
                cache.summary.insert(chunk_id);
            }
        }

        player_state.last_chunk = Some(new_chunk);

        // Update cache
        cache.eviction = new_eviction;
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

/// New chunk-based discovery system
///
/// Generates terrain for discovered chunks and sends to clients.
/// When `summary_only` is true, sends a lightweight ChunkSummary (~12 bytes)
/// instead of full ChunkData (~2.6KB). Summary-only chunks skip server-side
/// terrain generation entirely — no map insertion, no cache usage.
/// Actor discovery is handled by the AOI system (aoi.rs) via LoadedBy tracking.
pub fn try_discover_chunk(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    terrain: Res<Terrain>,
    mut map: ResMut<Map>,
) {
    for &message in reader.read() {
        if let Try { event: Event::DiscoverChunk { ent, chunk_id, summary_only } } = message {
            if summary_only {
                // Summary mode: compute representative elevation, send lightweight summary
                let center = chunk_id.center();
                let elevation = terrain.get(center.q, center.r);
                writer.write(Do {
                    event: Event::ChunkSummary {
                        ent,
                        chunk_id,
                        elevation,
                        biome: EntityType::Decorator(Decorator { index: 3, is_solid: true }),
                    }
                });
                continue;
            }

            // Full detail mode: generate chunk, cache, insert into map, send data
            let chunk = if world_cache.chunks.contains_key(&chunk_id) {
                // Cache hit - update LRU and clone Arc
                world_cache.access_order.get_or_insert(chunk_id, || ());
                Arc::clone(world_cache.chunks.get(&chunk_id).unwrap())
            } else {
                // Cache miss - generate chunk
                let generated = Arc::new(generate_chunk(chunk_id, &terrain, &map));

                // Insert into cache (may trigger LRU eviction)
                if world_cache.chunks.len() >= world_cache.max_chunks {
                    if let Some((evicted_id, _)) = world_cache.access_order.pop_lru() {
                        world_cache.chunks.remove(&evicted_id);
                    }
                }

                world_cache.chunks.insert(chunk_id, Arc::clone(&generated));
                world_cache.access_order.get_or_insert(chunk_id, || ());

                generated
            };

            // Insert tiles into server's map for physics collision detection
            for &(qrz, typ) in &chunk.tiles {
                if map.get(qrz).is_none() {
                    map.insert(qrz, typ);
                }
            }

            // Send chunk terrain to player
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
    for &message in reader.read() {
        if let Try { event: Event::Discover { ent, qrz } } = message {
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
