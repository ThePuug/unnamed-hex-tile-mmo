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
    resources::{ChunkLodMeshes, ChunkLodState, LodTriangleStats, LoadedChunks, Server, TerrainMaterial},
};
use qrz::Convert;
use common_bevy::{
    chunk::{self, chunk_tiles, loc_to_chunk, CHUNK_EXTENT_WU},
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
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: crate::resources::TerrainExtension {},
    });
    commands.insert_resource(TerrainMaterial { handle: material });
}

// ─────────────────────────────────────────────────────────
// SYSTEM 1: Server-authoritative chunk eviction
// ─────────────────────────────────────────────────────────
// Server sends EvictChunks when chunks leave the player's visibility.
// Client removes tiles, meshes, and actors on those chunks.

/// Process EvictChunks messages from the server.
pub fn evict_data(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    mut l2r: ResMut<crate::resources::EntityMap>,
    map: Res<common_bevy::resources::map::Map>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let mut all_evicted = Vec::new();

    for message in reader.read() {
        let Do { event: Event::EvictChunks { chunks, .. } } = message else { continue };
        all_evicted.extend_from_slice(chunks);
    }

    if all_evicted.is_empty() { return; }
    let _t = client_timers.0.scope("evict");

    // Despawn actors on evicted chunks
    for (entity, loc, entity_type) in actor_query.iter() {
        let actor_chunk = loc_to_chunk(**loc);
        if all_evicted.contains(&actor_chunk) {
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

    // Remove tiles from map
    {
        let mut map_w = map.write();
        for &chunk_id in &all_evicted {
            for (q, r) in chunk_tiles(chunk_id) {
                if let Some((qrz, _)) = map_w.get_by_qr(q, r) {
                    map_w.remove(qrz);
                }
            }
        }
    }
    loaded_chunks.evict(&all_evicted);

    // Evict LoD mesh state for evicted chunks
    for &cid in &all_evicted {
        if let Some(state) = lod_meshes.states.remove(&cid) {
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
    map: Res<common_bevy::resources::map::Map>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("do_spawn");
    for message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };
        map.insert(*qrz, EntityType::Decorator(*decorator));
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

// ── Chunk Mesh Pipeline ──

const TILE_RADIUS: f32 = 1.0;
const RISE: f32 = 0.8;

/// Dispatch mesh build tasks for chunks that have tiles but no mesh yet.
pub fn dispatch_lod_tasks(
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    if !map.take_changed() {
        return;
    }
    let _t = client_timers.0.scope("lod_disp");

    let pool = bevy::tasks::AsyncComputeTaskPool::get();

    for &chunk_id in &loaded_chunks.chunks {
        // Skip if already built or in flight
        if let Some(state) = lod_meshes.states.get(&chunk_id) {
            if state.task.is_some() || state.entity.is_some() {
                continue;
            }
        }

        // Neighbor gate: all 6 chunk neighbors must have tiles in the Map
        let lattice_neighbors = [(1i32, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
        let all_neighbors_in_map = lattice_neighbors.iter().all(|&(dn, dm)| {
            let nid = common_bevy::chunk::ChunkId(chunk_id.0 + dn, chunk_id.1 + dm);
            let nc = nid.center();
            map.get_by_qr(nc.q, nc.r).is_some()
        });
        if !all_neighbors_in_map {
            continue;
        }

        let map_snap = map.clone();
        let task = pool.spawn(async move {
            collect_and_build_mesh(chunk_id, &map_snap)
        });

        let chunk_origin: Vec3 = map.convert(chunk_id.center());

        if let Some(state) = lod_meshes.states.get_mut(&chunk_id) {
            state.task = Some(task);
        } else {
            lod_meshes.states.insert(chunk_id, ChunkLodState {
                task: Some(task),
                entity: None,
                mesh_handle: None,
                tri_count: 0,
                chunk_origin,
            });
        }
    }
}

/// Build a full-detail tile mesh from Map data.
/// Runs off the main thread via AsyncComputeTaskPool.
fn collect_and_build_mesh(
    chunk_id: common_bevy::chunk::ChunkId,
    map: &common_bevy::resources::map::Map,
) -> crate::resources::MeshBuildResult {
    // Collect chunk tiles
    let chunk_tiles_vec: Vec<qrz::Qrz> = chunk_tiles(chunk_id)
        .filter_map(|(q, r)| map.get_by_qr(q, r).map(|(qrz, _)| qrz))
        .collect();

    // Build elevation lookup covering chunk + 1-ring neighbors
    let mut elevations = std::collections::HashMap::new();
    for &tile in &chunk_tiles_vec {
        elevations.insert((tile.q, tile.r), tile.z);
        for &(dq, dr) in &[(-1i32,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
            let nq = tile.q + dq;
            let nr = tile.r + dr;
            if !elevations.contains_key(&(nq, nr)) {
                if let Some((actual, _)) = map.get_by_qr(nq, nr) {
                    elevations.insert((actual.q, actual.r), actual.z);
                }
            }
        }
    }

    let chunk_origin: Vec3 = map.convert(chunk_id.center());

    let tile_geom = common_bevy::geometry::compute_tile_geometry(
        &chunk_tiles_vec, &elevations, TILE_RADIUS, RISE, chunk_origin,
    );

    let tri_count = tile_geom.indices.len() as u32 / 3;

    use bevy::render::render_resource::PrimitiveTopology;
    use bevy::asset::RenderAssetUsages;
    use bevy_mesh::Indices;

    let verts: Vec<Vec3> = tile_geom.positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = tile_geom.normals.iter().map(|n| Vec3::from_array(*n)).collect();
    let vert_count = verts.len();

    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
    .with_inserted_indices(Indices::U32(tile_geom.indices));

    crate::resources::MeshBuildResult { mesh, tri_count }
}

/// Poll completed mesh tasks, upload meshes, update diagnostics.
pub fn poll_and_swap_lod(
    mut commands: Commands,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    terrain_material: Option<Res<TerrainMaterial>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let Some(terrain_material) = terrain_material else { return; };
    let _t = client_timers.0.scope("lod_swap");

    let mut total_tris = 0u64;
    let mut mesh_count = 0u32;

    for (&chunk_id, state) in lod_meshes.states.iter_mut() {
        if let Some(task) = &mut state.task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.task = None;
                state.tri_count = result.tri_count;

                let mesh_handle = meshes.add(result.mesh);
                state.mesh_handle = Some(mesh_handle.clone());

                match state.entity {
                    Some(entity) => {
                        commands.entity(entity).insert(Mesh3d(mesh_handle));
                    }
                    None => {
                        let entity = commands
                            .spawn((
                                Mesh3d(mesh_handle),
                                MeshMaterial3d(terrain_material.handle.clone()),
                                Transform::from_translation(state.chunk_origin),
                                ChunkMesh { chunk_id },
                            ))
                            .id();
                        state.entity = Some(entity);
                    }
                }
            }
        }

        if state.entity.is_some() {
            total_tris += state.tri_count as u64;
            mesh_count += 1;
        }
    }

    tri_stats.total_tris = total_tris;
    tri_stats.mesh_count = mesh_count;
}
