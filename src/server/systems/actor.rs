use bevy::prelude::*;
use qrz::{Convert, Qrz};
use std::sync::Arc;

use crate::{
    common::{
        chunk::*,
        components::{
            entity_type::{ decorator::*, *},
            heading::Heading,
            offset::Offset,
            resources::RespawnTimer,
            *
        },
        message::{Component, Event, *},
        resources::map::*,
    },
    server::resources::terrain::*
};

/// Discover initial chunks when a player first spawns
pub fn do_spawn_discover(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_states: Query<&mut PlayerDiscoveryState>,
    query: Query<&Loc>,
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

        // Discover initial visible chunks
        let visible_chunks = calculate_visible_chunks(current_chunk, FOV_CHUNK_RADIUS);

        for chunk_id in visible_chunks {
            writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
            player_state.seen_chunks.insert(chunk_id);
        }

        player_state.last_chunk = Some(current_chunk);
    }
}

/// Server-side system: Generates Try::DiscoverChunk events when the server authoritatively changes an entity's Loc
/// Uses chunk-based boundary detection to reduce discovery events dramatically
pub fn do_incremental(
    mut reader: MessageReader<Do>,
    mut writer: MessageWriter<Try>,
    mut player_states: Query<&mut PlayerDiscoveryState>,
    _query: Query<(&Loc, &Heading)>,
) {
    for &message in reader.read() {
        let Do { event: Event::Incremental { ent, component } } = message else { continue; };

        // Only process Loc changes for chunk-based discovery
        let Component::Loc(loc) = component else { continue };

        // Get or skip if entity doesn't have PlayerDiscoveryState
        let Ok(mut player_state) = player_states.get_mut(ent) else { continue };

        let new_chunk = loc_to_chunk(*loc);

        // Skip if still in same chunk (most movements stay within chunk)
        if player_state.last_chunk == Some(new_chunk) {
            continue;
        }

        // Client evicts chunks outside FOV_CHUNK_RADIUS + 1 buffer
        // Mirror client's eviction logic: retain only chunks the client would keep
        let client_kept_chunks: std::collections::HashSet<_> =
            calculate_visible_chunks(new_chunk, FOV_CHUNK_RADIUS + 1)
            .into_iter()
            .collect();

        player_state.seen_chunks.retain(|chunk_id| client_kept_chunks.contains(chunk_id));

        // Calculate visible chunks based on FOV (what client can see)
        let visible_chunks = calculate_visible_chunks(new_chunk, FOV_CHUNK_RADIUS);

        for chunk_id in visible_chunks {
            if !player_state.seen_chunks.contains(&chunk_id) {
                writer.write(Try { event: Event::DiscoverChunk { ent, chunk_id } });
                player_state.seen_chunks.insert(chunk_id);
            }
        }

        player_state.last_chunk = Some(new_chunk);
    }
}

/// Generate a chunk of terrain tiles
fn generate_chunk(chunk_id: ChunkId, terrain: &Terrain, map: &Map) -> TerrainChunk {
    let mut tiles = tinyvec::ArrayVec::new();

    for offset_q in 0..CHUNK_SIZE as u8 {
        for offset_r in 0..CHUNK_SIZE as u8 {
            let qrz_base = chunk_to_tile(chunk_id, offset_q, offset_r);

            // Check if tile already exists in map (player-modified or pre-placed)
            let (qrz, typ) = if let Some((qrz, typ)) = map.find(qrz_base + Qrz{q:0,r:0,z:30}, -60) {
                (qrz, typ)
            } else {
                // Generate new procedural tile with actual terrain height
                // Use .xz() to get horizontal world coordinates (x=left-right, z=forward-back)
                let px = map.convert(qrz_base).xz();
                let z = terrain.get(px.x, px.y);
                let qrz = Qrz { q: qrz_base.q, r: qrz_base.r, z };
                let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
                (qrz, typ)
            };

            tiles.push((qrz, typ));
        }
    }

    TerrainChunk::new(tiles)
}

/// New chunk-based discovery system
pub fn try_discover_chunk(
    mut reader: MessageReader<Try>,
    mut writer: MessageWriter<Do>,
    mut world_cache: ResMut<WorldDiscoveryCache>,
    terrain: Res<Terrain>,
    mut map: ResMut<Map>,
    actors_query: Query<(
        Entity,
        &Loc,
        &EntityType,
        Option<&ActorAttributes>,
        Option<&crate::common::components::behaviour::PlayerControlled>,
        Option<&Heading>,
        Option<&crate::common::components::resources::Health>,
        Option<&crate::common::components::resources::Stamina>,
        Option<&crate::common::components::resources::Mana>,
        Option<&crate::common::components::resources::CombatState>,
    ), Without<RespawnTimer>>,
) {
    use crate::common::chunk::is_loc_in_chunk;

    for &message in reader.read() {
        if let Try { event: Event::DiscoverChunk { ent, chunk_id } } = message {
            // Check cache first
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
            // Design note: Server's map is authoritative persistent terrain state.
            // The chunk cache is only for network optimization (avoid regenerating same chunks).
            // When cache evicts a chunk, tiles remain in map so NPCs can still walk on them.
            // The is_none() check makes this idempotent (cache hit or miss both work).
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

            // Send all actors (NPCs and players) that are in this chunk
            for (actor_ent, actor_loc, actor_type, attrs, player_controlled, heading, health, stamina, mana, combat_state) in actors_query.iter() {
                if is_loc_in_chunk(**actor_loc, chunk_id) {
                    // Send Spawn + all actor components using shared helper
                    use crate::server::systems::world::generate_actor_spawn_events;
                    let spawn_events = generate_actor_spawn_events(
                        actor_ent,
                        *actor_type,
                        **actor_loc,
                        attrs.copied(),
                        player_controlled,
                        heading,
                        health,
                        stamina,
                        mana,
                        combat_state,
                    );

                    for event in spawn_events {
                        writer.write(event);
                    }
                }
            }
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
            if let Some((qrz, typ)) = map.find(qrz + Qrz{q:0,r:0,z:30}, -60) {
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            } else {
                // Use .xz() to get horizontal world coordinates (x=left-right, z=forward-back)
                let px = map.convert(qrz).xz();
                let qrz = Qrz { q:qrz.q, r:qrz.r, z:terrain.get(px.x, px.y)};
                let typ = EntityType::Decorator(Decorator { index: 3, is_solid: true });
                map.insert(qrz, typ);
                writer.write(Do { event: Event::Spawn { ent: Entity::PLACEHOLDER, typ, qrz, attrs: None } });
            }
        }
    }
}

/// Broadcast movement intent to nearby players (ADR-011 Phase 1)
///
/// Sends MovementIntent messages to enable client-side prediction.
/// Each intent is self-correcting by resetting interpolation from current visual position.
pub fn broadcast_movement_intent(
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
    mut query: Query<(Entity, &Loc, &Offset, Option<&mut crate::common::components::movement_intent_state::MovementIntentState>, Option<&ActorAttributes>), Changed<Offset>>,
    map: Res<Map>,
    time: Res<Time>,
) {
    for (ent, loc, offset, o_intent_state, attrs) in &mut query {
        let current_tile = **loc;

        // ADR-011: Predict destination based on movement direction, not current position
        // Broadcast intent when entity STARTS moving toward a tile, not when it arrives
        // This gives client time to predict before the Loc update arrives

        // Calculate movement direction and magnitude
        let offset_magnitude = offset.state.xz().length();

        // Skip if not moving significantly (stationary or very small movement)
        const MIN_MOVEMENT_THRESHOLD: f32 = 0.2; // Broadcast when moving at least 0.2 units
        if offset_magnitude < MIN_MOVEMENT_THRESHOLD {
            continue;
        }

        // Find which adjacent tile we're moving toward
        // Check all 6 hex directions to find closest match to our movement direction
        let offset_dir = offset.state.xz().normalize();
        let mut best_neighbor = current_tile;
        let mut best_dot = -1.0;

        for direction in qrz::DIRECTIONS.iter() {
            let neighbor = current_tile + *direction;
            let neighbor_world = map.convert(neighbor);
            let to_neighbor = (neighbor_world - map.convert(current_tile)).xz().normalize();
            let dot = offset_dir.dot(to_neighbor);

            if dot > best_dot {
                best_dot = dot;
                best_neighbor = neighbor;
            }
        }

        let dest_tile = best_neighbor;

        // Skip if not clearly moving toward a different tile (dot product threshold)
        const DIRECTION_THRESHOLD: f32 = 0.5; // Must be moving reasonably toward the tile
        if dest_tile == current_tile || best_dot < DIRECTION_THRESHOLD {
            continue;
        }

        // Calculate heading from movement direction
        use crate::common::components::heading::Heading;
        let movement_direction = dest_tile - current_tile;
        let heading = Heading::new(movement_direction);

        // Get or initialize MovementIntentState
        let mut intent_state = if let Some(state) = o_intent_state {
            state
        } else {
            // First time seeing this entity - add component
            commands.entity(ent).insert(crate::common::components::movement_intent_state::MovementIntentState::default());
            continue; // Will process next frame after component is added
        };

        // Skip if already broadcast this destination and heading
        if dest_tile == intent_state.last_broadcast_dest && heading == intent_state.last_broadcast_heading {
            continue;
        }

        // Calculate expected duration based on movement speed
        let movement_speed = attrs.map(|a| a.movement_speed()).unwrap_or(0.005);
        let current_world_pos = map.convert(current_tile) + offset.state;
        let dest_world_pos = map.convert(dest_tile);
        let distance = (dest_world_pos - current_world_pos).length();
        let duration_ms = (distance / movement_speed) as u16;

        // Update broadcast state
        intent_state.last_broadcast_dest = dest_tile;
        intent_state.last_broadcast_heading = heading;

        // Broadcast intent to all clients (will add relevance filtering in Phase 2)
        writer.write(Do {
            event: Event::MovementIntent {
                ent,
                destination: dest_tile,
                duration_ms,
            }
        });
    }
}

pub fn update(
    mut writer: MessageWriter<Try>,
    mut query: Query<(Entity, &mut Loc, &mut Offset), Changed<Offset>>,
    map: Res<Map>,
) {
    for (ent, mut loc0, mut offset) in &mut query {
        let px = map.convert(**loc0);
        let qrz = map.convert(px + offset.state);
        if **loc0 != qrz {
            // Adjust offset to be relative to new tile center
            let world_pos = px + offset.state;
            let new_tile_center = map.convert(qrz);
            offset.state = world_pos - new_tile_center;

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
