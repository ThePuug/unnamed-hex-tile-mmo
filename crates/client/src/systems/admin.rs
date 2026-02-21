use std::collections::HashSet;
use std::f32::consts::PI;

use bevy::prelude::*;

use common::{
    chunk::{ChunkId, CHUNK_SIZE, FOV_CHUNK_RADIUS, calculate_visible_chunks, chunk_to_tile, loc_to_chunk},
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
    resources::{LoadedChunks, SkipNeighborRegen},
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
    pub admin_chunks: HashSet<ChunkId>,
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
            saved_camera_scale: 1.0,
        }
    }
}

/// Marker for admin-generated chunk mesh entities (cleanup target).
#[derive(Component)]
pub struct AdminChunk;

/// Wraps terrain::Terrain for local chunk generation.
#[derive(Resource)]
pub struct AdminTerrain(pub terrain::Terrain);

impl Default for AdminTerrain {
    fn default() -> Self {
        Self(terrain::Terrain::default())
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
const MAX_FLYOVER_RADIUS: u8 = 15;

/// Compute a generation radius that scales with the camera's orthographic zoom.
/// At scale 1.0 the normal FOV radius suffices; at scale 20.0 we need much more.
fn flyover_radius(camera_scale: f32) -> u8 {
    // Each chunk ≈ 28 world units across. Orthographic viewport is ~40*scale
    // vertically, wider horizontally. Diagonal half-extent ≈ 1.2 * scale chunks.
    let zoom_chunks = (camera_scale * 1.2).ceil() as u8;
    FOV_CHUNK_RADIUS.max(zoom_chunks).min(MAX_FLYOVER_RADIUS)
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
    admin_terrain: Res<AdminTerrain>,
    map_state: Res<MapState>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
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

            // Despawn tiles for admin-only chunks (skip server-loaded ones)
            for &chunk_id in &flyover.admin_chunks {
                if loaded_chunks.chunks.contains(&chunk_id) {
                    continue;
                }
                for oq in 0..CHUNK_SIZE {
                    for or in 0..CHUNK_SIZE {
                        let tile = chunk_to_tile(chunk_id, oq as u8, or as u8);
                        let z = admin_terrain.0.get_height(tile.q, tile.r);
                        let qrz = qrz::Qrz { q: tile.q, r: tile.r, z };
                        map_state.queue_event(TileEvent::Despawn(qrz));
                    }
                }
            }
            flyover.admin_chunks.clear();
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
/// Generates all missing chunks at once — the neighbor mesh cascade is prevented
/// by adding generated chunk IDs to `SkipNeighborRegen` so `spawn_missing_chunk_meshes`
/// won't trigger redundant neighbor regeneration.
/// Radius scales with camera zoom so zoomed-out views stay filled.
pub fn flyover_generate_chunks(
    mut flyover: ResMut<FlyoverState>,
    map: Res<Map>,
    map_state: Res<MapState>,
    admin_terrain: Res<AdminTerrain>,
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    camera_query: Query<&Projection, With<Camera3d>>,
) {
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });
    let radius = flyover_radius(scale);

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let center = loc_to_chunk(qrz);

    let candidates: Vec<ChunkId> = calculate_visible_chunks(center, radius)
        .into_iter()
        .filter(|id| !flyover.admin_chunks.contains(id) && !loaded_chunks.chunks.contains(id))
        .collect();

    for chunk_id in candidates {
        for oq in 0..CHUNK_SIZE {
            for or in 0..CHUNK_SIZE {
                let tile = chunk_to_tile(chunk_id, oq as u8, or as u8);
                let z = admin_terrain.0.get_height(tile.q, tile.r);
                let qrz = qrz::Qrz { q: tile.q, r: tile.r, z };
                let decorator = Decorator { index: 3, is_solid: true };
                map_state.queue_event(TileEvent::Spawn(qrz, EntityType::Decorator(decorator)));
            }
        }

        flyover.admin_chunks.insert(chunk_id);
        skip_regen.chunks.insert(chunk_id);
    }
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

/// Evicts admin chunks that are far from the camera position.
pub fn flyover_evict_chunks(
    mut commands: Commands,
    mut flyover: ResMut<FlyoverState>,
    map: Res<Map>,
    map_state: Res<MapState>,
    admin_terrain: Res<AdminTerrain>,
    loaded_chunks: Res<LoadedChunks>,
    mut skip_regen: ResMut<SkipNeighborRegen>,
    admin_chunk_query: Query<(Entity, &ChunkMesh), With<AdminChunk>>,
    camera_query: Query<&Projection, With<Camera3d>>,
) {
    let scale = camera_query.single().ok().map_or(1.0, |p| {
        if let Projection::Orthographic(o) = p { o.scale } else { 1.0 }
    });
    let radius = flyover_radius(scale);

    let qrz: qrz::Qrz = map.convert(flyover.world_position);
    let center = loc_to_chunk(qrz);
    let keep: HashSet<ChunkId> = calculate_visible_chunks(center, radius + 1)
        .into_iter()
        .collect();

    let evictable: Vec<ChunkId> = flyover.admin_chunks
        .iter()
        .filter(|id| !keep.contains(id))
        .copied()
        .collect();

    if evictable.is_empty() {
        return;
    }

    let evict_set: HashSet<ChunkId> = evictable.iter().copied().collect();

    // Despawn mesh entities for evicted admin chunks
    for (entity, chunk_mesh) in admin_chunk_query.iter() {
        if evict_set.contains(&chunk_mesh.chunk_id) {
            commands.entity(entity).despawn();
        }
    }

    // Queue tile despawns (skip server-loaded chunks)
    for &chunk_id in &evictable {
        if loaded_chunks.chunks.contains(&chunk_id) {
            continue;
        }
        for oq in 0..CHUNK_SIZE {
            for or in 0..CHUNK_SIZE {
                let tile = chunk_to_tile(chunk_id, oq as u8, or as u8);
                let z = admin_terrain.0.get_height(tile.q, tile.r);
                let qrz = qrz::Qrz { q: tile.q, r: tile.r, z };
                map_state.queue_event(TileEvent::Despawn(qrz));
            }
        }
    }

    flyover.admin_chunks.retain(|id| !evict_set.contains(id));
    skip_regen.chunks.retain(|id| !evict_set.contains(id));
}
