//! The sea: a single camera-following plane at sea level, and the material
//! every water surface is drawn with.

//! The sea is a plane of constant height everywhere in the world, so it
//! needs no per-chunk geometry, no decimation, and no streaming: one quad
//! parented to the camera's XZ covers it to the horizon. Lakes and rivers
//! stand above it at a surface per tile, built with each mesh region by the
//! terrain pipeline and drawn with the material this plugin owns.

use bevy::prelude::*;
use crate::systems::closeup::CloseupCamera;
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

/// Side of the water square: past the reach in every direction, so the sea
/// meets the haze and never its own edge.
fn water_extent() -> f32 {
    common_bevy::summary::reach_wu() * 2.5
}

/// Marker for the water surface entity.
#[derive(Component)]
struct WaterSurface;

/// The one material every water surface is drawn with: the sea's, shared
/// by the lakes and rivers the mesh regions build.
#[derive(Resource)]
pub struct WaterMaterial(pub Handle<StandardMaterial>);

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
    let mesh = meshes.add(Plane3d::default().mesh().size(water_extent(), water_extent()));

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
    commands.insert_resource(WaterMaterial(material.clone()));

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
    camera: Query<&Transform, (With<Camera3d>, Without<WaterSurface>, Without<CloseupCamera>)>,
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

    /// The quad must outrun the reach in every direction.
    #[test]
    fn water_extent_covers_the_reach() {
        let reach = common_bevy::summary::reach_wu();
        assert!(
            water_extent() / 2.0 > reach,
            "water half-extent {} does not reach the haze at {reach}",
            water_extent() / 2.0,
        );
    }
}
