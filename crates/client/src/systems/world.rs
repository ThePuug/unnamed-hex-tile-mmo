use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*,
    tasks::{block_on, futures_lite::future},
};
use bevy_light::{CascadeShadowConfig, CascadeShadowConfigBuilder};

pub const TILE_SIZE: f32 = 1.;

use crate::{
    plugins::diagnostics::DiagnosticsState,
    resources::{
        ForcedSummaryRadius, LodTriangleStats, LoadedChunks,
        Server, SummaryMesh, SummaryMeshBuildResult, SummaryMeshState, SummaryMeshes,
        TerrainMaterial,
    },
};
use common_bevy::{
    chunk::{self, loc_to_chunk, CHUNK_EXTENT_WU},
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
/// Removes tiles and loaded_chunks tracking. Mesh lifecycle is handled
/// by dispatch_summary_tasks (auto-evicts regions when chunks disappear).
pub fn evict_data(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut loaded_chunks: ResMut<LoadedChunks>,
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

    // Remove tiles from map (triggers mesh rebuild via changed flag)
    for &chunk_id in &all_evicted {
        map.remove_chunk(chunk_id);
    }
    loaded_chunks.evict(&all_evicted);
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
        use crate::systems::camera::{CAMERA_DISTANCE, gameplay_camera_height};
        let height = gameplay_camera_height();
        let loading_r = chunk::terrain_chunk_radius(player_loc.z) as f32;
        let camera_to_player = (CAMERA_DISTANCE * CAMERA_DISTANCE + height * height).sqrt();
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

// ── Summary Mesh Pipeline ──
// All terrain rendering — r=0 tiles through r=N summaries — goes through
// this unified pipeline. No separate chunk mesh path.

/// Dispatch async tasks to build summary mesh regions.
///
/// **Forced mode** (`Some(r)`): single band at that radius, all visible regions.
/// **Auto mode** (`None`): multiple bands from `compute_active_bands`, with overlap.
pub fn dispatch_summary_tasks(
    mut commands: Commands,
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut summary_meshes: ResMut<SummaryMeshes>,
    forced_radius: Res<ForcedSummaryRadius>,
    summary_cache: Res<crate::resources::SummaryCache>,
    client_timers: Res<crate::resources::ClientTimers>,
    player_query: Query<&Transform, With<common_bevy::components::behaviour::PlayerControlled>>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    let map_changed = map.take_changed();
    let summaries_changed = summary_cache.take_changed();
    if !map_changed && !summaries_changed {
        return;
    }
    let _t = client_timers.0.scope("sum_disp");

    // Determine camera world position for local band computation
    #[cfg(feature = "admin")]
    let camera_pos = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.world_position)
        .or_else(|| player_query.single().ok().map(|t| t.translation));
    #[cfg(not(feature = "admin"))]
    let camera_pos = player_query.single().ok().map(|t| t.translation);

    // Local regions: camera-dependent (bands within streaming radius).
    // Only computed when we have a camera position.
    let mut needed: std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey> =
        match (camera_pos, forced_radius.0) {
            (_, Some(r)) => {
                common_bevy::summary_mesh::visible_mesh_regions(r, &loaded_chunks.chunks)
            }
            (Some(pos), None) => {
                #[cfg(feature = "admin")]
                let max_fov = if flyover.as_ref().map_or(false, |f| f.active) {
                    crate::systems::camera::MAX_FLYOVER_FOV
                } else {
                    crate::systems::camera::MAX_GAMEPLAY_FOV
                };
                #[cfg(not(feature = "admin"))]
                let max_fov = crate::systems::camera::MAX_GAMEPLAY_FOV;
                compute_auto_mode_regions(pos, &loaded_chunks.chunks, max_fov)
            }
            (None, None) => {
                // No camera yet — can't compute local regions.
                // Re-arm map changed so we retry once the player spawns.
                if map_changed { map.force_changed(); }
                std::collections::HashSet::new()
            }
        };

    // Server/flyover regions: add any mesh regions with new summary data.
    // Only includes regions that changed since last dispatch — O(dirty), not O(cache).
    if summaries_changed {
        needed.extend(summary_cache.take_dirty_regions());
    }

    // Evict any mesh region not in the current needed set.
    let stale: Vec<common_bevy::summary_mesh::MeshRegionKey> = summary_meshes
        .states
        .keys()
        .filter(|k| !needed.contains(k))
        .copied()
        .collect();
    for key in stale {
        if let Some(state) = summary_meshes.states.remove(&key) {
            if let Some(entity) = state.entity {
                commands.entity(entity).despawn();
            }
        }
    }

    // Dispatch build tasks for needed regions
    let pool = bevy::tasks::AsyncComputeTaskPool::get();

    for region_key in &needed {
        if let Some(state) = summary_meshes.states.get(region_key) {
            if state.task.is_some() {
                continue;
            }
            if state.entity.is_some()
                && state.summaries_built >= common_bevy::summary_mesh::MESH_REGION_SUMMARIES
            {
                continue;
            }
        }

        let radius = region_key.r;
        let rk = *region_key;
        let map_snap = map.clone();
        let cache_snap = summary_cache.clone();

        let task = pool.spawn(async move {
            collect_and_build_summary_mesh(radius, rk, &map_snap, &cache_snap)
        });

        let summary_lat = common_bevy::summary::summary_lattice(radius);
        let region_lat = common_bevy::summary::mesh_region_lattice();
        let region_center = region_lat.cell_center((region_key.mn, region_key.mm));
        let (cq, cr) = summary_lat.cell_center(region_center);
        let (wx, wz) = common_bevy::geometry::flat_top_tile_center(cq, cr, 1.0);
        let mesh_origin = Vec3::new(wx, 0.0, wz);

        if let Some(state) = summary_meshes.states.get_mut(region_key) {
            state.task = Some(task);
        } else {
            summary_meshes.states.insert(
                *region_key,
                SummaryMeshState {
                    task: Some(task),
                    entity: None,
                    mesh_handle: None,
                    tri_count: 0,
                    mesh_origin,
                    base_positions: Vec::new(),
                    base_normals: Vec::new(),
                    base_indices: Vec::new(),
                    base_tri_count: 0,
                    perimeter_edges: Vec::new(),
                    summaries_built: 0,
                },
            );
        }
    }

}

/// Compute visible mesh regions for auto mode (multi-band).
///
/// Local bands (within streaming radius): gated on loaded chunks.
/// Remote bands (beyond streaming radius): ungated — data from server summaries.
fn compute_auto_mode_regions(
    camera_pos: Vec3,
    loaded_chunks: &std::collections::HashSet<common_bevy::chunk::ChunkId>,
    fov: f32,
) -> std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey> {
    use common_bevy::summary::{compute_active_bands, mesh_region_extent_wu};
    use common_bevy::summary_mesh::{visible_mesh_regions_in_band, visible_mesh_regions_in_band_ungated};

    // Max render distance = visual horizon from camera geometry.
    // camera_height = CAMERA_DISTANCE * tan(half_fov + margin) + player_z * RISE
    // far_ground = camera_height / tan(margin)
    // Approximate player_z from camera Y (camera Y ≈ player terrain Y + camera height offset)
    let camera_height_offset = crate::systems::camera::gameplay_camera_height();
    let player_approx_y = (camera_pos.y - camera_height_offset).max(0.0);
    let camera_total_height = camera_height_offset + player_approx_y;
    let margin_rad = crate::systems::camera::HORIZON_MARGIN_DEG.to_radians();
    let far_ground = camera_total_height / margin_rad.tan();

    // Also compute max loaded distance for local bands
    let mut max_loaded = 0.0_f32;
    for &chunk_id in loaded_chunks {
        let center = chunk_id.center();
        let (wx, wz) = common_bevy::geometry::flat_top_tile_center(center.q, center.r, 1.0);
        let dx = wx - camera_pos.x;
        let dz = wz - camera_pos.z;
        max_loaded = max_loaded.max((dx * dx + dz * dz).sqrt());
    }

    // Cap local-only distance at loaded extent (minus margin for readiness)
    let local_outer_r = common_bevy::summary::summary_radius(max_loaded);
    let local_margin = mesh_region_extent_wu(local_outer_r);
    let local_max = (max_loaded - local_margin).max(0.0);

    let bands = compute_active_bands(far_ground, camera_height_offset, fov);
    let mut all_regions = std::collections::HashSet::new();

    for band in &bands {
        let outer = band.outer_wu;

        if outer <= local_max {
            // Band within loaded extent: gate on loaded chunks.
            let regions = visible_mesh_regions_in_band(
                band.r,
                camera_pos.x,
                camera_pos.z,
                band.inner_wu,
                outer,
                loaded_chunks,
            );
            all_regions.extend(regions);
        } else {
            // Band beyond loaded extent: ungated (server or flyover local server provides data)
            let inner = band.inner_wu.max(local_max);
            let regions = visible_mesh_regions_in_band_ungated(
                band.r,
                camera_pos.x,
                camera_pos.z,
                inner,
                outer,
            );
            all_regions.extend(regions);
        }
    }

    all_regions
}

/// Build a summary mesh region (runs off main thread).
///
/// r=0: full tile geometry from Map (slope blending, cliff skirts).
/// r>0: reads SummaryCache first, falls back to Map tile data.
///      Writes computed values back to cache for future rebuilds.
fn collect_and_build_summary_mesh(
    radius: u32,
    region_key: common_bevy::summary_mesh::MeshRegionKey,
    map: &common_bevy::resources::map::Map,
    cache: &crate::resources::SummaryCache,
) -> SummaryMeshBuildResult {
    let empty = SummaryMeshBuildResult {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
        tri_count: 0,
        mesh_origin: Vec3::ZERO,
        perimeter_edges: Vec::new(),
        summaries_built: 0,
    };

    if radius == 0 {
        let elevation_fn = |q: i32, r: i32| -> Option<i32> {
            map.get_by_qr(q, r).map(|(qrz, _)| qrz.z)
        };
        return common_bevy::summary_mesh::build_summary_mesh_region(0, region_key, &elevation_fn)
            .as_ref()
            .map_or(empty, smr_to_result);
    }

    // r>0: cache → Map → Composite fallback chain.
    let summary_lat = common_bevy::summary::summary_lattice(radius);
    let summary_z_fn = |sq: i32, sr: i32| -> Option<i32> {
        // 1. Cache hit (server-sent or previously computed)
        if let Some(z) = cache.get_by_lattice(radius, sq, sr) {
            return Some(z);
        }
        // 2. Compute from Map tile data
        let mut tile_zs = Vec::new();
        let mut all_present = true;
        for (tq, tr) in summary_lat.tiles_in_cell((sq, sr)) {
            if let Some((qrz, _)) = map.get_by_qr(tq, tr) {
                tile_zs.push(qrz.z);
            } else {
                all_present = false;
                break;
            }
        }
        if all_present && !tile_zs.is_empty() {
            let center_z = common_bevy::summary::select_center_z(&tile_zs);
            cache.insert(common_bevy::message::SummaryKey { r: radius, sq, sr }, center_z);
            return Some(center_z);
        }
        None
    };

    common_bevy::summary_mesh::build_summary_mesh_region_from_summaries(
        radius, region_key, &summary_z_fn,
    )
    .as_ref()
    .map_or(empty, smr_to_result)
}

fn smr_to_result(smr: &common_bevy::summary_mesh::SummaryMeshResult) -> SummaryMeshBuildResult {
    SummaryMeshBuildResult {
        positions: smr.positions.clone(),
        normals: smr.normals.clone(),
        indices: smr.indices.clone(),
        tri_count: smr.tri_count,
        mesh_origin: smr.mesh_origin,
        perimeter_edges: smr.perimeter_edges.clone(),
        summaries_built: smr.summaries_built,
    }
}

/// Build a Bevy Mesh from raw geometry buffers.
fn build_bevy_mesh(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
) -> Mesh {
    use bevy::render::render_resource::PrimitiveTopology;
    use bevy::asset::RenderAssetUsages;
    use bevy_mesh::Indices;

    let verts: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = normals.iter().map(|n| Vec3::from_array(*n)).collect();
    let vert_count = verts.len();

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
    .with_inserted_indices(Indices::U32(indices.to_vec()))
}

/// Append cross-region skirt geometry owned by `my_key` to the given buffers.
/// Only emits skirts for neighbors where `my_key < neighbor_key` (ownership).
fn append_cross_region_skirts(
    my_key: common_bevy::summary_mesh::MeshRegionKey,
    my_edges: &[common_bevy::summary_mesh::PerimeterEdge],
    all_states: &std::collections::HashMap<common_bevy::summary_mesh::MeshRegionKey, SummaryMeshState>,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    mesh_origin: Vec3,
) -> u32 {
    use common_bevy::summary_mesh::{compute_cross_region_skirts, mesh_region_neighbors};

    let mut tris = 0u32;
    for neighbor_key in mesh_region_neighbors(my_key) {
        // Ownership: lower key owns the skirt (same band only)
        if neighbor_key.r != my_key.r {
            continue;
        }
        if (my_key.mn, my_key.mm) >= (neighbor_key.mn, neighbor_key.mm) {
            continue;
        }
        let Some(neighbor_state) = all_states.get(&neighbor_key) else { continue };
        if neighbor_state.perimeter_edges.is_empty() {
            continue;
        }

        let quads = compute_cross_region_skirts(my_edges, &neighbor_state.perimeter_edges);
        for quad in &quads {
            let base = positions.len() as u32;
            for &pos in &quad.positions {
                let v = pos - mesh_origin;
                positions.push([v.x, v.y, v.z]);
            }
            let n: [f32; 3] = quad.normal.into();
            normals.extend([n; 4]);
            indices.extend([base, base + 1, base + 2]);
            indices.extend([base, base + 2, base + 3]);
            tris += 2;
        }
    }
    tris
}

/// Poll completed summary mesh tasks, build meshes with cross-region skirts,
/// spawn/update entities.
pub fn poll_summary_meshes(
    mut commands: Commands,
    mut summary_meshes: ResMut<SummaryMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    terrain_material: Option<Res<TerrainMaterial>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let Some(terrain_material) = terrain_material else { return };
    let _t = client_timers.0.scope("sum_poll");

    // Phase 1: Poll async tasks, store base geometry + perimeter edges.
    // Collect keys of newly-completed regions.
    let mut just_completed: Vec<common_bevy::summary_mesh::MeshRegionKey> = Vec::new();

    for (&region_key, state) in summary_meshes.states.iter_mut() {
        if let Some(task) = &mut state.task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.task = None;
                state.mesh_origin = result.mesh_origin;
                state.base_positions = result.positions;
                state.base_normals = result.normals;
                state.base_indices = result.indices;
                state.base_tri_count = result.tri_count;
                state.perimeter_edges = result.perimeter_edges;
                state.summaries_built = result.summaries_built;
                just_completed.push(region_key);
            }
        }
    }

    // Phase 2: For each just-completed region, build its mesh including
    // cross-region skirts, and mark neighbors that need re-patching.
    let mut needs_rebuild: Vec<common_bevy::summary_mesh::MeshRegionKey> = Vec::new();

    for &region_key in &just_completed {
        // Check which already-complete neighbors with higher key need us
        // to own their cross-region skirts. Those neighbors need to be
        // rebuilt by THEIR lower-key neighbor (us). But also check if any
        // neighbor with lower key now has new data (our perimeter edges)
        // and needs to rebuild.
        for neighbor_key in common_bevy::summary_mesh::mesh_region_neighbors(region_key) {
            if just_completed.contains(&neighbor_key) {
                continue; // Will be built fresh in this same pass
            }
            let Some(neighbor_state) = summary_meshes.states.get(&neighbor_key) else { continue };
            if neighbor_state.base_positions.is_empty() {
                continue; // Not yet complete
            }
            // If neighbor has lower key, it owns the cross-region skirts
            // and needs to rebuild to include skirts against our new data.
            if neighbor_key.r == region_key.r
                && (neighbor_key.mn, neighbor_key.mm) < (region_key.mn, region_key.mm)
            {
                if !needs_rebuild.contains(&neighbor_key) {
                    needs_rebuild.push(neighbor_key);
                }
            }
        }
    }

    // Combine: all regions that need a mesh (re)build this frame
    let mut all_build: Vec<common_bevy::summary_mesh::MeshRegionKey> = just_completed;
    all_build.extend(needs_rebuild);

    // Also handle orphaned states: have base geometry but no entity.
    // Occurs after a flyover stash restore (entities were despawned on toggle-on).
    // A task may already be pending (dispatch_summary_tasks ran first this frame);
    // we still rebuild immediately from the stored base geometry so there is no flash.
    for (&region_key, state) in summary_meshes.states.iter() {
        if !state.base_positions.is_empty() && state.entity.is_none() {
            if !all_build.contains(&region_key) {
                all_build.push(region_key);
            }
        }
    }

    // Phase 3: Build/rebuild meshes with cross-region skirts.
    // We need read access to all states for neighbor perimeter lookups,
    // so collect the data we need first, then mutate.
    struct MeshBuild {
        key: common_bevy::summary_mesh::MeshRegionKey,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
        tri_count: u32,
    }

    let mut builds: Vec<MeshBuild> = Vec::new();

    for &region_key in &all_build {
        let states = &summary_meshes.states;
        let Some(state) = states.get(&region_key) else { continue };
        if state.base_positions.is_empty() {
            continue;
        }

        let mut positions = state.base_positions.clone();
        let mut normals = state.base_normals.clone();
        let mut indices = state.base_indices.clone();
        let mut tri_count = state.base_tri_count;

        tri_count += append_cross_region_skirts(
            region_key,
            &state.perimeter_edges,
            states,
            &mut positions,
            &mut normals,
            &mut indices,
            state.mesh_origin,
        );

        builds.push(MeshBuild {
            key: region_key,
            positions,
            normals,
            indices,
            tri_count,
        });
    }

    // Phase 4: Upload meshes, spawn/update entities.
    for build in builds {
        if build.tri_count == 0 {
            continue;
        }

        let mesh = build_bevy_mesh(&build.positions, &build.normals, &build.indices);
        let mesh_handle = meshes.add(mesh);

        let state = summary_meshes.states.get_mut(&build.key).unwrap();
        state.mesh_handle = Some(mesh_handle.clone());
        state.tri_count = build.tri_count;

        match state.entity {
            Some(entity) => {
                commands.entity(entity).insert(Mesh3d(mesh_handle));
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(mesh_handle),
                        MeshMaterial3d(terrain_material.handle.clone()),
                        Transform::from_translation(state.mesh_origin),
                        SummaryMesh { region_key: build.key },
                    ))
                    .id();
                state.entity = Some(entity);
            }
        }
    }

    // Phase 5: Diagnostics.
    let mut total_tris = 0u64;
    let mut mesh_count = 0u32;
    tri_stats.per_band.clear();
    for (&region_key, state) in summary_meshes.states.iter() {
        if state.entity.is_some() {
            total_tris += state.tri_count as u64;
            mesh_count += 1;
            let entry = tri_stats.per_band.entry(region_key.r).or_insert((0, 0));
            entry.0 += state.tri_count as u64;
            entry.1 += 1;
        }
    }
    tri_stats.total_tris = total_tris;
    tri_stats.mesh_count = mesh_count;
}

