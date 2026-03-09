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
    resources::{ChunkSummaries, LoadedChunks, PendingChunkMeshes, PendingSummaryMeshes, Server, SkipNeighborRegen, TerrainMaterial},
};
use common_bevy::{
    chunk::{chunk_to_tile, loc_to_chunk, terrain_chunk_radius, visibility_radius, CHUNK_SIZE, FOV_CHUNK_RADIUS},
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

/// Scan the map and spawn mesh generation tasks for chunks that have tiles but no meshes
/// This system runs periodically and automatically generates meshes after the drain loop
/// processes tile events, avoiding the need for coordination between systems
pub fn spawn_missing_chunk_meshes(
    map: Res<common_bevy::resources::map::Map>,
    chunk_mesh_query: Query<&ChunkMesh, Without<SummaryChunk>>,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    diagnostics_state: Res<DiagnosticsState>,
    skip_neighbor_regen: Res<SkipNeighborRegen>,
    loaded_chunks: Res<LoadedChunks>,
) {
    use std::collections::HashSet;
    use common_bevy::chunk::{ChunkId, calculate_visible_chunks};
    use bevy::tasks::AsyncComputeTaskPool;

    // Get chunks that already have full-detail mesh entities (exclude summary LoD)
    let chunks_with_meshes: HashSet<ChunkId> = chunk_mesh_query
        .iter()
        .map(|mesh| mesh.chunk_id)
        .collect();

    // Build set of actively-tracked chunks that have at least one tile in the map.
    // Iterates known chunk IDs and spot-checks a single tile (center) instead of
    // scanning every tile in the map.
    let candidate_chunks = loaded_chunks.chunks.iter()
        .chain(skip_neighbor_regen.chunks.iter());

    let mut chunks_with_tiles: HashSet<ChunkId> = HashSet::new();
    for &chunk_id in candidate_chunks {
        // Spot-check center tile — if the chunk is loaded, its center tile exists
        let center = chunk_to_tile(chunk_id, 8, 8);
        if map.get_by_qr(center.q, center.r).is_some() {
            chunks_with_tiles.insert(chunk_id);
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
    loaded_chunks: Res<LoadedChunks>,
    skip_regen: Res<SkipNeighborRegen>,
    #[cfg(feature = "admin")]
    flyover: Option<Res<crate::systems::admin::FlyoverState>>,
) {
    use common_bevy::chunk::ChunkId;

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
                // Skip orphaned tasks — chunk was evicted before mesh completed
                let wanted = loaded_chunks.chunks.contains(&chunk_id)
                    || skip_regen.chunks.contains(&chunk_id);
                #[cfg(feature = "admin")]
                let wanted = wanted || flyover.as_ref().map_or(false, |f| f.admin_chunks.contains(&chunk_id));
                if !wanted { continue; }

                // Spawn new mesh entity. If a summary mesh exists for
                // this chunk, resolve_lod_overlap will remove it.
                commands.spawn((
                    Mesh3d(meshes.add(chunk_mesh)),
                    MeshMaterial3d(terrain_material.handle.clone()),
                    ChunkMesh { chunk_id },
                ));
            }
        }
    }
}

/// When a chunk has both a full-detail mesh and a summary mesh, remove whichever
/// is wrong based on FOV distance. Inside FOV: full-detail wins (remove summary).
/// Outside FOV: summary wins (remove full-detail). Because the surviving mesh
/// already exists when the other is despawned, there is never a visible gap.
pub fn resolve_lod_overlap(
    mut commands: Commands,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    full_detail_meshes: Query<(Entity, &ChunkMesh), Without<SummaryChunk>>,
    summary_meshes: Query<(Entity, &ChunkMesh), With<SummaryChunk>>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    #[cfg(feature = "admin")]
    flyover: Option<Res<crate::systems::admin::FlyoverState>>,
) {
    let Ok(player_loc) = player_query.single() else { return };
    let player_chunk = loc_to_chunk(**player_loc);
    let player_z = player_loc.z;
    let fov_buffer = FOV_CHUNK_RADIUS as i32 + 1;

    let full_detail_set: std::collections::HashSet<common_bevy::chunk::ChunkId> =
        full_detail_meshes.iter().map(|(_, cm)| cm.chunk_id).collect();

    let summary_set: std::collections::HashSet<common_bevy::chunk::ChunkId> =
        summary_meshes.iter().map(|(_, cm)| cm.chunk_id).collect();

    // During flyover, use admin_chunks membership instead of FOV distance
    // to decide whether full-detail or summary wins. Flyover chunks are near
    // the camera but far from the player, so the FOV check would be wrong.
    #[cfg(feature = "admin")]
    let flyover_inner = flyover.as_ref()
        .filter(|f| f.active)
        .map(|f| &f.admin_chunks);
    #[cfg(not(feature = "admin"))]
    let flyover_inner: Option<&std::collections::HashSet<common_bevy::chunk::ChunkId>> = None;

    // Pass 1: Resolve overlaps (both full-detail and summary exist for same chunk)
    for (entity, cm) in full_detail_meshes.iter() {
        if !summary_set.contains(&cm.chunk_id) { continue; }

        let full_detail_wins = if let Some(admin_chunks) = flyover_inner {
            admin_chunks.contains(&cm.chunk_id)
        } else {
            let chebyshev = (cm.chunk_id.0 - player_chunk.0).abs()
                .max((cm.chunk_id.1 - player_chunk.1).abs());
            chebyshev <= fov_buffer
        };

        if full_detail_wins {
            // Full-detail wins, remove summary data
            // (spawn_summary_meshes handles entity despawn via change detection)
            chunk_summaries.summaries.remove(&cm.chunk_id);
        } else {
            // Summary wins, remove full-detail mesh
            commands.entity(entity).despawn();
        }
    }

    // Pass 2: Despawn summary meshes beyond max visibility (no overlap, just too far)
    // Skipped during flyover — flyover_evict_chunks handles this using camera position.
    let skip_pass2 = flyover_inner.is_some();

    if !skip_pass2 {
        let base_plus_buffer = terrain_chunk_radius(player_z) as i32 + 1;
        for (_entity, cm) in summary_meshes.iter() {
            if full_detail_set.contains(&cm.chunk_id) { continue; } // handled above

            let chebyshev = (cm.chunk_id.0 - player_chunk.0).abs()
                .max((cm.chunk_id.1 - player_chunk.1).abs());
            if chebyshev <= base_plus_buffer { continue; }

            let chunk_z = chunk_summaries.summaries.get(&cm.chunk_id)
                .map(|s| s.elevation).unwrap_or(player_z);
            let vis = visibility_radius(player_z, chunk_z, 40.0) as i32 + 1;
            if chebyshev > vis {
                // Remove data only; spawn_summary_meshes handles entity despawn
                chunk_summaries.summaries.remove(&cm.chunk_id);
            }
        }
    }
}

/// Detect summary changes, despawn removed meshes, and dispatch async tasks for regen set.
/// Chunks being regenerated keep their existing mesh entity visible until the async task
/// completes (poll_summary_mesh_tasks updates the mesh asset in place).
pub fn spawn_summary_meshes(
    mut commands: Commands,
    summaries: Res<ChunkSummaries>,
    map: Res<common_bevy::resources::map::Map>,
    existing_meshes: Query<(Entity, &ChunkMesh), With<SummaryChunk>>,
    mut pending: ResMut<PendingSummaryMeshes>,
    mut prev_keys: Local<std::collections::HashSet<common_bevy::chunk::ChunkId>>,
) {
    use std::collections::{HashMap, HashSet};
    use common_bevy::chunk::{ChunkId, ChunkSummary};
    use bevy::tasks::AsyncComputeTaskPool;

    if !summaries.is_changed() {
        return;
    }

    let current_keys: HashSet<ChunkId> = summaries.summaries.keys().copied().collect();

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

    // Despawn only removed meshes (regen meshes stay visible until async task completes)
    for (entity, chunk_mesh) in existing_meshes.iter() {
        if removed.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).despawn();
        }
    }

    // Spawn async tasks for regen set
    let pool = AsyncComputeTaskPool::get();
    for &chunk_id in &regen {
        if !current_keys.contains(&chunk_id) { continue; }

        // Cancel any pending task for this chunk
        pending.tasks.remove(&chunk_id);

        // Capture self + 8 neighbors (9 entries max) — small clone
        let mut neighbor_summaries: HashMap<ChunkId, ChunkSummary> = HashMap::with_capacity(9);
        for dq in -1..=1_i32 {
            for dr in -1..=1_i32 {
                let nid = ChunkId(chunk_id.0 + dq, chunk_id.1 + dr);
                if let Some(s) = summaries.summaries.get(&nid) {
                    neighbor_summaries.insert(nid, *s);
                }
            }
        }

        let map_snapshot = map.clone();

        let task = pool.spawn(async move {
            generate_summary_mesh(chunk_id, &neighbor_summaries, &map_snapshot)
        });

        pending.tasks.insert(chunk_id, task);
    }

    *prev_keys = current_keys;
}

/// Poll pending summary mesh tasks and spawn/update mesh entities when ready.
pub fn poll_summary_mesh_tasks(
    mut commands: Commands,
    mut pending: ResMut<PendingSummaryMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    summary_mesh_query: Query<(Entity, &Mesh3d, &ChunkMesh), With<SummaryChunk>>,
) {
    use common_bevy::chunk::ChunkId;

    let mut completed: Vec<(ChunkId, Mesh)> = Vec::new();

    pending.tasks.retain(|&chunk_id, task| {
        let result = block_on(future::poll_once(task));
        if let Some(mesh) = result {
            completed.push((chunk_id, mesh));
            false
        } else {
            true
        }
    });

    for (chunk_id, summary_mesh) in completed {
        // Check if a summary mesh entity already exists for this chunk
        let existing = summary_mesh_query.iter()
            .find(|(_, _, cm)| cm.chunk_id == chunk_id)
            .map(|(entity, mesh_handle, _)| (entity, mesh_handle.clone()));

        if let Some((_entity, mesh_handle)) = existing {
            // Update mesh asset in place — no despawn, no visual gap
            if let Some(mesh_asset) = meshes.get_mut(&mesh_handle.0) {
                *mesh_asset = summary_mesh;
            }
        } else {
            // Spawn new summary mesh entity
            commands.spawn((
                Mesh3d(meshes.add(summary_mesh)),
                MeshMaterial3d(terrain_material.handle.clone()),
                ChunkMesh { chunk_id },
                SummaryChunk,
            ));
        }
    }
}

/// Generate a hex-grid mesh for a chunk summary.
///
/// Elevation is bilinearly interpolated PER VERTEX (not per tile) from this chunk
/// and its 8 neighbors. Since adjacent hex tiles share edge vertices at the same
/// axial position, both tiles compute the same interpolated elevation for shared
/// vertices — eliminating seams. Color is computed in the terrain shader.
fn generate_summary_mesh(
    chunk_id: common_bevy::chunk::ChunkId,
    summaries: &std::collections::HashMap<common_bevy::chunk::ChunkId, common_bevy::chunk::ChunkSummary>,
    map: &common_bevy::resources::map::Map,
) -> Mesh {
    use common_bevy::chunk::ChunkId;
    use common_bevy::resources::map::Map as CommonMap;
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

    let c00 = (self_elev + nelev(-1, 0) + nelev(0, -1) + nelev(-1, -1)) * 0.25;
    let c10 = (self_elev + nelev( 1, 0) + nelev(0, -1) + nelev( 1, -1)) * 0.25;
    let c01 = (self_elev + nelev(-1, 0) + nelev(0,  1) + nelev(-1,  1)) * 0.25;
    let c11 = (self_elev + nelev( 1, 0) + nelev(0,  1) + nelev( 1,  1)) * 0.25;

    let xz_offsets: [[f32; 2]; 7] = [
        [0.0, 0.0], [0.0, -radius], [w, -h], [w, h],
        [0.0, radius], [-w, h], [-w, -h],
    ];

    let dq_axial: [f32; 7] = [0.0,  1.0/3.0, 2.0/3.0, 1.0/3.0, -1.0/3.0, -2.0/3.0, -1.0/3.0];
    let dr_axial: [f32; 7] = [0.0, -2.0/3.0, -1.0/3.0, 1.0/3.0,  2.0/3.0,  1.0/3.0, -1.0/3.0];
    let y_rise: [f32; 7] = [0.0, rise, rise, rise, rise, rise, rise];

    let cs = CHUNK_SIZE as f32;
    let tile_count = (CHUNK_SIZE * CHUNK_SIZE) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(tile_count * 7);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(tile_count * 7);
    let mut indices: Vec<u32> = Vec::with_capacity(tile_count * 18);

    for oq in 0..CHUNK_SIZE {
        for or in 0..CHUNK_SIZE {
            let tile = chunk_to_tile(chunk_id, oq as u8, or as u8);
            let center_flat: Vec3 = inner.convert(Qrz { q: tile.q, r: tile.r, z: 0 });

            let base = positions.len() as u32;

            let mut hex_pos = [Vec3::ZERO; 7];
            for vi in 0..7 {
                let fq = (oq as f32 + dq_axial[vi]) / cs;
                let fr = (or as f32 + dr_axial[vi]) / cs;

                let e_top = c00 + (c10 - c00) * fq;
                let e_bot = c01 + (c11 - c01) * fq;
                let elev = e_top + (e_bot - e_top) * fr;

                hex_pos[vi] = Vec3::new(
                    center_flat.x + xz_offsets[vi][0],
                    elev * rise + y_rise[vi],
                    center_flat.z + xz_offsets[vi][1],
                );
            }

            // hex_vertex_normal expects [0..5]=outer, [6]=center
            let remapped = [
                hex_pos[1], hex_pos[2], hex_pos[3],
                hex_pos[4], hex_pos[5], hex_pos[6],
                hex_pos[0],
            ];
            for vi in 0..7 {
                positions.push(hex_pos[vi].into());
                let remap_idx = if vi == 0 { 6 } else { vi - 1 };
                normals.push(CommonMap::hex_vertex_normal(&remapped, remap_idx).into());
            }

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
    map_state: Res<common_bevy::resources::map::MapState>,
) {
    use common_bevy::resources::map::TileEvent;

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

/// Evict full-detail chunks that are outside the player's FOV radius.
/// Removes tile data from Map, despawns actors, and generates summary data
/// for evicted chunks. Summary mesh eviction is handled by `resolve_lod_overlap`.
pub fn evict_distant_chunks(
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    mut l2r: ResMut<crate::resources::EntityMap>,
    map: Res<common_bevy::resources::map::Map>,
    map_state: Res<common_bevy::resources::map::MapState>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
) {
    // Only evict if we have a player
    let Ok(player_loc) = player_query.single() else {
        return;
    };

    let player_chunk = loc_to_chunk(**player_loc);
    let fov_buffer = FOV_CHUNK_RADIUS as i32 + 1;

    // Pass 1: Evict full-detail chunk data (tiles + actors)
    // Full-detail mesh entities are cleaned up by resolve_lod_overlap once
    // a summary mesh exists for the same chunk.
    let active_chunks: std::collections::HashSet<_> = loaded_chunks.chunks.iter().copied()
        .filter(|chunk_id| {
            let chebyshev = (chunk_id.0 - player_chunk.0).abs()
                .max((chunk_id.1 - player_chunk.1).abs());
            chebyshev <= fov_buffer
        })
        .collect();

    let evictable = loaded_chunks.get_evictable(&active_chunks);

    if !evictable.is_empty() {
        // Generate summaries from tile data before tiles are removed.
        // resolve_lod_overlap will despawn the full-detail mesh entity once
        // spawn_summary_meshes has created the summary mesh from this data.
        for &chunk_id in &evictable {
            if !chunk_summaries.summaries.contains_key(&chunk_id) {
                let center = chunk_to_tile(chunk_id, 8, 8);
                if let Some((qrz, biome)) = map.get_by_qr(center.q, center.r) {
                    chunk_summaries.summaries.insert(chunk_id, common_bevy::chunk::ChunkSummary {
                        chunk_id,
                        elevation: qrz.z,
                        biome,
                    });
                }
            }
        }

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

        // Queue tile despawns — enumerate tile coords from chunk IDs instead
        // of scanning every tile in the map (O(evicted × 256) vs O(all tiles)).
        {
            use common_bevy::resources::map::TileEvent;

            let mut any_despawned = false;
            for &chunk_id in &evictable {
                for oq in 0..CHUNK_SIZE as u8 {
                    for or_ in 0..CHUNK_SIZE as u8 {
                        let tile = chunk_to_tile(chunk_id, oq, or_);
                        if let Some((qrz, _)) = map.get_by_qr(tile.q, tile.r) {
                            map_state.queue_event(TileEvent::Despawn(qrz));
                            any_despawned = true;
                        }
                    }
                }
            }

            if any_despawned {
                loaded_chunks.evict(&evictable);
            }
        }
    }

    // Summary mesh eviction is handled by resolve_lod_overlap (Pass 2).
}
