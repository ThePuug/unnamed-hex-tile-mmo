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
    chunk::{self, chunk_hex_distance, chunk_tiles, loc_to_chunk, CHUNK_EXTENT_WU},
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

// ── Client-Side Hex-Native LoD ──

/// Maximum hexball radius. Decimation tries r=max down to r=1.
const MAX_HEXBALL_RADIUS: u32 = 3;

const TILE_RADIUS: f32 = 1.0;
const RISE: f32 = 0.8;

/// Compute the decimation threshold for a chunk at a given hex distance from the player.
/// Derived from vertical visual acuity: how many z-levels fit in TILE_PIXEL_THRESHOLD pixels.
///
/// threshold(D) = floor(P × 2 × D_wu × tan(FOV/2) / (RISE × screen_height))
pub fn lod_threshold(chunk_distance: i32) -> u32 {
    if chunk_distance <= 0 {
        return 0;
    }
    const TILE_PIXEL_THRESHOLD: f32 = 4.0;
    const THRESHOLD_FOV: f32 = std::f32::consts::PI / 3.0; // 60°
    const SCREEN_HEIGHT: f32 = 1080.0;

    let d_wu = chunk_distance as f32 * CHUNK_EXTENT_WU;
    let raw = TILE_PIXEL_THRESHOLD * 2.0 * d_wu * (THRESHOLD_FOV / 2.0).tan()
        / (RISE * SCREEN_HEIGHT);
    raw.floor() as u32
}

/// Dispatch hex-native decimation tasks for newly loaded chunks.
/// Runs every frame — checks for chunks that have tiles but no task yet.
/// Also re-dispatches when a chunk's distance crosses a threshold boundary.
pub fn dispatch_lod_tasks(
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    client_timers: Res<crate::resources::ClientTimers>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::systems::admin::FlyoverState>>,
    #[cfg(feature = "admin")] flyover_config: Option<Res<crate::systems::admin::FlyoverDecimationConfig>>,
) {
    if !map.take_changed() && player_query.is_empty() {
        return;
    }
    let _t = client_timers.0.scope("lod_disp");

    // Use flyover position for threshold calculation when active
    let player_chunk = {
        #[cfg(feature = "admin")]
        {
            if let Some(ref fly) = flyover {
                if fly.active {
                    use qrz::Convert;
                    let qrz: qrz::Qrz = map.convert(fly.world_position);
                    Some(loc_to_chunk(qrz))
                } else {
                    player_query.iter().next().map(|loc| loc_to_chunk(**loc))
                }
            } else {
                player_query.iter().next().map(|loc| loc_to_chunk(**loc))
            }
        }
        #[cfg(not(feature = "admin"))]
        { player_query.iter().next().map(|loc| loc_to_chunk(**loc)) }
    };

    // When flyover has a manual threshold, use it instead of distance-based
    #[cfg(feature = "admin")]
    let manual_threshold = flyover
        .as_ref()
        .filter(|f| f.active)
        .and_then(|_| flyover_config.as_ref())
        .map(|c| c.threshold);
    let pool = bevy::tasks::AsyncComputeTaskPool::get();

    for &chunk_id in &loaded_chunks.chunks {
        let threshold = {
            #[cfg(feature = "admin")]
            {
                manual_threshold.unwrap_or_else(|| {
                    let dist = player_chunk
                        .map(|pc| chunk_hex_distance(chunk_id, pc))
                        .unwrap_or(0);
                    lod_threshold(dist)
                })
            }
            #[cfg(not(feature = "admin"))]
            {
                let dist = player_chunk
                    .map(|pc| chunk_hex_distance(chunk_id, pc))
                    .unwrap_or(0);
                lod_threshold(dist)
            }
        };

        // Skip if already at correct threshold (and not currently re-dispatching)
        if let Some(state) = lod_meshes.states.get(&chunk_id) {
            if state.task.is_some() {
                continue; // task in flight — don't double-dispatch
            }
            if state.current_threshold == threshold && state.entity.is_some() {
                continue; // already built at this threshold
            }
            // Threshold changed — need re-dispatch. Fall through.
        }

        // Neighbor gate: all 6 lattice neighbors must have tiles in the Map.
        let lattice_neighbors = [(1i32, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
        let all_neighbors_in_map = lattice_neighbors.iter().all(|&(dn, dm)| {
            let nid = common_bevy::chunk::ChunkId(chunk_id.0 + dn, chunk_id.1 + dm);
            let nc = nid.center();
            map.get_by_qr(nc.q, nc.r).is_some()
        });
        if !all_neighbors_in_map {
            continue;
        }

        // Collect available neighbor perimeter edges for cross-chunk skirts
        let nb_perimeters: Vec<common_bevy::hexball_geometry::ChunkPerimeterEdges> = {
            let lattice_neighbors = [(1i32, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
            lattice_neighbors.iter().filter_map(|&(dn, dm)| {
                let nid = common_bevy::chunk::ChunkId(chunk_id.0 + dn, chunk_id.1 + dm);
                lod_meshes.states.get(&nid)?.perimeter.clone()
            }).collect()
        };

        let map_snap = map.clone();
        let task = pool.spawn(async move {
            collect_and_build_mesh(chunk_id, &map_snap, threshold, nb_perimeters)
        });

        let chunk_origin: Vec3 = map.convert(chunk_id.center());

        // Insert or update state — preserve existing entity for in-place swap
        if let Some(state) = lod_meshes.states.get_mut(&chunk_id) {
            state.task = Some(task);
        } else {
            lod_meshes.states.insert(
                chunk_id,
                ChunkLodState {
                    task: Some(task),
                    entity: None,
                    mesh_handle: None,
                    perimeter: None,
                    tri_count: 0,
                    full_detail_tris: 0,
                    current_threshold: threshold,
                    chunk_origin: chunk_origin,
                },
            );
        }
    }
}

/// Collect tiles from Map snapshot, run hex-native decimation, build mesh.
/// Runs entirely off the main thread via AsyncComputeTaskPool.
fn collect_and_build_mesh(
    chunk_id: common_bevy::chunk::ChunkId,
    map: &common_bevy::resources::map::Map,
    threshold: u32,
    neighbor_perimeters: Vec<common_bevy::hexball_geometry::ChunkPerimeterEdges>,
) -> crate::resources::MeshBuildResult {
    // Collect chunk tiles as (q, r, z) triples
    let chunk_tile_list: Vec<(i32, i32, i32)> = chunk_tiles(chunk_id)
        .filter_map(|(q, r)| map.get_by_qr(q, r).map(|(qrz, _)| (qrz.q, qrz.r, qrz.z)))
        .collect();

    // Build elevation lookup covering chunk + 1-ring neighbors
    let mut elevations = std::collections::HashMap::new();
    for &(q, r, z) in &chunk_tile_list {
        elevations.insert((q, r), z);
        for &(dq, dr) in &[(-1i32,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
            let nq = q + dq;
            let nr = r + dr;
            if !elevations.contains_key(&(nq, nr)) {
                if let Some((actual, _)) = map.get_by_qr(nq, nr) {
                    elevations.insert((actual.q, actual.r), actual.z);
                }
            }
        }
    }

    let chunk_origin: Vec3 = map.convert(chunk_id.center());
    let lookup = |q: i32, r: i32| -> Option<i32> { elevations.get(&(q, r)).copied() };

    // Count full-detail triangles: 6 per tile + 2 per downward cliff edge
    let full_detail_tris = {
        let mut count = chunk_tile_list.len() as u32 * 6;
        for &(q, r, z) in &chunk_tile_list {
            for &(dq, dr) in &[(-1i32,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
                if let Some(nz) = lookup(q + dq, r + dr) {
                    if nz < z { count += 2; }
                }
            }
        }
        count
    };

    // Run hex-native decimation then build mesh via hexball geometry
    let dec = common::hex_decimate::decimate_chunk(&chunk_tile_list, MAX_HEXBALL_RADIUS, threshold, &lookup);
    let mut effective_z = std::collections::HashMap::new();
    for hb in &dec.hexballs {
        effective_z.extend(&hb.effective_z);
    }
    let plan = common_bevy::hexball_geometry::ChunkDecimation {
        hexballs: dec.hexballs.iter().map(|hb| common_bevy::hexball_geometry::HexballDecimation {
            center_q: hb.center_q,
            center_r: hb.center_r,
            center_z: hb.center_z,
            radius: hb.radius,
        }).collect(),
        survivors: dec.survivors.clone(),
        effective_z,
    };
    let nb_refs: Vec<&common_bevy::hexball_geometry::ChunkPerimeterEdges> =
        neighbor_perimeters.iter().collect();
    let (mesh, perimeter, _skirt_stats) = common_bevy::hexball_geometry::build_chunk_mesh(
        &plan, TILE_RADIUS, RISE, chunk_origin, &lookup, &nb_refs,
    );
    let tri_count = mesh.indices().map(|i| i.len() / 3).unwrap_or(0) as u32;

    crate::resources::MeshBuildResult { mesh, tri_count, full_detail_tris, perimeter }
}

/// Poll completed decimation tasks, upload meshes, update diagnostics.
pub fn poll_and_swap_lod(
    mut commands: Commands,
    mut lod_meshes: ResMut<ChunkLodMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    terrain_material: Option<Res<TerrainMaterial>>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let Some(terrain_material) = terrain_material else { return; };
    let _t = client_timers.0.scope("lod_swap");

    let player_chunk = player_query.iter().next().map(|loc| loc_to_chunk(**loc));

    // Accumulate per-tier stats from all active chunks
    let mut tier_map: std::collections::BTreeMap<u32, (u32, u64, u64)> = std::collections::BTreeMap::new();
    let mut total_tris = 0u64;
    let mut mesh_count = 0u32;

    let mut newly_built: Vec<common_bevy::chunk::ChunkId> = Vec::new();

    for (&chunk_id, state) in lod_meshes.states.iter_mut() {
        // Poll task
        if let Some(task) = &mut state.task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.task = None;
                state.tri_count = result.tri_count;
                state.full_detail_tris = result.full_detail_tris;
                state.perimeter = Some(result.perimeter);
                // current_threshold was set at dispatch time — don't overwrite

                let mesh_handle = meshes.add(result.mesh);
                state.mesh_handle = Some(mesh_handle.clone());

                match state.entity {
                    Some(entity) => {
                        // In-place swap — old mesh stays visible until this frame
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

                newly_built.push(chunk_id);
            }
        }

        // Accumulate per-tier stats
        if state.entity.is_some() {
            total_tris += state.tri_count as u64;
            mesh_count += 1;
            let entry = tier_map.entry(state.current_threshold).or_insert((0, 0, 0));
            entry.0 += 1;
            entry.1 += state.tri_count as u64;
            entry.2 += state.full_detail_tris as u64;
        }
    }

    // Phase 2: Deferred cross-chunk skirt patch. All perimeters from this frame
    // are stored (phase 1 complete) before any patching runs — no race between
    // chunks that completed in the same frame.
    let lattice_dirs = [(1i32, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
    for new_chunk_id in &newly_built {
        let new_perimeter = match lod_meshes.states.get(new_chunk_id) {
            Some(s) => s.perimeter.clone(),
            None => continue,
        };
        let Some(new_perimeter) = new_perimeter else { continue };

        for &(dn, dm) in &lattice_dirs {
            let nid = common_bevy::chunk::ChunkId(new_chunk_id.0 + dn, new_chunk_id.1 + dm);
            if nid == *new_chunk_id { continue; }

            // Patch neighbor using this chunk's perimeter
            let neighbor_perimeter = lod_meshes.states.get(&nid)
                .and_then(|s| s.perimeter.clone());

            if let Some(_nb_peri) = &neighbor_perimeter {
                let state = lod_meshes.states.get_mut(&nid).unwrap();
                if let (Some(ref mesh_handle), Some(ref mut perimeter)) =
                    (&state.mesh_handle, &mut state.perimeter)
                {
                    let (skirt_geom, matched_keys) =
                        common_bevy::hexball_geometry::match_cross_chunk_skirts(
                            perimeter, &new_perimeter, state.chunk_origin,
                        );
                    if !matched_keys.is_empty() {
                        for key in &matched_keys { perimeter.edges.remove(key); }
                        if let Some(mesh) = meshes.get_mut(mesh_handle) {
                            common_bevy::hexball_geometry::append_geometry_to_mesh(mesh, &skirt_geom);
                        }
                    }
                }
            }

            // Patch self using neighbor's already-stored perimeter
            if let Some(nb_peri) = neighbor_perimeter {
                let state = lod_meshes.states.get_mut(new_chunk_id).unwrap();
                if let (Some(ref mesh_handle), Some(ref mut perimeter)) =
                    (&state.mesh_handle, &mut state.perimeter)
                {
                    let (skirt_geom, matched_keys) =
                        common_bevy::hexball_geometry::match_cross_chunk_skirts(
                            perimeter, &nb_peri, state.chunk_origin,
                        );
                    if !matched_keys.is_empty() {
                        for key in &matched_keys { perimeter.edges.remove(key); }
                        if let Some(mesh) = meshes.get_mut(mesh_handle) {
                            common_bevy::hexball_geometry::append_geometry_to_mesh(mesh, &skirt_geom);
                        }
                    }
                }
            }
        }
    }

    // Build tier vec
    let max_tier = tier_map.keys().last().copied().unwrap_or(0) as usize;
    let mut tiers = vec![crate::resources::TierStats::default(); max_tier + 1];
    for (&t, &(chunks, tris, full)) in &tier_map {
        tiers[t as usize] = crate::resources::TierStats { chunks, tris, full_detail_tris: full };
    }
    tri_stats.tiers = tiers;
    tri_stats.total_tris = total_tris;
    tri_stats.mesh_count = mesh_count;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_at_distance_0_is_0() {
        assert_eq!(lod_threshold(0), 0);
    }

    #[test]
    fn threshold_increases_with_distance() {
        // At 60° FOV with spec constants, threshold boundaries are:
        // 0: 0–7 chunks, 1: 7–13, 2: 13–20, 3: 20–26
        assert_eq!(lod_threshold(1), 0);
        assert_eq!(lod_threshold(6), 0);
        assert!(lod_threshold(8) >= 1);
        assert!(lod_threshold(14) >= 2);
        assert!(lod_threshold(21) >= 3);
    }

    #[test]
    fn threshold_is_monotonic() {
        let mut prev = 0u32;
        for d in 0..100 {
            let t = lod_threshold(d);
            assert!(t >= prev, "threshold({d})={t} < threshold({})={prev}", d - 1);
            prev = t;
        }
    }
}
