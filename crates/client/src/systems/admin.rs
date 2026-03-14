use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};

use common_bevy::{
    chunk::{
        ChunkId, CHUNK_TILES, DEFAULT_FOV, FOV_CHUNK_RADIUS,
        calculate_visible_chunks_adaptive, chunk_hex_distance,
        chunk_tiles, loc_to_chunk, visibility_radius,
    },
    components::{
        behaviour::PlayerControlled,
        entity_type::{EntityType, decorator::Decorator},
        Actor,
    },
    resources::map::{Map, MapState, TileEvent},
};
use qrz::Convert;

use crate::{
    components::ChunkMesh,
    plugins::console::DevConsoleAction,
    resources::{ChunkSummaries, LoadedChunks, PendingChunkMeshes, PendingSummaryMeshes, SkipNeighborRegen},
    systems::camera::{CameraOrbit, CAMERA_DISTANCE, CAMERA_HEIGHT},
};

// ──── Resources ────

/// Flyover camera state — tracks position, speed ramp, and owned chunks.
#[derive(Resource)]
pub struct FlyoverState {
    pub active: bool,
    pub world_position: Vec3,
    pub speed_multiplier: f32,
    pub hold_time: f32,
    /// Full-detail chunks owned by flyover (inner ring)
    pub admin_chunks: HashSet<ChunkId>,
    /// Summary chunks owned by flyover (outer ring)
    pub admin_summary_chunks: HashSet<ChunkId>,
    pub saved_camera_scale: f32,
}

impl Default for FlyoverState {
    fn default() -> Self {
        Self {
            active: false,
            world_position: Vec3::ZERO,
            speed_multiplier: 1.0,
            hold_time: 0.0,
            admin_chunks: HashSet::new(),
            admin_summary_chunks: HashSet::new(),
            saved_camera_scale: 1.0,
        }
    }
}

/// Marker for admin-generated chunk mesh entities (cleanup target).
#[derive(Component)]
pub struct AdminChunk;

/// Wraps terrain::Terrain for local chunk generation.
/// Arc-wrapped so async tile generation tasks can share it cheaply.
#[derive(Resource)]
pub struct AdminTerrain(pub Arc<terrain::Terrain>);

impl Default for AdminTerrain {
    fn default() -> Self {
        Self(Arc::new(terrain::Terrain::default()))
    }
}

/// Tracks pending async flyover tile/summary generation tasks.
#[derive(Default, Resource)]
pub struct PendingFlyoverTiles {
    pub inner: HashMap<ChunkId, Task<Vec<(qrz::Qrz, EntityType)>>>,
    pub outer: HashMap<ChunkId, Task<common_bevy::chunk::ChunkSummary>>,
}

// ──── Run Conditions ────

pub fn flyover_active(flyover: Res<FlyoverState>) -> bool {
    flyover.active
}

pub fn not_in_flyover(flyover: Option<Res<FlyoverState>>) -> bool {
    flyover.map_or(true, |f| !f.active)
}

// ──── Constants ────

const BASE_SPEED: f32 = 15.0;
const RAMP_SECONDS: f32 = 3.0;
const MAX_SPEED_MULTIPLIER: f32 = 10.0;
const ADMIN_ZOOM_MAX: f32 = 10.0;
const NORMAL_ZOOM_MIN: f32 = 0.08;
const NORMAL_ZOOM_MAX: f32 = 2.0;
const SURFACE_FOLLOW_SPEED: f32 = 5.0;

/// Absolute cap on flyover generation radius
const MAX_FLYOVER_RADIUS: u8 = 60;

/// Compute a generation radius from camera zoom AND player elevation combined.
///
/// Samples terrain at the FOV boundary to find the lowest nearby ground,
/// then projects a camera ray to that ground plane. This avoids the old
/// worst-case sea-level assumption that produced radius ~82 at z=1745.
fn flyover_radius(
    camera_scale: f32,
    player_z: i32,
    center: ChunkId,
    terrain: &terrain::Terrain,
) -> u8 {
    // Orthographic scale → virtual perspective FOV.
    // Zooming out (scale > 1) narrows the virtual FOV → loads more chunks.
    let fov = DEFAULT_FOV / camera_scale.max(0.1);

    // Sample terrain at 9 points around the FOV boundary to find the
    // lowest nearby ground elevation — this is the surface our camera
    // ray needs to reach.
    let detail_r = common_bevy::chunk::detail_boundary_radius(player_z, fov) as i32;
    let mut min_z = player_z;
    for &dq in &[-detail_r, 0, detail_r] {
        for &dr in &[-detail_r, 0, detail_r] {
            let ct = ChunkId(center.0 + dq, center.1 + dr).center();
            let z = terrain.get_height(ct.q, ct.r);
            min_z = min_z.min(z);
        }
    }

    visibility_radius(player_z, min_z, fov).min(MAX_FLYOVER_RADIUS)
}

// ──── Systems ────

/// Reads DevConsoleAction events and toggles flyover on/off.
pub fn execute_admin_actions(
    mut flyover: ResMut<FlyoverState>,
    mut reader: MessageReader<DevConsoleAction>,
    player_query: Query<&Transform, (With<Actor>, With<PlayerControlled>, Without<Camera3d>)>,
    mut camera_query: Query<&mut Projection, With<Camera3d>>,
    mut commands: Commands,
    admin_chunk_query: Query<Entity, With<AdminChunk>>,
    loaded_chunks: Res<LoadedChunks>,
    map: Res<Map>,
    map_state: Res<MapState>,
    admin_terrain: Res<AdminTerrain>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::GotoWorldUnits(wx, wy) => {
                // Terrain world units use a different coordinate system than Bevy Vec3.
                // terrain::hex_to_world: wx = q + r*0.5, wy = r * sqrt(3)/2
                // Invert to get hex q,r, then convert to Vec3 via Map.
                let sqrt_3 = 1.7320508075688772_f64;
                let rf = wy * 2.0 / sqrt_3;
                let qf = wx - rf * 0.5;
                let q = qf.round() as i32;
                let r = rf.round() as i32;
                let z = admin_terrain.0.get_height(q, r);
                let target_qrz = qrz::Qrz { q, r, z };
                let target: Vec3 = map.convert(target_qrz);

                if flyover.active {
                    flyover.world_position = target;
                    info!("Goto: world units ({}, {}) → qr ({}, {}) → flyover", wx, wy, q, r);
                } else {
                    info!("Goto: world units ({}, {}) — enable flyover first", wx, wy);
                }
                continue;
            }
            DevConsoleAction::GotoQR(q, r) => {
                let z = admin_terrain.0.get_height(*q, *r);
                let target_qrz = qrz::Qrz { q: *q, r: *r, z };
                let target: Vec3 = map.convert(target_qrz);

                if flyover.active {
                    flyover.world_position = target;
                    info!("Goto: QR ({}, {}) → flyover", q, r);
                } else {
                    info!("Goto: QR ({}, {}) — enable flyover first", q, r);
                }
                continue;
            }
            _ => {}
        }
        let DevConsoleAction::ToggleFlyover = action else { continue };

        if !flyover.active {
            // Toggle ON
            if let Ok(player_transform) = player_query.single() {
                flyover.world_position = player_transform.translation;
            }

            // Save current camera scale
            if let Ok(projection) = camera_query.single() {
                if let Projection::Orthographic(ortho) = &*projection {
                    flyover.saved_camera_scale = ortho.scale;
                }
            }

            flyover.active = true;
            flyover.hold_time = 0.0;
            flyover.speed_multiplier = 1.0;
            info!("Flyover camera: ON");
        } else {
            // Toggle OFF — cleanup
            for entity in admin_chunk_query.iter() {
                commands.entity(entity).despawn();
            }

            // Cancel all pending async flyover tasks
            pending_tiles.inner.clear();
            pending_tiles.outer.clear();

            // Despawn tiles for admin-only full-detail chunks (skip server-loaded ones)
            // Use O(1) map lookup instead of terrain evaluation
            for &chunk_id in &flyover.admin_chunks {
                if loaded_chunks.chunks.contains(&chunk_id) {
                    continue;
                }
                for (q, r) in chunk_tiles(chunk_id) {
                    if let Some((qrz, _)) = map.get_by_qr(q, r) {
                        map_state.queue_event(TileEvent::Despawn(qrz));
                    }
                }
            }

            // Remove admin summary chunks from ChunkSummaries
            for &chunk_id in &flyover.admin_summary_chunks {
                chunk_summaries.summaries.remove(&chunk_id);
            }

            flyover.admin_chunks.clear();
            flyover.admin_summary_chunks.clear();
            skip_regen.chunks.clear();

            // Restore camera scale (clamped to normal range)
            if let Ok(mut projection) = camera_query.single_mut() {
                if let Projection::Orthographic(ortho) = projection.as_mut() {
                    ortho.scale = flyover.saved_camera_scale.clamp(NORMAL_ZOOM_MIN, NORMAL_ZOOM_MAX);
                }
            }

            flyover.active = false;
            info!("Flyover camera: OFF");
        }
    }
}

/// Flat-top hex direction table for flyover movement (same as input.rs).
const HEX_DIRECTIONS_FLAT: [qrz::Qrz; 6] = [
    qrz::Qrz { q: 0, r: -1, z: 0 },   // 0: N   (0°)
    qrz::Qrz { q: 1, r: -1, z: 0 },   // 1: NE  (60°)
    qrz::Qrz { q: 1, r: 0, z: 0 },    // 2: SE  (120°)
    qrz::Qrz { q: 0, r: 1, z: 0 },    // 3: S   (180°)
    qrz::Qrz { q: -1, r: 1, z: 0 },   // 4: SW  (240°)
    qrz::Qrz { q: -1, r: 0, z: 0 },   // 5: NW  (300°)
];

/// Rotate a hex direction by stepping through the direction table.
fn rotate_hex(dir: &qrz::Qrz, steps: i32) -> qrz::Qrz {
    if let Some(idx) = HEX_DIRECTIONS_FLAT.iter().position(|d| d.q == dir.q && d.r == dir.r) {
        let new_idx = (idx as i32 + steps).rem_euclid(6) as usize;
        HEX_DIRECTIONS_FLAT[new_idx]
    } else {
        *dir
    }
}

/// Smooth camera movement using hex-direction arrow keys with speed ramp.
/// Camera rotation is integrated into movement input (same scheme as player input).
pub fn flyover_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    time: Res<Time>,
    map: Res<Map>,
    admin_terrain: Res<AdminTerrain>,
    mut flyover: ResMut<FlyoverState>,
) {
    let dt = time.delta_secs();

    // Skip movement when Shift is held (camera orbit mode)
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let has_arrows = !shift_pressed && keyboard.any_pressed([
        KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,
    ]);

    if has_arrows {
        let up = keyboard.pressed(KeyCode::ArrowUp);
        let down = keyboard.pressed(KeyCode::ArrowDown);
        let left = keyboard.pressed(KeyCode::ArrowLeft);
        let right = keyboard.pressed(KeyCode::ArrowRight);

        // Use discrete target_index as the stable camera frame
        let camera_idx = orbit.target_index;

        // Same movement-driven rotation scheme as player input:
        // Up+Left/Up+Right: move diagonally + rotate camera.
        // Down variants: move backward, no rotation.
        // Left/Right alone: rotate only.
        let visual_dir = if up && !down {
            if left && !right {
                orbit.step_ccw();
                qrz::Qrz { q: -1, r: 0, z: 0 }     // NW (forward-left)
            } else if right && !left {
                orbit.step_cw();
                qrz::Qrz { q: 1, r: -1, z: 0 }      // NE (forward-right)
            } else {
                qrz::Qrz { q: 0, r: -1, z: 0 }      // N (forward)
            }
        } else if down && !up {
            if left && !right {
                qrz::Qrz { q: -1, r: 1, z: 0 }      // SW (backward-left)
            } else if right && !left {
                qrz::Qrz { q: 1, r: 0, z: 0 }       // SE (backward-right)
            } else {
                qrz::Qrz { q: 0, r: 1, z: 0 }       // S (backward)
            }
        } else if left && !right {
            orbit.step_ccw();
            qrz::Qrz { q: 0, r: 0, z: 0 }           // Rotate only
        } else if right && !left {
            orbit.step_cw();
            qrz::Qrz { q: 0, r: 0, z: 0 }           // Rotate only
        } else {
            qrz::Qrz { q: 0, r: 0, z: 0 }
        };

        if visual_dir.q != 0 || visual_dir.r != 0 {
            // Speed ramp: quadratic ease-in over RAMP_SECONDS
            flyover.hold_time += dt;
            let t = (flyover.hold_time / RAMP_SECONDS).min(1.0);
            flyover.speed_multiplier = 1.0 + (MAX_SPEED_MULTIPLIER - 1.0) * t * t;

            // Rotate visual direction to world space using pre-rotation camera frame
            let world_dir = rotate_hex(&visual_dir, -(camera_idx as i32));

            // Convert hex direction to world-space Vec3 via Map
            let origin = qrz::Qrz { q: 0, r: 0, z: 0 };
            let origin_world: Vec3 = map.convert(origin);
            let neighbor_world: Vec3 = map.convert(world_dir);
            let mut direction = (neighbor_world - origin_world).normalize();
            direction.y = 0.0; // Keep movement horizontal

            let speed = flyover.speed_multiplier;
            flyover.world_position += direction * BASE_SPEED * speed * dt;
        } else {
            // Rotation-only input: reset speed ramp
            flyover.hold_time = 0.0;
            flyover.speed_multiplier = 1.0;
        }
    } else {
        flyover.hold_time = 0.0;
        flyover.speed_multiplier = 1.0;
    }

    // Surface following: lerp Y toward terrain height
    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let terrain_y = admin_terrain.0.get_height(qrz.q, qrz.r) as f32 * map.rise();
    flyover.world_position.y += (terrain_y - flyover.world_position.y) * SURFACE_FOLLOW_SPEED * dt;
}

/// Replaces camera::update when flyover is active. Same orbital math with extended zoom.
/// Camera rotation is driven by flyover_movement (movement-integrated rotation).
pub fn flyover_camera_update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera3d>>,
    map: Res<Map>,
    time: Res<Time>,
    flyover: Res<FlyoverState>,
) {
    // Smooth interpolation toward target (same logic as camera::update)
    let target = orbit.target_angle();
    let diff = {
        let d = (target - orbit.current).rem_euclid(2.0 * PI);
        if d > PI { d - 2.0 * PI } else { d }
    };
    const INTERPOLATION_SPEED: f32 = 12.0;
    const SNAP_THRESHOLD: f32 = 0.005;
    if diff.abs() > SNAP_THRESHOLD {
        orbit.current += diff * (1.0 - (-INTERPOLATION_SPEED * time.delta_secs()).exp());
        orbit.current = orbit.current.rem_euclid(2.0 * PI);
    } else {
        orbit.current = target;
    }

    if let Ok((c_projection, mut c_transform)) = camera.single_mut() {
        // Zoom controls (extended range for flyover)
        match c_projection.into_inner() {
            Projection::Perspective(c_perspective) => {
                const MIN: f32 = 6_f32.to_radians();
                const MAX: f32 = 90_f32.to_radians();
                if keyboard.any_pressed([KeyCode::Minus]) {
                    c_perspective.fov = (c_perspective.fov * 1.01).clamp(MIN, MAX);
                }
                if keyboard.any_pressed([KeyCode::Equal]) {
                    c_perspective.fov = (c_perspective.fov / 1.01).clamp(MIN, MAX);
                }
            }
            Projection::Orthographic(c_orthographic) => {
                if keyboard.any_pressed([KeyCode::Minus]) {
                    c_orthographic.scale = (c_orthographic.scale * 1.01).clamp(NORMAL_ZOOM_MIN, ADMIN_ZOOM_MAX);
                }
                if keyboard.any_pressed([KeyCode::Equal]) {
                    c_orthographic.scale = (c_orthographic.scale / 1.01).clamp(NORMAL_ZOOM_MIN, ADMIN_ZOOM_MAX);
                }
            }
            _ => {}
        }

        // Position camera using same orbital math as camera::update
        let offset = Vec3::new(
            orbit.current.sin() * CAMERA_DISTANCE,
            CAMERA_HEIGHT,
            orbit.current.cos() * CAMERA_DISTANCE,
        );

        c_transform.translation = flyover.world_position + offset;
        c_transform.look_at(flyover.world_position + Vec3::Y * map.radius(), Vec3::Y);
    }
}

/// Generates terrain chunks around the flyover camera position.
/// Inner ring: full tiles (prevents mesh cascade via SkipNeighborRegen).
/// Outer ring: summaries stored in ChunkSummaries resource.
/// Radius scales with camera zoom so zoomed-out views stay filled.
pub fn flyover_generate_chunks(
    mut flyover: ResMut<FlyoverState>,
    map: Res<Map>,
    admin_terrain: Res<AdminTerrain>,
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    camera_query: Query<&Projection, With<Camera3d>>,
) {
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = admin_terrain.0.get_height(qrz.q, qrz.r);
    let center = loc_to_chunk(qrz);
    let fov = DEFAULT_FOV / scale.max(0.1);

    // Always extend beyond detail boundary so there's an outer LoD ring
    let vis_radius = flyover_radius(scale, player_z, center, &admin_terrain.0);
    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let max_radius = vis_radius.max(detail_radius + 3);
    // Use FOV as unconditional base so per-chunk elevation checks in
    // calculate_visible_chunks_adaptive actually run for distant chunks.
    // Without this, base==max makes every chunk unconditionally included.
    let base_radius = FOV_CHUNK_RADIUS;

    let (inner, outer) = calculate_visible_chunks_adaptive(
        center, player_z, base_radius, max_radius, detail_radius, fov,
        |q, r| admin_terrain.0.get_height(q, r),
    );

    // Inner ring: dispatch async tile generation (height computation off main thread)
    let inner_candidates: Vec<ChunkId> = inner.into_iter()
        .filter(|id| !flyover.admin_chunks.contains(id) && !loaded_chunks.chunks.contains(id))
        .collect();

    let pool = AsyncComputeTaskPool::get();

    for chunk_id in inner_candidates {
        // Track immediately to prevent re-generation on next tick.
        // Leave summary DATA in ChunkSummaries so summary mesh persists until
        // resolve_lod_overlap despawns it (once the full-detail mesh exists).
        flyover.admin_summary_chunks.remove(&chunk_id);
        flyover.admin_chunks.insert(chunk_id);
        skip_regen.chunks.insert(chunk_id);

        let terrain = admin_terrain.0.clone();
        let task = pool.spawn(async move {
            let mut tiles = Vec::with_capacity(CHUNK_TILES);
            for (q, r) in chunk_tiles(chunk_id) {
                let z = terrain.get_height(q, r);
                let qrz = qrz::Qrz { q, r, z };
                let decorator = Decorator { index: 3, is_solid: true };
                tiles.push((qrz, EntityType::Decorator(decorator)));
            }
            tiles
        });
        pending_tiles.inner.insert(chunk_id, task);
    }

    // Outer ring: dispatch async summary generation (no tiles in Map)
    let outer_candidates: Vec<ChunkId> = outer.into_iter()
        .filter(|id| {
            !flyover.admin_chunks.contains(id)
            && !flyover.admin_summary_chunks.contains(id)
            && !loaded_chunks.chunks.contains(id)
        })
        .collect();

    for chunk_id in outer_candidates {
        flyover.admin_summary_chunks.insert(chunk_id);

        let terrain = admin_terrain.0.clone();
        let task = pool.spawn(async move {
            let ct = chunk_id.center();
            let elevation = terrain.get_height(ct.q, ct.r);
            common_bevy::chunk::ChunkSummary {
                chunk_id,
                elevation,
                biome: EntityType::Decorator(Decorator { index: 3, is_solid: true }),
            }
        });
        pending_tiles.outer.insert(chunk_id, task);
    }
}

/// Poll pending flyover tile/summary tasks and queue events when ready.
pub fn poll_flyover_tile_tasks(
    mut pending: ResMut<PendingFlyoverTiles>,
    map_state: Res<MapState>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
) {
    // Poll inner tile generation tasks
    pending.inner.retain(|_chunk_id, task| {
        if let Some(tiles) = block_on(future::poll_once(task)) {
            for (qrz, entity_type) in tiles {
                map_state.queue_event(TileEvent::Spawn(qrz, entity_type));
            }
            false
        } else {
            true
        }
    });

    // Poll outer summary generation tasks
    pending.outer.retain(|&chunk_id, task| {
        if let Some(summary) = block_on(future::poll_once(task)) {
            chunk_summaries.summaries.insert(chunk_id, summary);
            false
        } else {
            true
        }
    });
}

/// Tags newly spawned chunk meshes that belong to admin-generated chunks.
/// Excludes SummaryChunk entities — those are managed by spawn_summary_meshes.
pub fn tag_admin_chunks(
    mut commands: Commands,
    flyover: Res<FlyoverState>,
    new_chunks: Query<(Entity, &ChunkMesh), (Added<ChunkMesh>, Without<crate::components::SummaryChunk>)>,
) {
    for (entity, chunk_mesh) in new_chunks.iter() {
        if flyover.admin_chunks.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).insert(AdminChunk);
        }
    }
}

/// Evicts admin chunks (full-detail and summary) that are far from the camera position.
pub fn flyover_evict_chunks(
    mut flyover: ResMut<FlyoverState>,
    map: Res<Map>,
    map_state: Res<MapState>,
    admin_terrain: Res<AdminTerrain>,
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    mut pending_meshes: ResMut<PendingChunkMeshes>,
    mut pending_summary_meshes: ResMut<PendingSummaryMeshes>,
    camera_query: Query<&Projection, With<Camera3d>>,
) {
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = admin_terrain.0.get_height(qrz.q, qrz.r);
    let center = loc_to_chunk(qrz);
    let radius = flyover_radius(scale, player_z, center, &admin_terrain.0);
    let fov = DEFAULT_FOV / scale.max(0.1);

    // Full-detail keep: detail_boundary + 1 (matches generation inner ring)
    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let detail_buffer = detail_radius as i32 + 1;

    // Summary keep: per-chunk adaptive visibility.
    // detail_buffer is the unconditional inner region; beyond that,
    // each chunk must pass a per-chunk elevation-based visibility test.
    let summary_keep: HashSet<ChunkId> = {
        let r = (radius as i32) + 1;
        let base = detail_radius as i32 + 1;
        let mut kept = HashSet::new();
        for dq in -r..=r {
            let dr_min = (-r).max(-dq - r);
            let dr_max = r.min(-dq + r);
            for dr in dr_min..=dr_max {
                let hex_dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
                let chunk_id = ChunkId(center.0 + dq, center.1 + dr);
                if hex_dist <= base {
                    kept.insert(chunk_id);
                    continue;
                }
                let ct = chunk_id.center();
                let cz = admin_terrain.0.get_height(ct.q, ct.r);
                let vis = visibility_radius(player_z, cz, fov) as i32 + 1;
                if hex_dist <= vis {
                    kept.insert(chunk_id);
                }
            }
        }
        kept
    };

    // Evict full-detail admin chunk data beyond detail boundary + 1.
    // Mesh entities are cleaned up by resolve_lod_overlap once a summary
    // mesh exists for the same chunk.
    let evictable: Vec<ChunkId> = flyover.admin_chunks
        .iter()
        .filter(|id| {
            chunk_hex_distance(**id, center) > detail_buffer
        })
        .copied()
        .collect();

    // Track summaries promoted from full-detail eviction this frame.
    // These must survive the summary eviction pass so resolve_lod_overlap
    // has time to despawn the full-detail mesh entity.
    let mut newly_promoted: HashSet<ChunkId> = HashSet::new();

    if !evictable.is_empty() {
        // Generate summaries from tile data before tiles are removed.
        for &chunk_id in &evictable {
            if !chunk_summaries.summaries.contains_key(&chunk_id) {
                let center_tile = chunk_id.center();
                if let Some((tile_qrz, biome)) = map.get_by_qr(center_tile.q, center_tile.r) {
                    chunk_summaries.summaries.insert(chunk_id, common_bevy::chunk::ChunkSummary {
                        chunk_id,
                        elevation: tile_qrz.z,
                        biome,
                    });
                    flyover.admin_summary_chunks.insert(chunk_id);
                    newly_promoted.insert(chunk_id);
                }
            }
        }

        let evict_set: HashSet<ChunkId> = evictable.iter().copied().collect();

        for &chunk_id in &evictable {
            // Cancel pending tile and mesh generation tasks if still running
            pending_tiles.inner.remove(&chunk_id);
            pending_meshes.tasks.remove(&chunk_id);

            if loaded_chunks.chunks.contains(&chunk_id) {
                continue;
            }
            // Use O(1) map lookup instead of terrain evaluation for despawn
            for (q, r) in chunk_tiles(chunk_id) {
                if let Some((qrz, _)) = map.get_by_qr(q, r) {
                    map_state.queue_event(TileEvent::Despawn(qrz));
                }
            }
        }

        flyover.admin_chunks.retain(|id| !evict_set.contains(id));
        skip_regen.chunks.retain(|id| !evict_set.contains(id));
    }

    // Evict summary admin chunks (cancel pending tasks too).
    // Protect newly_promoted summaries — they need one frame for
    // spawn_summary_meshes → resolve_lod_overlap to clean up the
    // corresponding full-detail mesh entity.
    let summary_evictable: Vec<ChunkId> = flyover.admin_summary_chunks
        .iter()
        .filter(|id| !summary_keep.contains(id) && !newly_promoted.contains(id))
        .copied()
        .collect();

    for &chunk_id in &summary_evictable {
        pending_tiles.outer.remove(&chunk_id);
        pending_summary_meshes.tasks.remove(&chunk_id);
        chunk_summaries.summaries.remove(&chunk_id);
    }
    flyover.admin_summary_chunks.retain(|id| summary_keep.contains(id));
    // Summary mesh entities cleaned up by spawn_summary_meshes (change detection)
}
