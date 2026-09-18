//! Faces of a worn piece under another piece are not drawn.
//!
//! A piece's node extras name the pieces it lies over and the regions of the
//! body it covers. A face of a named piece goes when each of its vertices
//! lies in one of those regions, `margin` in, judged at its `_CENTRE`, the
//! centre of the ring it was cut at, or at the vertex where no ring made it.
//! The build applies the same test before it renders a proof sheet, so the
//! sheet shows what the client draws.
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
use std::collections::HashMap;

use common_bevy::components::equipment::Item;

use crate::systems::equipment::Worn;

/// The centre of the ring a vertex was cut at, zero where no ring made it.
pub const ATTRIBUTE_CENTRE: MeshVertexAttribute =
    MeshVertexAttribute::new("Centre", 0xC3_47_12_01, VertexFormat::Float32x3);
/// The point of the body a vertex stands off.
pub const ATTRIBUTE_SKIN: MeshVertexAttribute =
    MeshVertexAttribute::new("Skin", 0xC3_47_12_02, VertexFormat::Float32x3);

/// The glTF loader with the attributes a worn piece ships; it drops any it
/// is not told of.
pub fn gltf_plugin() -> GltfPlugin {
    GltfPlugin::default()
        .add_custom_vertex_attribute("_CENTRE", ATTRIBUTE_CENTRE)
        .add_custom_vertex_attribute("_SKIN", ATTRIBUTE_SKIN)
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

/// What a piece's node declares it hides.
#[derive(Clone, Component, Debug)]
pub struct Hides {
    pub over: Vec<String>,
    pub regions: Vec<Region>,
    pub margin: f32,
}

#[derive(Deserialize)]
struct Extras {
    hides: Option<HidesJson>,
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
    pub fn parse(extras: &str) -> Option<Hides> {
        let parsed: Extras = serde_json::from_str(extras).ok()?;
        let h = parsed.hides?;
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
        Some(Hides { over: h.over, regions, margin: h.margin })
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

/// `mesh` with the faces under `hiders` left out of its index buffer.
fn with_hidden_faces(mesh: &Mesh, hiders: &[(&[Region], f32)]) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let centres = match mesh.attribute(ATTRIBUTE_CENTRE) {
        Some(VertexAttributeValues::Float32x3(c)) => Some(c.as_slice()),
        _ => None,
    };
    let judged = judged(positions, centres);
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let kept = drawn(&judged, &indices, hiders);
    let mut out = mesh.clone();
    out.insert_indices(Indices::U32(kept));
    Some(out)
}

/// The actor's worn set changed: hiding is recomputed once every piece is
/// bound.
#[derive(Component)]
pub struct Redress;

/// The mesh a primitive shipped with, kept while a copy with faces hidden
/// is drawn in its place.
#[derive(Component)]
pub struct Shipped(Handle<Mesh>);

/// Copies with faces hidden, by the shipped mesh and the items over it,
/// shared by every actor wearing that combination.
#[derive(Default, Resource)]
pub struct HiddenMeshes(HashMap<(AssetId<Mesh>, Vec<Item>), Handle<Mesh>>);

/// Reads what each newly spawned node declares it hides.
pub fn parse_hides(
    mut commands: Commands,
    nodes: Query<(Entity, &GltfExtras), Added<GltfExtras>>,
) {
    for (node, extras) in &nodes {
        if let Some(hides) = Hides::parse(&extras.value) {
            commands.entity(node).insert(hides);
        }
    }
}

/// Redraws each piece an actor wears with the faces under its other pieces
/// left out, once all of them are bound.
pub fn hide_under(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<HiddenMeshes>,
    actors: Query<(Entity, &Children), With<Redress>>,
    worn: Query<&Worn>,
    children: Query<&Children>,
    hides: Query<&Hides>,
    mut primitives: Query<(&mut Mesh3d, &mut Visibility, Option<&Shipped>)>,
) {
    for (actor, kids) in &actors {
        let pieces: Vec<(Entity, &Worn)> = kids.iter().filter_map(|c| worn.get(c).ok().map(|w| (c, w))).collect();
        if pieces.iter().any(|(_, w)| !w.is_bound()) {
            continue;
        }
        commands.entity(actor).remove::<Redress>();

        let declared: Vec<(Item, Vec<&Hides>)> = pieces
            .iter()
            .map(|&(piece, w)| (w.item, children.iter_descendants(piece).filter_map(|d| hides.get(d).ok()).collect()))
            .collect();

        for &(under, w) in &pieces {
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
            let key_items: Vec<Item> = hiders.iter().map(|(item, _)| *item).collect();
            let tests: Vec<(&[Region], f32)> = hiders.iter().flat_map(|(_, over)| over.iter().copied()).collect();

            for primitive in children.iter_descendants(under) {
                let Ok((mut mesh3d, mut visibility, shipped)) = primitives.get_mut(primitive) else { continue };
                let shipped = match shipped {
                    Some(s) => s.0.clone(),
                    None => {
                        commands.entity(primitive).insert(Shipped(mesh3d.0.clone()));
                        mesh3d.0.clone()
                    }
                };
                if tests.is_empty() {
                    mesh3d.0 = shipped;
                    *visibility = Visibility::Inherited;
                    continue;
                }
                let key = (shipped.id(), key_items.clone());
                let handle = match cache.0.get(&key) {
                    Some(handle) => handle.clone(),
                    None => {
                        let Some(hidden) = meshes.get(&shipped).and_then(|m| with_hidden_faces(m, &tests)) else { continue };
                        let handle = meshes.add(hidden);
                        cache.0.insert(key, handle.clone());
                        handle
                    }
                };
                // A primitive with nothing left to draw is hidden outright
                // rather than drawn from an empty index buffer.
                let empty = meshes.get(&handle).and_then(|m| m.indices()).is_some_and(|i| i.is_empty());
                *visibility = if empty { Visibility::Hidden } else { Visibility::Inherited };
                mesh3d.0 = handle;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::components::equipment::Piece;

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
    fn hides_parse_from_node_extras() {
        let extras = r#"{"hides":{"over":["leather-boots"],"regions":[{"planes":[0,0,1, 0,0,-1, 0,0,0, 0,0,1],"run":[0,0,0, 0,0,1],"span":[0.0,0.5]}],"margin":0.01},"skin":true,"actor":"player"}"#;
        let hides = Hides::parse(extras).expect("hides");
        assert_eq!(hides.over, vec!["leather-boots".to_string()]);
        assert_eq!(hides.regions.len(), 1);
        assert_eq!(hides.regions[0].planes.len(), 2);
        assert_eq!(hides.regions[0].run.len(), 2);
        assert_eq!(hides.regions[0].span, Some((0.0, 0.5)));
        assert!(Hides::parse(r#"{"skin":true,"actor":"player"}"#).is_none());
        let _ = Piece::LeatherBoots;
    }
}
