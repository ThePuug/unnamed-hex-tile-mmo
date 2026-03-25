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
    pub saved_camera_scale: f32,
    /// Stashed normal-play state, restored on flyover toggle-off.
    pub stashed_lod: Option<HashMap<ChunkId, crate::resources::ChunkLodState>>,
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
            saved_camera_scale: 1.0,
            stashed_lod: None,
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

/// Pending async tile generation tasks.
#[derive(Default, Resource)]
pub struct PendingFlyoverTiles {
    pub tasks: HashMap<ChunkId, Task<Vec<(qrz::Qrz, EntityType)>>>,
}

/// Manual decimation threshold for flyover inspection.
/// Number keys 0-9 set the threshold during flyover.
#[derive(Resource)]
pub struct FlyoverDecimationConfig {
    pub threshold: u32,
}

impl Default for FlyoverDecimationConfig {
    fn default() -> Self {
        Self { threshold: 0 }
    }
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
    mut loaded_chunks: ResMut<LoadedChunks>,
    map: Res<Map>,
    admin_composite: Res<AdminComposite>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    mut flyover_config: ResMut<FlyoverDecimationConfig>,
    mut lod_meshes: ResMut<crate::resources::ChunkLodMeshes>,
    cursor_query: Query<Entity, With<FlyoverCursor>>,
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
            DevConsoleAction::SetDecimationThreshold(value) => {
                let value = *value;
                if value != flyover_config.threshold {
                    info!("Decimation threshold: {} → {}", flyover_config.threshold, value);
                    flyover_config.threshold = value;
                    evict_generated_chunks(
                        &mut commands, &mut flyover, &mut pending_tiles,
                        &mut loaded_chunks, &map, &mut skip_regen, &mut lod_meshes,
                    );
                }
                continue;
            }
            DevConsoleAction::ReportTerrain => {
                if flyover.active {
                    report_terrain_at_cursor(&flyover, &map, &lod_meshes, flyover_config.threshold);
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
            if let Ok(projection) = camera_query.single() {
                if let Projection::Orthographic(ortho) = &*projection {
                    flyover.saved_camera_scale = ortho.scale;
                }
            }

            // Stash existing mesh + loaded state and hide entities — restored on toggle-off.
            let stashed: HashMap<ChunkId, crate::resources::ChunkLodState> = lod_meshes.states.drain().collect();
            for (_, state) in &stashed {
                if let Some(entity) = state.entity {
                    commands.entity(entity).insert(Visibility::Hidden);
                }
            }
            flyover.stashed_lod = Some(stashed);
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
                &mut loaded_chunks, &map, &mut skip_regen, &mut lod_meshes,
            );

            // Restore stashed normal-play state
            if let Some(stashed) = flyover.stashed_lod.take() {
                for (chunk_id, state) in stashed {
                    if let Some(entity) = state.entity {
                        commands.entity(entity).insert(Visibility::Inherited);
                    }
                    lod_meshes.states.insert(chunk_id, state);
                }
            }
            if let Some(stashed) = flyover.stashed_loaded.take() {
                loaded_chunks.chunks.extend(stashed);
            }

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

        let offset = Vec3::new(
            orbit.current.sin() * CAMERA_DISTANCE,
            CAMERA_HEIGHT,
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
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let player_z = (flyover.world_position.y / map.rise()) as i32;
    let center = loc_to_chunk(qrz);
    let fov = DEFAULT_FOV / scale.max(0.1);

    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let gen_radius = (detail_radius + 3).min(MAX_FLYOVER_RADIUS);

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
}

/// Tags newly spawned chunk meshes that belong to admin-generated chunks.
pub fn tag_admin_chunks(
    mut commands: Commands,
    flyover: Res<FlyoverState>,
    new_chunks: Query<(Entity, &ChunkMesh), Added<ChunkMesh>>,
) {
    for (entity, chunk_mesh) in new_chunks.iter() {
        if flyover.generated_chunks.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).insert(AdminChunk);
        }
    }
}

/// Evicts generated chunks that are far from the flyover camera position.
pub fn flyover_evict_chunks(
    mut flyover: ResMut<FlyoverState>,
    mut commands: Commands,
    map: Res<Map>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    mut lod_meshes: ResMut<crate::resources::ChunkLodMeshes>,
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

    let detail_radius = common_bevy::chunk::detail_boundary_radius(player_z, fov);
    let evict_radius = (detail_radius + 5).min(MAX_FLYOVER_RADIUS) as i32;

    let evictable: Vec<ChunkId> = flyover.generated_chunks
        .iter()
        .filter(|id| chunk_hex_distance(**id, center) > evict_radius)
        .copied()
        .collect();

    if evictable.is_empty() { return; }

    // Despawn mesh entities first (before removing state)
    for &chunk_id in &evictable {
        pending_tiles.tasks.remove(&chunk_id);
        if let Some(state) = lod_meshes.states.remove(&chunk_id) {
            if let Some(entity) = state.entity {
                commands.entity(entity).despawn();
            }
        }
        loaded_chunks.chunks.remove(&chunk_id);
    }

    // Remove tiles from Map
    {
        let mut map_w = map.write();
        for &chunk_id in &evictable {
            for (q, r) in chunk_tiles(chunk_id) {
                if let Some((qrz, _)) = map_w.get_by_qr(q, r) {
                    map_w.remove(qrz);
                }
            }
        }
    }

    let evict_set: HashSet<ChunkId> = evictable.into_iter().collect();
    flyover.generated_chunks.retain(|id| !evict_set.contains(id));
    skip_regen.chunks.retain(|id| !evict_set.contains(id));
}

/// Number keys 0-9 set the flyover decimation threshold (keyboard shortcut).
pub fn flyover_threshold_control(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<FlyoverDecimationConfig>,
    mut flyover: ResMut<FlyoverState>,
    mut commands: Commands,
    mut pending_tiles: ResMut<PendingFlyoverTiles>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    map: Res<Map>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    mut lod_meshes: ResMut<crate::resources::ChunkLodMeshes>,
) {
    let digit = [
        (KeyCode::Digit0, 0u32), (KeyCode::Digit1, 1), (KeyCode::Digit2, 2),
        (KeyCode::Digit3, 3), (KeyCode::Digit4, 4), (KeyCode::Digit5, 5),
        (KeyCode::Digit6, 6), (KeyCode::Digit7, 7), (KeyCode::Digit8, 8),
        (KeyCode::Digit9, 9),
    ];

    for &(key, value) in &digit {
        if keyboard.just_pressed(key) && value != config.threshold {
            info!("Flyover decimation threshold: {} → {}", config.threshold, value);
            config.threshold = value;
            evict_generated_chunks(
                &mut commands, &mut flyover, &mut pending_tiles,
                &mut loaded_chunks, &map, &mut skip_regen, &mut lod_meshes,
            );
            break;
        }
    }
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
    lod_meshes: &mut crate::resources::ChunkLodMeshes,
) {
    pending_tiles.tasks.clear();

    // Despawn ALL mesh entities — ensures no stale-threshold meshes survive
    for (_, state) in lod_meshes.states.drain() {
        if let Some(entity) = state.entity {
            commands.entity(entity).despawn();
        }
    }

    // Remove generated chunk data
    for &chunk_id in &flyover.generated_chunks {
        loaded_chunks.chunks.remove(&chunk_id);
    }

    // Remove tiles from Map
    {
        let mut map_w = map.write();
        for &chunk_id in &flyover.generated_chunks {
            for (q, r) in chunk_tiles(chunk_id) {
                if let Some((qrz, _)) = map_w.get_by_qr(q, r) {
                    map_w.remove(qrz);
                }
            }
        }
    }

    flyover.generated_chunks.clear();
    skip_regen.chunks.clear();
}

/// Report terrain data at the flyover cursor position.
/// Runs decimation on the chunk and logs the hexball containing the cursor tile.
fn report_terrain_at_cursor(
    flyover: &FlyoverState,
    map: &Map,
    _lod_meshes: &crate::resources::ChunkLodMeshes,
    threshold: u32,
) {
    use qrz::Convert;
    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let chunk_id = loc_to_chunk(qrz);
    let rise = map.rise();

    info!("=== TERRAIN REPORT at ({},{}) z={} chunk=({},{}) threshold={} ===",
        qrz.q, qrz.r, qrz.z, chunk_id.0, chunk_id.1, threshold);

    // Collect chunk tiles + elevations (same as collect_and_build_mesh)
    let chunk_tile_list: Vec<(i32, i32, i32)> = chunk_tiles(chunk_id)
        .filter_map(|(q, r)| map.get_by_qr(q, r).map(|(t, _)| (t.q, t.r, t.z)))
        .collect();

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

    let elev_lookup = |q: i32, r: i32| -> Option<i32> { elevations.get(&(q, r)).copied() };
    let decimation = common::hex_decimate::decimate_chunk(&chunk_tile_list, 1, threshold, &elev_lookup);

    info!("  chunk tiles={} hexballs={} survivors={}",
        chunk_tile_list.len(), decimation.hexballs.len(), decimation.survivors.len());

    // Find hexball containing cursor tile
    for (hi, hb) in decimation.hexballs.iter().enumerate() {
        let dq = qrz.q - hb.center_q;
        let dr = qrz.r - hb.center_r;
        let dist = dq.abs().max(dr.abs()).max((dq + dr).abs());
        if dist > 1 { continue; } // r=1 hexball

        // Compute the actual rendered surface — single source of truth.
        let surface = common_bevy::hexball_geometry::compute_hexball_surface(
            hb.center_q, hb.center_r, hb.center_z, 1,
            map.radius(), rise, &elev_lookup,
        );
        let bv = surface.hex_boundary.unwrap();

        info!("  HEXBALL[{hi}] center=({},{}) z={} bv_y={:?}",
            hb.center_q, hb.center_r, hb.center_z,
            bv.map(|v| format!("{:.2}", v.y)));

        // Log tiles in this hexball
        for ddq in -1..=1 {
            let dr_min = (-1).max(-ddq - 1);
            let dr_max = 1.min(-ddq + 1);
            for ddr in dr_min..=dr_max {
                let tq = hb.center_q + ddq;
                let tr = hb.center_r + ddr;
                if let Some(tz) = elev_lookup(tq, tr) {
                    let mut neighbors = Vec::new();
                    for &(ndq, ndr) in &[(-1i32,0),(-1,1),(0,1),(1,0),(1,-1),(0,-1)] {
                        if let Some(nz) = elev_lookup(tq + ndq, tr + ndr) {
                            if nz != tz {
                                neighbors.push(format!("{}({},{})", nz, ndq, ndr));
                            }
                        }
                    }
                    let marker = if tq == qrz.q && tr == qrz.r { " <<<" } else { "" };
                    let nb = if neighbors.is_empty() { String::new() } else { format!(" nb=[{}]", neighbors.join(",")) };
                    info!("    tile({tq},{tr}) z={tz}{nb}{marker}");
                }
            }
        }

        // Partial fans — all values read from the rendered surface struct
        for (pi, fan) in surface.partial_fans.iter().enumerate() {
            let st = fan.surviving_triangles;
            let edge = st[1] as usize;
            let edge_next = (edge + 1) % 6;
            let absorbed: Vec<u8> = (0..6u8).filter(|t| !st.contains(t)).collect();

            let d0 = fan.outer[0].y - bv[edge].y;
            let d3 = fan.outer[3].y - bv[edge_next].y;

            info!("    partial[{pi}] ({},{}) oz={} center_y={:.4} edge=V{}→V{} surv={:?} abs={:?}",
                fan.q, fan.r, fan.z, fan.center.y,
                edge, edge_next, st, absorbed);
            info!("      ov[0] y={:.4} bv[{}] y={:.4} d={:.4}",
                fan.outer[0].y, edge, bv[edge].y, d0);
            info!("      ov[1] y={:.4}  ov[2] y={:.4}",
                fan.outer[1].y, fan.outer[2].y);
            info!("      ov[3] y={:.4} bv[{}] y={:.4} d={:.4}",
                fan.outer[3].y, edge_next, bv[edge_next].y, d3);
        }

        // Skirts are now chunk-level (not per-hexball) — see build_chunk_mesh.
    }

    // Check if cursor tile is a survivor instead
    if decimation.survivors.iter().any(|&(q, r, _)| q == qrz.q && r == qrz.r) {
        info!("  cursor tile ({},{}) is a SURVIVOR (not decimated)", qrz.q, qrz.r);
    }
}
