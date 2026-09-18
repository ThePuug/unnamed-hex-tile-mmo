//! Faces under a worn piece are not drawn: the faces of the pieces under
//! it, and the faces of the body it lies over.
//!
//! A piece's node extras name the pieces it lies over and the regions of the
//! body it covers. A face of a named piece goes when each of its vertices
//! lies in one of those regions, `margin` in, judged at its `_CENTRE`, the
//! centre of the ring it was cut at, or at the vertex where no ring made it.
//! The same extras list `covers`, the triangles of the wearer's own mesh the
//! piece lies over, which go while it is worn. The build applies both before
//! it renders a proof sheet, so the sheet shows what the client draws.
//!
//! Regions and centres are in the build's own frame, z up with the front
//! along +y; a position read from the mesh is turned back into it before it
//! is judged.

use bevy::{
    gltf::{GltfExtras, GltfPlugin},
    mesh::{Indices, MeshVertexAttribute, VertexAttributeValues, VertexFormat},
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

/// A region of the body: behind every plane, a point with its outward
/// normal, and, given a run, at a station along that polyline within the
/// span.
#[derive(Clone, Debug)]
pub struct Region {
    planes: Vec<(Vec3, Vec3)>,
    run: Vec<Vec3>,
    span: Option<(f32, f32)>,
}

impl Region {
    /// The slab between `a` and `b`, square to the line between them.
    #[cfg(test)]
    pub fn slab(a: Vec3, b: Vec3) -> Self {
        let n = (b - a).normalize();
        Self { planes: vec![(a, -n), (b, n)], run: Vec::new(), span: None }
    }

    /// The slab, bounded along `run` to the stations in `span`.
    #[cfg(test)]
    pub fn along(self, run: Vec<Vec3>, span: (f32, f32)) -> Self {
        Self { run, span: Some(span), ..self }
    }

    pub fn contains(&self, p: Vec3, margin: f32) -> bool {
        if self.planes.iter().any(|&(c, n)| (p - c).dot(n) > margin) {
            return false;
        }
        match self.span {
            Some((s0, s1)) if !self.run.is_empty() => {
                let s = station(&self.run, p);
                s0 - margin <= s && s <= s1 + margin
            }
            _ => true,
        }
    }
}

/// Where `p` lies along `run` by length from its start: on the segment it
/// projects onto nearest, beyond either end by projection past it.
fn station(run: &[Vec3], p: Vec3) -> f32 {
    let mut best: Option<(f32, f32)> = None;
    let mut walked = 0.0;
    let last = run.len().saturating_sub(2);
    for (k, pair) in run.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let ab = b - a;
        let length = ab.length();
        let t = if length > 0.0 { (p - a).dot(ab) / (length * length) } else { 0.0 };
        let mut tt = t.clamp(0.0, 1.0);
        if (k == 0 && t < 0.0) || (k == last && t > 1.0) {
            tt = t;
        }
        let d = (p - (a + ab * tt)).length();
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, walked + tt * length));
        }
        walked += length;
    }
    best.map_or(0.0, |(_, s)| s)
}

/// What a piece's node declares it hides of other pieces.
#[derive(Clone, Component, Debug)]
pub struct Hides {
    pub over: Vec<String>,
    pub regions: Vec<Region>,
    pub margin: f32,
}

/// The triangles of the wearer's mesh a piece's node lies over.
#[derive(Clone, Component, Debug)]
pub struct Covers(pub Vec<u32>);

#[derive(Deserialize)]
struct Extras {
    hides: Option<HidesJson>,
    covers: Option<Vec<u32>>,
    socket: Option<String>,
}

#[derive(Deserialize)]
struct HidesJson {
    over: Vec<String>,
    regions: Vec<RegionJson>,
    margin: f32,
}

/// Flat lists: `planes` as point then normal per plane, `run` as points,
/// `span` as two stations or empty.
#[derive(Deserialize)]
struct RegionJson {
    planes: Vec<f32>,
    run: Vec<f32>,
    span: Vec<f32>,
}

fn vec3s(flat: &[f32]) -> Vec<Vec3> {
    flat.chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect()
}

impl Hides {
    fn from_json(h: HidesJson) -> Hides {
        let regions = h
            .regions
            .iter()
            .map(|r| {
                let points = vec3s(&r.planes);
                Region {
                    planes: points.chunks_exact(2).map(|c| (c[0], c[1])).collect(),
                    run: vec3s(&r.run),
                    span: (r.span.len() == 2).then(|| (r.span[0], r.span[1])),
                }
            })
            .collect();
        Hides { over: h.over, regions, margin: h.margin }
    }

    #[cfg(test)]
    pub fn parse(extras: &str) -> Option<Hides> {
        serde_json::from_str::<Extras>(extras).ok()?.hides.map(Hides::from_json)
    }
}

/// The build's frame from the mesh's: glTF's y up turned back to z up.
fn build_frame(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], -p[2], p[1])
}

/// The point each vertex is judged at.
pub fn judged(positions: &[[f32; 3]], centres: Option<&[[f32; 3]]>) -> Vec<Vec3> {
    positions
        .iter()
        .enumerate()
        .map(|(i, &p)| match centres.and_then(|c| c.get(i)) {
            Some(&c) if c != [0.0; 3] => Vec3::from(c),
            _ => build_frame(p),
        })
        .collect()
}

/// The triangles of `indices` still drawn under `hiders`: one goes when
/// some hider's regions hold every corner of it, that hider's margin in.
pub fn drawn(judged: &[Vec3], indices: &[u32], hiders: &[(&[Region], f32)]) -> Vec<u32> {
    let inside = |i: u32, regions: &[Region], margin: f32| {
        regions.iter().any(|r| r.contains(judged[i as usize], margin))
    };
    indices
        .chunks_exact(3)
        .filter(|tri| {
            !hiders
                .iter()
                .any(|&(regions, margin)| tri.iter().all(|&i| inside(i, regions, margin)))
        })
        .flatten()
        .copied()
        .collect()
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

/// `mesh` with the faces under `hiders` left out of its index buffer.
fn without_hidden_faces(mesh: &Mesh, hiders: &[(&[Region], f32)]) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let centres = match mesh.attribute(ATTRIBUTE_CENTRE) {
        Some(VertexAttributeValues::Float32x3(c)) => Some(c.as_slice()),
        _ => None,
    };
    let judged = judged(positions, centres);
    let kept = drawn(&judged, &triangles(mesh)?, hiders);
    Some(with_indices(mesh, kept))
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

/// Reads what each newly spawned node declares: what it hides, what of
/// the body it covers, and the socket it hangs from.
pub fn parse_extras(
    mut commands: Commands,
    nodes: Query<(Entity, &GltfExtras), Added<GltfExtras>>,
) {
    for (node, extras) in &nodes {
        let Ok(parsed) = serde_json::from_str::<Extras>(&extras.value) else { continue };
        let mut node = commands.entity(node);
        if let Some(hides) = parsed.hides {
            node.insert(Hides::from_json(hides));
        }
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

/// Redraws each piece an actor wears with the faces under its other pieces
/// left out, and the actor's own mesh without the faces its pieces cover,
/// once all of them are bound.
pub fn hide_under(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<HiddenMeshes>,
    actors: Query<(Entity, &Children, &EntityType), With<Redress>>,
    worn: Query<&Worn>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    hides: Query<&Hides>,
    covers: Query<&Covers>,
    mut primitives: Primitives,
) {
    for (actor, kids, typ) in &actors {
        let pieces: Vec<(Entity, &Worn)> = kids.iter().filter_map(|c| worn.get(c).ok().map(|w| (c, w))).collect();
        if pieces.iter().any(|(_, w)| !w.is_bound()) {
            continue;
        }
        commands.entity(actor).remove::<Redress>();

        let declared: Vec<(Item, Vec<&Hides>)> = pieces
            .iter()
            .map(|&(piece, w)| (w.item, w.nodes(piece, &children).into_iter().filter_map(|n| hides.get(n).ok()).collect()))
            .collect();

        for &(piece, w) in &pieces {
            let name = w.item.piece.name();
            let mut hiders: Vec<(Item, Vec<(&[Region], f32)>)> = declared
                .iter()
                .filter(|(item, _)| *item != w.item)
                .filter_map(|(item, declarations)| {
                    let over: Vec<(&[Region], f32)> = declarations
                        .iter()
                        .filter(|h| h.over.iter().any(|o| o == name))
                        .map(|h| (h.regions.as_slice(), h.margin))
                        .collect();
                    (!over.is_empty()).then_some((*item, over))
                })
                .collect();
            hiders.sort_by_key(|(item, _)| *item);
            let by: Vec<Item> = hiders.iter().map(|(item, _)| *item).collect();
            let tests: Vec<(&[Region], f32)> = hiders.iter().flat_map(|(_, over)| over.iter().copied()).collect();
            let hide = |m: &Mesh| without_hidden_faces(m, &tests);
            let hidden = (!tests.is_empty()).then_some((by.as_slice(), &hide as &dyn Fn(&Mesh) -> Option<Mesh>));
            for primitive in w.nodes(piece, &children) {
                redraw(&mut commands, &mut meshes, &mut cache, &mut primitives, primitive, hidden);
            }
        }

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

    const Z: f32 = 1.0;

    fn trunk_slab() -> Region {
        Region::slab(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, Z))
    }

    #[test]
    fn slab_holds_between_its_planes_and_a_margin_past_them() {
        let slab = trunk_slab();
        let margin = 0.05;
        assert!(slab.contains(Vec3::new(0.3, -0.2, 0.5), margin));
        assert!(slab.contains(Vec3::new(0.0, 0.0, Z + margin / 2.0), margin));
        assert!(!slab.contains(Vec3::new(0.0, 0.0, Z + margin * 2.0), margin));
        assert!(!slab.contains(Vec3::new(0.0, 0.0, -margin * 2.0), margin));
    }

    #[test]
    fn run_bounds_a_region_along_a_limb() {
        let run = vec![Vec3::new(0.1, 0.0, 0.0), Vec3::new(0.1, 0.0, 0.5), Vec3::new(0.1, 0.0, Z)];
        let region = trunk_slab().along(run, (0.0, 0.4));
        assert!(region.contains(Vec3::new(0.1, 0.0, 0.3), 0.0));
        assert!(!region.contains(Vec3::new(0.1, 0.0, 0.6), 0.0));
        assert!(region.contains(Vec3::new(0.1, 0.0, 0.45), 0.1));
    }

    #[test]
    fn a_triangle_goes_only_when_one_hider_holds_every_corner() {
        let low = Region::slab(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.4));
        let high = Region::slab(Vec3::new(0.0, 0.0, 0.6), Vec3::new(0.0, 0.0, Z));
        let judged = vec![
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.1, 0.0, 0.2),
            Vec3::new(0.0, 0.1, 0.3),
            Vec3::new(0.0, 0.0, 0.8),
        ];
        let inside_low = [0, 1, 2];
        let across = [0, 1, 3];
        let indices: Vec<u32> = [inside_low, across].concat();
        let lows = [low.clone()];
        let highs = [high.clone()];
        let one_hider: Vec<(&[Region], f32)> = vec![(&lows, 0.0)];
        assert_eq!(drawn(&judged, &indices, &one_hider), across.to_vec());
        let two_hiders: Vec<(&[Region], f32)> = vec![(&lows, 0.0), (&highs, 0.0)];
        assert_eq!(drawn(&judged, &indices, &two_hiders), across.to_vec());
        let both = [low, high];
        let one_hider_both_regions: Vec<(&[Region], f32)> = vec![(&both, 0.0)];
        assert!(drawn(&judged, &indices, &one_hider_both_regions).is_empty());
    }

    #[test]
    fn a_vertex_with_no_ring_is_judged_where_it_lies_in_the_build_frame() {
        let positions = [[0.1, 0.9, -0.2], [0.3, 0.7, 0.4]];
        let centres = [[0.0, 0.0, 0.0], [0.0, -0.05, 0.7]];
        let judged = judged(&positions, Some(&centres));
        assert_eq!(judged[0], Vec3::new(0.1, 0.2, 0.9));
        assert_eq!(judged[1], Vec3::new(0.0, -0.05, 0.7));
    }

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
    fn hides_parse_from_node_extras() {
        let extras = r#"{"hides":{"over":["leather-boots"],"regions":[{"planes":[0,0,1, 0,0,-1, 0,0,0, 0,0,1],"run":[0,0,0, 0,0,1],"span":[0.0,0.5]}],"margin":0.01},"covers":[3,4],"socket":"waist","actor":"player"}"#;
        let hides = Hides::parse(extras).expect("hides");
        assert_eq!(hides.over, vec!["leather-boots".to_string()]);
        assert_eq!(hides.regions.len(), 1);
        assert_eq!(hides.regions[0].planes.len(), 2);
        assert_eq!(hides.regions[0].run.len(), 2);
        assert_eq!(hides.regions[0].span, Some((0.0, 0.5)));
        let parsed: Extras = serde_json::from_str(extras).unwrap();
        assert_eq!(parsed.covers, Some(vec![3, 4]));
        assert_eq!(parsed.socket.as_deref(), Some("waist"));
        assert!(Hides::parse(r#"{"skin":true,"actor":"player"}"#).is_none());
    }
}
