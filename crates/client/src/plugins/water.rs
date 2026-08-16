//! Water surface — a single camera-following plane at sea level.

//! `SeaEvent` gives every tile below the regime land threshold a negative
//! elevation, so the seabed is already real terrain that the elevation ramp in
//! `terrain.wgsl` shades blue. What was missing is the surface itself: without
//! it the ocean reads as a flat blue plain rather than water.

//! Deliberately independent of the chunk/LoD mesh pipeline. Water is a plane of
//! constant height everywhere in the world, so it needs no per-chunk geometry,
//! no decimation, and no streaming — one quad parented to the camera's XZ
//! covers every case the terrain pipeline would otherwise have to special-case.

use bevy::prelude::*;
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::NotShadowCaster;

use common::camera::RISE;

/// World-space Y of the waterline.

/// A tile's rendered surface sits at `(z + 1) * RISE`: z=0 renders at `RISE`,
/// z=-1 at 0. `discretize_elevation` rounds, so the z=0 band straddles true
/// elevation 0 — the dry/wet boundary belongs between the two surfaces.
/// Halfway leaves z=0 dry, z=-1 submerged, and coplanar with neither, so
/// nothing z-fights against the water.
pub const SEA_LEVEL_Y: f32 = RISE / 2.0;

/// Half-extent of the water quad. The camera's far plane is 10,000 WU, so a
/// 30,000 WU square still reaches the horizon at the shallowest view angle.
const WATER_EXTENT: f32 = 30_000.0;

/// Marker for the water surface entity.
#[derive(Component)]
struct WaterSurface;

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_water_surface);
        // After camera movement so the plane never lags a frame behind and
        // reveals its own edge at the horizon.
        app.add_systems(PostUpdate, follow_camera);
    }
}

fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Plane3d::default().mesh().size(WATER_EXTENT, WATER_EXTENT));

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.07, 0.26, 0.42, 0.78),
        perceptual_roughness: 0.12,
        metallic: 0.0,
        reflectance: 0.4,
        alpha_mode: AlphaMode::Blend,
        // Seen from below when the camera dips under the waterline.
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, SEA_LEVEL_Y, 0.0),
        // The quad is re-centred on the camera every frame in PostUpdate,
        // which races the visibility pass that would cull it against the
        // previous frame's bounds. It is always directly beneath the viewer,
        // so there is nothing to gain by culling it — skip the test entirely.
        // (Never insert `Aabb::default()` here: Bevy only computes real mesh
        // bounds `Without<Aabb>`, so supplying a zero-extent one makes the
        // 30,000 WU quad a single point and it disappears.)
        NoFrustumCulling,
        NotShadowCaster,
        WaterSurface,
    ));
}

/// Keep the quad centred under the camera. Y is fixed at the waterline — only
/// XZ tracks, so the surface stays a true horizontal plane at constant height.
fn follow_camera(
    camera: Query<&Transform, (With<Camera3d>, Without<WaterSurface>)>,
    mut water: Query<&mut Transform, With<WaterSurface>>,
) {
    let Ok(cam) = camera.single() else { return };
    for mut transform in &mut water {
        transform.translation.x = cam.translation.x;
        transform.translation.z = cam.translation.z;
        transform.translation.y = SEA_LEVEL_Y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The waterline must fall strictly between the rendered surfaces of z=0
    /// and z=-1, or it will z-fight with one of them.
    #[test]
    fn sea_level_sits_between_z0_and_z_minus_1() {
        let surface_of = |z: i32| (z + 1) as f32 * RISE;
        assert!(
            surface_of(-1) < SEA_LEVEL_Y && SEA_LEVEL_Y < surface_of(0),
            "sea level {SEA_LEVEL_Y} not between z=-1 ({}) and z=0 ({})",
            surface_of(-1),
            surface_of(0),
        );
    }

    /// The quad must outrun the camera's far plane in every direction.
    #[test]
    fn water_extent_covers_far_plane() {
        const CAMERA_FAR: f32 = 10_000.0;
        assert!(
            WATER_EXTENT / 2.0 > CAMERA_FAR,
            "water half-extent {} does not reach the far plane {CAMERA_FAR}",
            WATER_EXTENT / 2.0,
        );
    }
}
