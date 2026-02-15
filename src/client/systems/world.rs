use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool},
};

pub const TILE_RISE: f32 = 0.8;
pub const TILE_SIZE: f32 = 1.;

use crate::{
    client::{
        components::ChunkMesh,
        plugins::diagnostics::DiagnosticsState,
        resources::{LoadedChunks, PendingChunkMeshes, Server, TerrainMaterial}
    },
    common::{
        chunk::{FOV_CHUNK_RADIUS, calculate_visible_chunks, loc_to_chunk},
        components::{ *,
            behaviour::PlayerControlled,
            entity_type::*,
        },
        message::{Event, *},
        resources::map::*,
        systems::*,
    }
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

/// Poll pending chunk mesh generation tasks and spawn/update mesh entities when ready
pub fn poll_chunk_mesh_tasks(
    mut commands: Commands,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    chunk_mesh_query: Query<(Entity, &Mesh3d, &ChunkMesh)>,
) {
    use crate::common::chunk::ChunkId;

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
    mut map: ResMut<Map>,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    diagnostics_state: Res<DiagnosticsState>,
) {
    use std::collections::HashSet;
    use crate::common::chunk::{ChunkId, calculate_visible_chunks};

    // Track which chunks have new tiles
    let mut chunks_with_new_tiles: HashSet<ChunkId> = HashSet::new();

    for &message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };
        if map.get(qrz).is_some() { continue }
        map.insert(qrz, EntityType::Decorator(decorator));

        let chunk_id = loc_to_chunk(qrz);
        chunks_with_new_tiles.insert(chunk_id);
    }

    // For each chunk with new tiles, regenerate it AND its neighbors
    // This ensures edge vertices are recalculated with correct neighbor data
    let pool = AsyncComputeTaskPool::get();
    let mut chunks_to_regenerate: HashSet<ChunkId> = HashSet::new();

    for chunk_id in chunks_with_new_tiles {
        // Regenerate this chunk
        chunks_to_regenerate.insert(chunk_id);

        // Also regenerate adjacent chunks (radius 1) to fix their edge vertices
        for adjacent_chunk in calculate_visible_chunks(chunk_id, 1) {
            chunks_to_regenerate.insert(adjacent_chunk);
        }
    }

    for chunk_id in chunks_to_regenerate {
        // Cancel any pending task for this chunk and spawn a new one
        pending_meshes.tasks.remove(&chunk_id);

        // Spawn async task to generate mesh for this chunk
        let map_clone = map.clone();
        let apply_slopes = diagnostics_state.slope_rendering_enabled;
        let task = pool.spawn(async move {
            map_clone.generate_chunk_mesh(chunk_id, apply_slopes)
        });

        pending_meshes.tasks.insert(chunk_id, task);
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

/// Evict chunks that are outside the player's FOV radius
/// This prevents unlimited memory growth as the player explores
/// Also despawns any actors (NPCs/players) on evicted chunks
pub fn evict_distant_chunks(
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut map: ResMut<Map>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
    chunk_mesh_query: Query<(Entity, &ChunkMesh)>,
) {
    // Only evict if we have a player
    let Ok(player_loc) = player_query.single() else {
        return;
    };

    // Calculate which chunks should be kept (FOV + 1 buffer to prevent flickering)
    let player_chunk = loc_to_chunk(**player_loc);
    let active_chunks: std::collections::HashSet<_> = calculate_visible_chunks(player_chunk, FOV_CHUNK_RADIUS + 1)
        .into_iter()
        .collect();

    // Find chunks to evict
    let evictable = loaded_chunks.get_evictable(&active_chunks);
    if evictable.is_empty() {
        return;
    }

    // Despawn all actors on evicted chunks (prevents "ghost NPCs")
    for (entity, loc, entity_type) in actor_query.iter() {
        let actor_chunk = loc_to_chunk(**loc);
        if evictable.contains(&actor_chunk) {
            // Only despawn non-player actors (players handle their own despawn)
            let is_player = matches!(
                entity_type,
                EntityType::Actor(actor_impl) if matches!(
                    actor_impl.identity,
                    crate::common::components::entity_type::actor::ActorIdentity::Player
                )
            );

            if !is_player {
                commands.entity(entity).despawn();
            }
        }
    }

    // Despawn mesh entities for evicted chunks
    for (entity, chunk_mesh) in chunk_mesh_query.iter() {
        if evictable.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).despawn();
        }
    }

    // Remove all tiles belonging to evicted chunks from the map
    let tiles_to_remove: Vec<_> = map.iter_tiles()
        .filter_map(|(qrz, _typ)| {
            let tile_chunk = loc_to_chunk(qrz);
            if evictable.contains(&tile_chunk) {
                Some(qrz)
            } else {
                None
            }
        })
        .collect();

    if !tiles_to_remove.is_empty() {
        for qrz in &tiles_to_remove {
            map.remove(*qrz);
        }

        // Remove chunk from LoadedChunks
        loaded_chunks.evict(&evictable);
    }
}
