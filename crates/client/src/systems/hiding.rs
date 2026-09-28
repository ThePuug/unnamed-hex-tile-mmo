//! The faces of the body under a worn piece are not drawn. A piece's node
//! extras list `covers`, the triangles of the wearer's own mesh its
//! leather lies over, which go while it is worn; no piece hides any of
//! another. The build drops the same faces before it renders a proof
//! sheet, so the sheet shows what the client draws.

use bevy::{
    gltf::{GltfExtras, GltfPlugin},
    mesh::{Indices, MeshVertexAttribute, VertexFormat},
    prelude::*,
};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap, HashSet};

use common_bevy::components::{entity_type::EntityType, equipment::Item};

use crate::systems::{
    actor::actor_name,
    equipment::{rig, SocketAnchor, Worn},
};

/// The centre of the ring a vertex was cut at, zero where no ring made it.
pub const ATTRIBUTE_CENTRE: MeshVertexAttribute =
    MeshVertexAttribute::new("Centre", 0xC3_47_12_01, VertexFormat::Float32x3);
/// The point of the body a vertex stands off.
pub const ATTRIBUTE_SKIN: MeshVertexAttribute =
    MeshVertexAttribute::new("Skin", 0xC3_47_12_02, VertexFormat::Float32x3);

/// The glTF loader with the attributes a worn piece ships; it drops any it
/// is not told of. The glTF crate hands the loader a custom attribute's
/// name with its leading underscore stripped, so both spellings are
/// registered.
pub fn gltf_plugin() -> GltfPlugin {
    GltfPlugin::default()
        .add_custom_vertex_attribute("_CENTRE", ATTRIBUTE_CENTRE)
        .add_custom_vertex_attribute("CENTRE", ATTRIBUTE_CENTRE)
        .add_custom_vertex_attribute("_SKIN", ATTRIBUTE_SKIN)
        .add_custom_vertex_attribute("SKIN", ATTRIBUTE_SKIN)
}

/// The triangles of the wearer's mesh a piece's node lies over.
#[derive(Clone, Component, Debug)]
pub struct Covers(pub Vec<u32>);

#[derive(Deserialize)]
struct Extras {
    covers: Option<Vec<u32>>,
    socket: Option<String>,
}

fn triangles(mesh: &Mesh) -> Option<Vec<u32>> {
    Some(match mesh.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..mesh.count_vertices() as u32).collect(),
    })
}

fn with_indices(mesh: &Mesh, kept: Vec<u32>) -> Mesh {
    let mut out = mesh.clone();
    out.insert_indices(Indices::U32(kept));
    out
}

/// `mesh` with the triangles numbered in `faces` left out.
fn without_faces(mesh: &Mesh, faces: &BTreeSet<u32>) -> Option<Mesh> {
    let kept = triangles(mesh)?
        .chunks_exact(3)
        .enumerate()
        .filter(|(t, _)| !faces.contains(&(*t as u32)))
        .flat_map(|(_, tri)| tri.iter().copied())
        .collect();
    Some(with_indices(mesh, kept))
}

/// The actor's worn set changed: hiding is recomputed once every piece is
/// bound.
#[derive(Component)]
pub struct Redress;

/// The mesh a primitive shipped with, kept while a copy with faces hidden
/// is drawn in its place.
#[derive(Component)]
pub struct Shipped(Handle<Mesh>);

/// Copies with faces hidden, by the shipped mesh and the items that hide
/// them, shared by every actor wearing that combination.
#[derive(Default, Resource)]
pub struct HiddenMeshes(HashMap<(AssetId<Mesh>, Vec<Item>), Handle<Mesh>>);

/// Reads what each newly spawned node declares: what of the body it
/// covers, and the socket it hangs from.
pub fn parse_extras(
    mut commands: Commands,
    nodes: Query<(Entity, &GltfExtras), Added<GltfExtras>>,
) {
    for (node, extras) in &nodes {
        let Ok(parsed) = serde_json::from_str::<Extras>(&extras.value) else { continue };
        let mut node = commands.entity(node);
        if let Some(covers) = parsed.covers {
            node.insert(Covers(covers));
        }
        if let Some(socket) = parsed.socket {
            node.insert(SocketAnchor(socket));
        }
    }
}

type Primitives<'w, 's> = Query<'w, 's, (&'static mut Mesh3d, &'static mut Visibility, Option<&'static Shipped>)>;

/// Draws `primitive` from its shipped mesh, or from the copy `hidden` makes
/// of it, cached under `key`. A primitive with nothing left to draw is
/// hidden outright rather than drawn from an empty index buffer.
fn redraw(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    cache: &mut HiddenMeshes,
    primitives: &mut Primitives,
    primitive: Entity,
    hidden: Option<(&[Item], &dyn Fn(&Mesh) -> Option<Mesh>)>,
) {
    let Ok((mut mesh3d, mut visibility, shipped)) = primitives.get_mut(primitive) else { return };
    let shipped = match shipped {
        Some(s) => s.0.clone(),
        None => {
            commands.entity(primitive).insert(Shipped(mesh3d.0.clone()));
            mesh3d.0.clone()
        }
    };
    let Some((by, hide)) = hidden else {
        mesh3d.0 = shipped;
        *visibility = Visibility::Inherited;
        return;
    };
    let key = (shipped.id(), by.to_vec());
    let handle = match cache.0.get(&key) {
        Some(handle) => handle.clone(),
        None => {
            let Some(copy) = meshes.get(&shipped).and_then(hide) else { return };
            let handle = meshes.add(copy);
            cache.0.insert(key, handle.clone());
            handle
        }
    };
    let empty = meshes.get(&handle).and_then(|m| m.indices()).is_some_and(|i| i.is_empty());
    *visibility = if empty { Visibility::Hidden } else { Visibility::Inherited };
    mesh3d.0 = handle;
}

/// Redraws the actor's own mesh without the faces its pieces cover, once
/// all of them are bound.
pub fn hide_under(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<HiddenMeshes>,
    actors: Query<(Entity, &Children, &EntityType), With<Redress>>,
    worn: Query<&Worn>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    covers: Query<&Covers>,
    mut primitives: Primitives,
) {
    for (actor, kids, typ) in &actors {
        let pieces: Vec<(Entity, &Worn)> = kids.iter().filter_map(|c| worn.get(c).ok().map(|w| (c, w))).collect();
        if pieces.iter().any(|(_, w)| !w.is_bound()) {
            continue;
        }
        commands.entity(actor).remove::<Redress>();

        // The actor's own mesh, under its rig, loses the faces its pieces cover.
        let body = actor_name(*typ);
        let all: HashSet<Entity> = pieces.iter().map(|(e, _)| *e).collect();
        let Some(rig) = rig(actor, body, &children, &parents, &names, &all) else { continue };
        let covered: BTreeSet<u32> = pieces
            .iter()
            .flat_map(|&(piece, w)| w.nodes(piece, &children))
            .filter_map(|n| covers.get(n).ok())
            .flat_map(|c| c.0.iter().copied())
            .collect();
        let mut by: Vec<Item> = pieces.iter().map(|(_, w)| w.item).collect();
        by.sort();
        let hide = |m: &Mesh| without_faces(m, &covered);
        let hidden = (!covered.is_empty()).then_some((by.as_slice(), &hide as &dyn Fn(&Mesh) -> Option<Mesh>));
        let body_mesh = format!("{body}-mesh");
        let body_nodes: Vec<Entity> = children
            .iter_descendants(rig)
            .filter(|&n| names.get(n).is_ok_and(|x| x.as_str() == body_mesh))
            .collect();
        for node in body_nodes {
            for primitive in children.iter_descendants(node) {
                redraw(&mut commands, &mut meshes, &mut cache, &mut primitives, primitive, hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology};

    #[test]
    fn covered_triangles_go_by_their_number() {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]; 4]);
        mesh.insert_indices(Indices::U16(vec![0, 1, 2, 1, 2, 3, 2, 3, 0]));
        let gone = BTreeSet::from([1]);
        let out = without_faces(&mesh, &gone).expect("indexed mesh");
        assert_eq!(out.indices(), Some(&Indices::U32(vec![0, 1, 2, 2, 3, 0])));
    }

    #[test]
    fn covers_parse_from_node_extras() {
        let parsed: Extras = serde_json::from_str(r#"{"covers":[3,4],"socket":"waist","actor":"player"}"#).unwrap();
        assert_eq!(parsed.covers, Some(vec![3, 4]));
        assert_eq!(parsed.socket.as_deref(), Some("waist"));
    }


}
