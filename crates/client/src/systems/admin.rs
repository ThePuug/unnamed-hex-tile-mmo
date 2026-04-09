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
    plugins::console::DevConsoleAction,
    resources::{ForcedSummaryRadius, LoadedChunks, SkipNeighborRegen},
    systems::camera::{CameraOrbit, CAMERA_DISTANCE, MAX_FLYOVER_FOV, camera_height},
};

// ──── Resources ────

/// Flyover camera state.
///
/// The flyover overrides two things in the normal pipeline:
/// 1. Position — threshold calculations use flyover position instead of player position
/// 2. Tile source — tiles come from a local composite instead of the server
///
/// `generated_chunks` tracks which chunks the flyover generated tiles for,
/// so they can be cleaned up when flyover toggles off or threshold changes.
#[derive(Resource)]
pub struct FlyoverState {
    pub active: bool,
    pub world_position: Vec3,
    pub speed_multiplier: f32,
    pub hold_time: f32,
    /// Chunks whose tiles were generated locally (not server-sourced).
    pub generated_chunks: HashSet<ChunkId>,
    /// Stashed normal-play state, restored on flyover toggle-off.
    pub stashed_summary_meshes: Option<HashMap<common_bevy::summary_mesh::MeshRegionKey, crate::resources::SummaryMeshState>>,
    pub stashed_loaded: Option<HashSet<ChunkId>>,
}

impl Default for FlyoverState {
    fn default() -> Self {
        Self {
            active: false,
            world_position: Vec3::ZERO,
            speed_multiplier: 1.0,
            hold_time: 0.0,
            generated_chunks: HashSet::new(),
            stashed_summary_meshes: None,
            stashed_loaded: None,
        }
    }
}

/// Marker for chunk mesh entities spawned while flyover is active.
#[derive(Component)]
pub struct AdminChunk;

/// Marker for the flyover ground cursor.
#[derive(Component)]
pub struct FlyoverCursor;

/// Client-side Composite for local chunk generation (flyover).
/// Same event stack as the server — deterministic from seed.
#[derive(Resource)]
pub struct AdminComposite(pub Arc<world::events::Composite>);

impl Default for AdminComposite {
    fn default() -> Self {
        let seed = 0x9E3779B97F4A7C15;
        let plate_cache = std::sync::Mutex::new(world::PlateCache::new(seed));
        let plate_cache = std::sync::Arc::new(plate_cache);
        let mut composite = world::events::Composite::new(seed);
        composite.add_event(Box::new(world::events::plates::PlateEvent::with_cache(plate_cache.clone())));
        composite.add_event(Box::new(world::events::spines::SpineEvent::with_cache(plate_cache, seed)));
        composite.add_event(Box::new(world::events::spawner::SpawnerEvent::new(seed)));
        Self(Arc::new(composite))
    }
}

/// Tracks the visible summary set during flyover for addition/removal diffing.
/// Mirrors the server's VisibleSummaryCache pattern — additions flow through
/// `summary_cache.apply_batch()` so the downstream mesh pipeline sees them
/// identically to server-sent data.
#[derive(Resource, Default)]
pub struct FlyoverSummaryTracker {
    tracked: HashSet<common_bevy::message::SummaryKey>,
    tasks: Vec<(Vec<common_bevy::message::SummaryKey>, Task<Vec<common_bevy::message::SummaryData>>)>,
    in_flight: HashSet<common_bevy::message::SummaryKey>,
    last_pos: Vec3,
}

/// Pending async tile generation tasks.
#[derive(Default, Resource)]
pub struct PendingFlyoverTiles {
    pub tasks: HashMap<ChunkId, Task<Vec<(qrz::Qrz, EntityType)>>>,
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
const SURFACE_FOLLOW_SPEED: f32 = 5.0;

const MIN_UPDATE_DISTANCE: f32 = 20.0;


// ──── Systems ────

/// Reads DevConsoleAction events and toggles flyover on/off.
pub fn execute_admin_actions(
    mut flyover: ResMut<FlyoverState>,
    mut reader: MessageReader<DevConsoleAction>,
    player_query: Query<&Transform, (With<Actor>, With<PlayerControlled>, Without<Camera3d>)>,
    mut commands: Commands,
    mut loaded_chunks: ResMut<LoadedChunks>,
    map: Res<Map>,
    admin_composite: Res<AdminComposite>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    mut summary_meshes: ResMut<crate::resources::SummaryMeshes>,
    mut forced_radius: ResMut<ForcedSummaryRadius>,
    summary_cache: Res<crate::resources::SummaryCache>,
    cursor_query: Query<Entity, With<FlyoverCursor>>,
    mut flyover_tracker: ResMut<FlyoverSummaryTracker>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for action in reader.read() {
        match action {
            DevConsoleAction::GotoWorldUnits(wx, wy) => {
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
            DevConsoleAction::SetForcedSummaryRadius(new_radius) => {
                if forced_radius.0 != *new_radius {
                    let label = new_radius.map_or("Auto".to_string(), |r| format!("r={r}"));
                    info!("Summary radius: {label}");
                    forced_radius.0 = *new_radius;
                    // Clear all mesh entities so the new band configuration rebuilds
                    for (_, state) in summary_meshes.states.drain() {
                        if let Some(entity) = state.entity {
                            commands.entity(entity).despawn();
                        }
                    }
                    map.force_changed();
                }
                continue;
            }
            DevConsoleAction::ReportTerrain => {
                if flyover.active {
                    report_terrain_at_cursor(&flyover, &map);
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

            // Clear remote summaries (server-sent data won't match flyover terrain)
            summary_cache.clear();

            // Stash existing mesh state so geometry can be restored on toggle-off.
            // Despawn the entities rather than hiding them — on toggle-off we rebuild
            // from stored base_positions via poll_summary_meshes Phase 0, avoiding
            // command-ordering races with dispatch_summary_tasks.
            let mut stashed: HashMap<common_bevy::summary_mesh::MeshRegionKey, crate::resources::SummaryMeshState> =
                summary_meshes.states.drain().collect();
            for state in stashed.values_mut() {
                if let Some(entity) = state.entity.take() {
                    commands.entity(entity).despawn();
                }
                state.mesh_handle = None;
            }
            flyover.stashed_summary_meshes = Some(stashed);
            flyover.stashed_loaded = Some(loaded_chunks.chunks.drain().collect());

            // Spawn ground cursor
            let cursor_mesh = meshes.add(Sphere::new(0.25));
            let cursor_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.0, 0.0, 0.5),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            });
            commands.spawn((
                Mesh3d(cursor_mesh),
                MeshMaterial3d(cursor_mat),
                Transform::from_translation(flyover.world_position),
                bevy_light::NotShadowCaster,
                FlyoverCursor,
            ));

            flyover.active = true;
            flyover.hold_time = 0.0;
            flyover.speed_multiplier = 1.0;
            info!("Flyover camera: ON");
        } else {
            // Toggle OFF — despawn cursor
            for entity in cursor_query.iter() {
                commands.entity(entity).despawn();
            }

            // Clean up flyover-generated chunks
            evict_generated_chunks(
                &mut commands, &mut flyover, &mut pending_tiles,
                &mut loaded_chunks, &map, &mut skip_regen, &mut summary_meshes,
            );

            // Restore stashed normal-play state.
            // Entities were despawned on toggle-on; poll_summary_meshes Phase 0
            // will rebuild them from stored base_positions this same frame.
            if let Some(stashed) = flyover.stashed_summary_meshes.take() {
                for (key, state) in stashed {
                    summary_meshes.states.insert(key, state);
                }
            }
            if let Some(stashed) = flyover.stashed_loaded.take() {
                loaded_chunks.chunks.extend(stashed);
            }
            // Clear flyover summaries so they don't contaminate normal gameplay.
            // Server will resend its summaries on the next movement update.
            summary_cache.clear();

            flyover_tracker.tracked.clear();
            flyover_tracker.tasks.clear();
            flyover_tracker.in_flight.clear();

            // Signal dispatch_summary_tasks to re-evaluate visible regions.
            map.force_changed();

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
pub fn flyover_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    time: Res<Time>,
    map: Res<Map>,
    mut flyover: ResMut<FlyoverState>,
    mut cursor_query: Query<&mut Transform, With<FlyoverCursor>>,
) {
    let dt = time.delta_secs();

    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let has_arrows = !shift_pressed && keyboard.any_pressed([
        KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,
    ]);

    if has_arrows {
        let up = keyboard.pressed(KeyCode::ArrowUp);
        let down = keyboard.pressed(KeyCode::ArrowDown);
        let left = keyboard.pressed(KeyCode::ArrowLeft);
        let right = keyboard.pressed(KeyCode::ArrowRight);

        let camera_idx = orbit.target_index;

        let visual_dir = if up && !down {
            if left && !right {
                orbit.step_ccw();
                qrz::Qrz { q: -1, r: 0, z: 0 }
            } else if right && !left {
                orbit.step_cw();
                qrz::Qrz { q: 1, r: -1, z: 0 }
            } else {
                qrz::Qrz { q: 0, r: -1, z: 0 }
            }
        } else if down && !up {
            if left && !right {
                qrz::Qrz { q: -1, r: 1, z: 0 }
            } else if right && !left {
                qrz::Qrz { q: 1, r: 0, z: 0 }
            } else {
                qrz::Qrz { q: 0, r: 1, z: 0 }
            }
        } else if left && !right {
            orbit.step_ccw();
            qrz::Qrz { q: 0, r: 0, z: 0 }
        } else if right && !left {
            orbit.step_cw();
            qrz::Qrz { q: 0, r: 0, z: 0 }
        } else {
            qrz::Qrz { q: 0, r: 0, z: 0 }
        };

        if visual_dir.q != 0 || visual_dir.r != 0 {
            flyover.hold_time += dt;
            let t = (flyover.hold_time / RAMP_SECONDS).min(1.0);
            flyover.speed_multiplier = 1.0 + (MAX_SPEED_MULTIPLIER - 1.0) * t * t;

            let world_dir = rotate_hex(&visual_dir, -(camera_idx as i32));

            let origin = qrz::Qrz { q: 0, r: 0, z: 0 };
            let origin_world: Vec3 = map.convert(origin);
            let neighbor_world: Vec3 = map.convert(world_dir);
            let mut direction = (neighbor_world - origin_world).normalize();
            direction.y = 0.0;

            let speed = flyover.speed_multiplier;
            flyover.world_position += direction * BASE_SPEED * speed * dt;
        } else {
            flyover.hold_time = 0.0;
            flyover.speed_multiplier = 1.0;
        }
    } else {
        flyover.hold_time = 0.0;
        flyover.speed_multiplier = 1.0;
    }

    // Surface following
    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    if let Some((loaded, _)) = map.get_by_qr(qrz.q, qrz.r) {
        let terrain_y = loaded.z as f32 * map.rise() + map.rise();
        flyover.world_position.y += (terrain_y - flyover.world_position.y) * SURFACE_FOLLOW_SPEED * dt;
    }

    // Update ground cursor position
    if let Ok(mut cursor_tf) = cursor_query.single_mut() {
        cursor_tf.translation = flyover.world_position;
    }
}

/// Replaces camera::update when flyover is active.
pub fn flyover_camera_update(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<CameraOrbit>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera3d>>,
    map: Res<Map>,
    time: Res<Time>,
    flyover: Res<FlyoverState>,
) {
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
        if let Projection::Perspective(c_perspective) = c_projection.into_inner() {
            const MIN: f32 = 6_f32.to_radians();
            if keyboard.any_pressed([KeyCode::Minus]) {
                c_perspective.fov = (c_perspective.fov * 1.01).clamp(MIN, MAX_FLYOVER_FOV);
            }
            if keyboard.any_pressed([KeyCode::Equal]) {
                c_perspective.fov = (c_perspective.fov / 1.01).clamp(MIN, MAX_FLYOVER_FOV);
            }
        }

        let height = camera_height(MAX_FLYOVER_FOV);
        let offset = Vec3::new(
            orbit.current.sin() * CAMERA_DISTANCE,
            height,
            orbit.current.cos() * CAMERA_DISTANCE,
        );

        c_transform.translation = flyover.world_position + offset;
        c_transform.look_at(flyover.world_position + Vec3::Y * map.radius(), Vec3::Y);
    }
}

/// Generates tiles around the flyover camera position using the local composite.
/// Tiles are inserted into the Map — the normal mesh pipeline handles rendering.
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
    let fov = camera_query.single().ok().map_or(DEFAULT_FOV, |p| {
        if let Projection::Perspective(persp) = p { persp.fov } else { DEFAULT_FOV }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = (flyover.world_position.y / map.rise()) as i32;
    let center = loc_to_chunk(qrz);

    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let gen_radius = detail_radius + 3;

    let candidates: Vec<ChunkId> = common_bevy::chunk::calculate_visible_chunks(center, gen_radius)
        .into_iter()
        .filter(|id| !flyover.generated_chunks.contains(id) && !loaded_chunks.chunks.contains(id))
        .filter(|id| !pending_tiles.tasks.contains_key(id))
        .collect();

    let pool = AsyncComputeTaskPool::get();

    for chunk_id in candidates {
        flyover.generated_chunks.insert(chunk_id);
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
        pending_tiles.tasks.insert(chunk_id, task);
    }
}

/// Poll pending tile generation tasks — inserts tiles into Map.
/// Meshing is handled by the normal dispatch_lod_tasks pipeline.
pub fn poll_flyover_tile_tasks(
    mut pending: ResMut<PendingFlyoverTiles>,
    map: Res<Map>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("fly_poll");
    pending.tasks.retain(|&chunk_id, task| {
        if let Some(tiles) = block_on(future::poll_once(task)) {
            for (qrz, entity_type) in tiles {
                map.insert(qrz, entity_type);
            }
            loaded_chunks.insert(chunk_id);
            false
        } else {
            true
        }
    });
}

/// Tags newly spawned mesh entities that belong to admin-generated chunks.
pub fn tag_admin_chunks(
    mut commands: Commands,
    flyover: Res<FlyoverState>,
    new_meshes: Query<Entity, Added<crate::resources::SummaryMesh>>,
) {
    if !flyover.active { return; }
    for entity in new_meshes.iter() {
        // try_insert: silently no-ops if the entity is already despawned by the
        // time this command is applied (dispatch_summary_tasks may evict it in the
        // same frame).
        commands.entity(entity).try_insert(AdminChunk);
    }
}

/// Evicts generated chunks that are far from the flyover camera position.
/// Removes tiles from Map; dispatch_summary_tasks handles mesh entity lifecycle.
pub fn flyover_evict_chunks(
    mut flyover: ResMut<FlyoverState>,
    map: Res<Map>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    camera_query: Query<&Projection, With<Camera3d>>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("fly_evic");
    let fov = camera_query.single().ok().map_or(DEFAULT_FOV, |p| {
        if let Projection::Perspective(persp) = p { persp.fov } else { DEFAULT_FOV }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = (flyover.world_position.y / map.rise()) as i32;
    let center = loc_to_chunk(qrz);

    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let evict_radius = (detail_radius + 5) as i32;

    let evictable: Vec<ChunkId> = flyover.generated_chunks
        .iter()
        .filter(|id| chunk_hex_distance(**id, center) > evict_radius)
        .copied()
        .collect();

    if evictable.is_empty() { return; }

    for &chunk_id in &evictable {
        pending_tiles.tasks.remove(&chunk_id);
        loaded_chunks.chunks.remove(&chunk_id);
    }

    // Remove tiles from Map (triggers mesh rebuild via changed flag)
    for &chunk_id in &evictable {
        map.remove_chunk(chunk_id);
    }

    let evict_set: HashSet<ChunkId> = evictable.into_iter().collect();
    flyover.generated_chunks.retain(|id| !evict_set.contains(id));
    skip_regen.chunks.retain(|id| !evict_set.contains(id));
}

/// Remove all flyover-generated tiles, meshes, and tracking state.
/// The normal pipeline + flyover_generate_chunks will repopulate.
fn evict_generated_chunks(
    commands: &mut Commands,
    flyover: &mut FlyoverState,
    pending_tiles: &mut PendingFlyoverTiles,
    loaded_chunks: &mut LoadedChunks,
    map: &Map,
    skip_regen: &mut SkipNeighborRegen,
    summary_meshes: &mut crate::resources::SummaryMeshes,
) {
    pending_tiles.tasks.clear();

    // Despawn ALL mesh entities — ensures no stale meshes survive
    for (_, state) in summary_meshes.states.drain() {
        if let Some(entity) = state.entity {
            commands.entity(entity).despawn();
        }
    }

    // Remove generated chunk data
    for &chunk_id in &flyover.generated_chunks {
        loaded_chunks.chunks.remove(&chunk_id);
    }

    // Remove tiles from Map
    for &chunk_id in &flyover.generated_chunks {
        map.remove_chunk(chunk_id);
    }

    flyover.generated_chunks.clear();
    skip_regen.chunks.clear();
}

/// Report terrain data at the flyover cursor position.
fn report_terrain_at_cursor(
    flyover: &FlyoverState,
    map: &Map,
) {
    use qrz::Convert;
    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let chunk_id = loc_to_chunk(qrz);

    info!("=== TERRAIN REPORT at ({},{}) z={} chunk=({},{}) ===",
        qrz.q, qrz.r, qrz.z, chunk_id.0, chunk_id.1);

    let chunk_tile_count = chunk_tiles(chunk_id)
        .filter(|&(q, r)| map.get_by_qr(q, r).is_some())
        .count();
    info!("  chunk tiles={}", chunk_tile_count);

    if let Some((_, _)) = map.get_by_qr(qrz.q, qrz.r) {
        let mut neighbors = Vec::new();
        for &(dq, dr) in &[(-1i32,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
            if let Some((n, _)) = map.get_by_qr(qrz.q + dq, qrz.r + dr) {
                if n.z != qrz.z {
                    neighbors.push(format!("{}({},{})", n.z, dq, dr));
                }
            }
        }
        let nb = if neighbors.is_empty() { String::new() } else { format!(" nb=[{}]", neighbors.join(",")) };
        info!("  tile({},{}) z={}{nb}", qrz.q, qrz.r, qrz.z);
    }
}

/// Computes visible summary keys, produces additions and removals through
/// `summary_cache.apply_batch()`. Cache hits are added immediately; misses
/// are dispatched as async tasks (polled by `flyover_poll_summary_tasks`).
pub fn flyover_summary_dispatch(
    flyover: Res<FlyoverState>,
    mut tracker: ResMut<FlyoverSummaryTracker>,
    summary_cache: Res<crate::resources::SummaryCache>,
    admin_composite: Res<AdminComposite>,
) {
    if !flyover.active { return; }

    let pos = flyover.world_position;
    let horiz_dist = ((pos.x - tracker.last_pos.x).powi(2) + (pos.z - tracker.last_pos.z).powi(2)).sqrt();
    if horiz_dist < MIN_UPDATE_DISTANCE && !tracker.tracked.is_empty() {
        return;
    }
    tracker.last_pos = pos;

    use common_bevy::chunk::FIXED_STREAM_RADIUS_WU;
    use common_bevy::message::{SummaryKey, SummaryData};
    use common_bevy::summary::{compute_active_bands, visible_summary_cells_in_band, summary_lattice, select_center_z};
    use crate::systems::camera::{gameplay_camera_height, HORIZON_MARGIN_DEG};

    let camera_height_offset = gameplay_camera_height();
    let player_approx_y = (pos.y - camera_height_offset).max(0.0);
    let camera_total_height = camera_height_offset + player_approx_y;
    let far_ground = camera_total_height / HORIZON_MARGIN_DEG.to_radians().tan();

    let bands = compute_active_bands(far_ground);
    let mut visible: HashSet<SummaryKey> = HashSet::new();
    for band in &bands {
        if band.outer_wu <= FIXED_STREAM_RADIUS_WU { continue; }
        let inner = band.inner_wu.max(FIXED_STREAM_RADIUS_WU);
        let cells = visible_summary_cells_in_band(band.r, pos.x, pos.z, inner, band.outer_wu);
        for (sq, sr) in cells {
            visible.insert(SummaryKey { r: band.r, sq, sr });
        }
    }

    // Removals: tracked - visible
    let removals: Vec<SummaryKey> = tracker.tracked
        .iter()
        .filter(|k| !visible.contains(k))
        .copied()
        .collect();
    if !removals.is_empty() {
        summary_cache.apply_batch(&[], &removals);
    }
    for key in &removals {
        tracker.tracked.remove(key);
    }

    // Additions: visible - tracked
    let mut cached_additions = Vec::new();
    let mut to_compute = Vec::new();
    for key in &visible {
        if tracker.tracked.contains(key) { continue; }
        if let Some(center_z) = summary_cache.get(key) {
            cached_additions.push(SummaryData {
                r: key.r, sq: key.sq, sr: key.sr, center_z,
            });
            tracker.tracked.insert(*key);
        } else if !tracker.in_flight.contains(key) {
            to_compute.push(*key);
        }
    }
    if !cached_additions.is_empty() {
        summary_cache.apply_batch(&cached_additions, &[]);
    }

    // Dispatch async for cache misses
    if !to_compute.is_empty() {
        for key in &to_compute {
            tracker.in_flight.insert(*key);
        }
        let composite = admin_composite.0.clone();
        let keys = to_compute.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            keys.iter().map(|key| {
                let lat = summary_lattice(key.r);
                let tile_zs: Vec<i32> = lat
                    .tiles_in_cell((key.sq, key.sr))
                    .map(|(tq, tr)| composite.elevation_at(tq, tr))
                    .collect();
                let center_z = select_center_z(&tile_zs);
                SummaryData { r: key.r, sq: key.sq, sr: key.sr, center_z }
            }).collect()
        });
        tracker.tasks.push((to_compute, task));
    }
}

/// Polls completed async summary tasks and feeds results through `apply_batch`.
pub fn flyover_poll_summary_tasks(
    flyover: Res<FlyoverState>,
    mut tracker: ResMut<FlyoverSummaryTracker>,
    summary_cache: Res<crate::resources::SummaryCache>,
) {
    if !flyover.active { return; }

    let current = std::mem::take(&mut tracker.tasks);
    let mut pending = Vec::new();

    for (keys, mut task) in current {
        if let Some(results) = block_on(future::poll_once(&mut task)) {
            summary_cache.apply_batch(&results, &[]);
            for key in &keys {
                tracker.in_flight.remove(key);
                tracker.tracked.insert(*key);
            }
        } else {
            pending.push((keys, task));
        }
    }
    tracker.tasks = pending;
}
