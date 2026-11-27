//! The world's curve: everything drawn drops below the level ground by
//! the square of its distance from the camera, so the land ends at a
//! horizon. The drop is the shaders' (`shaders/curve.wesl`), on the way to
//! the screen and nowhere else; the map, the physics and the light stay
//! flat. This is the same rule for what is culled before it is drawn: a
//! thing's bounds stand where the shader will put it.

use bevy::{camera::primitives::Aabb, math::{Affine3A, Vec3A}, prelude::*};

use crate::systems::closeup::CloseupCamera;

/// The radius the ground curves at, in world units: the largest at which
/// nothing past the reach shows over the horizon, `reach² / 2(√eye +
/// √peak)²` for a camera over the tallest ground looking at the tallest
/// ground. The same number as `RADIUS` in `shaders/curve.wesl`.
pub const RADIUS: f32 = 215_000.0;

/// How far below the level ground a point stands, `distance` over the
/// ground from the camera.
pub fn drop(distance: f32) -> f32 {
    distance * distance / (2.0 * RADIUS)
}

/// Bounds in `frame`, an upright frame of unit scale, lowered to hold
/// what they hold once it is drawn from `eye`.
pub fn lowered(bounds: &Aabb, frame: &Affine3A, eye: Vec3) -> Aabb {
    let centre = frame.transform_point3a(bounds.center);
    let away = Vec2::new(centre.x - eye.x, centre.z - eye.z).length();
    let wide = Vec2::new(bounds.half_extents.x, bounds.half_extents.z).length();
    let (near, far) = (drop((away - wide).max(0.0)), drop(away + wide));
    Aabb {
        center: bounds.center - Vec3A::Y * (0.5 * (near + far)),
        half_extents: bounds.half_extents + Vec3A::Y * (0.5 * (far - near)),
    }
}

/// A mesh's bounds as its mesh gave them, and as they were last lowered:
/// bounds found to be neither are its mesh's anew.
#[derive(Component)]
pub struct Curved {
    flat: Aabb,
    lowered: Aabb,
}

/// Lowers every mesh's bounds to where the camera sees it, before the
/// view is culled by them.
pub fn bound(
    mut commands: Commands,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<CloseupCamera>)>,
    mut meshes: Query<(Entity, &mut Aabb, &GlobalTransform, Option<&mut Curved>), With<Mesh3d>>,
) {
    let Ok(eye) = camera.single().map(|at| at.translation()) else { return };
    for (entity, mut bounds, frame, curved) in &mut meshes {
        let flat = match &curved {
            Some(curved) if curved.lowered == *bounds => curved.flat,
            _ => *bounds,
        };
        let lowered = lowered(&flat, &frame.affine(), eye);
        if *bounds != lowered {
            *bounds = lowered;
        }
        match curved {
            Some(mut curved) => *curved = Curved { flat, lowered },
            None => {
                commands.entity(entity).try_insert(Curved { flat, lowered });
            }
        }
    }
}
