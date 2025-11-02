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
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
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
    mut reader: EventReader<Do>,
    mut writer: EventWriter<Try>,
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
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
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
    mut reader: EventReader<Try>,
    mut writer: EventWriter<Do>,
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

pub fn update(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    mut query: Query<(Entity, &Loc, &mut Offset, &Heading), Changed<Offset>>,
    map: Res<Map>,
    time: Res<Time>,
) {
    for (ent, &loc0, mut offset, &heading) in &mut query {
        let px = map.convert(*loc0);
        let qrz = map.convert(px + offset.state);
        if *loc0 != qrz {
            // Adjust offset to be relative to new tile center
            let world_pos = px + offset.state;
            let new_tile_center = map.convert(qrz);
            offset.state = world_pos - new_tile_center;

            // Update Loc component directly
            commands.entity(ent).insert(Loc::new(qrz));

            // Send Loc update to client
            writer.write(Try { event: Event::Incremental { ent, component: Component::Loc(Loc::new(qrz)) } });

            // Send Heading update so client can calculate proper interpolation target
            writer.write(Try { event: Event::Incremental { ent, component: Component::Heading(heading) } });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::{App, Update};
    use qrz::Qrz;
    use crate::common::components::heading::Heading;
    use crate::common::components::entity_type::actor::*;

    #[test]
    fn test_server_discovers_chunks_on_authoritative_loc_change() {
        // Setup
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));
        app.insert_resource(Terrain::default());
        app.init_resource::<WorldDiscoveryCache>();
        app.init_resource::<crate::common::resources::InputQueues>();

        app.add_systems(Update, (
            crate::common::systems::world::try_incremental,
            crate::common::systems::world::do_incremental,
            do_incremental.after(crate::common::systems::world::do_incremental),
            try_discover_chunk,
        ));

        // Create a player entity with PlayerDiscoveryState
        let player = app.world_mut().spawn((
            Loc::new(Qrz { q: 0, r: 0, z: 0 }),
            Heading::new(Qrz { q: 0, r: 1, z: 0 }), // south-east direction (valid DIRECTIONS entry)
            Offset::default(),
            EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player)),
            PlayerDiscoveryState::default(),
        )).id();

        app.update();

        // Clear any initial discovery events
        app.world_mut().resource_mut::<Events<Try>>().clear();

        // Act: Server changes player's Loc (simulating authoritative position update)
        // Moving from chunk (0,0) to a different location but staying in the same chunk
        app.world_mut().send_event(Try {
            event: Event::Incremental {
                ent: player,
                component: Component::Loc(Loc::new(Qrz { q: 1, r: 0, z: -1 })),
            }
        });

        app.update();

        // Run another update to process the events
        app.update();

        // Assert: Server should generate DiscoverChunk Try events based on new position
        let all_try_events: Vec<_> = {
            let mut try_reader = app.world_mut().resource_mut::<Events<Try>>().get_cursor();
            let try_events = app.world().resource::<Events<Try>>();
            try_reader.read(try_events).cloned().collect()
        };

        let chunk_discoveries: Vec<_> = all_try_events.iter()
            .filter_map(|t| {
                if let Try { event: Event::DiscoverChunk { ent, chunk_id } } = t {
                    Some((*ent, *chunk_id))
                } else {
                    None
                }
            })
            .collect();

        // Since we're at (1, 0, -1) which is in chunk (0, 0), and FOV_CHUNK_RADIUS=2,
        // we should discover chunks in a 5x5 area (radius 2 = -2 to +2)
        // That's 25 chunks total
        assert!(!chunk_discoveries.is_empty(), "Server should generate chunk discovery events when authoritative Loc changes");
        assert_eq!(chunk_discoveries.len(), 25, "Should discover 25 chunks (5x5 area with radius 2), got {}", chunk_discoveries.len());
        assert!(chunk_discoveries.iter().any(|(e, _)| *e == player), "Discoveries should be for the player entity");

        // Verify we got ChunkData Do events
        let all_do_events: Vec<_> = {
            let mut do_reader = app.world_mut().resource_mut::<Events<Do>>().get_cursor();
            let do_events = app.world().resource::<Events<Do>>();
            do_reader.read(do_events).cloned().collect()
        };

        let chunk_data_events: Vec<_> = all_do_events.iter()
            .filter_map(|d| {
                if let Do { event: Event::ChunkData { chunk_id, tiles, .. } } = d {
                    Some((*chunk_id, tiles.len()))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(chunk_data_events.len(), 25, "Should send 25 ChunkData events");
        // Each chunk should have up to 64 tiles (8x8)
        for (_chunk_id, tile_count) in &chunk_data_events {
            assert!(*tile_count <= 64, "Chunk should have at most 64 tiles");
        }
    }

    #[test]
    fn test_initial_spawn_discovers_chunks() {
        // Setup
        let mut app = App::new();
        app.add_event::<Do>();
        app.add_event::<Try>();
        app.insert_resource(Map::new(qrz::Map::<EntityType>::new(1., 0.8)));
        app.insert_resource(Terrain::default());
        app.init_resource::<WorldDiscoveryCache>();

        app.add_systems(Update, (
            do_spawn_discover,
            try_discover_chunk,
        ));

        // Create a player entity with PlayerDiscoveryState at spawn location
        let spawn_loc = Qrz { q: 0, r: 0, z: 4 };
        let player = app.world_mut().spawn((
            Loc::new(spawn_loc),
            EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player)),
            PlayerDiscoveryState::default(),
        )).id();

        // Send spawn event
        app.world_mut().send_event(Do {
            event: Event::Spawn {
                ent: player,
                typ: EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player)),
                qrz: spawn_loc,
                attrs: None,
            }
        });

        app.update();

        // Verify DiscoverChunk events were generated
        let all_try_events: Vec<_> = {
            let mut try_reader = app.world_mut().resource_mut::<Events<Try>>().get_cursor();
            let try_events = app.world().resource::<Events<Try>>();
            try_reader.read(try_events).cloned().collect()
        };

        let chunk_discoveries: Vec<_> = all_try_events.iter()
            .filter_map(|t| {
                if let Try { event: Event::DiscoverChunk { ent, chunk_id } } = t {
                    Some((*ent, *chunk_id))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(chunk_discoveries.len(), 25, "Should discover 25 initial chunks on spawn");
        assert!(chunk_discoveries.iter().all(|(e, _)| *e == player), "All discoveries should be for the player");

        // Verify player state was updated
        let player_state = app.world().get::<PlayerDiscoveryState>(player).unwrap();
        assert_eq!(player_state.seen_chunks.len(), 25, "Player should have 25 chunks in seen_chunks");
        assert_eq!(player_state.last_chunk, Some(ChunkId(0, 0)), "Player's last_chunk should be set");
    }
}
