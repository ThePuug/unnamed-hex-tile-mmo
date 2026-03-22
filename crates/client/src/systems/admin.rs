use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};

use common_bevy::{
    chunk::{
        ChunkId, CHUNK_TILES, DEFAULT_FOV,
        chunk_hex_distance, chunk_tiles, loc_to_chunk,
    },
    components::{
        behaviour::PlayerControlled,
        entity_type::{EntityType, decorator::Decorator},
        Actor,
    },
    resources::map::Map,
};
use qrz::Convert;

use crate::{
    components::ChunkMesh,
    plugins::console::DevConsoleAction,
    resources::{LoadedChunks, SkipNeighborRegen},
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

/// Client-side Composite for local chunk generation (flyover).
/// Same event stack as the server — deterministic from seed.
/// Arc-wrapped so async tile generation tasks can share it cheaply.
#[derive(Resource)]
pub struct AdminComposite(pub Arc<world::events::Composite>);

impl Default for AdminComposite {
    fn default() -> Self {
        let seed = 0x9E3779B97F4A7C15; // Same seed as server (Terrain::default)
        let plate_cache = std::sync::Mutex::new(world::PlateCache::new(seed));
        let plate_cache = std::sync::Arc::new(plate_cache);
        let mut composite = world::events::Composite::new(seed);
        composite.add_event(Box::new(world::events::plates::PlateEvent::with_cache(plate_cache.clone())));
        composite.add_event(Box::new(world::events::spines::SpineEvent::with_cache(plate_cache, seed)));
        composite.add_event(Box::new(world::events::spawner::SpawnerEvent::new(seed)));
        Self(Arc::new(composite))
    }
}

/// Tracks pending async flyover tile/summary generation tasks.
#[derive(Default, Resource)]
pub struct PendingFlyoverTiles {
    pub inner: HashMap<ChunkId, Task<Vec<(qrz::Qrz, EntityType)>>>,
    /// Outer ring returns (DecimatedMesh, chunk_origin) for entity spawning.
    pub outer: HashMap<ChunkId, Task<(common_bevy::qem::DecimatedMesh, Vec3)>>,
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
    admin_composite: Res<AdminComposite>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::GotoWorldUnits(wx, wy) => {
                // Terrain world units use a different coordinate system than Bevy Vec3.
                // world::hex_to_world: wx = q + r*0.5, wy = r * sqrt(3)/2
                // Invert to get hex q,r, then convert to Vec3 via Map.
                let sqrt_3 = 1.7320508075688772_f64;
                let rf = wy * 2.0 / sqrt_3;
                let qf = wx - rf * 0.5;
                let q = qf.round() as i32;
                let r = rf.round() as i32;
                let z = admin_composite.0.elevation_at(q, r);
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
                let z = admin_composite.0.elevation_at(*q, *r);
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
            {
                let mut map_w = map.write();
                for &chunk_id in &flyover.admin_chunks {
                    if loaded_chunks.chunks.contains(&chunk_id) {
                        continue;
                    }
                    for (q, r) in chunk_tiles(chunk_id) {
                        if let Some((qrz, _)) = map_w.get_by_qr(q, r) {
                            map_w.remove(qrz);
                        }
                    }
                }
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

    // Surface following: lerp Y toward terrain height from Map (no Composite call on main thread).
    // If tile isn't loaded yet, hold current Y — camera lerps to correct height when chunk arrives.
    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    if let Some((loaded, _)) = map.get_by_qr(qrz.q, qrz.r) {
        let terrain_y = loaded.z as f32 * map.rise();
        flyover.world_position.y += (terrain_y - flyover.world_position.y) * SURFACE_FOLLOW_SPEED * dt;
    }
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
    admin_composite: Res<AdminComposite>,
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    camera_query: Query<&Projection, With<Camera3d>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("fly_gen");
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    // Use the camera's Bevy Y position to estimate player_z instead of
    // calling elevation_at on the main thread (which triggers cold deform cascades).
    let player_z = (flyover.world_position.y / map.rise()) as i32;
    let center = loc_to_chunk(qrz);
    let fov = DEFAULT_FOV / scale.max(0.1);

    // Simple fixed radius — avoid per-chunk elevation queries on the main thread.
    // The async tasks compute actual elevation off-thread.
    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let gen_radius = (detail_radius + 3).min(MAX_FLYOVER_RADIUS);

    let inner = common_bevy::chunk::calculate_visible_chunks(center, gen_radius);
    // Outer ring: 1 chunk beyond inner
    let outer_radius = (gen_radius + 1).min(MAX_FLYOVER_RADIUS);
    let all_outer = common_bevy::chunk::calculate_visible_chunks(center, outer_radius);
    let inner_set: HashSet<ChunkId> = inner.iter().copied().collect();
    let outer: Vec<ChunkId> = all_outer.into_iter().filter(|id| !inner_set.contains(id)).collect();

    // Inner ring: dispatch async tile generation (height computation off main thread)
    let inner_candidates: Vec<ChunkId> = inner.into_iter()
        .filter(|id| !flyover.admin_chunks.contains(id) && !loaded_chunks.chunks.contains(id))
        .filter(|id| !pending_tiles.inner.contains_key(id))
        .collect();

    let pool = AsyncComputeTaskPool::get();

    for chunk_id in inner_candidates {
        // Track immediately to prevent re-generation on next tick.
        // Leave summary DATA in ChunkSummaries so summary mesh persists until
        // resolve_lod_overlap despawns it (once the full-detail mesh exists).
        flyover.admin_summary_chunks.remove(&chunk_id);
        flyover.admin_chunks.insert(chunk_id);
        skip_regen.chunks.insert(chunk_id);

        let composite = admin_composite.0.clone();
        let task = pool.spawn(async move {
            let mut tiles = Vec::with_capacity(CHUNK_TILES);
            for (q, r) in chunk_tiles(chunk_id) {
                let z = composite.elevation_at(q, r);
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

        let composite = admin_composite.0.clone();
        let task = pool.spawn(async move {
            use common_bevy::chunk::chunk_tiles;
            let chunk_tiles_vec: Vec<qrz::Qrz> = chunk_tiles(chunk_id)
                .map(|(q, r)| {
                    let z = composite.elevation_at(q, r);
                    qrz::Qrz { q, r, z }
                })
                .collect();
            let mut elevations = std::collections::HashMap::new();
            for &qrz in &chunk_tiles_vec {
                elevations.insert((qrz.q, qrz.r), qrz.z);
                for direction in qrz::DIRECTIONS.iter() {
                    let n = qrz + *direction;
                    elevations.entry((n.q, n.r))
                        .or_insert_with(|| composite.elevation_at(n.q, n.r));
                }
            }
            let map: qrz::Map<()> = qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop);
            let chunk_origin: bevy::math::Vec3 = map.convert(chunk_id.center());
            let geometry = common_bevy::geometry::compute_tile_geometry(
                &chunk_tiles_vec, &elevations, 1.0, 0.8, chunk_origin,
            );
            let mesh = common_bevy::qem::decimate_geometry(&geometry, 2.0);
            (mesh, chunk_origin)
        });
        pending_tiles.outer.insert(chunk_id, task);
    }
}

/// Poll pending flyover tile/summary tasks and queue events when ready.
pub fn poll_flyover_tile_tasks(
    mut commands: Commands,
    mut pending: ResMut<PendingFlyoverTiles>,
    map: Res<Map>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Option<Res<crate::resources::TerrainMaterial>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("fly_poll");
    // Poll inner tile generation tasks
    pending.inner.retain(|&chunk_id, task| {
        if let Some(tiles) = block_on(future::poll_once(task)) {
            let mut map_w = map.write();
            for (qrz, entity_type) in tiles {
                map_w.insert(qrz, entity_type);
            }
            drop(map_w);
            loaded_chunks.insert(chunk_id);
            false
        } else {
            true
        }
    });

    // Poll outer summary generation tasks → spawn mesh entities
    let Some(terrain_material) = terrain_material else { return; };
    pending.outer.retain(|&chunk_id, task| {
        if let Some((summary, chunk_origin)) = block_on(future::poll_once(task)) {
            if !summary.positions.is_empty() {
                let mesh = meshes.add(crate::systems::world::build_bevy_mesh(&summary));
                commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(terrain_material.handle.clone()),
                    Transform::from_translation(chunk_origin),
                    ChunkMesh { chunk_id },
                    AdminChunk,
                ));
            }
            false
        } else {
            true
        }
    });
}

/// Tags newly spawned chunk meshes that belong to admin-generated chunks.
pub fn tag_admin_chunks(
    mut commands: Commands,
    flyover: Res<FlyoverState>,
    new_chunks: Query<(Entity, &ChunkMesh), Added<ChunkMesh>>,
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
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    camera_query: Query<&Projection, With<Camera3d>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("fly_evic");
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = (flyover.world_position.y / map.rise()) as i32;
    let center = loc_to_chunk(qrz);
    let fov = DEFAULT_FOV / scale.max(0.1);

    // Simple radius-based eviction — matches generation radius + buffer
    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let detail_buffer = detail_radius as i32 + 1;
    let evict_radius = (detail_radius + 5).min(MAX_FLYOVER_RADIUS) as i32;

    let summary_keep: HashSet<ChunkId> = common_bevy::chunk::calculate_visible_chunks(center, evict_radius as u8)
        .into_iter().collect();

    // Evict full-detail admin chunk data beyond detail boundary + 1.
    let evictable: Vec<ChunkId> = flyover.admin_chunks
        .iter()
        .filter(|id| {
            chunk_hex_distance(**id, center) > detail_buffer
        })
        .copied()
        .collect();

    if !evictable.is_empty() {
        let evict_set: HashSet<ChunkId> = evictable.iter().copied().collect();

        {
            let mut map_w = map.write();
            for &chunk_id in &evictable {
                pending_tiles.inner.remove(&chunk_id);
                if loaded_chunks.chunks.contains(&chunk_id) {
                    continue;
                }
                for (q, r) in chunk_tiles(chunk_id) {
                    if let Some((qrz, _)) = map_w.get_by_qr(q, r) {
                        map_w.remove(qrz);
                    }
                }
            }
        }

        flyover.admin_chunks.retain(|id| !evict_set.contains(id));
        skip_regen.chunks.retain(|id| !evict_set.contains(id));
    }

    // Evict summary admin chunks (cancel pending tasks too).
    let summary_evictable: Vec<ChunkId> = flyover.admin_summary_chunks
        .iter()
        .filter(|id| !summary_keep.contains(id))
        .copied()
        .collect();

    for &chunk_id in &summary_evictable {
        pending_tiles.outer.remove(&chunk_id);
    }
    flyover.admin_summary_chunks.retain(|id| summary_keep.contains(id));
}
