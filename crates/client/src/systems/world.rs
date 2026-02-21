use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*,
    tasks::{block_on, futures_lite::future},
};

pub const TILE_SIZE: f32 = 1.;

use crate::{
    components::{ChunkMesh, SummaryChunk},
    plugins::diagnostics::DiagnosticsState,
    resources::{ChunkSummaries, LoadedChunks, PendingChunkMeshes, Server, SkipNeighborRegen, TerrainMaterial},
};
use common::{
    chunk::{chunk_to_tile, loc_to_chunk, terrain_chunk_radius, visibility_radius, CHUNK_SIZE},
    components::{ *,
        behaviour::PlayerControlled,
        entity_type::*,
    },
    message::{Event, *},
    systems::*,
};

pub fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(
        GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 10.,
            ..default()});

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            shadow_depth_bias: 0.02,
            shadow_normal_bias: 0.6,
            ..default()},
        Transform::default(),
        Sun::default()));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            color: Color::WHITE,
            ..default()},
        Transform::default(),
        Moon::default()));

    // Initialize shared terrain material (white to let vertex colors show through)
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.,
        ..default()
    });
    commands.insert_resource(TerrainMaterial { handle: material });
}

/// Scan the map and spawn mesh generation tasks for chunks that have tiles but no meshes
/// This system runs periodically and automatically generates meshes after the drain loop
/// processes tile events, avoiding the need for coordination between systems
pub fn spawn_missing_chunk_meshes(
    map: Res<common::resources::map::Map>,
    chunk_mesh_query: Query<&ChunkMesh, Without<SummaryChunk>>,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    diagnostics_state: Res<DiagnosticsState>,
    skip_neighbor_regen: Res<SkipNeighborRegen>,
    loaded_chunks: Res<LoadedChunks>,
) {
    use std::collections::HashSet;
    use common::chunk::{ChunkId, calculate_visible_chunks, loc_to_chunk};
    use bevy::tasks::AsyncComputeTaskPool;

    // Get chunks that already have full-detail mesh entities (exclude summary LoD)
    let chunks_with_meshes: HashSet<ChunkId> = chunk_mesh_query
        .iter()
        .map(|mesh| mesh.chunk_id)
        .collect();

    // Scan the map to find which chunks have tiles, but only consider chunks
    // that are actively tracked. Tiles queued for despawn may still linger in
    // the map snapshot — without this filter we'd re-mesh evicted chunks.
    // Accept: server-loaded chunks (LoadedChunks) and admin-generated chunks
    // (SkipNeighborRegen, which tracks flyover-generated chunks).
    let mut chunks_with_tiles: HashSet<ChunkId> = HashSet::new();
    for (qrz, _) in map.iter_tiles() {
        let chunk = loc_to_chunk(qrz);
        if loaded_chunks.chunks.contains(&chunk) || skip_neighbor_regen.chunks.contains(&chunk) {
            chunks_with_tiles.insert(chunk);
        }
    }

    // Spawn mesh tasks for chunks with tiles but no mesh (and no pending task)
    let pool = AsyncComputeTaskPool::get();

    for chunk_id in &chunks_with_tiles {
        // Skip if mesh already exists or task is pending
        if chunks_with_meshes.contains(chunk_id) || pending_meshes.tasks.contains_key(chunk_id) {
            continue;
        }

        // Clone Map (O(1) Arc clone) for async task
        let map_snapshot = map.clone();
        let apply_slopes = diagnostics_state.slope_rendering_enabled;
        let chunk_id = *chunk_id; // Copy ChunkId for async move

        let task = pool.spawn(async move {
            map_snapshot.generate_chunk_mesh(chunk_id, apply_slopes)
        });

        pending_meshes.tasks.insert(chunk_id, task);
    }

    // Also regenerate adjacent chunks when a new chunk appears (fixes edge vertices).
    // Skip cascade for chunks in SkipNeighborRegen — their meshes were generated with
    // complete neighbor data already present (e.g. admin flyover buffer zone).
    let mut chunks_to_regenerate: HashSet<ChunkId> = HashSet::new();
    for chunk_id in &chunks_with_tiles {
        // If this chunk is new (no mesh yet) and not marked to skip cascade
        if !chunks_with_meshes.contains(chunk_id) && !skip_neighbor_regen.chunks.contains(chunk_id) {
            for adjacent_chunk in calculate_visible_chunks(*chunk_id, 1) {
                if chunks_with_tiles.contains(&adjacent_chunk) && chunks_with_meshes.contains(&adjacent_chunk) {
                    chunks_to_regenerate.insert(adjacent_chunk);
                }
            }
        }
    }

    // Regenerate neighbor chunks (cancel pending tasks first)
    for chunk_id in chunks_to_regenerate {
        pending_meshes.tasks.remove(&chunk_id);

        let map_snapshot = map.clone();
        let apply_slopes = diagnostics_state.slope_rendering_enabled;

        let task = pool.spawn(async move {
            map_snapshot.generate_chunk_mesh(chunk_id, apply_slopes)
        });

        pending_meshes.tasks.insert(chunk_id, task);
    }
}

/// Poll pending chunk mesh generation tasks and spawn/update mesh entities when ready
pub fn poll_chunk_mesh_tasks(
    mut commands: Commands,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    chunk_mesh_query: Query<(Entity, &Mesh3d, &ChunkMesh), Without<SummaryChunk>>,
) {
    use common::chunk::ChunkId;

    let mut completed_chunks: Vec<(ChunkId, Mesh)> = Vec::new();

    // Poll all pending tasks (non-blocking)
    pending_meshes.tasks.retain(|&chunk_id, task| {
        let result = block_on(future::poll_once(task));
        if let Some((chunk_mesh, _aabb)) = result {
            completed_chunks.push((chunk_id, chunk_mesh));
            false // Remove from pending
        } else {
            true // Keep pending
        }
    });

    // Spawn or update mesh entities for completed tasks
    if !completed_chunks.is_empty() {
        for (chunk_id, chunk_mesh) in completed_chunks {
            // Check if mesh entity for this chunk already exists
            let existing_entity = chunk_mesh_query.iter()
                .find(|(_, _, c)| c.chunk_id == chunk_id)
                .map(|(entity, mesh_handle, _)| (entity, mesh_handle.clone()));

            if let Some((_entity, mesh_handle)) = existing_entity {
                // Update existing mesh
                if let Some(mesh_asset) = meshes.get_mut(&mesh_handle.0) {
                    *mesh_asset = chunk_mesh;
                }
            } else {
                // Spawn new mesh entity
                commands.spawn((
                    Mesh3d(meshes.add(chunk_mesh)),
                    MeshMaterial3d(terrain_material.handle.clone()),
                    ChunkMesh { chunk_id },
                ));
            }
        }
    }
}

/// Spawn or update mesh entities for chunk summaries (outer ring LoD).
/// Regenerates neighbor meshes when new summaries appear so interpolation stays consistent.
pub fn spawn_summary_meshes(
    mut commands: Commands,
    summaries: Res<ChunkSummaries>,
    map: Res<common::resources::map::Map>,
    terrain_material: Res<TerrainMaterial>,
    existing_meshes: Query<(Entity, &ChunkMesh), With<SummaryChunk>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut prev_keys: Local<std::collections::HashSet<common::chunk::ChunkId>>,
) {
    use std::collections::HashSet;
    use common::chunk::ChunkId;

    if !summaries.is_changed() {
        return;
    }

    let current_keys: HashSet<ChunkId> = summaries.summaries.keys().copied().collect();

    // Identify added and removed chunks
    let added: HashSet<ChunkId> = current_keys.difference(&prev_keys).copied().collect();
    let removed: HashSet<ChunkId> = prev_keys.difference(&current_keys).copied().collect();

    // Chunks needing mesh (re)generation: new chunks + existing neighbors of new/removed
    let mut regen: HashSet<ChunkId> = added.clone();
    for changed in added.iter().chain(removed.iter()) {
        for dq in -1..=1_i32 {
            for dr in -1..=1_i32 {
                if dq == 0 && dr == 0 { continue; }
                let nid = ChunkId(changed.0 + dq, changed.1 + dr);
                if current_keys.contains(&nid) {
                    regen.insert(nid);
                }
            }
        }
    }

    // Despawn removed + stale meshes
    for (entity, chunk_mesh) in existing_meshes.iter() {
        if removed.contains(&chunk_mesh.chunk_id) || regen.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).despawn();
        }
    }

    // Spawn meshes for new/regenerated chunks
    for &chunk_id in &regen {
        if !current_keys.contains(&chunk_id) { continue; }
        let mesh = generate_summary_mesh(chunk_id, &summaries.summaries, &map);
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(terrain_material.handle.clone()),
            ChunkMesh { chunk_id },
            SummaryChunk,
        ));
    }

    *prev_keys = current_keys;
}

/// Generate a hex-grid mesh for a chunk summary.
///
/// Elevation is bilinearly interpolated PER VERTEX (not per tile) from this chunk
/// and its 8 neighbors. Since adjacent hex tiles share edge vertices at the same
/// axial position, both tiles compute the same interpolated elevation for shared
/// vertices — eliminating seams. Each vertex is colored by `height_color_tint`.
fn generate_summary_mesh(
    chunk_id: common::chunk::ChunkId,
    summaries: &std::collections::HashMap<common::chunk::ChunkId, common::chunk::ChunkSummary>,
    map: &common::resources::map::Map,
) -> Mesh {
    use common::chunk::ChunkId;
    use common::resources::map::Map as CommonMap;
    use qrz::{Convert, Qrz};

    let inner = map.inner_arc();
    let radius = inner.radius();
    let rise = inner.rise();
    let w = (radius as f64 * (3.0_f64).sqrt() / 2.0) as f32;
    let h = radius / 2.0;

    let self_elev = summaries.get(&chunk_id).map(|s| s.elevation as f32).unwrap_or(0.0);

    let nelev = |dq: i32, dr: i32| -> f32 {
        summaries.get(&ChunkId(chunk_id.0 + dq, chunk_id.1 + dr))
            .map(|s| s.elevation as f32)
            .unwrap_or(self_elev)
    };

    // 4 corner elevations averaged from self + 3 adjacent neighbors
    let c00 = (self_elev + nelev(-1, 0) + nelev(0, -1) + nelev(-1, -1)) * 0.25;
    let c10 = (self_elev + nelev( 1, 0) + nelev(0, -1) + nelev( 1, -1)) * 0.25;
    let c01 = (self_elev + nelev(-1, 0) + nelev(0,  1) + nelev(-1,  1)) * 0.25;
    let c11 = (self_elev + nelev( 1, 0) + nelev(0,  1) + nelev( 1,  1)) * 0.25;

    // World-space XZ offsets for each vertex relative to hex center.
    // Order: Center, N, NE, SE, S, SW, NW  (center pushed first in mesh)
    let xz_offsets: [[f32; 2]; 7] = [
        [0.0, 0.0],        // Center
        [0.0, -radius],    // N
        [w, -h],            // NE
        [w, h],             // SE
        [0.0, radius],     // S
        [-w, h],            // SW
        [-w, -h],           // NW
    ];

    // Axial-space offsets for bilinear interpolation.
    // Adjacent tiles sharing an edge vertex compute the same (dq,dr) at the same
    // axial position, so the interpolated elevation matches → no seam.
    let dq_axial: [f32; 7] = [0.0,  1.0/3.0, 2.0/3.0, 1.0/3.0, -1.0/3.0, -2.0/3.0, -1.0/3.0];
    let dr_axial: [f32; 7] = [0.0, -2.0/3.0, -1.0/3.0, 1.0/3.0,  2.0/3.0,  1.0/3.0, -1.0/3.0];

    // Y offset: center vertex flush, outer vertices raised by `rise` (concave hex shape)
    let y_rise: [f32; 7] = [0.0, rise, rise, rise, rise, rise, rise];

    let normal_arr: [f32; 3] = [0.0, 1.0, 0.0];
    let cs = CHUNK_SIZE as f32;
    let tile_count = (CHUNK_SIZE * CHUNK_SIZE) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(tile_count * 7);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(tile_count * 7);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(tile_count * 7);
    let mut indices: Vec<u32> = Vec::with_capacity(tile_count * 18);

    for oq in 0..CHUNK_SIZE {
        for or in 0..CHUNK_SIZE {
            let tile = chunk_to_tile(chunk_id, oq as u8, or as u8);
            let center_flat: Vec3 = inner.convert(Qrz { q: tile.q, r: tile.r, z: 0 });

            let base = positions.len() as u32;

            // 7 vertices: center first, then 6 outer
            for vi in 0..7 {
                let fq = (oq as f32 + dq_axial[vi]) / cs;
                let fr = (or as f32 + dr_axial[vi]) / cs;

                // Bilinear interpolation (allow slight extrapolation at chunk edges)
                let e_top = c00 + (c10 - c00) * fq;
                let e_bot = c01 + (c11 - c01) * fq;
                let elev = e_top + (e_bot - e_top) * fr;

                positions.push([
                    center_flat.x + xz_offsets[vi][0],
                    elev * rise + y_rise[vi],
                    center_flat.z + xz_offsets[vi][1],
                ]);
                normals.push(normal_arr);
                colors.push(CommonMap::height_color_tint(elev.round() as i32));
            }

            // 6 triangles: center(base) → outer pairs
            for i in 0..6u32 {
                let v1 = base + 1 + i;
                let v2 = base + 1 + ((i + 1) % 6);
                indices.extend_from_slice(&[base, v2, v1]);
            }
        }
    }

    let mut mesh = Mesh::new(
        bevy_mesh::PrimitiveTopology::TriangleList,
        bevy_asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(bevy_mesh::Indices::U32(indices));

    mesh
}

pub fn do_init(
    mut reader: MessageReader<Do>,
    mut try_writer: MessageWriter<Try>,
    mut server: ResMut<Server>,
    time: Res<Time>,
) {
    for &message in reader.read() {
        let Do { event: Event::Init { dt, .. } } = message else { continue };
        let client_now = time.elapsed().as_millis();

        // CRITICAL: The server captured dt when it SENT Init, but we're receiving it now
        // During the client startup time (client_now ms), the server's clock also advanced
        // We need to add that startup time to server_time_at_init to compensate
        server.server_time_at_init = dt.saturating_add(server.smoothed_latency).saturating_add(client_now);
        server.client_time_at_init = client_now;
        server.last_ping_time = client_now; // Track when we sent initial ping

        // Send initial Ping to measure actual network latency
        try_writer.write(Try { event: Event::Ping { client_time: client_now } });
    }
}

pub fn do_spawn(
    mut reader: MessageReader<Do>,
    map_state: Res<common::resources::map::MapState>,
) {
    use common::resources::map::TileEvent;

    // Queue tile spawn events (drain loop will process them)
    // refresh_map system will swap in new snapshot and trigger Bevy change detection
    for &message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };

        map_state.queue_event(TileEvent::Spawn(qrz, EntityType::Decorator(decorator)));
    }
}


#[allow(clippy::type_complexity)]
pub fn update(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>,Without<Moon>)>,
    mut q_moon: Query<(&mut DirectionalLight, &mut Transform), (With<Moon>,Without<Sun>)>,
    mut a_light: ResMut<GlobalAmbientLight>,
    server: Res<Server>,
    diagnostics_state: Res<DiagnosticsState>,
) {
    let dt = server.current_time(time.elapsed().as_millis());
    // Use fixed lighting at 9 AM if enabled, otherwise dynamic cycle
    let dtd = if diagnostics_state.fixed_lighting_enabled {
        0.375 // 9 hours / 24 hours = 0.375
    } else {
        (dt % DAY_MS) as f32 / DAY_MS as f32
    };
    let dtm = (dt % SEASON_MS) as f32 / SEASON_MS as f32;
    let dty = (dt % YEAR_MS) as f32 / YEAR_MS as f32;

    // sun
    let (mut s_light, mut s_transform) = q_sun.single_mut().expect("no result in q_sun");
    let mut s_rad_d = dtd * 2. * PI;
    let s_rad_y = dty * 2. * PI;

    // days are longer than nights
    s_rad_d = s_rad_d.clamp(PI/3., 5.*PI/3.);

    let s_illuminance = 1.-cos(0.75*s_rad_d + 3.*PI/4.).powf(16.);
    s_light.color = Color::linear_rgb(1., s_illuminance, s_illuminance);
    s_light.illuminance = 10_000.*s_illuminance;
    // Greatly increased ambient light to soften shadows during day (800 vs 100)
    a_light.brightness = 800.*s_illuminance;
    // Add sky-like blue tint to ambient light during day
    a_light.color = Color::linear_rgb(0.7 + 0.3*s_illuminance, 0.8 + 0.2*s_illuminance, 1.0);
    s_transform.translation.x = 1_000.*cos(0.75*s_rad_d + 3.*PI/4.);
    s_transform.translation.y = 1_000.*sin(0.75*s_rad_d + 3.*PI/4.).powf(2.);
    s_transform.translation.z = 1_000.*cos(s_rad_y);
    s_transform.look_at(Vec3::ZERO, Vec3::Y);

    // moon
    let (mut m_light, mut m_transform) = q_moon.single_mut().expect("no result in q_moon");
    let mut m_rad_d = dtd * 2. * PI;
    let m_rad_m = dtm * 2. * PI;

    // overlap sun cycle by PI/6 to avoid no lightsource at dusk/dawn
    if PI/2. < m_rad_d && m_rad_d < 3.*PI/2. { m_rad_d = 3.*PI/2. };

    m_light.illuminance = 200.                  // max illuminance at full moon
        *(0.1+0.9*cos(0.5*m_rad_m).powf(2.))    // phase moon through month
        *(1.-cos(m_rad_d+3.*PI/2.).powf(16.));  // moon rise/fall
    m_transform.translation.x = 1_000.*cos(m_rad_d+3.*PI/2.);
    m_transform.translation.y = 1_000.*sin(m_rad_d+3.*PI/2.).powf(2.);
    m_transform.look_at(Vec3::ZERO, Vec3::Y);
}

/// Evict chunks that are outside the player's FOV radius.
/// Two-pass: evicts full-detail chunks (tiles in Map) and summary chunks
/// (entries in ChunkSummaries). Also despawns actors on evicted chunks.
pub fn evict_distant_chunks(
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    mut l2r: ResMut<crate::resources::EntityMap>,
    map: Res<common::resources::map::Map>,
    map_state: Res<common::resources::map::MapState>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
    chunk_mesh_query: Query<(Entity, &ChunkMesh), Without<SummaryChunk>>,
) {
    // Only evict if we have a player
    let Ok(player_loc) = player_query.single() else {
        return;
    };

    let player_chunk = loc_to_chunk(**player_loc);
    let player_z = player_loc.z;
    let base_plus_buffer = terrain_chunk_radius(player_z) as i32 + 1;

    // Pass 1: Evict full-detail chunks (tiles in Map)
    let active_chunks: std::collections::HashSet<_> = loaded_chunks.chunks.iter().copied()
        .filter(|chunk_id| {
            let chebyshev = (chunk_id.0 - player_chunk.0).abs()
                .max((chunk_id.1 - player_chunk.1).abs());

            if chebyshev <= base_plus_buffer {
                return true;
            }

            let center_tile = chunk_id.center();
            let chunk_z = map.get_by_qr(center_tile.q, center_tile.r)
                .map(|(qrz, _)| qrz.z)
                .unwrap_or(player_z);

            let vis = visibility_radius(player_z, chunk_z, 40.0) as i32 + 1;
            chebyshev <= vis
        })
        .collect();

    let evictable = loaded_chunks.get_evictable(&active_chunks);

    if !evictable.is_empty() {
        // Despawn actors on evicted chunks
        for (entity, loc, entity_type) in actor_query.iter() {
            let actor_chunk = loc_to_chunk(**loc);
            if evictable.contains(&actor_chunk) {
                let is_player = matches!(
                    entity_type,
                    EntityType::Actor(actor_impl) if matches!(
                        actor_impl.identity,
                        common::components::entity_type::actor::ActorIdentity::Player
                    )
                );
                if !is_player {
                    l2r.remove_by_left(&entity);
                    commands.entity(entity).despawn();
                }
            }
        }

        // Despawn mesh entities for evicted full-detail chunks
        for (entity, chunk_mesh) in chunk_mesh_query.iter() {
            if evictable.contains(&chunk_mesh.chunk_id) {
                commands.entity(entity).despawn();
            }
        }

        // Queue tile despawns
        {
            use common::resources::map::TileEvent;

            let tiles_to_remove: Vec<_> = map.iter_tiles()
                .filter_map(|(qrz, _typ)| {
                    let tile_chunk = loc_to_chunk(qrz);
                    if evictable.contains(&tile_chunk) { Some(qrz) } else { None }
                })
                .collect();

            if !tiles_to_remove.is_empty() {
                for qrz in &tiles_to_remove {
                    map_state.queue_event(TileEvent::Despawn(*qrz));
                }
                loaded_chunks.evict(&evictable);
            }
        }
    }

    // Pass 2: Evict summary chunks
    let summary_evictable: Vec<_> = chunk_summaries.summaries.keys().copied()
        .filter(|chunk_id| {
            let chebyshev = (chunk_id.0 - player_chunk.0).abs()
                .max((chunk_id.1 - player_chunk.1).abs());

            if chebyshev <= base_plus_buffer {
                return false; // keep
            }

            let chunk_z = chunk_summaries.summaries.get(chunk_id)
                .map(|s| s.elevation)
                .unwrap_or(player_z);

            let vis = visibility_radius(player_z, chunk_z, 40.0) as i32 + 1;
            chebyshev > vis // evict if beyond visibility
        })
        .collect();

    for chunk_id in &summary_evictable {
        chunk_summaries.summaries.remove(chunk_id);
    }
    // Summary mesh entities are cleaned up by spawn_summary_meshes (change detection)
}
