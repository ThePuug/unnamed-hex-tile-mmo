//! The trees and the boulders: each tile's sites and boulder slots drawn
//! as instances of a kind's model, in
//! batches that are children of the mesh region their tiles lie in, so
//! they are evicted and re-based with the ground as the water is. A kind's
//! GLB carries its variations as its meshes; each is merged once into one
//! mesh coloured by its materials, and a region's trees of one variation
//! are one draw from one instance buffer, built with the region.

pub mod draw;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RecursiveDependencyLoadState;
use bevy::gltf::{Gltf, GltfMesh, GltfNode};
use bevy::prelude::*;
use bevy_mesh::{Indices, VertexAttributeValues};
use serde::Deserialize;
use common::{Content, SITES, TILE_SLOTS};
use common_bevy::geometry::{boulder_center, flat_top_tile_center, slot_center};
use common_bevy::surface::height_y;
use common_bevy::summary_mesh::MeshRegionKey;

/// What stands on the ground and is drawn: what grows at a site or the
/// stump a felled tree left there, a boulder in a slot, or a pile a player
/// left there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Pine,
    Deciduous,
    Brush,
    Boulder,
    PineStump,
    DeciduousStump,
    SoftwoodPile,
    HardwoodPile,
    SandstonePile,
    LimestonePile,
    BasementPile,
}

impl From<Content> for Kind {
    fn from(content: Content) -> Kind {
        match content {
            Content::Pine => Kind::Pine,
            Content::Deciduous => Kind::Deciduous,
            Content::PineStump => Kind::PineStump,
            Content::DeciduousStump => Kind::DeciduousStump,
            _ => Kind::Brush,
        }
    }
}

impl Kind {
    /// The pile `content` draws as on a tile of `rock`: its wood, or its
    /// stone in the rock's colour.
    fn pile(content: Content, rock: common::Rock) -> Option<Kind> {
        match content {
            Content::SoftwoodPile => Some(Kind::SoftwoodPile),
            Content::HardwoodPile => Some(Kind::HardwoodPile),
            Content::StonePile => Some(match rock {
                common::Rock::Limestone => Kind::LimestonePile,
                common::Rock::Basement => Kind::BasementPile,
                common::Rock::Sandstone | common::Rock::Shale => Kind::SandstonePile,
            }),
            _ => None,
        }
    }

    fn is_pile(self) -> bool {
        matches!(self, Kind::SoftwoodPile | Kind::HardwoodPile | Kind::SandstonePile | Kind::LimestonePile | Kind::BasementPile)
    }

    /// The tree a stump is the base of: it is built at that tree's scale,
    /// so it is drawn at the scale that tree was.
    fn felled(self) -> Option<Kind> {
        match self {
            Kind::PineStump => Some(Kind::Pine),
            Kind::DeciduousStump => Some(Kind::Deciduous),
            _ => None,
        }
    }
}

/// The model each kind is drawn with: one GLB for each tree and the
/// boulder, three for brush, each carrying its variations as its meshes.
const MODELS: &[(Kind, &str)] = &[
    (Kind::Pine, "models/pine-tree.glb"),
    (Kind::Deciduous, "models/deciduous-tree.glb"),
    (Kind::Brush, "models/scrub-mound.glb"),
    (Kind::Brush, "models/scrub-broom.glb"),
    (Kind::Brush, "models/scrub-sprawl.glb"),
    (Kind::Boulder, "models/boulder.glb"),
    (Kind::PineStump, "models/pine-stump.glb"),
    (Kind::DeciduousStump, "models/deciduous-stump.glb"),
    (Kind::SoftwoodPile, "models/log-pile-softwood.glb"),
    (Kind::HardwoodPile, "models/log-pile-hardwood.glb"),
    (Kind::SandstonePile, "models/stone-pile-sandstone.glb"),
    (Kind::LimestonePile, "models/stone-pile-limestone.glb"),
    (Kind::BasementPile, "models/stone-pile-basement.glb"),
];

/// The least a bush stands, as a share of its model.
pub const BRUSH_SMALL: f32 = 0.6;

/// How far from the camera a region's trees are drawn as models, in
/// world units, and the further reach they are kept to once drawn, so a
/// How far past the ring a region of tiles keeps its models: its own
/// half-width and a margin, so a region astride the ring carries them
/// and none flickers at its edge.
const RING_MARGIN: f32 = 40.0;

/// The six tiles around a tile, as coordinate offsets.
const NEIGHBOURS: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// The trees a tile stands, bushes aside.
pub fn trees_on(map: &common_bevy::resources::map::Map, q: i32, r: i32) -> usize {
    map.cover_at(q, r).filled().filter(|&(_, s)| s != Content::Brush).count()
}

/// Whether a tree stands on this tile or any of the six around it,
/// bushes aside: the camera closes in over the shoulder there, since a
/// boom at its length would stand among the crowns.
pub fn among_trees(map: &common_bevy::resources::map::Map, q: i32, r: i32) -> bool {
    trees_on(map, q, r) > 0 || NEIGHBOURS.iter().any(|&(dq, dr)| trees_on(map, q + dq, r + dr) > 0)
}

/// One variation of a kind: its mesh in the tree's own frame, foot at the
/// origin, up +y, the GLB's units the world's, and its height there; and
/// its model's cards, where the model shipped them, with which seed of
/// the model it is, the layer its pictures start at.
pub struct Variation {
    pub mesh: Handle<Mesh>,
    pub height: f32,
    /// Its footprint across, and the mean of its colour over its vertices:
    /// what the far ground wears for it.
    pub width: f32,
    pub color: Vec3,
    pub cards: Option<Arc<draw::Cards>>,
    pub seed: u32,
}

/// Every variation of every kind, and the quad every card is drawn on.
#[derive(Default)]
pub struct Kit {
    kinds: HashMap<Kind, Vec<Variation>>,
    quad: Handle<Mesh>,
}

/// What a model declares of its cards in its first node's extras, as
/// modelgen writes it: the texture array's path, and each view's
/// elevation in degrees and frame in world units, the side then the top.
#[derive(Deserialize)]
struct CardExtras {
    card: CardDecl,
}

#[derive(Deserialize)]
struct CardDecl {
    texture: String,
    views: Vec<CardViewDecl>,
}

#[derive(Deserialize)]
struct CardViewDecl {
    elevation: f32,
    width: f32,
    height: f32,
}

impl CardDecl {
    fn parse(extras: &str) -> Option<CardDecl> {
        serde_json::from_str::<CardExtras>(extras).ok().map(|e| e.card)
    }

    fn cards(&self, asset_server: &AssetServer) -> Option<draw::Cards> {
        let view = |d: &CardViewDecl| draw::CardView { width: d.width, height: d.height, elevation: d.elevation.to_radians() };
        let [side, top] = self.views.as_slice() else { return None };
        // Trilinear, so a card crossing a level does not step, and
        // clamped, since a picture has an edge and no repeat.
        let texture = asset_server
            .load_builder()
            .with_settings(|s: &mut bevy::image::ImageLoaderSettings| {
                s.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor::linear());
            })
            .load(self.texture.clone());
        Some(draw::Cards { texture, side: view(side), top: view(top) })
    }
}

/// The quad a card is drawn on: a unit wide about its foot and a unit
/// tall from it, the picture's top at its top.
fn card_quad() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::PrimitiveTopology;
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0], [0.5, 1.0, 0.0], [-0.5, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

impl Kit {
    pub fn of(&self, kind: Kind) -> &[Variation] {
        self.kinds.get(&kind).map_or(&[], |v| v.as_slice())
    }

    /// The scale variation `k` of `kind` stands at, `growth` of the way
    /// grown: a stump at the scale its tree's same variation stood.
    fn drawn_scale(&self, kind: Kind, k: usize, growth: f32) -> f32 {
        let (as_kind, variations) = match kind.felled() {
            Some(tree) if !self.of(tree).is_empty() => (tree, self.of(tree)),
            _ => (kind, self.of(kind)),
        };
        Kit::scale(as_kind, variations[k % variations.len()].height, growth)
    }

    /// The scale a tree `growth` of the way grown is drawn at: a tree from
    /// the sapling's height toward `common::cover::GROWN` times its model's,
    /// a bush from
    /// [`BRUSH_SMALL`] of its model toward the whole, a boulder from
    /// `common::cover::BOULDER_SMALL` toward `BOULDER_LARGE`.
    pub fn scale(kind: Kind, model_height: f32, growth: f32) -> f32 {
        let growth = growth.clamp(0.0, 1.0);
        if kind.is_pile() {
            return 1.0;
        }
        match kind {
            Kind::Brush => return BRUSH_SMALL + (1.0 - BRUSH_SMALL) * growth,
            Kind::Boulder => return common::cover::boulder_scale(growth),
            _ => {}
        }
        common::cover::tree_scale(model_height, growth)
    }
}

/// The kit, present once every model has loaded.
#[derive(Resource)]
pub struct TreeKit {
    pub kit: Arc<Kit>,
}

#[derive(Resource)]
struct Loading(Vec<(Kind, Handle<Gltf>)>);

/// A tree or a boulder to draw: where it stands from its region's origin,
/// its turn, how far it has grown, and which model.
#[derive(Clone, Copy, Debug)]
pub struct TreeInstance {
    pub translation: Vec3,
    pub yaw: f32,
    pub growth: f32,
    pub kind: Kind,
    pub variation: u32,
    /// Whether it stands for a crag past the tiles, drawn in the far band.
    pub far: bool,
}

/// What the wood costs to draw: the stands drawing and the trees in
/// them, models and cards apart. A stand is one call a pass however many
/// regions its trees stand in, so the two numbers say which of the two a
/// frame is paying for.
#[derive(Resource, Default)]
pub struct ForestDraws {
    pub models: u32,
    pub model_trees: u32,
    pub cards: u32,
    pub card_trees: u32,
}

fn count_draws(mut draws: ResMut<ForestDraws>, trees: Query<&draw::TreeStand>, cards: Query<&draw::CardStand>) {
    use draw::Stand;
    *draws = ForestDraws::default();
    let standing = |stand: &draw::StandDraw| stand.ranges.iter().map(|r| r.count).sum::<u32>();
    for stand in trees.iter().filter(|s| s.draw().buffer.is_some()) {
        draws.models += 1;
        draws.model_trees += standing(stand.draw());
    }
    for stand in cards.iter().filter(|s| s.draw().buffer.is_some()) {
        draws.cards += 1;
        draws.card_trees += standing(stand.draw());
    }
}

pub struct ForestPlugin;

impl Plugin for ForestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(draw::TreeDrawPlugin);
        app.init_resource::<ForestDraws>();
        app.add_systems(Startup, begin_loading);
        app.add_systems(Update, (count_draws, draw::update_stands));
        app.add_systems(Update, load_kit.run_if(resource_exists::<Loading>));
        app.add_systems(Update, dress_far_ground.run_if(resource_added::<TreeKit>));
        app.add_systems(
            Update,
            update_trees
                .run_if(resource_exists::<TreeKit>)
                .after(crate::systems::world::poll_summary_meshes),
        );
    }
}

/// The growth a crown on the far ground stands for: a tree well along,
/// as most are.
const CROWN_GROWTH: f32 = 0.75;

/// Once the kit is up, the far ground wears each kind as its models
/// look: the mean of their colour over every vertex, and the width a
/// grown crown of the kind's first variation stands, so the canopy past
/// the cards is the colour of the trees before them. A kind with no
/// model is colourless and never drawn there.
fn dress_far_ground(
    kit: Res<TreeKit>,
    mut terrain_material: ResMut<crate::resources::TerrainMaterial>,
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
) {
    use crate::resources::KindLook;
    let of = |kind: Kind| -> KindLook {
        let variations = kit.kit.of(kind);
        let Some(first) = variations.first() else { return KindLook { color: Vec3::ZERO, width: 0.0, height: 0.0 } };
        let color = variations.iter().map(|v| v.color).sum::<Vec3>() / variations.len() as f32;
        let scale = Kit::scale(kind, first.height, CROWN_GROWTH);
        KindLook { color, width: first.width * scale, height: first.height * scale }
    };
    terrain_material.set_kinds([of(Kind::Pine), of(Kind::Deciduous), of(Kind::Brush)], &mut materials);
}

fn begin_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = MODELS.iter().map(|(kind, path)| (*kind, asset_server.load::<Gltf>(*path))).collect();
    commands.insert_resource(Loading(handles));
}

/// Once every model and everything under it has loaded, or failed, merge
/// each variation's primitives into one mesh and publish the kit. A model
/// that failed leaves its kind with no mesh and a warning; regions built
/// before the kit is ready get their trees when it is.
fn load_kit(
    mut commands: Commands,
    loading: Res<Loading>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<bevy::gltf::GltfMaterial>>,
) {
    for (_, handle) in &loading.0 {
        match asset_server.recursive_dependency_load_state(handle.id()) {
            RecursiveDependencyLoadState::Loaded | RecursiveDependencyLoadState::Failed(_) => {}
            _ => return,
        }
    }
    let mut kit = Kit { quad: meshes.add(card_quad()), ..default() };
    for (kind, handle) in &loading.0 {
        let Some(gltf) = gltfs.get(handle) else {
            warn!("model for {kind:?} did not load; its kind is drawn as nothing");
            continue;
        };
        // The model's cards, declared on its first node; a model without
        // them is drawn as nothing past the trees' reach.
        let cards = gltf
            .nodes
            .first()
            .and_then(|n| gltf_nodes.get(n))
            .and_then(|n| n.extras.as_ref())
            .and_then(|e| CardDecl::parse(&e.value))
            .and_then(|d| d.cards(&asset_server))
            .map(Arc::new);
        if cards.is_none() {
            info!("model for {kind:?} ships no cards; nothing of it stands past the trees' reach");
        }
        for (seed, mesh_handle) in gltf.meshes.iter().enumerate() {
            let Some(gm) = gltf_meshes.get(mesh_handle) else { continue };
            let merged = merge(gm, &meshes, &materials);
            let (height, width) = match merged.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => {
                    let across = |i: usize| p.iter().map(|v| v[i]).fold(f32::MIN, f32::max) - p.iter().map(|v| v[i]).fold(f32::MAX, f32::min);
                    (p.iter().map(|v| v[1]).fold(0.0, f32::max), across(0).max(across(2)))
                }
                _ => (0.0, 0.0),
            };
            let color = surface_color(&merged);
            let variation = Variation { mesh: meshes.add(merged), height, width, color, cards: cards.clone(), seed: seed as u32 };
            kit.kinds.entry(*kind).or_default().push(variation);
        }
    }
    info!(
        "tree kit: {} pine, {} deciduous, {} brush, {} boulder, {} stump, {} pile variations",
        kit.of(Kind::Pine).len(),
        kit.of(Kind::Deciduous).len(),
        kit.of(Kind::Brush).len(),
        kit.of(Kind::Boulder).len(),
        kit.of(Kind::PineStump).len() + kit.of(Kind::DeciduousStump).len(),
        kit.kinds.iter().filter(|(k, _)| k.is_pile()).map(|(_, v)| v.len()).sum::<usize>()
    );
    commands.insert_resource(TreeKit { kit: Arc::new(kit) });
    commands.remove_resource::<Loading>();
}

/// What a stand of this model reads as from far off: the mean of its own
/// colour over its surface, weighted by area. A mean over the vertices
/// counts a trunk's rings as heavily as a crown's faces, and comes out
/// the bark's colour as much as the leaves'.
fn surface_color(mesh: &Mesh) -> Vec3 {
    let (Some(VertexAttributeValues::Float32x3(p)), Some(VertexAttributeValues::Float32x4(c)), Some(Indices::U32(idx))) =
        (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.attribute(Mesh::ATTRIBUTE_COLOR), mesh.indices())
    else {
        return Vec3::ZERO;
    };
    let mut sum = Vec3::ZERO;
    let mut area = 0.0f32;
    for face in idx.chunks_exact(3) {
        let [a, b, d] = [face[0] as usize, face[1] as usize, face[2] as usize];
        let (pa, pb, pd) = (Vec3::from(p[a]), Vec3::from(p[b]), Vec3::from(p[d]));
        let face_area = (pb - pa).cross(pd - pa).length() * 0.5;
        let face_color = [a, b, d].iter().map(|&i| Vec3::new(c[i][0], c[i][1], c[i][2])).sum::<Vec3>() / 3.0;
        sum += face_color * face_area;
        area += face_area;
    }
    if area > 0.0 {
        sum / area
    } else {
        Vec3::ZERO
    }
}

/// One variation's primitives as one mesh, each vertex coloured by its
/// primitive's material, so one white material draws the whole tree.
fn merge(gm: &GltfMesh, meshes: &Assets<Mesh>, materials: &Assets<bevy::gltf::GltfMaterial>) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::PrimitiveTopology;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for prim in &gm.primitives {
        let Some(mesh) = meshes.get(&prim.mesh) else { continue };
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
        let color = prim
            .material
            .as_ref()
            .and_then(|m| materials.get(m))
            .map_or(LinearRgba::WHITE, |m| m.base_color.to_linear());
        let base = positions.len() as u32;
        positions.extend_from_slice(p);
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(n)) => normals.extend_from_slice(n),
            _ => normals.extend(p.iter().map(|_| [0.0, 1.0, 0.0])),
        }
        colors.extend(p.iter().map(|_| [color.red, color.green, color.blue, 1.0]));
        match mesh.indices() {
            Some(Indices::U16(v)) => indices.extend(v.iter().map(|&i| base + i as u32)),
            Some(Indices::U32(v)) => indices.extend(v.iter().map(|&i| base + i)),
            None => indices.extend((0..p.len() as u32).map(|i| base + i)),
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

/// The trees and boulders of a mesh region at level `radius`, from the
/// map's covers: one instance per filled site and boulder slot of every
/// tile the region's cells cover, standing on the level's drawn surface
/// at its slot, from the region's origin. The same covers at every level the map reaches, so a tree is
/// the same tree on both sides of a band edge and the edge hands one
/// drawing of the wood to the other. Reads the map's covers and the
/// level's heights and nothing else, so it runs where the ground is
/// built; a tile not yet streamed in stands nothing, and a tree at the
/// rim, where a corner's cell is not there yet, stands at its cell's
/// own height.
pub fn place_trees(
    radius: u32,
    region_key: MeshRegionKey,
    mesh_origin: Vec3,
    map: &common_bevy::resources::map::Map,
    height: &dyn Fn(i32, i32) -> Option<i32>,
) -> Vec<TreeInstance> {
    let lattice = common_bevy::summary::summary_lattice(radius);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let mut surface = common_bevy::summary_mesh::LevelSurface::new(radius, height);
    let mut out = Vec::new();
    for cell in region_lat.tiles_in_cell((region_key.mn, region_key.mm)) {
        let Some(cell_z) = height(cell.0, cell.1) else { continue };
        for (q, r) in lattice.tiles_covered(cell) {
            let cover = map.cover_at(q, r);
            if cover.is_empty() {
                continue;
            }
            for (k, slot) in cover.filled().chain(cover.stumps()) {
                let sway = common::sway(q, r, k);
                let (x, z) = slot_center(q, r, k, &sway);
                let y = surface.at(Vec2::new(x, z)).map_or(height_y(cell_z as f32), |(y, _)| y);
                out.push(TreeInstance {
                    translation: Vec3::new(x, y, z) - mesh_origin,
                    yaw: sway.yaw as f32,
                    growth: common::cover::tree_growth(cover, q, r, k),
                    kind: slot.into(),
                    variation: sway.variation,
                    far: false,
                });
            }
            // A pile lies in its slot as its model stands, turned and placed
            // by the slot's own sway as a boulder's.
            for k in 0..TILE_SLOTS as usize {
                let Some(kind) = Kind::pile(cover.content(k), cover.rock()) else { continue };
                let sway = common::boulder_sway(q, r, k);
                let (x, z) = boulder_center(q, r, k, &sway);
                let y = surface.at(Vec2::new(x, z)).map_or(height_y(cell_z as f32), |(y, _)| y);
                out.push(TreeInstance {
                    translation: Vec3::new(x, y, z) - mesh_origin,
                    yaw: sway.yaw as f32,
                    growth: 1.0,
                    kind,
                    variation: sway.variation,
                    far: false,
                });
            }
            for k in cover.boulders() {
                let sway = common::boulder_sway(q, r, k);
                let (x, z) = boulder_center(q, r, k, &sway);
                let y = surface.at(Vec2::new(x, z)).map_or(height_y(cell_z as f32), |(y, _)| y);
                out.push(TreeInstance {
                    translation: Vec3::new(x, y, z) - mesh_origin,
                    yaw: sway.yaw as f32,
                    growth: common::cover::boulder_growth(cover, q, r, k),
                    kind: Kind::Boulder,
                    variation: sway.variation,
                    far: false,
                });
            }
        }
    }
    out
}

/// How much larger a far crag's blocks stand, and how much wider its slots
/// lie, than a tile's: a summary past the tiles is one tile of rock seen
/// from far off, a few blocks a landmark's size where the tiles would
/// stand a hundred stones too small to see.
pub const FAR_BLOCK: f32 = 3.0;

/// The crags a mesh region at level `radius` stands past the tiles, from
/// its summaries' outcrops: each summary a tile [`FAR_BLOCK`] times the
/// size, its seven slots each holding a block by its own draw against the
/// square root of the share of rock the summary read, so a thin reading
/// still stands a few, and the centre always one. Each stands in the far
/// band.
pub fn place_crags(
    radius: u32,
    region_key: MeshRegionKey,
    mesh_origin: Vec3,
    outcrop: &dyn Fn(i32, i32) -> Option<common::Outcrop>,
    height: &dyn Fn(i32, i32) -> Option<i32>,
) -> Vec<TreeInstance> {
    let lattice = common_bevy::summary::summary_lattice(radius);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let mut surface = common_bevy::summary_mesh::LevelSurface::new(radius, height);
    let mut out = Vec::new();
    for cell in region_lat.tiles_in_cell((region_key.mn, region_key.mm)) {
        let Some(rock) = outcrop(cell.0, cell.1).filter(|o| !o.is_empty()) else { continue };
        let Some(cell_z) = height(cell.0, cell.1) else { continue };
        let share = rock.density();
        let (sq, sr) = cell;
        let (cq, cr) = lattice.cell_center(cell);
        let (cx, cz) = flat_top_tile_center(cq, cr, 1.0);
        for k in 0..TILE_SLOTS as usize {
            if k != 0 && common::boulder_draw(sq, sr, k) >= share.sqrt() {
                continue;
            }
            let sway = common::boulder_sway(sq, sr, k);
            let (dx, dz) = boulder_center(0, 0, k, &sway);
            let (x, z) = (cx + dx * FAR_BLOCK, cz + dz * FAR_BLOCK);
            let y = surface.at(Vec2::new(x, z)).map_or(height_y(cell_z as f32), |(y, _)| y);
            out.push(TreeInstance {
                translation: Vec3::new(x, y, z) - mesh_origin,
                yaw: sway.yaw as f32,
                growth: sway.growth as f32 * share.sqrt() as f32,
                kind: Kind::Boulder,
                variation: sway.variation,
                far: true,
            });
        }
    }
    out
}

/// Stand a region's trees as models: its wood, one part per variation
/// present, each the variation's mesh and every tree drawn with it. Trees
/// cast no shadow: the cascades would draw every tree four times over for
/// it.
pub fn spawn_trees(commands: &mut Commands, entity: Entity, trees: &[TreeInstance], kit: &TreeKit) {
    let mut parts: HashMap<(Kind, usize), Vec<draw::Instance>> = HashMap::new();
    let mut reach = 0.0f32;
    for t in trees {
        let all = kit.kit.of(t.kind);
        if all.is_empty() {
            continue;
        }
        let k = t.variation as usize % all.len();
        let v = &all[k];
        let scale = kit.kit.drawn_scale(t.kind, k, t.growth);
        reach = reach.max(v.height * scale);
        parts.entry((t.kind, k)).or_default().push(draw::Instance::new(t.translation, t.yaw, scale));
    }
    let parts = parts
        .into_iter()
        .map(|((kind, k), instances)| draw::WoodPart {
            key: draw::StandKey::Model(kind, k),
            mesh: kit.kit.of(kind)[k].mesh.clone(),
            cards: None,
            instances,
        })
        .collect();
    commands.entity(entity).insert(draw::RegionWood::new(parts, reach));
}

/// Stand a region's trees as cards: its wood, one part per model whose
/// cards stand here, each the shared quad and every tree drawn from that
/// model's pictures. A model's variations differ only by which layer of
/// its texture an instance names, so they go in together. The cards stand
/// past the ring, far enough that their overlaps do not show, so they
/// keep the quad's own depth.
pub fn spawn_cards(commands: &mut Commands, entity: Entity, trees: &[TreeInstance], kit: &TreeKit) {
    let mut parts: HashMap<AssetId<Image>, (Arc<draw::Cards>, Vec<draw::Instance>)> = HashMap::new();
    let mut reach = 0.0f32;
    for t in trees {
        let all = kit.kit.of(t.kind);
        if all.is_empty() {
            continue;
        }
        let k = t.variation as usize % all.len();
        let v = &all[k];
        let Some(cards) = &v.cards else { continue };
        let scale = kit.kit.drawn_scale(t.kind, k, t.growth) * if t.far { FAR_BLOCK } else { 1.0 };
        reach = reach.max(cards.side.height.max(cards.side.width) * scale);
        parts
            .entry(cards.texture.id())
            .or_insert_with(|| (cards.clone(), Vec::new()))
            .1
            .push(draw::Instance::card(t.translation, t.yaw, scale, v.seed * draw::CARD_LAYERS, v.height * scale, t.far));
    }
    let parts = parts
        .into_iter()
        .map(|(texture, (cards, instances))| draw::WoodPart {
            key: draw::StandKey::Cards(texture),
            mesh: kit.kit.quad.clone(),
            cards: Some(cards),
            instances,
        })
        .collect();
    commands.entity(entity).insert(draw::RegionWood::new(parts, reach));
}

/// Stand the trees of every region within reach of the camera as models
/// and those beyond the keep as cards, and take each down where the
/// other's ground begins: the region's origin is its measure, and a
/// region built before the kit loaded gets its trees here too.
#[allow(clippy::too_many_arguments)]
fn update_trees(
    mut commands: Commands,
    kit: Res<TreeKit>,
    mut summary_meshes: ResMut<crate::resources::SummaryMeshes>,
    origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, (With<common_bevy::components::behaviour::PlayerControlled>, With<common_bevy::components::Actor>)>,
    band: Res<crate::resources::CardBand>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    #[cfg(feature = "admin")]
    let camera = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.world_position)
        .or_else(|| player_query.single().ok().map(|t| origin.world(t.translation)));
    #[cfg(not(feature = "admin"))]
    let camera = player_query.single().ok().map(|t| origin.world(t.translation));
    let Some(camera) = camera else { return };
    // The ring, or where it will be once the cuts are set.
    let ring = if band.inner > 0.0 { band.inner } else { common_bevy::summary::threshold_horiz(0) };

    for (key, state) in summary_meshes.states.iter_mut() {
        let Some(entity) = state.entity else { continue };
        if state.base_trees.is_empty() {
            continue;
        }
        // The tiles' trees are models and the summaries' are cards; the
        // ring between the two levels is where one dithers out and the
        // other in, which is the shaders' to do. A region of tiles keeps
        // its models while it can reach the ring, and a summary's cards
        // stand from the moment its ground does.
        if key.r == 0 {
            let d = state.mesh_origin.xz().distance(camera.xz());
            if !state.trees_spawned && d <= ring + RING_MARGIN {
                spawn_trees(&mut commands, entity, &state.base_trees, &kit);
                state.trees_spawned = true;
            } else if state.trees_spawned && d > ring + 2.0 * RING_MARGIN {
                commands.entity(entity).remove::<draw::RegionWood>();
                state.trees_spawned = false;
            }
        } else if !state.cards_spawned {
            spawn_cards(&mut commands, entity, &state.base_trees, &kit);
            state.cards_spawned = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Cover;
    use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
    use common_bevy::geometry::flat_top_tile_center;
    use common_bevy::resources::map::Map;
    use qrz::{Convert, Qrz};

    /// On flat wooded, rocky ground a region places one tree per filled
    /// site and one boulder per boulder slot, each standing on the surface
    /// within its own tile.
    #[test]
    fn a_region_places_one_instance_per_filled_slot() {
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let region_lat = common_bevy::summary::mesh_region_lattice();
        let lattice = common_bevy::summary::summary_lattice(0);
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        let mut expected = 0usize;
        for cell in region_lat.tiles_in_cell((0, 0)) {
            let (q, r) = lattice.cell_center(cell);
            let n = ((q - 3 * r).rem_euclid(8)) as usize;
            let mut cover = Cover::NONE;
            for k in 0..n.min(SITES.len()) {
                cover = cover.with(k, if k % 2 == 0 { Content::Pine } else { Content::Brush });
            }
            if n >= 4 {
                cover = cover.with_boulder(0).with_boulder(n - 2);
            }
            expected += cover.filled().count() + cover.boulders().count();
            map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
        let (oq, or) = lattice.cell_center(region_lat.cell_center((0, 0)));
        let (ox, oz) = flat_top_tile_center(oq, or, 1.0);
        let origin = Vec3::new(ox, 0.0, oz);
        let trees = place_trees(0, key, origin, &map, &|q, r| map.get_by_qr(q, r).map(|(qrz, _)| qrz.z));
        assert_eq!(trees.len(), expected);
        for t in &trees {
            assert!((t.translation.y - height_y(0.0)).abs() < 1e-4, "a tree off the ground at {:?}", t.translation);
            assert!((0.0..=1.0).contains(&t.growth));
            let world = t.translation + origin;
            let here: Qrz = map.convert(world);
            let (cx, cz) = flat_top_tile_center(here.q, here.r, 1.0);
            let own = (world.x - cx).hypot(world.z - cz);
            assert!(own < 0.87, "a tree {own} from its tile's centre, past the edge");
        }
    }

    /// A felled tree stands as its stump where it stood, turned as it was
    /// and as grown, and the tree beside it keeps its size.
    #[test]
    fn a_felled_tree_stands_as_its_stump() {
        let place = |cover: Cover| {
            let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
            map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
            let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
            place_trees(0, key, Vec3::ZERO, &map, &|q, r| map.get_by_qr(q, r).map(|(qrz, _)| qrz.z))
        };
        let wood = Cover::NONE.with(0, Content::Pine).with(1, Content::Deciduous);
        let felled = common::gathering::harvest(wood, common::SITE_SLOTS[0][0]).unwrap().cover;
        let (before, after) = (place(wood), place(felled));
        let find = |trees: &[TreeInstance], kind: Kind| *trees.iter().find(|t| t.kind == kind).unwrap();
        let (tree, stump) = (find(&before, Kind::Pine), find(&after, Kind::PineStump));
        assert_eq!((stump.translation, stump.yaw, stump.growth, stump.variation), (tree.translation, tree.yaw, tree.growth, tree.variation));
        let (standing, still) = (find(&before, Kind::Deciduous), find(&after, Kind::Deciduous));
        assert_eq!(standing.growth, still.growth);
        assert!(after.iter().all(|t| t.kind != Kind::Pine));
    }

    /// A pile lies where it was left, as its wood or as stone in the
    /// tile's rock.
    #[test]
    fn a_pile_lies_as_its_kind() {
        let place = |cover: Cover| {
            let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
            map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
            let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
            place_trees(0, key, Vec3::ZERO, &map, &|q, r| map.get_by_qr(q, r).map(|(qrz, _)| qrz.z))
        };
        let crag = Cover::NONE.with_boulder(3).with_boulder(4).with_rock(common::Rock::Limestone);
        let mined = common::gathering::harvest(crag, 4).unwrap();
        let piled = common::gathering::left(mined.cover, mined.freed, mined.material);
        let trees = place(piled);
        assert_eq!(trees.iter().filter(|t| t.kind == Kind::LimestonePile).count(), 1);
        assert_eq!(trees.iter().filter(|t| t.kind == Kind::Boulder).count(), 1);
    }
}

/// A crag past the tiles stands boulders only in the summaries that read
/// rock, all in the far band, and more of them where more was read.
#[cfg(test)]
mod crag_tests {
    use super::*;
    use common::{Cover, Outcrop, Rock};

    fn crags(boulders: usize) -> Vec<TreeInstance> {
        let radius = common_bevy::summary::LOD_LEVELS[2];
        let region_lat = common_bevy::summary::mesh_region_lattice();
        let cells: Vec<(i32, i32)> = region_lat.tiles_in_cell((0, 0)).collect();
        let rocky = cells[cells.len() / 2];
        let face = (0..boulders).fold(Cover::NONE, |c, k| c.with_boulder(k)).with_rock(Rock::Basement);
        let outcrop = move |sq: i32, sr: i32| Some(if (sq, sr) == rocky { Outcrop::of(&[face; 7]) } else { Outcrop::NONE });
        let key = MeshRegionKey { r: radius, mn: 0, mm: 0 };
        place_crags(radius, key, Vec3::ZERO, &outcrop, &|_, _| Some(0))
    }

    #[test]
    fn a_crag_stands_where_its_summary_read_rock() {
        assert!(crags(0).is_empty());
        let (few, many) = (crags(1), crags(7));
        assert!(!few.is_empty() && many.len() >= few.len());
        assert_eq!(many.len(), TILE_SLOTS as usize, "a summary all rock stands every slot");
        assert!(many.iter().all(|t| t.far && t.kind == Kind::Boulder));
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    /// A tree's scale runs from a sapling's height to the grown height and
    /// never below the sapling; a bush from its small share to its model.
    #[test]
    fn growth_runs_from_a_sapling_to_grown() {
        let model = 5.0;
        assert!((Kit::scale(Kind::Pine, model, 0.0) * model - common::cover::SAPLING_HEIGHT).abs() < 1e-5);
        assert!((Kit::scale(Kind::Pine, model, 1.0) * model - model * common::cover::GROWN).abs() < 1e-5);
        let mut last = 0.0;
        for i in 0..=10 {
            let s = Kit::scale(Kind::Deciduous, 3.0, i as f32 / 10.0);
            assert!(s >= last);
            last = s;
        }
        assert!((Kit::scale(Kind::Brush, 1.0, 0.0) - BRUSH_SMALL).abs() < 1e-6);
        assert!((Kit::scale(Kind::Brush, 1.0, 1.0) - 1.0).abs() < 1e-6);
    }
}

/// The shared table of tree forms stays what the models are: a model rebuilt
/// taller or thicker fails here before collision and drawing part.
#[cfg(test)]
mod form_tests {
    fn positions(model: &str) -> Vec<Vec<([f32; 3], [f32; 3])>> {
        let path = format!("{}/../../assets/models/{model}.glb", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + len]).unwrap();
        let bound = |a: &serde_json::Value, k: &str| -> [f32; 3] {
            let v: Vec<f32> = a[k].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
            [v[0], v[1], v[2]]
        };
        json["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mesh| {
                mesh["primitives"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        let a = &json["accessors"][p["attributes"]["POSITION"].as_u64().unwrap() as usize];
                        (bound(a, "min"), bound(a, "max"))
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn tree_forms_are_the_models() {
        for (tree, stump, forms) in [
            ("pine-tree", "pine-stump", common::cover::PINE_FORMS),
            ("deciduous-tree", "deciduous-stump", common::cover::DECIDUOUS_FORMS),
        ] {
            let trees = positions(tree);
            let stumps = positions(stump);
            assert_eq!(trees.len(), forms.len(), "{tree}'s variations");
            for (k, form) in forms.iter().enumerate() {
                let height = trees[k].iter().map(|(_, max)| max[1]).fold(0.0, f32::max);
                let (min, max) = stumps[k][0];
                let trunk = [min[0].abs(), max[0].abs(), min[2].abs(), max[2].abs()].into_iter().fold(0.0, f32::max);
                let stump_height = stumps[k].iter().map(|(_, max)| max[1]).fold(0.0, f32::max);
                assert!((height - form.height).abs() < 1e-3, "{tree} {k}: height {height} against {}", form.height);
                assert!((trunk - form.trunk).abs() < 1e-3, "{stump} {k}: trunk {trunk} against {}", form.trunk);
                assert!((stump_height - form.stump).abs() < 1e-3, "{stump} {k}: height {stump_height} against {}", form.stump);
            }
        }
    }
}

/// The reach each gather stands the player at stays what the player's clips
/// declare.
#[cfg(test)]
mod reach_tests {
    #[test]
    fn reaches_are_the_clips() {
        let path = format!("{}/../../assets/actors/player-basic.glb", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + len]).unwrap();
        let declared = json["nodes"].as_array().unwrap().iter().find_map(|n| n["extras"].get("animgen")).expect("animgen extras");
        for (clip, reach) in [
            ("chop", common::gathering::CHOP_REACH),
            ("mine", common::gathering::MINE_REACH),
            ("pickup", common::gathering::PICKUP_REACH),
        ] {
            let declared = declared[clip]["reach"].as_f64().unwrap_or_else(|| panic!("{clip} declares no reach")) as f32;
            assert!((declared - reach).abs() < 1e-3, "{clip}: declared {declared} against {reach}");
        }
    }
}
