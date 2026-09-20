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

pub fn setup(mut commands: Commands) {
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
    player_query: Query<&Loc, (With<PlayerControlled>, With<common_bevy::components::Actor>)>,
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

/// **Forced mode** (`Some(r)`): single band at that radius, all visible regions.
/// **Auto mode** (`None`): multiple bands from `compute_active_bands`, with overlap.
/// Camera movement (WU) that forces a region re-evaluation even without new
/// map or cache data. Band edges are player-centric — without this, bands
/// only advance on chunk/summary arrival and then snap in bursts.
const REEVAL_MOVE_WU: f32 = 16.0;

/// Hysteresis margin on band edges: a region refines/coarsens only when it
/// is clearly past the threshold. Existing meshes within the margin survive
/// (keep set); new meshes only build inside the crisp band (needed set).
/// Prevents threshold flapping as the player oscillates near a band edge.
const BAND_HYSTERESIS_MARGIN: f32 = 0.08;

/// World-space center of a mesh region.
fn region_center_world(key: &common_bevy::summary_mesh::MeshRegionKey) -> (f32, f32) {
    let summary_lat = common_bevy::summary::summary_lattice(key.r);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let region_center = region_lat.cell_center((key.mn, key.mm));
    let (cq, cr) = summary_lat.cell_center(region_center);
    common_bevy::geometry::flat_top_tile_center(cq, cr, 1.0)
}

pub fn dispatch_summary_tasks(
    mut commands: Commands,
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut summary_meshes: ResMut<SummaryMeshes>,
    forced_radius: Res<ForcedSummaryRadius>,
    summary_cache: Res<crate::resources::SummaryCache>,
    client_timers: Res<crate::resources::ClientTimers>,
    player_query: Query<&Transform, (With<common_bevy::components::behaviour::PlayerControlled>, With<common_bevy::components::Actor>)>,
    mut last_eval_pos: Local<Option<Vec3>>,
    mut backlog: Local<bool>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    // Determine camera world position for local band computation
    #[cfg(feature = "admin")]
    let camera_pos = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.world_position)
        .or_else(|| player_query.single().ok().map(|t| t.translation));
    #[cfg(not(feature = "admin"))]
    let camera_pos = player_query.single().ok().map(|t| t.translation);

    let map_changed = map.take_changed();
    let cache_changed = summary_cache.take_new_data();
    let moved = match (*last_eval_pos, camera_pos) {
        (Some(prev), Some(pos)) => prev.distance_squared(pos) >= REEVAL_MOVE_WU * REEVAL_MOVE_WU,
        (None, Some(_)) => true,
        _ => false,
    };
    // A run that exhausted its budget leaves a backlog, picked up next frame.
    let data_changed = map_changed || cache_changed;
    if !data_changed && !moved && !*backlog {
        return;
    }
    *backlog = false;
    if let Some(pos) = camera_pos {
        *last_eval_pos = Some(pos);
    }
    let _t = client_timers.0.scope("sum_disp");

    // Local-data boundary: the largest circle inside guaranteed chunk
    // coverage (the hexagonal chunk set's APOTHEM — the circumradius
    // over-claims by ~80 WU in edge directions). In gameplay the server
    // streams chunks to FIXED_STREAM_RADIUS; in flyover only the
    // detail-chunk disc around the camera exists.
    #[cfg(feature = "admin")]
    let local_boundary = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.local_boundary_wu())
        .unwrap_or(common_bevy::chunk::FIXED_STREAM_APOTHEM_WU);
    #[cfg(not(feature = "admin"))]
    let local_boundary = common_bevy::chunk::FIXED_STREAM_APOTHEM_WU;

    // Local regions: camera-dependent (bands within streaming radius).
    // `needed` is the crisp band assignment (what to build); `keep` widens
    // each band edge by the hysteresis margin (what may survive).
    let (needed, keep): (
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
    ) = match (camera_pos, forced_radius.0) {
        (_, Some(r)) => {
            let n = common_bevy::summary_mesh::visible_mesh_regions(r, &loaded_chunks.chunks);
            let k = n.clone();
            (n, k)
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
            let n = compute_auto_mode_regions(
                pos,
                &loaded_chunks.chunks,
                max_fov,
                0.0,
                local_boundary,
            );
            let mut k = compute_auto_mode_regions(
                pos,
                &loaded_chunks.chunks,
                max_fov,
                BAND_HYSTERESIS_MARGIN,
                local_boundary,
            );
            k.extend(n.iter().copied());
            (n, k)
        }
        (None, None) => {
            // No camera yet — can't compute local regions.
            // Re-arm map changed so we retry once the player spawns.
            if map_changed { map.force_changed(); }
            (std::collections::HashSet::new(), std::collections::HashSet::new())
        }
    };

    // Evict mesh regions outside the keep set — but only once every needed
    // region overlapping the stale region's footprint has a live entity.
    // Coverage never drops during a band transition (build-before-evict):
    // the old level's plate stays until its replacement is on screen.
    let stale: Vec<common_bevy::summary_mesh::MeshRegionKey> = summary_meshes
        .states
        .keys()
        .filter(|k| !keep.contains(k))
        .copied()
        .collect();
    for key in stale {
        let (kx, kz) = region_center_world(&key);
        let k_half = 0.5 * common_bevy::summary::mesh_region_extent_wu(key.r);
        let covered = needed.iter().all(|n| {
            let (nx, nz) = region_center_world(n);
            let n_half = 0.5 * common_bevy::summary::mesh_region_extent_wu(n.r);
            let dx = nx - kx;
            let dz = nz - kz;
            let reach = k_half + n_half;
            if dx * dx + dz * dz > reach * reach {
                return true; // doesn't overlap the stale region
            }
            summary_meshes
                .states
                .get(n)
                .map_or(false, |s| s.entity.is_some())
        });
        if !covered {
            continue; // replacement not on screen yet — hold the old mesh
        }
        if let Some(state) = summary_meshes.states.remove(&key) {
            if let Some(entity) = state.entity {
                commands.entity(entity).despawn();
            }
        }
    }

    // Dispatch build tasks for needed regions, nearest-first so the terrain
    // in front of the camera fills before the horizon (matches server and
    // flyover dispatch order).
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
    const MAX_MESH_TASKS: usize = 16;
    let mut mesh_dispatched = 0;

    let mut ordered: Vec<(f32, common_bevy::summary_mesh::MeshRegionKey, Vec3)> = needed
        .iter()
        .map(|region_key| {
            let summary_lat = common_bevy::summary::summary_lattice(region_key.r);
            let region_lat = common_bevy::summary::mesh_region_lattice();
            let region_center = region_lat.cell_center((region_key.mn, region_key.mm));
            let (cq, cr) = summary_lat.cell_center(region_center);
            let (wx, wz) = common_bevy::geometry::flat_top_tile_center(cq, cr, 1.0);
            let d2 = camera_pos.map_or(0.0, |pos| {
                let dx = wx - pos.x;
                let dz = wz - pos.z;
                dx * dx + dz * dz
            });
            (d2, *region_key, Vec3::new(wx, 0.0, wz))
        })
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (d2, region_key, mesh_origin) in ordered {
        if mesh_dispatched >= MAX_MESH_TASKS { break; }
        if let Some(state) = summary_meshes.states.get(&region_key) {
            if state.task.is_some() {
                continue;
            }
            // Built regions are final: their heights are durable. A region
            // still waiting for data is retried only when data arrived.
            if state.entity.is_some() || (state.waiting && !data_changed) {
                continue;
            }
        }

        let radius = region_key.r;

        // r>0 regions beyond the local-data boundary are server/flyover-owned:
        // without cache data their build would produce nothing — wait for it.
        // Regions within the boundary are Map-built and dispatch regardless
        // of cache state.
        if radius > 0 && !summary_cache.contains_region(&region_key) {
            let reach = local_boundary
                + 0.5 * common_bevy::summary::mesh_region_extent_wu(radius);
            let is_local = camera_pos.is_none() || d2 <= reach * reach;
            if !is_local {
                continue;
            }
        }

        let rk = region_key;
        let map_snap = map.clone();
        let cache_snap = summary_cache.clone();

        let task = pool.spawn(async move {
            collect_and_build_summary_mesh(radius, rk, &map_snap, &cache_snap)
        });

        if let Some(state) = summary_meshes.states.get_mut(&region_key) {
            state.task = Some(task);
        } else {
            summary_meshes.states.insert(
                region_key,
                SummaryMeshState {
                    task: Some(task),
                    entity: None,
                    mesh_handle: None,
                    tri_count: 0,
                    mesh_origin,
                    base_positions: Vec::new(),
                    base_normals: Vec::new(),
                    base_coarse: Vec::new(),
                    base_indices: Vec::new(),
                    base_tri_count: 0,
                    base_water: Default::default(),
                    waiting: false,
                },
            );
        }
        mesh_dispatched += 1;
    }

    // Budget exhausted with regions possibly still undispatched.
    if mesh_dispatched >= MAX_MESH_TASKS {
        *backlog = true;
    }
}

/// Active bands out to the visual horizon for a camera under `fov` at
/// `camera_pos` — the player's (or flyover's) ground position; the camera
/// sits camera_height above it. The horizon is the top-corner ray distance
/// (see far_ground_wu), the same formula the server and flyover producers
/// use so the horizons agree. `margin` widens the horizon for the keep set.
fn horizon_bands(camera_pos: Vec3, fov: f32, margin: f32) -> Vec<common_bevy::summary::Band> {
    let camera_height_offset = crate::systems::camera::camera_height(fov);
    let camera_total_height = camera_height_offset + camera_pos.y.max(0.0);
    let far_ground = common::camera::far_ground_wu(camera_total_height, fov);
    common_bevy::summary::compute_active_bands(far_ground * (1.0 + margin))
}

/// Upload the band cut: the player's ground position, each level's window,
/// and the transition over the strip inside its outer edge. Regions are
/// built whole and the shaders drop fragments outside the window, so band
/// edges follow the player every frame with no rebuild and adjacent levels
/// overlap by the strip, never a region. A forced debug radius lifts the
/// cut.
pub fn update_terrain_cut(
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    terrain_material: Res<TerrainMaterial>,
    forced_radius: Res<ForcedSummaryRadius>,
    diagnostics: Res<crate::plugins::diagnostics::DiagnosticsState>,
    player_query: Query<&Transform, (With<PlayerControlled>, With<common_bevy::components::Actor>)>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    let player = || player_query.single().ok().map(|t| t.translation);
    #[cfg(feature = "admin")]
    let (origin, fov) = match flyover.as_ref().filter(|f| f.active) {
        Some(f) => (Some(f.world_position), crate::systems::camera::MAX_FLYOVER_FOV),
        None => (player(), crate::systems::camera::MAX_GAMEPLAY_FOV),
    };
    #[cfg(not(feature = "admin"))]
    let (origin, fov) = (player(), crate::systems::camera::MAX_GAMEPLAY_FOV);
    let Some(origin) = origin else { return };

    let bands = horizon_bands(origin, fov, 0.0);

    for (&r, handle) in &terrain_material.by_level {
        let Some(material) = materials.get_mut(handle) else { continue };
        let (inner, outer) = if forced_radius.0.is_some() {
            (0.0, f32::MAX)
        } else {
            common_bevy::summary::cut_window(r, &bands)
        };
        // The outermost band ends at the horizon: no strip, nothing to give
        // way to.
        let fade = if outer == f32::MAX { 0.0 } else { common_bevy::summary::transition_wu(r) };
        material.extension.cut = crate::resources::TerrainCut {
            center: origin.xz(),
            inner,
            outer,
            fade,
            mode: diagnostics.lod_transition as u32,
            ..default()
        };
    }
}

/// Compute visible mesh regions for auto mode (multi-band).

/// Local bands (within `local_boundary_wu`): gated on loaded chunks.
/// Remote bands (beyond it): ungated — data from server/flyover summaries.

/// `local_boundary_wu`: the extent the Map can serve — FIXED_STREAM_RADIUS_WU
/// in gameplay, the flyover's detail-chunk radius while flyover is active.
/// `margin`: hysteresis expansion of each band's annulus (0.0 = crisp band
/// assignment for building; > 0.0 = widened keep set for eviction).
fn compute_auto_mode_regions(
    camera_pos: Vec3,
    loaded_chunks: &std::collections::HashSet<common_bevy::chunk::ChunkId>,
    fov: f32,
    margin: f32,
    local_boundary_wu: f32,
) -> std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey> {
    use common_bevy::summary_mesh::{visible_mesh_regions_in_band, visible_mesh_regions_in_band_ungated};

    let bands = horizon_bands(camera_pos, fov, margin);
    let mut all_regions = std::collections::HashSet::new();

    // Bands are split at the stream-radius boundary, not assigned to one
    // side: a band straddling it contributes a gated segment (client-owned,
    // chunk-fed) AND an ungated segment (server/flyover-fed). Assigning the
    // whole band to one side left its other segment with no regions at all.
    for band in &bands {
        // Footprint-overlap enumeration over the level's window: a region
        // is built if its FOOTPRINT overlaps the window, not just its
        // center, so every fragment the cut keeps has geometry behind it.
        // Regions are up to mesh_region_extent_wu(r) across — center-only
        // membership left crescents near every band boundary covered by
        // neither level. Regions are built whole; the shader cut
        // (`update_terrain_cut`) is what confines a level to its window.
        let half_extent = 0.5 * common_bevy::summary::mesh_region_extent_wu(band.r);
        let (win_inner, win_outer) = band.window();
        let band_inner = (win_inner * (1.0 - margin) - half_extent).max(0.0);
        let band_outer = win_outer * (1.0 + margin) + half_extent;

        if band_inner < local_boundary_wu {
            // Segment within the local-data boundary: gate on loaded chunks
            let gated_outer = band_outer.min(local_boundary_wu);
            let regions = visible_mesh_regions_in_band(
                band.r,
                camera_pos.x,
                camera_pos.z,
                band_inner,
                gated_outer,
                loaded_chunks,
            );
            all_regions.extend(regions);
        }
        if band_outer > local_boundary_wu {
            // Segment beyond the boundary: ungated (SummaryCache-fed from
            // server or flyover producer)
            let inner = band_inner.max(local_boundary_wu);
            let regions = visible_mesh_regions_in_band_ungated(
                band.r,
                camera_pos.x,
                camera_pos.z,
                inner,
                band_outer,
            );
            all_regions.extend(regions);
        }
    }

    all_regions
}

/// Build a mesh region (runs off main thread).

/// One builder for every level; only the height lookup differs. r=0 reads
/// tile z straight from the Map. r>0 reads the SummaryCache (server- or
/// flyover-fed) and falls back to sampling the Map with the same 7-sample
/// rule every producer uses — Map z and server elevation agree by
/// construction, so the value is the same whichever side computed it. The
/// next coarser level, which the morph targets read, comes through the
/// same lookup at its own radius. Until every cell and ring cell has data
/// the build yields nothing, and the region is re-dispatched as data
/// streams in.
fn collect_and_build_summary_mesh(
    radius: u32,
    region_key: common_bevy::summary_mesh::MeshRegionKey,
    map: &common_bevy::resources::map::Map,
    cache: &crate::resources::SummaryCache,
) -> SummaryMeshBuildResult {
    let empty = SummaryMeshBuildResult {
        positions: Vec::new(),
        normals: Vec::new(),
        coarse: Vec::new(),
        indices: Vec::new(),
        tri_count: 0,
        mesh_origin: Vec3::ZERO,
        water: Default::default(),
    };

    let tile_z = |q: i32, r: i32| -> Option<i32> { map.get_by_qr(q, r).map(|(qrz, _)| qrz.z) };

    // The water over the region, built once the ground is: at r = 0 the
    // map's per-tile surface, above it the surface each summary carries.
    let with_water = |smr: &common_bevy::summary_mesh::SummaryMeshResult, water: &dyn Fn(i32, i32) -> Option<i32>| {
        let mut result = smr_to_result(smr);
        let w = common_bevy::summary_mesh::build_water_mesh_region(radius, region_key, water);
        result.water = crate::resources::WaterGeometry { positions: w.positions, normals: w.normals, indices: w.indices };
        result
    };

    // The builder also reads the ring of cells around the region, which
    // belong to neighbouring regions, and the coarser level's cells under
    // it: cache lookups are per region, memoised across the build.
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let regions: std::cell::RefCell<
        std::collections::HashMap<common_bevy::summary_mesh::MeshRegionKey, Option<std::sync::Arc<crate::resources::RegionData>>>,
    > = Default::default();
    let cached = |level: u32, sq: i32, sr: i32| -> Option<(i32, Option<i32>)> {
        let (mn, mm) = region_lat.cell_id(sq, sr);
        let key = common_bevy::summary_mesh::MeshRegionKey { r: level, mn, mm };
        regions
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| cache.get_region(&key))
            .as_ref()
            .and_then(|d| d.cells.get(&(sq, sr)).copied())
    };
    // A level's heights: the tile's own at r = 0, else the cached summary
    // or the same seven samples over the map's tiles.
    let level_z = |level: u32| {
        move |sq: i32, sr: i32| -> Option<i32> {
            if level == 0 {
                return tile_z(sq, sr);
            }
            if let Some((z, _)) = cached(level, sq, sr) {
                return Some(z);
            }
            common_bevy::summary::sample_center_z_opt(level, sq, sr, tile_z)
        }
    };
    let height = level_z(radius);
    let coarse = common_bevy::summary::coarser_level(radius).map(level_z);
    let coarse: Option<&dyn Fn(i32, i32) -> Option<i32>> = coarse.as_ref().map(|c| c as &dyn Fn(i32, i32) -> Option<i32>);

    if radius == 0 {
        let tile_water = |q: i32, r: i32| -> Option<i32> { map.water_at(q, r) };
        return common_bevy::summary_mesh::build_summary_mesh_region(0, region_key, &height, coarse)
            .as_ref()
            .map_or(empty, |smr| with_water(smr, &tile_water));
    }

    // Water follows the height's provenance: the cached surface where the
    // cell was sent, else the same seven samples over the map's tiles, and
    // nothing where the tiles are not all there.
    let summary_water = |sq: i32, sr: i32| -> Option<i32> {
        if let Some((_, water)) = cached(radius, sq, sr) {
            return water;
        }
        common_bevy::summary::sample_center_z_opt(radius, sq, sr, tile_z)?;
        common_bevy::summary::sample_center_water(radius, sq, sr, |q, r| map.water_at(q, r))
    };

    common_bevy::summary_mesh::build_summary_mesh_region(radius, region_key, &height, coarse)
        .as_ref()
        .map_or(empty, |smr| with_water(smr, &summary_water))
}

fn smr_to_result(smr: &common_bevy::summary_mesh::SummaryMeshResult) -> SummaryMeshBuildResult {
    SummaryMeshBuildResult {
        positions: smr.positions.clone(),
        normals: smr.normals.clone(),
        coarse: smr.coarse.clone(),
        indices: smr.indices.clone(),
        tri_count: smr.tri_count,
        mesh_origin: smr.mesh_origin,
        water: Default::default(),
    }
}

/// Build a Bevy Mesh from raw geometry buffers. `coarse` is the ground's
/// morph target per vertex; the water carries none.
fn build_bevy_mesh(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    coarse: Option<&[[f32; 4]]>,
    indices: &[u32],
) -> Mesh {
    use bevy::render::render_resource::PrimitiveTopology;
    use bevy::asset::RenderAssetUsages;
    use bevy_mesh::Indices;

    let verts: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = normals.iter().map(|n| Vec3::from_array(*n)).collect();
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
    .with_inserted_indices(Indices::U32(indices.to_vec()));
    match coarse {
        Some(coarse) => mesh.with_inserted_attribute(crate::resources::ATTRIBUTE_COARSE_SURFACE, coarse.to_vec()),
        None => mesh,
    }
}

/// Poll completed summary mesh tasks, upload their meshes, spawn/update
/// entities. A region's geometry is final when its task completes: regions
/// share corners by construction, so nothing is stitched afterwards.
pub fn poll_summary_meshes(
    mut commands: Commands,
    mut summary_meshes: ResMut<SummaryMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    mut terrain_material: ResMut<TerrainMaterial>,
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    water_material: Res<crate::plugins::water::WaterMaterial>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("sum_poll");

    // Poll async tasks, keeping the geometry so the entity can be respawned
    // without a rebuild.
    let mut to_upload: Vec<common_bevy::summary_mesh::MeshRegionKey> = Vec::new();

    for (&region_key, state) in summary_meshes.states.iter_mut() {
        if let Some(task) = &mut state.task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.task = None;
                state.mesh_origin = result.mesh_origin;
                state.base_positions = result.positions;
                state.base_normals = result.normals;
                state.base_coarse = result.coarse;
                state.base_indices = result.indices;
                state.base_tri_count = result.tri_count;
                state.base_water = result.water;
                state.waiting = result.tri_count == 0;
                to_upload.push(region_key);
            }
        }
    }

    // Orphaned states: geometry but no entity, after a flyover stash restore
    // (entities were despawned on toggle-on). A task may already be pending;
    // the stored geometry goes up now so there is no flash.
    for (&region_key, state) in summary_meshes.states.iter() {
        if !state.base_positions.is_empty() && state.entity.is_none() && !to_upload.contains(&region_key) {
            to_upload.push(region_key);
        }
    }

    struct MeshBuild {
        key: common_bevy::summary_mesh::MeshRegionKey,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        coarse: Vec<[f32; 4]>,
        indices: Vec<u32>,
        tri_count: u32,
        water: crate::resources::WaterGeometry,
    }

    let builds: Vec<MeshBuild> = to_upload
        .iter()
        .filter_map(|&key| {
            let state = summary_meshes.states.get(&key)?;
            (!state.base_positions.is_empty()).then(|| MeshBuild {
                key,
                positions: state.base_positions.clone(),
                normals: state.base_normals.clone(),
                coarse: state.base_coarse.clone(),
                indices: state.base_indices.clone(),
                tri_count: state.base_tri_count,
                water: state.base_water.clone(),
            })
        })
        .collect();

    // Upload meshes, spawn/update entities.
    for build in builds {
        if build.tri_count == 0 {
            continue;
        }

        let mesh = build_bevy_mesh(&build.positions, &build.normals, Some(&build.coarse), &build.indices);
        let mesh_handle = meshes.add(mesh);

        let state = summary_meshes.states.get_mut(&build.key).unwrap();
        state.mesh_handle = Some(mesh_handle.clone());
        state.tri_count = build.tri_count;

        let entity = match state.entity {
            Some(entity) => {
                commands.entity(entity).insert(Mesh3d(mesh_handle));
                // The water is the ground's child: rebuilt with it.
                commands.entity(entity).despawn_related::<Children>();
                entity
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(mesh_handle),
                        MeshMaterial3d(terrain_material.for_level(build.key.r, &mut materials)),
                        Transform::from_translation(state.mesh_origin),
                        SummaryMesh { region_key: build.key },
                    ))
                    .id();
                state.entity = Some(entity);
                entity
            }
        };

        if !build.water.indices.is_empty() {
            let water = build_bevy_mesh(&build.water.positions, &build.water.normals, None, &build.water.indices);
            commands.entity(entity).with_child((
                Mesh3d(meshes.add(water)),
                MeshMaterial3d(water_material.0.clone()),
                Transform::IDENTITY,
                bevy_light::NotShadowCaster,
                crate::resources::WaterMesh,
            ));
        }
    }

    // Diagnostics.
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
    tri_stats.async_mesh = summary_meshes.states.values().filter(|s| s.task.is_some()).count() as u32;
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Coverage invariant for the LoD band system: every ground point inside
    /// the horizon (a) lies within at least one needed region at its band's
    /// level, and (b) if that region's footprint extends past the local-data
    /// boundary, the producer enumerates it (so its data will exist).

    /// Run for gameplay (boundary = FIXED_STREAM_RADIUS_WU) and flyover
    /// (small detail-chunk boundary) — the flyover case caught the missing
    /// ring between the flyover's chunks and the first produced band.
    #[test]
    fn lod_bands_cover_horizon_without_gaps() {
        use common_bevy::chunk::{
            calculate_visible_chunks, ChunkId, APOTHEM_FACTOR, CHUNK_EXTENT_WU,
            FIXED_STREAM_APOTHEM_WU, FIXED_STREAM_RADIUS,
        };
        use common_bevy::summary::{compute_active_bands, mesh_region_extent_wu};
        use common_bevy::summary_mesh::visible_lod_regions;

        let cases: &[(f32, f32, u8, f32)] = &[
            // (fov, camera ground y, loaded chunk ring, local boundary)
            (common::camera::MAX_GAMEPLAY_FOV, 0.0, FIXED_STREAM_RADIUS, FIXED_STREAM_APOTHEM_WU),
            (common::camera::MAX_GAMEPLAY_FOV, 80.0, FIXED_STREAM_RADIUS, FIXED_STREAM_APOTHEM_WU),
            (
                crate::systems::camera::MAX_FLYOVER_FOV,
                0.0,
                6,
                6.0 * CHUNK_EXTENT_WU * APOTHEM_FACTOR,
            ),
            (
                crate::systems::camera::MAX_FLYOVER_FOV,
                200.0,
                9,
                9.0 * CHUNK_EXTENT_WU * APOTHEM_FACTOR,
            ),
        ];

        for &(fov, cam_y, chunk_ring, boundary) in cases {
            // Chunks loaded exactly as the game does (hexagonal coverage —
            // the boundary is its inscribed circle).
            let loaded: std::collections::HashSet<ChunkId> =
                calculate_visible_chunks(ChunkId(0, 0), chunk_ring).into_iter().collect();

            let cam = Vec3::new(0.0, cam_y, 0.0);
            let needed = compute_auto_mode_regions(cam, &loaded, fov, 0.0, boundary);

            // Producer set, from the same horizon formula as the consumer.
            let far_ground = common::camera::far_ground_wu(
                crate::systems::camera::camera_height(fov) + cam_y,
                fov,
            );
            let bands = compute_active_bands(far_ground);
            let produced = visible_lod_regions(&bands, 0.0, 0.0, boundary);

            // (b) Data coverage: needed r>0 regions reaching past the
            // boundary must be produced.
            for k in &needed {
                if k.r == 0 {
                    continue;
                }
                let (kx, kz) = region_center_world(k);
                let reach = (kx * kx + kz * kz).sqrt() + 0.5 * mesh_region_extent_wu(k.r);
                if reach > boundary {
                    assert!(
                        produced.contains(k),
                        "[fov={fov:.2} y={cam_y} b={boundary}] needed region {k:?} \
                         (reach {reach:.1}) is beyond the boundary but not produced"
                    );
                }
                // (c) A region is built only once its ring has heights, so
                // every neighbour of a needed region must be produced or lie
                // wholly inside the loaded tiles (its cells sample within a
                // summary of their centre).
                for (dn, dm) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
                    let n = common_bevy::summary_mesh::MeshRegionKey { r: k.r, mn: k.mn + dn, mm: k.mm + dm };
                    let (nx, nz) = region_center_world(&n);
                    let n_reach = (nx * nx + nz * nz).sqrt()
                        + 0.5 * mesh_region_extent_wu(n.r)
                        + common_bevy::summary::summary_width_wu(n.r);
                    assert!(
                        produced.contains(&n) || n_reach <= boundary,
                        "[fov={fov:.2} y={cam_y} b={boundary}] ring region {n:?} of needed \
                         {k:?} (reach {n_reach:.1}) is neither produced nor inside the Map \
                         — the region could never be built"
                    );
                }
            }

            // (d) A region's morph targets read the coarser cell under each
            // vertex and its ring, so every coarser region whose cells can
            // lie within that reach of a needed region must be produced or
            // sample wholly inside the loaded tiles.
            for k in &needed {
                let Some(c) = common_bevy::summary::coarser_level(k.r) else { continue };
                let (kx, kz) = region_center_world(k);
                let circum = |r: u32| mesh_region_extent_wu(r) / 3.0_f32.sqrt();
                let cell_reach = common_bevy::summary::summary_lattice(c).scale as f32
                    + common_bevy::summary::summary_width_wu(c);
                let reach = circum(k.r) + cell_reach;
                let coarse_lat = common_bevy::summary::mesh_region_lattice();
                let coarse_sum = common_bevy::summary::summary_lattice(c);
                // Every coarser region whose circumcircle meets the reach.
                let span = (reach + circum(c)) / common_bevy::summary::mesh_region_spacing_wu(c);
                let n = span.ceil() as i32 + 1;
                let (ksq, ksr) = coarse_sum.cell_id(
                    (kx as f64 / 1.5).round() as i32,
                    ((kz as f64 - (kx as f64 / 1.5) * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt()).round() as i32,
                );
                let (kmn, kmm) = coarse_lat.cell_id(ksq, ksr);
                for dn in -n..=n {
                    for dm in -n..=n {
                        let cr = common_bevy::summary_mesh::MeshRegionKey { r: c, mn: kmn + dn, mm: kmm + dm };
                        let (cx, cz) = region_center_world(&cr);
                        let dist = ((cx - kx).powi(2) + (cz - kz).powi(2)).sqrt();
                        if dist > reach + circum(c) {
                            continue;
                        }
                        let c_reach = (cx * cx + cz * cz).sqrt() + circum(c)
                            + common_bevy::summary::summary_width_wu(c);
                        assert!(
                            produced.contains(&cr) || c_reach <= boundary,
                            "[fov={fov:.2} y={cam_y} b={boundary}] coarser region {cr:?} under \
                             needed {k:?} (reach {c_reach:.1}) is neither produced nor inside \
                             the Map — the region could never be built"
                        );
                    }
                }
            }

            // (a) Geometric coverage: every ground point inside a level's
            // window — what the cut lets that level show — must lie within
            // some needed region of that level (region circumradius =
            // extent/sqrt(3)). Windows overlap at band edges, so a point
            // there is checked against both levels.
            for (az_deg, band) in (0..360).step_by(5).flat_map(|az| bands.iter().map(move |b| (az, b))) {
                let azr = (az_deg as f32).to_radians();
                let (win_inner, win_outer) = band.window();
                let mut d = win_inner.max(2.0);
                while d < win_outer.min(far_ground - 1.0) {
                    let px = d * azr.cos();
                    let pz = d * azr.sin();
                    let circum = mesh_region_extent_wu(band.r) / 3.0_f32.sqrt();
                    let covered = needed.iter().any(|k| {
                        if k.r != band.r {
                            return false;
                        }
                        let (kx, kz) = region_center_world(k);
                        let dx = kx - px;
                        let dz = kz - pz;
                        dx * dx + dz * dz <= circum * circum
                    });
                    if !covered {
                        // Owning region of this point at the band's level
                        let tile_q = (px as f64 / 1.5).round() as i32;
                        let tile_r = ((pz as f64 - (px as f64 / 1.5) * 3.0_f64.sqrt() / 2.0)
                            / 3.0_f64.sqrt())
                        .round() as i32;
                        let slat = common_bevy::summary::summary_lattice(band.r);
                        let (sq, sr) = slat.cell_id(tile_q, tile_r);
                        let rlat = common_bevy::summary::mesh_region_lattice();
                        let (mn, mm) = rlat.cell_id(sq, sr);
                        let owner = common_bevy::summary_mesh::MeshRegionKey { r: band.r, mn, mm };
                        let (ox, oz) = region_center_world(&owner);
                        let owner_dist = (ox * ox + oz * oz).sqrt();
                        // Probe the enumerators directly with the same args
                        // compute_auto_mode_regions uses for this band.
                        let h = 0.5 * mesh_region_extent_wu(band.r);
                        let b_inner = (win_inner - h).max(0.0);
                        let b_outer = win_outer + h;
                        let gated = common_bevy::summary_mesh::visible_mesh_regions_in_band(
                            band.r, 0.0, 0.0, b_inner, b_outer.min(boundary), &loaded,
                        );
                        let ungated = common_bevy::summary_mesh::visible_mesh_regions_in_band_ungated(
                            band.r, 0.0, 0.0, b_inner.max(boundary), b_outer,
                        );
                        panic!(
                            "[fov={fov:.2} y={cam_y} b={boundary}] uncovered ground at \
                             d={d:.1} az={az_deg} (band r={}, circum={circum:.1}); \
                             owner {owner:?} center_dist={owner_dist:.1} \
                             in_needed={} in_produced={} in_gated={} in_ungated={} \
                             gated_range=[{b_inner:.1},{:.1}] owner_summary=({sq},{sr})",
                            band.r,
                            needed.contains(&owner),
                            produced.contains(&owner),
                            gated.contains(&owner),
                            ungated.contains(&owner),
                            b_outer.min(boundary),
                        );
                    }
                    d += 7.0;
                }
            }
        }
    }
}
