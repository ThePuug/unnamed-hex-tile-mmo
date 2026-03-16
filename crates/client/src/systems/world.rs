use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*,
    tasks::{block_on, futures_lite::future},
};
use bevy_light::{CascadeShadowConfig, CascadeShadowConfigBuilder};

pub const TILE_SIZE: f32 = 1.;

use crate::{
    components::ChunkMesh,
    plugins::diagnostics::DiagnosticsState,
    resources::{ChunkLodMeshes, ChunkLodState, LodLevel, LoadedChunks, Server, TerrainMaterial},
};
use common_bevy::{
    chunk::{self, chunk_hex_distance, chunk_tiles, loc_to_chunk, terrain_chunk_radius, CHUNK_EXTENT_WU},
    components::{ *,
        behaviour::PlayerControlled,
        entity_type::*,
    },
    message::{Event, *},
    systems::*,
};

pub fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<bevy::pbr::ExtendedMaterial<StandardMaterial, crate::resources::TerrainExtension>>>,
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

    // Initialize shared terrain material (elevation color computed in shader)
    let material = materials.add(bevy::pbr::ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 1.,
            ..default()
        },
        extension: crate::resources::TerrainExtension {},
    });
    commands.insert_resource(TerrainMaterial { handle: material });
}

// ─────────────────────────────────────────────────────────
// SYSTEM 1: Data eviction (timer, every 5s)
// ─────────────────────────────────────────────────────────
// Manages what data the client holds. Never touches meshes.
// Tiles: evicted beyond detail_radius + 1.
// Summaries: evicted beyond terrain_chunk_radius + 1.

/// Evict data beyond the player's view range.
pub fn evict_data(
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    mut l2r: ResMut<crate::resources::EntityMap>,
    map: Res<common_bevy::resources::map::Map>,
    map_state: Res<common_bevy::resources::map::MapState>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    camera_query: Query<&Projection, With<Camera3d>>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
) {
    let Ok(player_loc) = player_query.single() else { return };
    let player_chunk = loc_to_chunk(**player_loc);
    let player_z = player_loc.z;

    let fov = camera_query.single().ok()
        .and_then(|p| match p { Projection::Perspective(pp) => Some(pp.fov), _ => None })
        .unwrap_or(chunk::DEFAULT_FOV);
    let detail_buffer = chunk::detail_boundary_radius(player_z, fov) as i32 + 2;
    let summary_buffer = terrain_chunk_radius(player_z) as i32 + 1;

    // ── Evict tiles beyond summary boundary ──
    // Use summary_buffer (not detail_buffer) — outer ring chunks need tiles
    // in the Map for LoD2 mesh generation and slope blending.
    let evictable: Vec<common_bevy::chunk::ChunkId> = loaded_chunks.chunks.iter()
        .filter(|&&cid| chunk_hex_distance(cid, player_chunk) > summary_buffer)
        .copied()
        .collect();

    if !evictable.is_empty() {
        // Despawn actors on evicted chunks
        for (entity, loc, entity_type) in actor_query.iter() {
            let actor_chunk = loc_to_chunk(**loc);
            if evictable.contains(&actor_chunk) {
                let is_player = matches!(
                    entity_type,
                    EntityType::Actor(actor_impl) if matches!(
                        actor_impl.identity,
                        common_bevy::components::entity_type::actor::ActorIdentity::Player
                    )
                );
                if !is_player {
                    l2r.remove_by_left(&entity);
                    commands.entity(entity).despawn();
                }
            }
        }

        // Queue tile despawns
        {
            use common_bevy::resources::map::TileEvent;
            for &chunk_id in &evictable {
                for (q, r) in chunk_tiles(chunk_id) {
                    if let Some((qrz, _)) = map.get_by_qr(q, r) {
                        map_state.queue_event(TileEvent::Despawn(qrz));
                    }
                }
            }
            loaded_chunks.evict(&evictable);
        }
    }

    // Evict LoD mesh state when tiles are evicted — same radius as tile eviction
    // to prevent stale entries blocking re-dispatch on chunk re-load.
    let lod_evictable: Vec<common_bevy::chunk::ChunkId> = lod_meshes.states.keys()
        .filter(|&&cid| !loaded_chunks.chunks.contains(&cid))
        .copied()
        .collect();
    for cid in &lod_evictable {
        if let Some(state) = lod_meshes.states.remove(cid) {
            if let Some(entity) = state.entity {
                commands.entity(entity).despawn();
            }
        }
    }
}

pub fn do_init(
    mut reader: MessageReader<Do>,
    mut try_writer: MessageWriter<Try>,
    mut server: ResMut<Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Do { event: Event::Init { dt, .. } } = message else { continue };
        let dt = *dt;
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
    map_state: Res<common_bevy::resources::map::MapState>,
) {
    use common_bevy::resources::map::TileEvent;

    // Queue tile spawn events (drain loop will process them)
    // refresh_map system will swap in new snapshot and trigger Bevy change detection
    for message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };
        let decorator = *decorator;
        let qrz = *qrz;

        map_state.queue_event(TileEvent::Spawn(qrz, EntityType::Decorator(decorator)));
    }
}


#[allow(clippy::type_complexity)]
pub fn update(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform, &mut CascadeShadowConfig), (With<Sun>,Without<Moon>)>,
    mut q_moon: Query<(&mut DirectionalLight, &mut Transform), (With<Moon>,Without<Sun>)>,
    mut a_light: ResMut<GlobalAmbientLight>,
    server: Res<Server>,
    diagnostics_state: Res<DiagnosticsState>,
    player_query: Query<&Loc, With<PlayerControlled>>,
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
    let (mut s_light, mut s_transform, mut cascade_config) = q_sun.single_mut().expect("no result in q_sun");
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

    // Anchor cascade shadow distance to the atmospheric fade start.
    // The shader fades terrain to horizon haze at 80% of the loading radius,
    // so shadows covering up to that point hide the shadow-to-no-shadow seam
    // inside the fade band. Summary meshes are cheap (7 verts/chunk).
    // maximum_distance is measured from the camera, not the player,
    // so add the camera-to-player distance to the terrain radius.
    if let Ok(player_loc) = player_query.single() {
        use crate::systems::camera::{CAMERA_DISTANCE, CAMERA_HEIGHT};
        let loading_r = chunk::terrain_chunk_radius(player_loc.z) as f32;
        let camera_to_player = (CAMERA_DISTANCE * CAMERA_DISTANCE + CAMERA_HEIGHT * CAMERA_HEIGHT).sqrt();
        let max_dist = camera_to_player + loading_r * 0.8 * CHUNK_EXTENT_WU;
        let current_max = cascade_config.bounds.last().copied().unwrap_or(0.0);
        if (max_dist - current_max).abs() > 1.0 {
            *cascade_config = CascadeShadowConfigBuilder {
                maximum_distance: max_dist,
                ..default()
            }.into();
        }
    }
}

// ── Client-Side LoD ──

/// QEM error threshold for LoD1 (lossless — removes only zero-error vertices).
const LOD1_ERROR_THRESHOLD: f32 = 0.0;
/// QEM error threshold for LoD2 (lossy — tune after viewing results).
const LOD2_ERROR_THRESHOLD: f32 = 2.0;

/// Dispatch dual-LoD QEM tasks for newly loaded chunks.
/// Runs every frame — checks for chunks that have tiles but no LoD tasks yet.
pub fn dispatch_lod_tasks(
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
) {
    // Only re-check when new tile data arrives. Once all reachable chunks are
    // meshed or waiting on out-of-range neighbors, this skips entirely.
    if !map.is_changed() {
        return;
    }

    let pool = bevy::tasks::AsyncComputeTaskPool::get();

    for &chunk_id in &loaded_chunks.chunks {
        if lod_meshes.states.contains_key(&chunk_id) {
            continue; // Already dispatched
        }

        // Neighbor gate (6 lookups). Skip the expensive 271-tile collection
        // when neighbors haven't arrived yet.
        let lattice_neighbors = [(1i32,0),(0,1),(-1,1),(-1,0),(0,-1),(1,-1)];
        let all_neighbors_in_map = lattice_neighbors.iter().all(|&(dn, dm)| {
            let nid = common_bevy::chunk::ChunkId(chunk_id.0 + dn, chunk_id.1 + dm);
            let nc = nid.center();
            map.get_by_qr(nc.q, nc.r).is_some()
        });
        if !all_neighbors_in_map {
            continue;
        }

        // Build chunk tiles from populated Map
        let chunk_tile_list: Vec<qrz::Qrz> = chunk_tiles(chunk_id)
            .filter_map(|(q, r)| map.get_by_qr(q, r).map(|(qrz, _)| qrz))
            .collect();

        if chunk_tile_list.len() < 271 {
            if chunk_tile_list.is_empty() {
                continue;
            }
            bevy::log::warn!("chunk ({},{}) has only {} of 271 tiles in map",
                chunk_id.0, chunk_id.1, chunk_tile_list.len());
            continue;
        }

        let mut elevations = std::collections::HashMap::new();
        for &qrz in &chunk_tile_list {
            elevations.insert((qrz.q, qrz.r), qrz.z);
            for direction in qrz::DIRECTIONS.iter() {
                let n = qrz + *direction;
                if let Some((actual, _)) = map.get_by_qr(n.q, n.r) {
                    elevations.insert((actual.q, actual.r), actual.z);
                }
            }
        }

        // Dispatch LoD1 task — raw geometry passthrough
        let tiles1 = chunk_tile_list.clone();
        let elevs1 = elevations.clone();
        let lod1_task = pool.spawn(async move {
            let geometry = common_bevy::geometry::compute_tile_geometry(
                &tiles1, &elevs1, 1.0, 0.8,
            );
            common_bevy::qem::DecimatedMesh {
                positions: geometry.positions,
                normals: geometry.normals,
                indices: geometry.indices,
            }
        });

        // Dispatch LoD2 task — QEM decimation
        let tiles2 = chunk_tile_list;
        let elevs2 = elevations;
        let lod2_task = pool.spawn(async move {
            let geometry = common_bevy::geometry::compute_tile_geometry(
                &tiles2, &elevs2, 1.0, 0.8,
            );
            common_bevy::qem::decimate_geometry(&geometry, LOD2_ERROR_THRESHOLD)
        });

        lod_meshes.states.insert(chunk_id, ChunkLodState {
            lod1_task: Some(lod1_task),
            lod2_task: Some(lod2_task),
            lod1_mesh: None,
            lod2_mesh: None,
            active_lod: LodLevel::Lod1,
            entity: None,
        });
    }

}

/// Poll completed LoD tasks, upload meshes, select active LoD per chunk.
pub fn poll_and_swap_lod(
    mut commands: Commands,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Option<Res<TerrainMaterial>>,
    player_query: Query<&Loc, With<PlayerControlled>>,
) {
    let Some(terrain_material) = terrain_material else { return; };

    let player_loc = player_query.iter().next();
    let player_chunk = player_loc.map(|loc| loc_to_chunk(**loc));

    for (&chunk_id, state) in lod_meshes.states.iter_mut() {
        // Poll LoD1
        if let Some(task) = &mut state.lod1_task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.lod1_task = None;
                state.lod1_mesh = Some(meshes.add(build_bevy_mesh(&result)));
            }
        }

        // Poll LoD2
        if let Some(task) = &mut state.lod2_task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.lod2_task = None;
                state.lod2_mesh = Some(meshes.add(build_bevy_mesh(&result)));
            }
        }

        // Determine which mesh to show
        let target_lod = target_lod_for_chunk(chunk_id, player_chunk);

        // Pick the best available mesh: prefer target, fallback to any
        let (mesh_handle, actual_lod) = match target_lod {
            LodLevel::Lod1 => {
                if let Some(h) = state.lod1_mesh.clone() { (h, LodLevel::Lod1) }
                else if let Some(h) = state.lod2_mesh.clone() { (h, LodLevel::Lod2) }
                else { continue; }
            }
            LodLevel::Lod2 => {
                if let Some(h) = state.lod2_mesh.clone() { (h, LodLevel::Lod2) }
                else if let Some(h) = state.lod1_mesh.clone() { (h, LodLevel::Lod1) }
                else { continue; }
            }
        };

        match state.entity {
            Some(entity) => {
                // Update mesh if what's displayed differs from what we want
                if state.active_lod != actual_lod {
                    commands.entity(entity).insert(Mesh3d(mesh_handle));
                    state.active_lod = actual_lod;
                }
            }
            None => {
                let entity = commands.spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(terrain_material.handle.clone()),
                    ChunkMesh { chunk_id },
                )).id();
                state.entity = Some(entity);
                state.active_lod = actual_lod;
            }
        }
    }
}

fn target_lod_for_chunk(chunk_id: common_bevy::chunk::ChunkId, player_chunk: Option<common_bevy::chunk::ChunkId>) -> LodLevel {
    let Some(pc) = player_chunk else { return LodLevel::Lod1; };
    let ring = chunk_hex_distance(chunk_id, pc);
    let detail_radius = chunk::detail_boundary_radius(0, chunk::MAX_FOV) as i32;
    if ring <= detail_radius {
        LodLevel::Lod1
    } else {
        LodLevel::Lod2
    }
}

fn build_bevy_mesh(decimated: &common_bevy::qem::DecimatedMesh) -> Mesh {
    let vert_count = decimated.positions.len();
    let verts: Vec<Vec3> = decimated.positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = decimated.normals.iter().map(|n| Vec3::from_array(*n)).collect();

    Mesh::new(
        bevy_mesh::PrimitiveTopology::TriangleList,
        bevy_asset::RenderAssetUsages::MAIN_WORLD | bevy_asset::RenderAssetUsages::RENDER_WORLD,
    )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0,
            (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
        .with_inserted_indices(bevy_mesh::Indices::U32(decimated.indices.clone()))
}
