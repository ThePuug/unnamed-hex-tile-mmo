//! The sea: a camera-following disc at sea level, and the material
//! every water surface is drawn with.

//! The sea is a surface of constant height everywhere in the world, so it
//! needs no per-chunk geometry, no decimation, and no streaming: one disc
//! centred under the camera covers it past the horizon, in rings the
//! world's curve bends it at (`systems::curve`). Lakes and rivers
//! stand above it at a surface per tile, built with each mesh region by the
//! terrain pipeline and drawn with the material this plugin owns.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError},
    shader::ShaderRef,
};
use crate::{
    resources::{RenderOrigin, TerrainOrigin},
    systems::closeup::CloseupCamera,
};
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::NotShadowCaster;

use common::grid::RISE;

/// World-space Y of the waterline.

/// A tile's rendered surface sits at `common_bevy::surface::height_y`,
/// `(z + 1) * RISE`: z=0 renders at `RISE`, z=-1 at 0. The z=0 band
/// straddles true elevation 0, so the dry/wet boundary belongs between the
/// two surfaces. Halfway leaves z=0 dry, z=-1 submerged, and coplanar with
/// neither, so nothing z-fights against the water.
pub const SEA_LEVEL_Y: f32 = RISE / 2.0;

/// How far out the sea is drawn: the reach, which its horizon never
/// passes.
fn water_extent() -> f32 {
    common_bevy::summary::reach_wu()
}

/// How far out the sea is seen through. Past it the sea is solid, so
/// land the horizon hides does not show through the water before it: no
/// further than the horizon of a camera a few units over the water.
const CLEAR_TO: f32 = 1000.0;

/// The spokes of the sea's disc, and how much wider each ring is than the
/// one inside it: a ring is flat across, so the curve it stands in for
/// sags under it by the square of its width.
const SPOKES: u32 = 128;
const RING_GROWTH: f32 = 1.1;

/// A level ring from `inner` to `outer` about the origin, in rings each
/// wider than the last from `first` wide; a disc where `inner` is zero.
/// Its outer rings are drawn first, so where the curve folds the far sea
/// under the near, the near is laid over it.
fn ring(inner: f32, outer: f32, first: f32) -> Mesh {
    let mut radii = vec![inner];
    let mut wide = first;
    while *radii.last().expect("a radius") < outer {
        let next = radii.last().expect("a radius") + wide;
        radii.push(if next + wide * 0.5 > outer { outer } else { next });
        wide *= RING_GROWTH;
    }
    let mut positions = Vec::new();
    for radius in &radii {
        for spoke in 0..SPOKES {
            let (sin, cos) = (spoke as f32 / SPOKES as f32 * std::f32::consts::TAU).sin_cos();
            positions.push([radius * cos, 0.0, radius * sin]);
        }
    }
    let mut indices = Vec::new();
    for band in (0..radii.len() as u32 - 1).rev() {
        for spoke in 0..SPOKES {
            let next = (spoke + 1) % SPOKES;
            let (a, b, c, d) = (band * SPOKES + spoke, band * SPOKES + next, (band + 1) * SPOKES + spoke, (band + 1) * SPOKES + next);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; positions.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; positions.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices))
}

/// Marker for the water surface entity.
#[derive(Component)]
struct WaterSurface;

/// The material the lakes and rivers the mesh regions build are drawn
/// with: the sea's water, with the rim of its surf and none of its
/// breakers, lit by the cel light as the ground is, so it holds its blue
/// from every angle.
#[derive(Resource)]
pub struct WaterMaterial(pub Handle<Water>);

pub type Water = ExtendedMaterial<StandardMaterial, Level>;

/// A level face over the standard material it is lit as, by the cel
/// light, and placed on the world's curve (`systems::curve`): water is
/// drawn as far as the land is, so it bends with it. Its surf is drawn
/// over it (`shaders/water.wesl`), read from the depth prepass: only
/// blended water has the ground under it there, so a solid one takes none.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct Level {
    #[uniform(100)]
    surf: Surf,
}

/// How much of the surf a water takes, each none to whole, and the origin
/// its shards are laid on the world's tiles from, so they hold through a
/// rebase: `lap` the foam along everything the water meets, `swell` the
/// shallows' lighter tone and the breakers coming in over them.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct Surf {
    origin: TerrainOrigin,
    lap: f32,
    swell: f32,
}

impl Level {
    fn surfed(lap: f32, swell: f32) -> Self {
        Self { surf: Surf { origin: TerrainOrigin::default(), lap, swell } }
    }
}

impl MaterialExtension for Level {
    fn vertex_shader() -> ShaderRef {
        "shaders/water_vertex.wesl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/water_vertex.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wesl".into()
    }
    /// A vertex's place alone, in every pass: a level face has one normal.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?];
        Ok(())
    }
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<Water>::default());
        app.add_systems(Startup, setup_water_surface);
        // After camera movement so the plane never lags a frame behind and
        // reveals its own edge at the horizon.
        app.add_systems(PostUpdate, follow_camera);
        app.add_systems(Update, follow_origin.run_if(resource_changed::<RenderOrigin>));
    }
}

fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<Water>>,
) {
    let near = meshes.add(ring(0.0, CLEAR_TO, 4.0));
    let far = meshes.add(ring(CLEAR_TO, water_extent(), CLEAR_TO * (RING_GROWTH - 1.0)));

    let clear = StandardMaterial {
        base_color: Color::srgba(0.07, 0.26, 0.42, 0.78),
        alpha_mode: AlphaMode::Blend,
        // Seen from below when the camera dips under the waterline.
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let sea = materials.add(Water { base: clear.clone(), extension: Level::surfed(1.0, 1.0) });
    // A river is shallow from bank to bank, and breakers would cover it.
    let inland = materials.add(Water { base: clear, extension: Level::surfed(1.0, 0.0) });
    commands.insert_resource(WaterMaterial(inland));
    let solid = materials.add(Water {
        base: StandardMaterial { base_color: Color::srgb(0.07, 0.26, 0.42), double_sided: true, cull_mode: None, ..default() },
        extension: Level::surfed(0.0, 0.0),
    });

    for (mesh, material) in [(near, sea), (far, solid)] {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, SEA_LEVEL_Y, 0.0),
            // Re-centred on the camera in PostUpdate, after the visibility
            // pass has already culled against where it was: never culled.
            NoFrustumCulling,
            NotShadowCaster,
            WaterSurface,
        ));
    }
}

/// Keep the disc centred under the camera, where the curve it is ringed
/// for is centred. Y is fixed at the waterline: only XZ tracks.
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

/// Lays every water's shards from the render origin as it moves.
fn follow_origin(origin: Res<RenderOrigin>, mut materials: ResMut<Assets<Water>>) {
    let origin = TerrainOrigin::of(&origin);
    for (_, material) in materials.iter_mut() {
        material.extension.surf.origin = origin;
    }
}
