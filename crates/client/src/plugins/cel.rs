//! Cel light for what glTF scenes bring: each material a scene spawns with —
//! an actor, what it wears, what it drops — is swapped for the same
//! material extended by the cel light, so an actor is shaded by the rule
//! the ground and the cover are (`shaders/cel.wesl`). The water builds its
//! own cel material (`plugins::water`); the markers, built in code, keep
//! the standard light. A worn piece's materials are then turned to the
//! colour its item is made in (`Dye`).

use bevy::{
    gltf::GltfMaterialName,
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
            .init_resource::<Dyed>()
            .add_systems(Update, (swap_materials, dye).chain());
    }
}

pub type CelMaterial = ExtendedMaterial<StandardMaterial, Cel>;

/// The cel light over a standard material: its fragment stage only.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct Cel {}

impl MaterialExtension for Cel {
    fn fragment_shader() -> ShaderRef {
        "shaders/cel_material.wesl".into()
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

/// The seed of a set of the kit a mesh is to be drawn in, the set by its
/// place in `common::kit::SETS`: set on a worn piece's meshes as they
/// spawn, and taken off once the mesh draws in it.
#[derive(Clone, Copy, Component, Eq, Hash, PartialEq)]
pub struct Dye {
    pub set: &'static str,
    pub seed: u16,
}

/// Each cel material in each seed of each set it has been dyed, so every
/// actor wearing a material in one shares one.
#[derive(Resource, Default)]
struct Dyed(HashMap<(AssetId<CelMaterial>, Dye), Handle<CelMaterial>>);

/// Turns each mesh awaiting a dye to its colour, once it has its cel
/// material: the material, by the name its glTF gives it, takes what the
/// seed says of that name, and one the seed does not name keeps its own.
fn dye(
    mut commands: Commands,
    meshes: Query<(Entity, &Dye, &GltfMaterialName, &MeshMaterial3d<CelMaterial>)>,
    mut cel: ResMut<Assets<CelMaterial>>,
    mut dyed: ResMut<Dyed>,
) {
    for (entity, dye, name, material) in &meshes {
        let colour = common::kit::SETS.iter().find(|set| set.name == dye.set).and_then(|set| set.colour(dye.seed, name));
        let Some([r, g, b]) = colour else {
            commands.entity(entity).remove::<Dye>();
            continue;
        };
        let key = (material.id(), *dye);
        let handle = match dyed.0.get(&key) {
            Some(handle) => handle.clone(),
            None => {
                let Some(base) = cel.get(material.id()) else { continue };
                let mut painted = base.clone();
                painted.base.base_color = Color::linear_rgb(r, g, b);
                let handle = cel.add(painted);
                dyed.0.insert(key, handle.clone());
                handle
            }
        };
        commands.entity(entity).remove::<Dye>().insert(MeshMaterial3d(handle));
    }
}
