//! The trees: each tile's slots drawn as instances of a kind's model, in
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
use bevy::render::renderer::RenderDevice;
use bevy_mesh::{Indices, VertexAttributeValues};
use serde::Deserialize;
use common::{Slot, SLOTS};
use common_bevy::geometry::{flat_top_tile_center, slot_center};
use common_bevy::surface::{height_y, surface_y};
use common_bevy::summary_mesh::MeshRegionKey;

/// The model each kind of slot is drawn with: one GLB for each tree, three
/// for scrub, each carrying its variations as its meshes.
const MODELS: &[(Slot, &str)] = &[
    (Slot::Pine, "models/pine-tree.glb"),
    (Slot::Deciduous, "models/deciduous-tree.glb"),
    (Slot::Scrub, "models/scrub-mound.glb"),
    (Slot::Scrub, "models/scrub-broom.glb"),
    (Slot::Scrub, "models/scrub-sprawl.glb"),
];

/// How far a tree on a tile with one slot filled has grown, as a share of
/// what its own growth gives it: a stand tapers at its edge, and a sparse
/// tile is an edge.
pub const EDGE_GROWTH: f32 = 0.5;

/// A sapling's height in world units, the least a tree stands: a player's.
pub const SAPLING_HEIGHT: f32 = 2.0;

/// A grown tree's height as a multiple of its model's: the model is built
/// to fit a tile, and the tree grows past it.
pub const GROWN: f32 = 2.5;

/// The least a bush stands, as a share of its model.
pub const SCRUB_SMALL: f32 = 0.6;

/// How far from the camera a region's trees are drawn as models, in
/// world units, and the further reach they are kept to once drawn, so a
/// step does not take a region's trees down and put them back; past the
/// keep a region's trees stand as cards, taken down again inside the
/// reach. Between the two a region keeps whichever it had. The reach is
/// what the frame can carry at full geometry.
pub const TREE_REACH: f32 = 120.0;
pub const TREE_KEEP: f32 = 140.0;

/// One variation of a kind: its mesh in the tree's own frame, foot at the
/// origin, up +y, the GLB's units the world's, and its height there; and
/// its model's cards, where the model shipped them, with which seed of
/// the model it is, the layer its pictures start at.
pub struct Variation {
    pub mesh: Handle<Mesh>,
    pub height: f32,
    pub cards: Option<Arc<draw::Cards>>,
    pub seed: u32,
}

/// Every variation of every kind, and the quad every card is drawn on.
#[derive(Default)]
pub struct Kit {
    pine: Vec<Variation>,
    deciduous: Vec<Variation>,
    scrub: Vec<Variation>,
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
        Some(draw::Cards { texture: asset_server.load(self.texture.clone()), side: view(side), top: view(top) })
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
    pub fn of(&self, slot: Slot) -> &Vec<Variation> {
        match slot {
            Slot::Pine => &self.pine,
            Slot::Deciduous => &self.deciduous,
            _ => &self.scrub,
        }
    }

    /// The scale a tree `growth` of the way grown is drawn at: a tree from
    /// the sapling's height toward [`GROWN`] times its model's, a bush from
    /// [`SCRUB_SMALL`] of its model toward the whole.
    pub fn scale(slot: Slot, model_height: f32, growth: f32) -> f32 {
        let growth = growth.clamp(0.0, 1.0);
        if slot == Slot::Scrub {
            return SCRUB_SMALL + (1.0 - SCRUB_SMALL) * growth;
        }
        let full = model_height * GROWN;
        let height = SAPLING_HEIGHT + (full - SAPLING_HEIGHT).max(0.0) * growth;
        height / model_height.max(1e-3)
    }
}

/// The kit, present once every model has loaded.
#[derive(Resource)]
pub struct TreeKit {
    pub kit: Arc<Kit>,
}

#[derive(Resource)]
struct Loading(Vec<(Slot, Handle<Gltf>)>);

/// A tree to draw: where it stands from its region's origin, its turn, how
/// far it has grown, and which model.
#[derive(Clone, Copy, Debug)]
pub struct TreeInstance {
    pub translation: Vec3,
    pub yaw: f32,
    pub growth: f32,
    pub slot: Slot,
    pub variation: u32,
}

/// A drawn tree.
#[derive(Component)]
pub struct Tree;

/// A drawn card.
#[derive(Component)]
pub struct Card;

pub struct ForestPlugin;

impl Plugin for ForestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(draw::TreeDrawPlugin);
        app.add_systems(Startup, begin_loading);
        app.add_systems(Update, load_kit.run_if(resource_exists::<Loading>));
        app.add_systems(
            Update,
            update_trees
                .run_if(resource_exists::<TreeKit>)
                .after(crate::systems::world::poll_summary_meshes),
        );
    }
}

fn begin_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = MODELS.iter().map(|(slot, path)| (*slot, asset_server.load::<Gltf>(*path))).collect();
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
    materials: Res<Assets<StandardMaterial>>,
) {
    for (_, handle) in &loading.0 {
        match asset_server.recursive_dependency_load_state(handle.id()) {
            RecursiveDependencyLoadState::Loaded | RecursiveDependencyLoadState::Failed(_) => {}
            _ => return,
        }
    }
    let mut kit = Kit { quad: meshes.add(card_quad()), ..default() };
    for (slot, handle) in &loading.0 {
        let Some(gltf) = gltfs.get(handle) else {
            warn!("tree model for {slot:?} did not load; its kind is drawn as nothing");
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
            info!("tree model for {slot:?} ships no cards; nothing of it stands past the trees' reach");
        }
        for (seed, mesh_handle) in gltf.meshes.iter().enumerate() {
            let Some(gm) = gltf_meshes.get(mesh_handle) else { continue };
            let merged = merge(gm, &meshes, &materials);
            let height = match merged.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|v| v[1]).fold(0.0, f32::max),
                _ => 0.0,
            };
            let variation = Variation { mesh: meshes.add(merged), height, cards: cards.clone(), seed: seed as u32 };
            match slot {
                Slot::Pine => kit.pine.push(variation),
                Slot::Deciduous => kit.deciduous.push(variation),
                _ => kit.scrub.push(variation),
            }
        }
    }
    info!(
        "tree kit: {} pine, {} deciduous, {} scrub variations",
        kit.pine.len(),
        kit.deciduous.len(),
        kit.scrub.len()
    );
    commands.insert_resource(TreeKit { kit: Arc::new(kit) });
    commands.remove_resource::<Loading>();
}

/// One variation's primitives as one mesh, each vertex coloured by its
/// primitive's material, so one white material draws the whole tree.
fn merge(gm: &GltfMesh, meshes: &Assets<Mesh>, materials: &Assets<StandardMaterial>) -> Mesh {
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

/// The trees of a mesh region at the tile level: one instance per filled
/// slot, standing on the ground surface at its slot, from the region's
/// origin. Reads the map's cover and heights and nothing else, so it runs
/// where the ground is built.
pub fn place_trees(region_key: MeshRegionKey, mesh_origin: Vec3, map: &common_bevy::resources::map::Map) -> Vec<TreeInstance> {
    let lattice = common_bevy::summary::summary_lattice(0);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let tile_z = |q: i32, r: i32| map.get_by_qr(q, r).map(|(qrz, _)| qrz.z);
    let mut out = Vec::new();
    for cell in region_lat.tiles_in_cell((region_key.mn, region_key.mm)) {
        let (q, r) = lattice.cell_center(cell);
        let cover = map.cover_at(q, r);
        if cover.is_empty() {
            continue;
        }
        let Some((floor, _)) = map.get_by_qr(q, r) else { continue };
        let (cx, cz) = flat_top_tile_center(q, r, 1.0);
        let floor_centre = Vec3::new(cx, height_y(floor.z as f32), cz);
        let edge = EDGE_GROWTH + (1.0 - EDGE_GROWTH) * cover.fullness() as f32 / SLOTS.len() as f32;
        for (k, slot) in cover.filled() {
            let sway = common::sway(q, r, k);
            let (x, z) = slot_center(q, r, k, &sway);
            let y = surface_y(Vec2::new(x, z), floor, floor_centre, tile_z);
            out.push(TreeInstance {
                translation: Vec3::new(x, y, z) - mesh_origin,
                yaw: sway.yaw as f32,
                growth: sway.growth as f32 * edge,
                slot,
                variation: sway.variation,
            });
        }
    }
    out
}

/// The trees of a mesh region at a summary level: every tile the region's
/// summaries cover has its slots, each filled from its summary's canopy by
/// the tile rule, standing on the level's drawn surface at its slot, from
/// the region's origin. Reads the level's cells and nothing else, so it
/// runs where the level's ground is built; nothing while a cell is absent.
pub fn place_canopy(
    radius: u32,
    region_key: MeshRegionKey,
    mesh_origin: Vec3,
    height: &dyn Fn(i32, i32) -> Option<i32>,
    canopy: &dyn Fn(i32, i32) -> Option<common::Canopy>,
) -> Vec<TreeInstance> {
    let lattice = common_bevy::summary::summary_lattice(radius);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let mut surface = common_bevy::summary_mesh::LevelSurface::new(radius, height);
    let mut out = Vec::new();
    for cell in region_lat.tiles_in_cell((region_key.mn, region_key.mm)) {
        let Some(canopy) = canopy(cell.0, cell.1) else { continue };
        if canopy.is_empty() {
            continue;
        }
        let edge = EDGE_GROWTH + (1.0 - EDGE_GROWTH) * canopy.density() as f32;
        for (q, r) in lattice.tiles_in_cell(cell) {
            for k in 0..SLOTS.len() {
                let slot = canopy.slot(q, r, k);
                if slot == Slot::Empty {
                    continue;
                }
                let sway = common::sway(q, r, k);
                let (x, z) = slot_center(q, r, k, &sway);
                let Some((y, _)) = surface.at(Vec2::new(x, z)) else { continue };
                out.push(TreeInstance {
                    translation: Vec3::new(x, y, z) - mesh_origin,
                    yaw: sway.yaw as f32,
                    growth: sway.growth as f32 * edge,
                    slot,
                    variation: sway.variation,
                });
            }
        }
    }
    out
}

/// Spawn a region's trees as children of its entity: one batch per
/// variation present, each the variation's mesh and an instance buffer of
/// every tree drawn with it, built now. Trees cast no shadow: the
/// cascades would draw every tree four times over for it.
pub fn spawn_trees(commands: &mut Commands, entity: Entity, trees: &[TreeInstance], kit: &TreeKit, render_device: &RenderDevice) {
    let mut batches: HashMap<(Slot, usize), Vec<draw::Instance>> = HashMap::new();
    let mut reach = 0.0f32;
    for t in trees {
        let all = kit.kit.of(t.slot);
        if all.is_empty() {
            continue;
        }
        let k = t.variation as usize % all.len();
        let v = &all[k];
        let scale = Kit::scale(t.slot, v.height, t.growth);
        reach = reach.max(v.height * scale);
        batches.entry((t.slot, k)).or_default().push(draw::Instance::new(t.translation, t.yaw, scale));
    }
    commands.entity(entity).with_children(|parent| {
        for ((slot, k), instances) in batches {
            let v = &kit.kit.of(slot)[k];
            let (batch, aabb) = draw::TreeBatch::new(render_device, &instances, reach);
            parent.spawn((Mesh3d(v.mesh.clone()), batch, aabb, bevy::camera::visibility::NoAutoAabb, Transform::IDENTITY, Tree));
        }
    });
}

/// Spawn a region's trees as cards, children of its entity: one batch
/// per variation present whose model shipped cards, each the shared quad
/// and an instance buffer of every tree drawn with it, the side layer of
/// its seed in the model's texture.
pub fn spawn_cards(commands: &mut Commands, entity: Entity, trees: &[TreeInstance], kit: &TreeKit, render_device: &RenderDevice) {
    let mut batches: HashMap<(Slot, usize), Vec<draw::Instance>> = HashMap::new();
    let mut reach = 0.0f32;
    for t in trees {
        let all = kit.kit.of(t.slot);
        if all.is_empty() {
            continue;
        }
        let k = t.variation as usize % all.len();
        let v = &all[k];
        let Some(cards) = &v.cards else { continue };
        let scale = Kit::scale(t.slot, v.height, t.growth);
        reach = reach.max(cards.side.height.max(cards.side.width) * scale);
        batches.entry((t.slot, k)).or_default().push(draw::Instance::card(t.translation, t.yaw, scale, v.seed * 2, v.height * scale));
    }
    commands.entity(entity).with_children(|parent| {
        for ((slot, k), instances) in batches {
            let v = &kit.kit.of(slot)[k];
            let Some(cards) = &v.cards else { continue };
            let (batch, aabb) = draw::CardBatch::new(render_device, &instances, reach, cards.clone());
            parent.spawn((Mesh3d(kit.kit.quad.clone()), batch, aabb, bevy::camera::visibility::NoAutoAabb, Transform::IDENTITY, Card));
        }
    });
}

/// Take down a region's children of one kind.
fn take_down<M: Component>(commands: &mut Commands, entity: Entity, children: &Query<&Children>, marked: &Query<(), With<M>>) {
    if let Ok(kids) = children.get(entity) {
        for &kid in kids {
            if marked.get(kid).is_ok() {
                commands.entity(kid).despawn();
            }
        }
    }
}

/// Stand the trees of every region within reach of the camera as models
/// and those beyond the keep as cards, and take each down where the
/// other's ground begins: the region's origin is its measure, and a
/// region built before the kit loaded gets its trees here too.
#[allow(clippy::too_many_arguments)]
fn update_trees(
    mut commands: Commands,
    kit: Res<TreeKit>,
    render_device: Res<RenderDevice>,
    mut summary_meshes: ResMut<crate::resources::SummaryMeshes>,
    origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, (With<common_bevy::components::behaviour::PlayerControlled>, With<common_bevy::components::Actor>)>,
    children: Query<&Children>,
    trees: Query<(), With<Tree>>,
    cards: Query<(), With<Card>>,
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

    for (key, state) in summary_meshes.states.iter_mut() {
        let Some(entity) = state.entity else { continue };
        if state.base_trees.is_empty() {
            continue;
        }
        // Only the tiles' own trees are ever models: a summary's stand as
        // cards from the moment its ground does.
        let d = if key.r == 0 { state.mesh_origin.xz().distance(camera.xz()) } else { f32::INFINITY };
        if !state.trees_spawned && d <= TREE_REACH {
            spawn_trees(&mut commands, entity, &state.base_trees, &kit, &render_device);
            state.trees_spawned = true;
        } else if state.trees_spawned && d > TREE_KEEP {
            take_down::<Tree>(&mut commands, entity, &children, &trees);
            state.trees_spawned = false;
        }
        if !state.cards_spawned && d > TREE_KEEP {
            spawn_cards(&mut commands, entity, &state.base_trees, &kit, &render_device);
            state.cards_spawned = true;
        } else if state.cards_spawned && d <= TREE_REACH {
            take_down::<Card>(&mut commands, entity, &children, &cards);
            state.cards_spawned = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Cover;
    use common_bevy::components::entity_type::{decorator::Decorator, EntityType};
    use common_bevy::resources::map::Map;
    use qrz::{Convert, Qrz};

    /// On flat wooded ground a region places one tree per filled slot,
    /// each standing on the surface within its own tile.
    #[test]
    fn a_region_places_one_tree_per_filled_slot() {
        let map = Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop));
        let region_lat = common_bevy::summary::mesh_region_lattice();
        let lattice = common_bevy::summary::summary_lattice(0);
        let key = MeshRegionKey { r: 0, mn: 0, mm: 0 };
        let mut expected = 0usize;
        for cell in region_lat.tiles_in_cell((0, 0)) {
            let (q, r) = lattice.cell_center(cell);
            let n = ((q - 3 * r).rem_euclid(8)) as usize;
            let mut cover = Cover::NONE;
            for k in 0..n.min(SLOTS.len()) {
                cover = cover.with(k, if k % 2 == 0 { Slot::Pine } else { Slot::Scrub });
            }
            expected += cover.fullness() as usize;
            map.insert(Qrz { q, r, z: 0 }, EntityType::Decorator(Decorator { cover, is_solid: true }));
        }
        let (oq, or) = lattice.cell_center(region_lat.cell_center((0, 0)));
        let (ox, oz) = flat_top_tile_center(oq, or, 1.0);
        let origin = Vec3::new(ox, 0.0, oz);
        let trees = place_trees(key, origin, &map);
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
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    /// A tree's scale runs from a sapling's height to the grown height and
    /// never below the sapling; a bush from its small share to its model.
    #[test]
    fn growth_runs_from_a_sapling_to_grown() {
        let model = 5.0;
        assert!((Kit::scale(Slot::Pine, model, 0.0) * model - SAPLING_HEIGHT).abs() < 1e-5);
        assert!((Kit::scale(Slot::Pine, model, 1.0) * model - model * GROWN).abs() < 1e-5);
        let mut last = 0.0;
        for i in 0..=10 {
            let s = Kit::scale(Slot::Deciduous, 3.0, i as f32 / 10.0);
            assert!(s >= last);
            last = s;
        }
        assert!((Kit::scale(Slot::Scrub, 1.0, 0.0) - SCRUB_SMALL).abs() < 1e-6);
        assert!((Kit::scale(Slot::Scrub, 1.0, 1.0) - 1.0).abs() < 1e-6);
    }
}

