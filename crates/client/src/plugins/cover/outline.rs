//! The line round what the keys will act on, the cover it is ([`Marked`])
//! or the actor ([`MarkedActor`]): the thing's own model drawn again, once for each way of [`WAYS`], each copy shifted a
//! line's width over the screen that way and set behind the thing
//! (`shaders/outline.wesl`). The thing covers the middle of them and what
//! shows round it is its outline, as wide on screen however far it
//! stands, whatever its faces' normals. The copies are drawn with the
//! transparent, after everything opaque and into no depth, so the ink
//! lines the thing and never its outline. An actor's copies share its
//! meshes and its skins, so they move as it does.

use bevy::{
    mesh::{skinning::SkinnedMesh, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
};
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::{NotShadowCaster, NotShadowReceiver};

use common_bevy::resources::map::Map;

use super::{standing_on, CoverKit, Marked};
use crate::{resources::RenderOrigin, systems::camera::NEAR_FADE_RADIUS};

/// The ways the copies are shifted, as many as keep a thin part's outline
/// whole at this width.
const WAYS: usize = 8;

/// The line's width, in pixels on a frame 720 high.
const WIDTH_PX: f32 = 2.5;

/// The line's colour: the mark's (`shaders/cover_shared.wesl`), brighter
/// than anything lit.
const COLOR: LinearRgba = LinearRgba::rgb(1.6, 1.3, 0.35);

/// How near a thing stands to the mark's foot to be the thing marked, as
/// the cover's shader finds it (`MARK_MATCH`).
const MATCH: f32 = 0.05;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct OutlineMaterial {
    /// The way this copy is shifted over the screen, a unit; then the
    /// line's width, and how near the camera nothing is drawn, where the
    /// thing itself is dithered away.
    #[uniform(0)]
    pub shift: Vec4,
    #[uniform(0)]
    pub color: LinearRgba,
}

impl Material for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/outline.wesl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/outline.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    /// A model's place, its joints where it has a skin, and nothing else
    /// of it; both sides of every face, since a copy is all silhouette.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        let mut attributes = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0)];
        if layout.0.contains(Mesh::ATTRIBUTE_JOINT_INDEX) && layout.0.contains(Mesh::ATTRIBUTE_JOINT_WEIGHT) {
            attributes.extend([Mesh::ATTRIBUTE_JOINT_INDEX.at_shader_location(6), Mesh::ATTRIBUTE_JOINT_WEIGHT.at_shader_location(7)]);
        }
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        Ok(())
    }
}

/// One copy of the cover marked.
#[derive(Component)]
struct Copy;

/// The actor the keys will act on, a settler; none while they act on
/// cover or nothing.
#[derive(Resource, Clone, Copy, Default, PartialEq)]
pub struct MarkedActor(pub Option<Entity>);

/// One copy of one skinned mesh of the actor marked.
#[derive(Component)]
struct ActorCopy;

/// The materials the copies are drawn with, one to each way.
#[derive(Resource)]
struct Ways(Vec<Handle<OutlineMaterial>>);

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<OutlineMaterial>::default());
        app.add_systems(Startup, setup);
        app.init_resource::<MarkedActor>();
        app.add_systems(Update, (follow.run_if(resource_exists::<CoverKit>), follow_actor));
    }
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<OutlineMaterial>>) {
    let ways: Vec<Handle<OutlineMaterial>> = (0..WAYS)
        .map(|way| {
            let (y, x) = (std::f32::consts::TAU * way as f32 / WAYS as f32).sin_cos();
            materials.add(OutlineMaterial { shift: Vec4::new(x, y, WIDTH_PX, NEAR_FADE_RADIUS), color: COLOR })
        })
        .collect();
    commands.insert_resource(Ways(ways.clone()));
    for way in ways {
        commands.spawn((
            Copy,
            Mesh3d::default(),
            MeshMaterial3d(way),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Stands the copies where the thing marked stands, as its own instance
/// is drawn (`standing_on`, `Instance::place`), and hides them while
/// nothing is marked or no model stands at the mark.
fn follow(marked: Res<Marked>, kit: Res<CoverKit>, map: Res<Map>, origin: Res<RenderOrigin>, mut copies: Query<(&mut Mesh3d, &mut Transform, &mut Visibility), With<Copy>>) {
    let stood = marked.0.and_then(|mark| {
        // The thing's place on its own tile, as the mark's foot has it
        let about = (mark.foot - origin.render_tile(&map, mark.tile)).xz();
        let mut found = None;
        standing_on(map.cover_at(mark.tile.q, mark.tile.r), (mark.tile.q, mark.tile.r), (mark.tile.q, mark.tile.r), |stood| {
            if stood.at.distance(about) < MATCH {
                found = Some(stood);
            }
        });
        let stood = found?;
        let variations = kit.kit.of(stood.kind);
        let k = stood.variation as usize % variations.len().max(1);
        let scale = kit.kit.drawn_scale(stood.kind, k, stood.growth);
        // An instance's turn runs the other way about the vertical from a rotation about +y
        let transform = Transform { translation: mark.foot, rotation: Quat::from_rotation_y(-stood.yaw), scale: Vec3::splat(scale) };
        Some((variations.get(k)?.mesh.clone(), transform))
    });
    for (mut mesh, mut transform, mut visibility) in &mut copies {
        let shown = match &stood {
            Some((model, at)) => {
                if mesh.0 != *model {
                    mesh.0 = model.clone();
                }
                if *transform != *at {
                    *transform = *at;
                }
                Visibility::Visible
            }
            None => Visibility::Hidden,
        };
        if *visibility != shown {
            *visibility = shown;
        }
    }
}

/// Draws the actor marked over again, a copy each way of every skinned
/// mesh under it, body and worn pieces, bound to the same joints. They
/// are made again whenever the meshes under the actor change, as a piece
/// is put on or the body's hidden faces are, and taken away when the mark
/// leaves it.
fn follow_actor(
    mut commands: Commands,
    marked: Res<MarkedActor>,
    ways: Res<Ways>,
    children: Query<&Children>,
    skinned: Query<(&Mesh3d, &SkinnedMesh), Without<ActorCopy>>,
    copies: Query<Entity, With<ActorCopy>>,
    mut drawn: Local<Vec<(Entity, AssetId<Mesh>)>>,
) {
    let stands: Vec<(Entity, AssetId<Mesh>)> = marked.0.map_or(Vec::new(), |actor| children.iter_descendants(actor).filter_map(|part| Some((part, skinned.get(part).ok()?.0.id()))).collect());
    if *drawn == stands {
        return;
    }
    for copy in &copies {
        commands.entity(copy).despawn();
    }
    for &(part, _) in &stands {
        let Ok((mesh, skin)) = skinned.get(part) else { continue };
        for way in &ways.0 {
            // Placed by its joints, never its own transform, so nothing culls it by one
            commands.spawn((ActorCopy, mesh.clone(), skin.clone(), MeshMaterial3d(way.clone()), Transform::default(), Visibility::Visible, NoFrustumCulling, NotShadowCaster, NotShadowReceiver));
        }
    }
    *drawn = stands;
}
