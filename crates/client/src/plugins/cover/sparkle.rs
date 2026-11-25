//! The glints over a pile: a few small stars about every pile a region of
//! tiles stands, each flaring and going out in its own time, so what a
//! player left lying is seen among the cover. Each is the region's child in
//! its frame, as a fire's light is, so it comes, re-bases and goes with the
//! pile drawn under it. A glint is unlit and added to what is behind it:
//! it shows by night as by day, and nothing stands in front of it sorted.

use std::f32::consts::{FRAC_PI_4, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_light::NotShadowCaster;
use bevy_mesh::Indices;

use super::draw::{RegionCover, StandKey};
use super::CoverKit;
use crate::systems::closeup::CloseupCamera;

/// The glints over one pile.
const GLINTS: usize = 3;
/// A glint's colour, and how far its points reach at its brightest.
const COLOR: Color = Color::srgb(1.0, 0.95, 0.7);
const REACH: f32 = 0.16;
/// How wide a star's waist is between its points, as a share of their reach.
const WAIST: f32 = 0.12;
/// How long a glint takes to flare, go out and rest, in seconds.
const PERIOD: f32 = 1.8;
/// Where a pile's glints stand, as shares of its model: how far out from
/// its middle, and from how high to how high.
const OUT: f32 = 0.35;
const HEIGHTS: (f32, f32) = (0.6, 1.2);

/// A glint, flaring from its own place in the period, in turns.
#[derive(Component)]
pub struct Glint {
    phase: f32,
}

/// The star every glint is drawn as, and its light.
#[derive(Resource)]
pub struct Glints {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// Makes the star and its light, which every glint shares.
pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let material = StandardMaterial { base_color: COLOR, unlit: true, alpha_mode: AlphaMode::Add, ..default() };
    commands.insert_resource(Glints { mesh: meshes.add(star()), material: materials.add(material) });
}

/// A four-pointed star a unit to each point, facing +z.
fn star() -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    for k in 0..8 {
        let reach = if k % 2 == 0 { 1.0 } else { WAIST * std::f32::consts::SQRT_2 };
        let (sin, cos) = (k as f32 * FRAC_PI_4).sin_cos();
        positions.push([cos * reach, sin * reach, 0.0]);
    }
    let indices = (0..8u32).flat_map(|k| [0, 1 + k, 1 + (k + 1) % 8]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; positions.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices))
}

/// How far a glint at `phase` has flared at `seconds`, none to one: up and
/// out over half its period, and nothing for the rest.
fn flare(seconds: f32, phase: f32) -> f32 {
    (TAU * (seconds / PERIOD + phase)).sin().max(0.0).powi(3)
}

/// Stands the glints over every pile a region of tiles has come to stand,
/// in place of those it had, and takes down those of a region standing
/// none.
pub fn scatter(
    mut commands: Commands,
    kit: Res<CoverKit>,
    look: Res<Glints>,
    regions: Query<(Entity, &RegionCover), Changed<RegionCover>>,
    mut gone: RemovedComponents<RegionCover>,
    glints: Query<(Entity, &ChildOf), With<Glint>>,
) {
    let changed: Vec<Entity> = regions.iter().map(|(region, _)| region).chain(gone.read()).collect();
    if changed.is_empty() {
        return;
    }
    for (glint, of) in &glints {
        if changed.contains(&of.parent()) {
            commands.entity(glint).try_despawn();
        }
    }
    for (region, cover) in &regions {
        for part in &cover.parts {
            let StandKey::Model(kind, k) = part.key else { continue };
            if !kind.is_pile() {
                continue;
            }
            let Some(model) = kit.kit.of(kind).get(k) else { continue };
            for instance in &part.instances {
                let [x, _, z, _] = instance.pos_scale;
                let own = (x * 0.137 + z * 0.731).rem_euclid(1.0);
                for g in 0..GLINTS {
                    // Each a third of the way round from the last and a
                    // third of the period after it, so one is always lit.
                    let share = g as f32 / GLINTS as f32;
                    let (sin, cos) = (TAU * (own + share)).sin_cos();
                    let up = HEIGHTS.0 + (HEIGHTS.1 - HEIGHTS.0) * (own + share * 1.618).fract();
                    let about = Vec3::new(cos * OUT * model.width, up * model.height, sin * OUT * model.width);
                    commands.entity(region).with_child((
                        Glint { phase: own + share },
                        Mesh3d(look.mesh.clone()),
                        MeshMaterial3d(look.material.clone()),
                        Transform::from_translation(instance.place(about)).with_scale(Vec3::ZERO),
                        NotShadowCaster,
                    ));
                }
            }
        }
    }
}

/// Flares every glint, facing the camera and turning as it flares. The
/// clock is the wrapped one: a float of the whole uptime would lose the
/// flare's fraction of a second in a day.
pub fn twinkle(
    time: Res<Time>,
    camera: Query<&Transform, (With<Camera3d>, Without<CloseupCamera>, Without<Glint>)>,
    mut glints: Query<(&Glint, &mut Transform)>,
) {
    let Ok(camera) = camera.single() else { return };
    let seconds = time.elapsed_secs_wrapped();
    for (glint, mut transform) in &mut glints {
        let flare = flare(seconds, glint.phase);
        transform.rotation = camera.rotation * Quat::from_rotation_z(FRAC_PI_4 * flare);
        transform.scale = Vec3::splat(REACH * flare);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A glint goes out wholly between its flares, and a pile's are never
    /// all out at once.
    #[test]
    fn a_pile_always_glints() {
        let mut rested = false;
        for i in 0..600 {
            let t = i as f32 / 60.0;
            let flares: Vec<f32> = (0..GLINTS).map(|g| flare(t, 0.3 + g as f32 / GLINTS as f32)).collect();
            assert!(flares.iter().all(|f| (0.0..=1.0).contains(f)));
            assert!(flares.iter().any(|&f| f > 0.0));
            rested |= flares[0] == 0.0;
        }
        assert!(rested);
    }
}
