//! Cel light for what glTF scenes bring: each material a scene spawns with —
//! an actor, what it wears, what it drops — is swapped for the same
//! material extended by the cel light, so an actor is shaded by the rule
//! the ground and the cover are (`shaders/cel.wgsl`). A material built in
//! code, the water and the markers, keeps the standard light.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub struct CelPlugin;

impl Plugin for CelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<CelMaterial>::default())
            .init_resource::<CelMaterials>()
            .add_systems(Update, swap_materials);
    }
}

pub type CelMaterial = ExtendedMaterial<StandardMaterial, Cel>;

/// The cel light over a standard material: its fragment stage only.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct Cel {}

impl MaterialExtension for Cel {
    fn fragment_shader() -> ShaderRef {
        "shaders/cel_material.wgsl".into()
    }
}

/// Each glTF material's cel counterpart, so every actor wearing a material
/// shares one.
#[derive(Resource, Default)]
struct CelMaterials(HashMap<AssetId<StandardMaterial>, Handle<CelMaterial>>);

/// A glTF's materials are the ones with a path (`actors/x.glb#Material0`).
/// Every standard material is looked at each frame, not only when added, so
/// one whose asset had not loaded when its mesh spawned is swapped once it
/// has.
fn swap_materials(
    mut commands: Commands,
    meshes: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
    asset_server: Res<AssetServer>,
    standard: Res<Assets<StandardMaterial>>,
    mut cel: ResMut<Assets<CelMaterial>>,
    mut swapped: ResMut<CelMaterials>,
) {
    for (entity, material) in &meshes {
        let id = material.id();
        let handle = match swapped.0.get(&id) {
            Some(handle) => handle.clone(),
            None => {
                if asset_server.get_path(id).is_none() {
                    continue;
                }
                let Some(base) = standard.get(id) else { continue };
                let handle = cel.add(CelMaterial { base: base.clone(), extension: Cel {} });
                swapped.0.insert(id, handle.clone());
                handle
            }
        };
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
}
