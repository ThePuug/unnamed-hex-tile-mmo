use bevy::prelude::*;
use qrz::Convert;
use crate::common::{
    components::{offset::Offset, projectile::Projectile, Loc},
    message::{Do, Event},
    resources::map::Map,
};

/// Component for hit flash visual effects
#[derive(Component)]
pub struct HitFlash {
    /// Time when flash was spawned
    pub spawn_time: f32,
    /// Duration of flash effect (seconds)
    pub duration: f32,
}

/// Client-side projectile movement system
///
/// Projectiles move on the client via local simulation using the Projectile component
/// data received from the server. The server does NOT broadcast Offset updates - clients
/// calculate projectile positions independently based on the initial Projectile data.
///
/// This system runs in FixedUpdate (125ms ticks) to match server physics timing.
pub fn update_projectiles(
    time: Res<Time>,
    map: Res<Map>,
    mut projectiles: Query<(&Projectile, &mut Loc, &mut Offset, &mut Transform)>,
) {
    for (projectile, mut loc, mut offset, mut transform) in projectiles.iter_mut() {
        // Calculate how far projectile should move this frame
        let delta_secs = time.delta_secs();
        let move_distance = projectile.calculate_move_distance(delta_secs);

        // Calculate current world position (NOT just offset!)
        let current_world = map.convert(**loc) + offset.state;

        // Calculate direction toward target using full world coordinates
        let direction = projectile.direction_to_target(current_world, &map);

        // Clamp movement to not overshoot target (prevents bouncing)
        let distance_to_target = projectile.distance_to_target(current_world, &map);
        let clamped_distance = move_distance.min(distance_to_target);

        // Move projectile
        offset.state += direction * clamped_distance;
        offset.step = offset.state; // Projectiles don't interpolate
        offset.prev_step = offset.state;

        // Check if crossed tile boundary (client needs to track this too)
        let world_pos = map.convert(**loc) + offset.state;
        let new_qrz: qrz::Qrz = map.convert(world_pos);
        let new_loc = Loc::new(new_qrz);

        if new_loc != *loc {
            // Recalculate offset relative to new tile center (match server behavior)
            let new_tile_center = map.convert(*new_loc);
            offset.state = world_pos - new_tile_center;
            offset.step = offset.state;
            offset.prev_step = offset.state;
            *loc = new_loc;
        }

        // Update Transform to match new offset position for rendering
        transform.translation = world_pos;

        // Note: Client projectiles are despawned when server sends Despawn event
        // Client does not detect hits - that's server-authoritative
    }
}

/// Spawn hit flash effects when projectiles despawn
pub fn spawn_hit_flash(
    mut commands: Commands,
    mut reader: EventReader<Do>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    map: Res<Map>,
) {
    for &message in reader.read() {
        let Do { event: Event::SpawnHitFlash { loc } } = message else { continue };

        // Convert Loc to world position (tile center + chest height)
        use qrz::Convert;
        let flash_position = map.convert(*loc) + Vec3::new(0.0, 0.5, 0.0);

        commands.spawn((
            HitFlash {
                spawn_time: time.elapsed_secs(),
                duration: 0.3, // 300ms flash
            },
            Mesh3d(meshes.add(Sphere::new(0.4))),
            MeshMaterial3d(materials.add(StandardMaterial {
                emissive: LinearRgba::rgb(10.0, 8.0, 2.0), // Bright yellow-orange flash
                ..default()
            })),
            Transform::from_translation(flash_position),
        ));
    }
}

/// Update and despawn hit flash effects
pub fn update_hit_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &HitFlash, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let current_time = time.elapsed_secs();

    for (entity, flash, mut transform, material_handle) in flashes.iter_mut() {
        let age = current_time - flash.spawn_time;
        let progress = (age / flash.duration).min(1.0);

        if progress >= 1.0 {
            // Flash finished, despawn
            commands.entity(entity).despawn();
        } else {
            // Fade out and expand
            let alpha = 1.0 - progress;
            let scale = 1.0 + progress * 2.0; // Expand to 3x size

            transform.scale = Vec3::splat(scale);

            // Update material alpha (fade out)
            if let Some(material) = materials.get_mut(&material_handle.0) {
                material.emissive = LinearRgba::rgb(10.0 * alpha, 8.0 * alpha, 2.0 * alpha);
            }
        }
    }
}
