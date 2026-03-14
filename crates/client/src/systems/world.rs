use std::f32::consts::PI;

use bevy::{
    math::ops::*,
    prelude::*,
    tasks::{block_on, futures_lite::future},
};
use bevy_light::{CascadeShadowConfig, CascadeShadowConfigBuilder};

pub const TILE_SIZE: f32 = 1.;

use crate::{
    components::{ChunkMesh, SummaryChunk},
    plugins::diagnostics::DiagnosticsState,
    resources::{ChunkSummaries, LoadedChunks, PendingChunkMeshes, PendingSummaryMeshes, Server, SkipNeighborRegen, TerrainMaterial},
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

/// Evict data beyond the player's view range. Summary data is created
/// before tile removal so `reconcile_meshes` can dispatch replacement
/// summary meshes without a visual gap.
pub fn evict_data(
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
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

    // ── Evict tiles beyond detail boundary ──
    let evictable: Vec<common_bevy::chunk::ChunkId> = loaded_chunks.chunks.iter()
        .filter(|&&cid| chunk_hex_distance(cid, player_chunk) > detail_buffer)
        .copied()
        .collect();

    if !evictable.is_empty() {
        // Create summary data before removing tiles — reconcile_meshes needs
        // it to dispatch summary mesh tasks as visual replacements.
        for &chunk_id in &evictable {
            if !chunk_summaries.summaries.contains_key(&chunk_id) {
                let center = chunk_id.center();
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

    // ── Evict summaries beyond visibility boundary ──
    let summary_evictable: Vec<common_bevy::chunk::ChunkId> = chunk_summaries.summaries.keys()
        .filter(|&&cid| chunk_hex_distance(cid, player_chunk) > summary_buffer)
        .copied()
        .collect();

    for cid in summary_evictable {
        chunk_summaries.summaries.remove(&cid);
    }
}

// ─────────────────────────────────────────────────────────
// SYSTEM 2: Mesh reconciliation (every frame)
// ─────────────────────────────────────────────────────────
// Single source of truth for all mesh spawn/despawn decisions.
// Ensures the right meshes exist for the data we have.

/// Tracking state for `reconcile_meshes` (previous frame's data for change detection).
#[derive(Default)]
pub(crate) struct ReconcileState {
    prev_loaded: std::collections::HashSet<common_bevy::chunk::ChunkId>,
    prev_summary_keys: std::collections::HashSet<common_bevy::chunk::ChunkId>,
}

/// Zone 1 (hex_dist < detail_radius): full-detail only.
/// Zone 2 (hex_dist == detail_radius): boundary ring — both meshes coexist,
///   summary uses dynamic tuck to hide under full-detail terrain.
/// Zone 3 (hex_dist > detail_radius, ≤ max_radius): summary only.
///   Eviction gate: keep orphaned full-detail mesh alive until summary mesh
///   is ready, preventing a visual gap.
/// Beyond max_radius: nothing should exist.
///
/// Also handles neighbor mesh regen and summary corner regen.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_meshes(
    mut commands: Commands,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    map: Res<common_bevy::resources::map::Map>,
    loaded_chunks: Res<LoadedChunks>,
    mut pending_chunks: ResMut<PendingChunkMeshes>,
    mut pending_summaries: ResMut<PendingSummaryMeshes>,
    full_detail_query: Query<(Entity, &ChunkMesh), Without<SummaryChunk>>,
    summary_query: Query<(Entity, &ChunkMesh), With<SummaryChunk>>,
    player_query: Query<&Loc, With<PlayerControlled>>,
    diagnostics_state: Res<DiagnosticsState>,
    skip_neighbor_regen: Res<SkipNeighborRegen>,
    mut state: Local<ReconcileState>,
    #[cfg(feature = "admin")]
    flyover: Option<Res<crate::systems::admin::FlyoverState>>,
) {
    use std::collections::{HashMap, HashSet};
    use common_bevy::chunk::ChunkId;
    use bevy::tasks::AsyncComputeTaskPool;

    let Ok(player_loc) = player_query.single() else { return };
    let player_chunk = loc_to_chunk(**player_loc);
    let player_z = player_loc.z;

    // Zone classification must use MAX_FOV to match the server's data split.
    // The server decides inner vs outer at detail_boundary_radius(max_z, MAX_FOV).
    // The vertex shader handles the visual tuck using actual camera parameters.
    let detail_radius = chunk::detail_boundary_radius(player_z, chunk::MAX_FOV) as i32;
    let max_radius = terrain_chunk_radius(player_z) as i32 + 1;

    #[cfg(feature = "admin")]
    let flyover_active = flyover.as_ref().map_or(false, |f| f.active);

    // Collect existing mesh entities
    let full_detail_entities: HashMap<ChunkId, Entity> = full_detail_query
        .iter().map(|(e, cm)| (cm.chunk_id, e)).collect();
    let summary_entities: HashMap<ChunkId, Entity> = summary_query
        .iter().map(|(e, cm)| (cm.chunk_id, e)).collect();

    // All known chunks (union of data + existing mesh entities)
    let mut all_chunks: HashSet<ChunkId> = HashSet::new();
    all_chunks.extend(&loaded_chunks.chunks);
    all_chunks.extend(chunk_summaries.summaries.keys());
    all_chunks.extend(full_detail_entities.keys());
    all_chunks.extend(summary_entities.keys());

    // ── Change detection for neighbor regen ──
    let current_summary_keys: HashSet<ChunkId> = chunk_summaries.summaries.keys().copied().collect();
    let new_loaded: HashSet<ChunkId> = loaded_chunks.chunks.difference(&state.prev_loaded).copied().collect();
    let added_summaries: HashSet<ChunkId> = current_summary_keys.difference(&state.prev_summary_keys).copied().collect();
    let removed_summaries: HashSet<ChunkId> = state.prev_summary_keys.difference(&current_summary_keys).copied().collect();

    // ── Summary regen set (corner vertex sharing requires neighbor updates) ──
    let mut summary_regen: HashSet<ChunkId> = HashSet::new();
    for changed in added_summaries.iter().chain(removed_summaries.iter()) {
        for dq in -1..=1_i32 {
            for dr in -1..=1_i32 {
                let nid = ChunkId(changed.0 + dq, changed.1 + dr);
                if current_summary_keys.contains(&nid) {
                    summary_regen.insert(nid);
                }
            }
        }
    }
    summary_regen.extend(&added_summaries);

    // ── Full-detail neighbor regen (edge vertex sharing) ──
    let mut full_regen: HashSet<ChunkId> = HashSet::new();
    for &new_chunk in &new_loaded {
        if skip_neighbor_regen.chunks.contains(&new_chunk) { continue; }
        for &(dn, dm) in &[(1i32,0),(0,1),(-1,1),(-1,0),(0,-1),(1,-1)] {
            let adj = ChunkId(new_chunk.0 + dn, new_chunk.1 + dm);
            if full_detail_entities.contains_key(&adj) {
                full_regen.insert(adj);
            }
        }
    }

    let pool = AsyncComputeTaskPool::get();
    let full_detail_set: HashSet<ChunkId> = full_detail_entities.keys().copied().collect();

    // ── Per-chunk zone reconciliation ──
    for &chunk_id in &all_chunks {
        let hex_dist = chunk_hex_distance(chunk_id, player_chunk);
        // loaded_chunks tracks receipt, but tiles may not be in the map yet
        // (queued via do_spawn, processed by refresh_map next frame).
        // Spot-check center tile to confirm tiles are physically present.
        let has_tiles = loaded_chunks.chunks.contains(&chunk_id)
            && map.get_by_qr(chunk_id.center().q, chunk_id.center().r).is_some();
        let has_summary = chunk_summaries.summaries.contains_key(&chunk_id);
        let has_full_mesh = full_detail_entities.contains_key(&chunk_id);
        let has_summary_mesh = summary_entities.contains_key(&chunk_id);

        // During flyover, skip admin chunks (admin module manages its own meshes)
        #[cfg(feature = "admin")]
        {
            if flyover_active && flyover.as_ref().map_or(false, |f| f.admin_chunks.contains(&chunk_id)) {
                continue;
            }
        }

        if hex_dist < detail_radius {
            // Zone 1: full-detail only

            if has_tiles && !has_full_mesh && !pending_chunks.tasks.contains_key(&chunk_id) {
                let map_snap = map.clone();
                let slopes = diagnostics_state.slope_rendering_enabled;
                let cid = chunk_id;
                pending_chunks.tasks.insert(chunk_id, pool.spawn(async move {
                    map_snap.generate_chunk_mesh(cid, slopes)
                }));
            }

            // Summary: despawn only once full-detail is ready (eviction gate).
            // A chunk can skip zone 2 and jump straight from zone 3 to zone 1
            // when the player crosses a chunk boundary. Without this gate the
            // summary would vanish before the async full-detail task completes.
            if has_summary_mesh && has_full_mesh {
                commands.entity(summary_entities[&chunk_id]).despawn();
            }

        } else if hex_dist == detail_radius {
            // Zone 2: boundary ring — both meshes coexist.
            // The vertex shader tucks the summary mesh under full-detail,
            // providing a smooth LoD transition. Both meshes are required.

            if has_tiles && !has_full_mesh && !pending_chunks.tasks.contains_key(&chunk_id) {
                let map_snap = map.clone();
                let slopes = diagnostics_state.slope_rendering_enabled;
                let cid = chunk_id;
                pending_chunks.tasks.insert(chunk_id, pool.spawn(async move {
                    map_snap.generate_chunk_mesh(cid, slopes)
                }));
            }

            // Summary data comes from the server (zone 2 boundary ring
            // receives both ChunkData and ChunkSummary). Dispatch mesh if missing.
            if chunk_summaries.summaries.contains_key(&chunk_id)
                && !has_summary_mesh && !pending_summaries.tasks.contains_key(&chunk_id)
            {
                summary_regen.insert(chunk_id);
            }

        } else if hex_dist <= max_radius {
            // Zone 3: summary only

            if has_full_mesh {
                if has_summary_mesh {
                    // Summary ready — safe to remove full-detail
                    commands.entity(full_detail_entities[&chunk_id]).despawn();
                }
                // else: eviction gate — keep full-detail visible until summary ready
            }

            if has_summary && !has_summary_mesh && !pending_summaries.tasks.contains_key(&chunk_id) {
                summary_regen.insert(chunk_id);
            }

        } else {
            // Beyond max visibility: nothing should exist
            if has_full_mesh {
                commands.entity(full_detail_entities[&chunk_id]).despawn();
            }
            if has_summary_mesh {
                commands.entity(summary_entities[&chunk_id]).despawn();
            }
        }
    }

    // ── Dispatch full-detail neighbor regen tasks ──
    for chunk_id in full_regen {
        pending_chunks.tasks.remove(&chunk_id);
        let map_snap = map.clone();
        let slopes = diagnostics_state.slope_rendering_enabled;
        pending_chunks.tasks.insert(chunk_id, pool.spawn(async move {
            map_snap.generate_chunk_mesh(chunk_id, slopes)
        }));
    }

    // ── Dispatch summary regen tasks ──
    for &chunk_id in &summary_regen {
        if !chunk_summaries.summaries.contains_key(&chunk_id) { continue; }
        pending_summaries.tasks.remove(&chunk_id);

        // Capture self + 8 grid neighbors for corner vertex elevation averaging
        let mut neighbor_summaries: std::collections::HashMap<ChunkId, common_bevy::chunk::ChunkSummary> = HashMap::with_capacity(9);
        for dq in -1..=1_i32 {
            for dr in -1..=1_i32 {
                let nid = ChunkId(chunk_id.0 + dq, chunk_id.1 + dr);
                if let Some(s) = chunk_summaries.summaries.get(&nid) {
                    neighbor_summaries.insert(nid, *s);
                }
            }
        }

        let map_snap = map.clone();
        let fd_neighbors: HashSet<ChunkId> = [
            (1,0),(0,1),(-1,1),(-1,0),(0,-1),(1,-1),
        ].iter()
            .map(|&(dn, dm)| ChunkId(chunk_id.0 + dn, chunk_id.1 + dm))
            .filter(|nid| full_detail_set.contains(nid))
            .collect();

        pending_summaries.tasks.insert(chunk_id, pool.spawn(async move {
            generate_summary_mesh(chunk_id, &neighbor_summaries, &map_snap, &fd_neighbors)
        }));
    }

    // Update tracking state
    state.prev_loaded = loaded_chunks.chunks.clone();
    state.prev_summary_keys = chunk_summaries.summaries.keys().copied().collect();
}

// ─────────────────────────────────────────────────────────
// SYSTEM 3: Async task completion (every frame)
// ─────────────────────────────────────────────────────────
// Polls async mesh generation tasks, inserts completed mesh entities.
// Validates data still exists before spawning (chunk may have been evicted
// while the async task was running). Only spawns — reconcile_meshes
// makes all despawn decisions.

pub fn poll_mesh_tasks(
    mut commands: Commands,
    mut pending_chunks: ResMut<PendingChunkMeshes>,
    mut pending_summaries: ResMut<PendingSummaryMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    full_detail_query: Query<(Entity, &Mesh3d, &ChunkMesh), Without<SummaryChunk>>,
    summary_query: Query<(Entity, &Mesh3d, &ChunkMesh), With<SummaryChunk>>,
    loaded_chunks: Res<LoadedChunks>,
    chunk_summaries: Res<ChunkSummaries>,
    skip_regen: Res<SkipNeighborRegen>,
    #[cfg(feature = "admin")]
    flyover: Option<Res<crate::systems::admin::FlyoverState>>,
) {
    use common_bevy::chunk::ChunkId;

    // ── Poll full-detail tasks ──
    let mut completed_full: Vec<(ChunkId, Mesh)> = Vec::new();
    pending_chunks.tasks.retain(|&cid, task| {
        match block_on(future::poll_once(task)) {
            Some((mesh, _aabb)) => { completed_full.push((cid, mesh)); false }
            None => true,
        }
    });

    for (chunk_id, chunk_mesh) in completed_full {
        let wanted = loaded_chunks.chunks.contains(&chunk_id)
            || skip_regen.chunks.contains(&chunk_id);
        #[cfg(feature = "admin")]
        let wanted = wanted || flyover.as_ref().map_or(false, |f| f.admin_chunks.contains(&chunk_id));

        let existing = full_detail_query.iter()
            .find(|(_, _, c)| c.chunk_id == chunk_id)
            .map(|(e, mh, _)| (e, mh.clone()));

        if let Some((_entity, mesh_handle)) = existing {
            if let Some(mesh_asset) = meshes.get_mut(&mesh_handle.0) {
                *mesh_asset = chunk_mesh;
            }
        } else if wanted {
            commands.spawn((
                Mesh3d(meshes.add(chunk_mesh)),
                MeshMaterial3d(terrain_material.handle.clone()),
                ChunkMesh { chunk_id },
            ));
        }
    }

    // ── Poll summary tasks ──
    let mut completed_summary: Vec<(ChunkId, Mesh)> = Vec::new();
    pending_summaries.tasks.retain(|&cid, task| {
        match block_on(future::poll_once(task)) {
            Some(mesh) => { completed_summary.push((cid, mesh)); false }
            None => true,
        }
    });

    for (chunk_id, summary_mesh) in completed_summary {
        if !chunk_summaries.summaries.contains_key(&chunk_id) { continue; }

        let existing = summary_query.iter()
            .find(|(_, _, cm)| cm.chunk_id == chunk_id)
            .map(|(e, mh, _)| (e, mh.clone()));

        if let Some((_entity, mesh_handle)) = existing {
            if let Some(mesh_asset) = meshes.get_mut(&mesh_handle.0) {
                *mesh_asset = summary_mesh;
            }
        } else {
            commands.spawn((
                Mesh3d(meshes.add(summary_mesh)),
                MeshMaterial3d(terrain_material.handle.clone()),
                ChunkMesh { chunk_id },
                SummaryChunk,
            ));
        }
    }
}

/// Generate a single chunk-sized hex mesh for a summary chunk.
///
/// Each summary is 1 hex (7 vertices, 6 triangles) whose corners sit at the
/// centroids of each triple of mutually adjacent chunk centers.  This aligns
/// the hex to the rotated chunk lattice so adjacent summaries share vertices
/// exactly.  Elevation at each corner is the average of the 3 chunk elevations.
///
/// All vertices are stored at their **natural (untucked)** elevation.
/// The vertex shader computes tuck per-frame using the live camera position,
/// producing perfectly smooth transitions as the player moves.
///
/// Per-vertex tuck topology is encoded in `uv.y`:
///   - `1.0` → tuckable (inner-facing corner adjacent to full-detail)
///   - `0.5` → non-tuckable summary (outer-facing, meets LoD1 ring)
/// The mesh only rebuilds when tuck topology changes (which corners are
/// inner vs outer), not when the player moves.
fn generate_summary_mesh(
    chunk_id: common_bevy::chunk::ChunkId,
    summaries: &std::collections::HashMap<common_bevy::chunk::ChunkId, common_bevy::chunk::ChunkSummary>,
    map: &common_bevy::resources::map::Map,
    full_detail_neighbors: &std::collections::HashSet<common_bevy::chunk::ChunkId>,
) -> Mesh {
    use common_bevy::chunk::{ChunkId, LATTICE_V1, LATTICE_V2};
    use common_bevy::resources::map::Map as CommonMap;
    use qrz::{Convert, Qrz};

    let inner = map.inner_arc();
    let rise = inner.rise();

    let self_elev = summaries.get(&chunk_id).map(|s| s.elevation as f32).unwrap_or(0.0);
    let has_tuck = !full_detail_neighbors.is_empty();

    // Chunk neighbor elevation in lattice coordinates (dn, dm)
    let nelev = |dn: i32, dm: i32| -> f32 {
        summaries.get(&ChunkId(chunk_id.0 + dn, chunk_id.1 + dm))
            .map(|s| s.elevation as f32)
            .unwrap_or(self_elev)
    };

    // Convert a tile (q,r) to world XZ via the map
    let to_world = |q: i32, r: i32| -> Vec3 {
        inner.convert(Qrz { q, r, z: 0 })
    };

    // Chunk center in world space
    let center_tile = chunk_id.center();
    let center_world = to_world(center_tile.q, center_tile.r);

    // 6 lattice neighbors in order (CW when viewed top-down in world space):
    //   v1, v2, v2-v1, -v1, -v2, v1-v2
    let lattice_neighbors: [(i32, i32); 6] = [
        (1, 0),   // v1
        (0, 1),   // v2
        (-1, 1),  // v2 - v1
        (-1, 0),  // -v1
        (0, -1),  // -v2
        (1, -1),  // v1 - v2
    ];

    let center_y = self_elev * rise + rise;
    // Center vertex is always tuckable if any neighbor has full detail
    let center_tuck_flag: f32 = if has_tuck { 1.0 } else { 0.5 };

    // Build 7 vertex positions at natural elevation: [0]=center, [1..6]=outer
    let mut hex_pos = [Vec3::ZERO; 7];
    let mut tuck_flags = [0.5_f32; 7]; // default: non-tuckable summary
    tuck_flags[0] = center_tuck_flag;
    hex_pos[0] = Vec3::new(center_world.x, center_y, center_world.z);

    for i in 0..6 {
        let (dn1, dm1) = lattice_neighbors[i];
        let (dn2, dm2) = lattice_neighbors[(i + 1) % 6];

        // Neighbor centers in tile coordinates
        let n1q = dn1 * LATTICE_V1.0 + dm1 * LATTICE_V2.0;
        let n1r = dn1 * LATTICE_V1.1 + dm1 * LATTICE_V2.1;
        let n2q = dn2 * LATTICE_V1.0 + dm2 * LATTICE_V2.0;
        let n2r = dn2 * LATTICE_V1.1 + dm2 * LATTICE_V2.1;

        // Centroid of three chunk centers in world space
        let w1 = to_world(center_tile.q + n1q, center_tile.r + n1r);
        let w2 = to_world(center_tile.q + n2q, center_tile.r + n2r);
        let cx = (center_world.x + w1.x + w2.x) / 3.0;
        let cz = (center_world.z + w1.z + w2.z) / 3.0;

        // Elevation: average of the 3 chunks sharing this vertex
        let elev = (self_elev + nelev(dn1, dm1) + nelev(dn2, dm2)) / 3.0;
        let y = elev * rise + rise;

        // Tuck flag: tuckable (1.0) if EITHER adjacent neighbor has full detail.
        // Non-tuckable (0.5) otherwise — stays at natural height to match LoD1.
        let corner_tuckable = if has_tuck {
            let n1_id = ChunkId(chunk_id.0 + dn1, chunk_id.1 + dm1);
            let n2_id = ChunkId(chunk_id.0 + dn2, chunk_id.1 + dm2);
            full_detail_neighbors.contains(&n1_id)
                || full_detail_neighbors.contains(&n2_id)
        } else {
            false
        };

        tuck_flags[i + 1] = if corner_tuckable { 1.0 } else { 0.5 };
        hex_pos[i + 1] = Vec3::new(cx, y, cz);
    }

    // hex_vertex_normal expects [0..5]=outer, [6]=center
    let remapped = [
        hex_pos[1], hex_pos[2], hex_pos[3],
        hex_pos[4], hex_pos[5], hex_pos[6],
        hex_pos[0],
    ];

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(7);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(7);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(7);

    for vi in 0..7 {
        positions.push(hex_pos[vi].into());
        let remap_idx = if vi == 0 { 6 } else { vi - 1 };
        normals.push(CommonMap::hex_vertex_normal(&remapped, remap_idx).into());
        // uv.x = natural world Y (for fragment color ramp)
        // uv.y = tuck flag (1.0 = tuckable, 0.5 = non-tuckable summary)
        uvs.push([hex_pos[vi].y, tuck_flags[vi]]);
    }

    // 6 triangles: [center, v_{i+1}, v_i] (CCW winding, matches full-detail)
    let mut indices: Vec<u32> = Vec::with_capacity(18);
    for i in 0..6u32 {
        let v1 = 1 + i;
        let v2 = 1 + ((i + 1) % 6);
        indices.extend_from_slice(&[0, v2, v1]);
    }

    let mut mesh = Mesh::new(
        bevy_mesh::PrimitiveTopology::TriangleList,
        bevy_asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
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
