use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};

use common_bevy::{
    chunk::{
        ChunkId, CHUNK_SIZE, FOV_CHUNK_RADIUS,
        calculate_visible_chunks_adaptive,
        chunk_to_tile, loc_to_chunk, visibility_radius,
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
    resources::{ChunkSummaries, LoadedChunks, SkipNeighborRegen},
    systems::camera::{CameraOrbitAngle, CAMERA_DISTANCE, CAMERA_HEIGHT},
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

/// Absolute cap on flyover generation radius (practically unlimited)
const MAX_FLYOVER_RADIUS: u8 = 255;

/// Compute a generation radius from camera zoom AND player elevation combined.
///
/// Delegates to the shared `visibility_radius` (ground at sea level, flyover viewport)
/// and caps at `MAX_FLYOVER_RADIUS`.
fn flyover_radius(camera_scale: f32, player_z: i32) -> u8 {
    // FixedVertical { viewport_height: 40 } → half-height = 20 * scale
    let half_viewport = 20.0 * camera_scale;
    visibility_radius(player_z, 0, half_viewport).min(MAX_FLYOVER_RADIUS)
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
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut chunk_summaries: ResMut<ChunkSummaries>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
) {
    for action in reader.read() {
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
                for oq in 0..CHUNK_SIZE as u8 {
                    for or_ in 0..CHUNK_SIZE as u8 {
                        let tile = chunk_to_tile(chunk_id, oq, or_);
                        if let Some((qrz, _)) = map.get_by_qr(tile.q, tile.r) {
                            map_state.queue_event(TileEvent::Despawn(qrz));
                        }
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

/// Smooth camera movement using arrow keys with speed ramp.
pub fn flyover_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    orbit_angle: Res<CameraOrbitAngle>,
    time: Res<Time>,
    map: Res<Map>,
    admin_terrain: Res<AdminTerrain>,
    mut flyover: ResMut<FlyoverState>,
) {
    let dt = time.delta_secs();

    // Skip movement when Shift is held (camera orbit mode)
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let moving = !shift_pressed && keyboard.any_pressed([
        KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,
    ]);

    if moving {
        // Speed ramp: quadratic ease-in over RAMP_SECONDS
        flyover.hold_time += dt;
        let t = (flyover.hold_time / RAMP_SECONDS).min(1.0);
        flyover.speed_multiplier = 1.0 + (MAX_SPEED_MULTIPLIER - 1.0) * t * t;

        // Build direction vector from arrow keys
        let mut dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::ArrowUp)    { dir.z -= 1.0; }
        if keyboard.pressed(KeyCode::ArrowDown)  { dir.z += 1.0; }
        if keyboard.pressed(KeyCode::ArrowLeft)  { dir.x -= 1.0; }
        if keyboard.pressed(KeyCode::ArrowRight) { dir.x += 1.0; }

        if dir.length_squared() > 0.0 {
            dir = dir.normalize();

            // Rotate direction by camera orbit angle (same visual mapping as player)
            let angle = orbit_angle.0;
            let rotated = Vec3::new(
                dir.x * angle.cos() + dir.z * angle.sin(),
                0.0,
                -dir.x * angle.sin() + dir.z * angle.cos(),
            );

            let speed = flyover.speed_multiplier;
            flyover.world_position += rotated * BASE_SPEED * speed * dt;
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
pub fn flyover_camera_update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit_angle: ResMut<CameraOrbitAngle>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera3d>>,
    map: Res<Map>,
    time: Res<Time>,
    flyover: Res<FlyoverState>,
) {
    // Camera orbit controls (Shift + Left/Right)
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if shift_pressed {
        const ORBIT_SPEED: f32 = 2.0;
        if keyboard.pressed(KeyCode::ArrowLeft) {
            orbit_angle.0 += ORBIT_SPEED * time.delta_secs();
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            orbit_angle.0 -= ORBIT_SPEED * time.delta_secs();
        }
        orbit_angle.0 = orbit_angle.0.rem_euclid(2.0 * PI);
    }

    if let Ok((c_projection, mut c_transform)) = camera.single_mut() {
        // Zoom controls (extended range for flyover)
        match c_projection.into_inner() {
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
            orbit_angle.0.sin() * CAMERA_DISTANCE,
            CAMERA_HEIGHT,
            orbit_angle.0.cos() * CAMERA_DISTANCE,
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
    let half_viewport = 20.0 * scale;

    // Always extend beyond FOV_CHUNK_RADIUS so there's an outer LoD ring
    let vis_radius = flyover_radius(scale, player_z);
    let max_radius = vis_radius.max(common_bevy::chunk::FOV_CHUNK_RADIUS + 3);
    let base_radius = vis_radius;

    let (inner, outer) = calculate_visible_chunks_adaptive(
        center, player_z, base_radius, max_radius, half_viewport,
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
            let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
            for oq in 0..CHUNK_SIZE as u8 {
                for or_ in 0..CHUNK_SIZE as u8 {
                    let tile = chunk_to_tile(chunk_id, oq, or_);
                    let z = terrain.get_height(tile.q, tile.r);
                    let qrz = qrz::Qrz { q: tile.q, r: tile.r, z };
                    let decorator = Decorator { index: 3, is_solid: true };
                    tiles.push((qrz, EntityType::Decorator(decorator)));
                }
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
    camera_query: Query<&Projection, With<Camera3d>>,
) {
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = admin_terrain.0.get_height(qrz.q, qrz.r);
    let radius = flyover_radius(scale, player_z);
    let center = loc_to_chunk(qrz);
    let half_viewport = 20.0 * scale;

    // Full-detail keep: FOV_CHUNK_RADIUS + 1 (matches generation inner ring)
    let fov_buffer = FOV_CHUNK_RADIUS as i32 + 1;

    // Summary keep: wider adaptive visibility + 1 buffer per chunk
    let summary_keep: HashSet<ChunkId> = {
        let r = (radius as i32) + 1;
        let base_plus_buffer = radius as i32 + 1;
        let mut kept = HashSet::new();
        for dq in -r..=r {
            for dr in -r..=r {
                let chebyshev = dq.abs().max(dr.abs());
                let chunk_id = ChunkId(center.0 + dq, center.1 + dr);
                if chebyshev <= base_plus_buffer {
                    kept.insert(chunk_id);
                    continue;
                }
                let ct = chunk_id.center();
                let cz = admin_terrain.0.get_height(ct.q, ct.r);
                let vis = visibility_radius(player_z, cz, half_viewport) as i32 + 1;
                if chebyshev <= vis {
                    kept.insert(chunk_id);
                }
            }
        }
        kept
    };

    // Evict full-detail admin chunk data beyond FOV + 1.
    // Mesh entities are cleaned up by resolve_lod_overlap once a summary
    // mesh exists for the same chunk.
    let evictable: Vec<ChunkId> = flyover.admin_chunks
        .iter()
        .filter(|id| {
            let chebyshev = (id.0 - center.0).abs().max((id.1 - center.1).abs());
            chebyshev > fov_buffer
        })
        .copied()
        .collect();

    if !evictable.is_empty() {
        // Generate summaries from tile data before tiles are removed.
        for &chunk_id in &evictable {
            if !chunk_summaries.summaries.contains_key(&chunk_id) {
                let center_tile = chunk_to_tile(chunk_id, 8, 8);
                if let Some((tile_qrz, biome)) = map.get_by_qr(center_tile.q, center_tile.r) {
                    chunk_summaries.summaries.insert(chunk_id, common_bevy::chunk::ChunkSummary {
                        chunk_id,
                        elevation: tile_qrz.z,
                        biome,
                    });
                    flyover.admin_summary_chunks.insert(chunk_id);
                }
            }
        }

        let evict_set: HashSet<ChunkId> = evictable.iter().copied().collect();

        for &chunk_id in &evictable {
            // Cancel pending tile generation task if still running
            pending_tiles.inner.remove(&chunk_id);

            if loaded_chunks.chunks.contains(&chunk_id) {
                continue;
            }
            // Use O(1) map lookup instead of terrain evaluation for despawn
            for oq in 0..CHUNK_SIZE as u8 {
                for or_ in 0..CHUNK_SIZE as u8 {
                    let tile = chunk_to_tile(chunk_id, oq, or_);
                    if let Some((qrz, _)) = map.get_by_qr(tile.q, tile.r) {
                        map_state.queue_event(TileEvent::Despawn(qrz));
                    }
                }
            }
        }

        flyover.admin_chunks.retain(|id| !evict_set.contains(id));
        skip_regen.chunks.retain(|id| !evict_set.contains(id));
    }

    // Evict summary admin chunks (cancel pending tasks too)
    let summary_evictable: Vec<ChunkId> = flyover.admin_summary_chunks
        .iter()
        .filter(|id| !summary_keep.contains(id))
        .copied()
        .collect();

    for &chunk_id in &summary_evictable {
        pending_tiles.outer.remove(&chunk_id);
        chunk_summaries.summaries.remove(&chunk_id);
    }
    flyover.admin_summary_chunks.retain(|id| summary_keep.contains(id));
    // Summary mesh entities cleaned up by spawn_summary_meshes (change detection)
}
