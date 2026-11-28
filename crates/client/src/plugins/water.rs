//! The sea: a camera-following disc at sea level, and the material
//! every water surface is drawn with.

//! The sea is a surface of constant height everywhere in the world, so it
//! needs no per-chunk geometry, no decimation, and no streaming: one disc
//! centred under the camera covers it past the horizon, in rings the
//! world's curve bends it at (`systems::curve`). Lakes and rivers
//! stand above it at a surface per tile, built with each mesh region by the
//! terrain pipeline and drawn with the material this plugin owns.

use std::{cmp::Reverse, collections::BinaryHeap};

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension, TextureFormat},
    shader::ShaderRef,
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use common_bevy::{components::Loc, resources::map::Map, summary_mesh::MeshRegionKey};

use crate::{
    components::Viewed,
    resources::{LoadedChunks, RenderOrigin, SummaryCache, TerrainOrigin},
    systems::closeup::CloseupCamera,
};
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::NotShadowCaster;

use common::grid::RISE;

/// The sea's surface, rendered: where ground standing exactly at sea
/// level is drawn (`common_bevy::surface::height_y` of zero). The shore
/// is then where the ground's own height crosses zero, the coast the
/// world models. Half a level lower it lay out on the shelf, which falls
/// away from the coast too gently to say where a shore is.
pub const SEA_LEVEL_Y: f32 = RISE;

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
    /// How far each tile round the player lies from the shore ([`Shore`]).
    #[texture(101)]
    #[sampler(102)]
    shore: Handle<Image>,
    /// How deep the sea stands over the ground round the player ([`Bed`]).
    #[texture(103)]
    #[sampler(104)]
    bed: Handle<Image>,
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
    /// The tile the shore map is centred on, and how far inland and how
    /// far out to sea it tells distance, in world units.
    field: IVec2,
    reach: Vec2,
    /// The tile the bed map is centred on, then the tiles to one of its
    /// texels and the depth its fullest tells, in world units.
    bed: IVec2,
    bed_told: Vec2,
}

impl Level {
    fn surfed(lap: f32, swell: f32, shore: Handle<Image>, bed: Handle<Image>) -> Self {
        let surf = Surf {
            origin: TerrainOrigin::default(),
            lap,
            swell,
            field: IVec2::ZERO,
            reach: Vec2::new(SHORE_LAND, SHORE_SEA),
            bed: IVec2::ZERO,
            bed_told: Vec2::new(BED_GRAIN as f32, BED_DEEPEST),
        };
        Self { surf, shore, bed }
    }
}

/// The bed map's side in texels and the tiles to a texel: the sea is
/// coloured by its depth this far round the player and is deep past it.
const BED_TEXELS: i32 = 256;
const BED_GRAIN: i32 = 4;
/// The depth the map's fullest texel tells, in world units: all the
/// sea's colour changes by is shallower.
const BED_DEEPEST: f32 = 48.0;
/// How far the player strays from the map's centre before it is centred
/// on them again, in tiles, and how often it is read, in seconds: ground
/// arrives for minutes after a world is entered, and no one says when.
const BED_STRAY: i32 = 96;
const BED_EVERY: f32 = 2.0;

/// How deep the sea stands over the ground round the player, straight
/// down, a texel to `BED_GRAIN` tiles: what the sea's colour goes by. The
/// depth a view reads off the screen is along its line of sight, which
/// looking across the water runs past a shelf's lip to the deep floor
/// behind it and draws the lip's outline on the surface. Read from the
/// tiles where they are held and the far summaries past them, the finest
/// held, off the frame.
#[derive(Resource)]
pub struct Bed {
    map: Handle<Image>,
    centre: Option<(i32, i32)>,
    reading: Option<Task<((i32, i32), Vec<u8>)>>,
    read: f32,
}

/// The bed map's texels about the tile `centre`, row by row along r: the
/// sea's depth over each texel's tile as a share of `BED_DEEPEST`, none
/// over land. Ground `height` does not hold reads as the deepest.
fn bed_texels(height: &dyn Fn(i32, i32) -> Option<f32>, centre: (i32, i32)) -> Vec<u8> {
    let half = BED_TEXELS / 2;
    (0..BED_TEXELS)
        .flat_map(|j| (0..BED_TEXELS).map(move |i| (i, j)))
        .map(|(i, j)| match height(centre.0 + (i - half) * BED_GRAIN, centre.1 + (j - half) * BED_GRAIN) {
            Some(levels) => (((WATERLINE - levels) * RISE / BED_DEEPEST).clamp(0.0, 1.0) * 255.0).round() as u8,
            None => 255,
        })
        .collect()
}

/// Reads the bed map about the player, off the frame, every while and as
/// they stray from its centre, and hands every water the tile it is
/// centred on as each reading lands.
fn read_bed(
    mut bed: ResMut<Bed>,
    map: Res<Map>,
    summaries: Res<SummaryCache>,
    player: Query<&Loc, With<Viewed>>,
    time: Res<Time<Real>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<Water>>,
) {
    if let Some(task) = &mut bed.reading {
        let Some((centre, texels)) = block_on(future::poll_once(task)) else { return };
        bed.reading = None;
        if images.get(&bed.map).is_some_and(|image| image.data.as_ref() != Some(&texels)) {
            if let Some(mut image) = images.get_mut(&bed.map) {
                image.data = Some(texels);
            }
        }
        if bed.centre != Some(centre) {
            bed.centre = Some(centre);
            for (_, material) in materials.iter_mut() {
                material.extension.surf.bed = IVec2::new(centre.0, centre.1);
            }
        }
        return;
    }
    let now = time.elapsed_secs();
    if bed.centre.is_some() && now - bed.read < BED_EVERY {
        return;
    }
    let Ok(loc) = player.single() else { return };
    bed.read = now;
    // A map that has not strayed is read again about the same tile, so
    // its texels keep their places and only what arrived changes.
    let centre = bed.centre.filter(|&(q, r)| {
        let (dq, dr) = (loc.q - q, loc.r - r);
        dq.abs().max(dr.abs()).max((dq + dr).abs()) <= BED_STRAY
    });
    let centre = centre.unwrap_or((loc.q, loc.r));
    let (map, summaries) = (map.clone(), summaries.clone());
    bed.reading = Some(AsyncComputeTaskPool::get().spawn(async move {
        let regions = common_bevy::summary::mesh_region_lattice();
        let height = |q: i32, r: i32| {
            if let Some(ground) = map.ground_at(q, r) {
                return Some(ground.height() as f32);
            }
            common_bevy::summary::LOD_LEVELS.iter().skip(1).find_map(|&level| {
                let (sq, sr) = common_bevy::summary::summary_lattice(level).cell_id(q, r);
                let (mn, mm) = regions.cell_id(sq, sr);
                let region = summaries.get_region(&MeshRegionKey { r: level, mn, mm })?;
                region.cells.get(&(sq, sr)).map(|cell| cell.ground().height() as f32)
            })
        };
        (centre, bed_texels(&height, centre))
    }));
}

/// The shore map's side, in tiles: the surf is drawn by it this far round
/// the player and no further.
const SHORE_TILES: i32 = 256;
/// How far inland and how far out to sea the map tells distance, in world
/// units: past either it says only that the shore is far.
const SHORE_LAND: f32 = 8.0;
const SHORE_SEA: f32 = 56.0;
/// How far the player strays from the map's centre before it is read
/// again about them, in tiles.
const SHORE_STRAY: i32 = 24;
/// The height the sea stands at, in levels: `SEA_LEVEL_Y` as a tile's
/// ground is measured.
const WATERLINE: f32 = 0.0;

/// How far each tile round the player lies from the sea's shore, a texel
/// to a tile: what the surf is drawn by. A bed's depth says nothing of the
/// shore where the bed falls a hand's width in a tile, so the waterline,
/// its foam and the breakers go by distance, which is smooth where depth
/// is nearly level. Read from the map off the frame, about the player,
/// again as they stray and as chunks arrive.
#[derive(Resource)]
pub struct Shore {
    map: Handle<Image>,
    centre: Option<(i32, i32)>,
    reading: Option<Task<((i32, i32), Vec<u8>)>>,
    stale: bool,
}

/// The shore map's texels about the tile `centre`, row by row along r: a
/// tile's distance from where the ground crosses the waterline, out to sea
/// above the middle of the range and inland below it, by `SHORE_LAND` and
/// `SHORE_SEA`. The shore is placed between a dry tile and a wet one where
/// the line between their heights crosses the waterline, and every tile
/// takes its distance to the nearest such point as the crow flies. A tile
/// `height` does not hold reads as far out to sea.
fn shore_texels(height: &dyn Fn(i32, i32) -> Option<f32>, centre: (i32, i32)) -> Vec<u8> {
    let n = SHORE_TILES;
    let half = n / 2;
    let at = |i: i32, j: i32| (i + j * n) as usize;
    let place = |i: i32, j: i32| {
        let (dq, dr) = ((i - half) as f32, (j - half) as f32);
        Vec2::new(1.5 * dq, 3f32.sqrt() * (dr + dq / 2.0)) * common::grid::HEX_RADIUS
    };
    let heights: Vec<Option<f32>> = (0..n).flat_map(|j| (0..n).map(move |i| (i, j))).map(|(i, j)| height(centre.0 + i - half, centre.1 + j - half)).collect();
    let around = |i: i32, j: i32| qrz::DIRECTIONS.iter().map(move |d| (i + d.q, j + d.r)).filter(move |&(a, b)| a >= 0 && b >= 0 && a < n && b < n);

    let mut far = vec![f32::INFINITY; (n * n) as usize];
    let mut shore = vec![Vec2::ZERO; (n * n) as usize];
    // Distances are never negative, so their bits order as they do.
    let mut heap = BinaryHeap::new();
    for j in 0..n {
        for i in 0..n {
            let Some(here) = heights[at(i, j)] else { continue };
            for (a, b) in around(i, j) {
                let Some(there) = heights[at(a, b)] else { continue };
                if (here < WATERLINE) == (there < WATERLINE) {
                    continue;
                }
                let crossing = place(i, j).lerp(place(a, b), (here - WATERLINE) / (here - there));
                let d = crossing.distance(place(i, j));
                if d < far[at(i, j)] {
                    far[at(i, j)] = d;
                    shore[at(i, j)] = crossing;
                }
            }
            if far[at(i, j)].is_finite() {
                heap.push(Reverse((far[at(i, j)].to_bits(), i, j)));
            }
        }
    }
    while let Some(Reverse((bits, i, j))) = heap.pop() {
        if f32::from_bits(bits) > far[at(i, j)] {
            continue;
        }
        let nearest = shore[at(i, j)];
        for (a, b) in around(i, j) {
            if heights[at(a, b)].is_none() {
                continue;
            }
            let d = nearest.distance(place(a, b));
            if d < far[at(a, b)] {
                far[at(a, b)] = d;
                shore[at(a, b)] = nearest;
                heap.push(Reverse((d.to_bits(), a, b)));
            }
        }
    }
    // A tile took the nearest point any tile beside it had when it was
    // reached, and two beside it may each hold a point to one side of its
    // own: it takes the nearer of what they hold now.
    for _ in 0..2 {
        for j in 0..n {
            for i in 0..n {
                if heights[at(i, j)].is_none() {
                    continue;
                }
                for (a, b) in around(i, j) {
                    if far[at(a, b)].is_finite() && shore[at(a, b)].distance(place(i, j)) < far[at(i, j)] {
                        far[at(i, j)] = shore[at(a, b)].distance(place(i, j));
                        shore[at(i, j)] = shore[at(a, b)];
                    }
                }
            }
        }
    }
    let out: Vec<f32> = heights
        .iter()
        .zip(&far)
        .map(|(height, &far)| match height {
            Some(h) if *h < WATERLINE => far,
            Some(_) => -far,
            None => f32::INFINITY,
        })
        .collect();
    // The shore is known only at points a tile apart, so the distance
    // from it ripples along it by a share of a tile: each tile is evened
    // with the six round it, and a line drawn at one distance runs smooth.
    (0..n)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .map(|(i, j)| {
            let own = out[at(i, j)];
            let (sum, count) = around(i, j).map(|(a, b)| out[at(a, b)]).filter(|d| d.is_finite()).fold((0.0, 0.0), |(sum, count), d| (sum + d, count + 1.0));
            let evened = if own.is_finite() && count > 0.0 { 0.5 * own + 0.5 * sum / count } else { own };
            (((evened + SHORE_LAND) / (SHORE_LAND + SHORE_SEA)).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

/// Reads the shore map again about the player as they stray from its
/// centre and as chunks arrive, off the frame, and hands every water the
/// tile it is centred on as each reading lands.
fn read_shore(
    mut shore: ResMut<Shore>,
    map: Res<Map>,
    loaded: Res<LoadedChunks>,
    player: Query<&Loc, With<Viewed>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<Water>>,
) {
    if loaded.is_changed() {
        shore.stale = true;
    }
    if let Some(task) = &mut shore.reading {
        let Some((centre, texels)) = block_on(future::poll_once(task)) else { return };
        shore.reading = None;
        shore.centre = Some(centre);
        if let Some(mut image) = images.get_mut(&shore.map) {
            image.data = Some(texels);
        }
        for (_, material) in materials.iter_mut() {
            material.extension.surf.field = IVec2::new(centre.0, centre.1);
        }
        return;
    }
    let Ok(loc) = player.single() else { return };
    let strayed = shore.centre.is_none_or(|(q, r)| {
        let (dq, dr) = (loc.q - q, loc.r - r);
        dq.abs().max(dr.abs()).max((dq + dr).abs()) > SHORE_STRAY
    });
    if !strayed && !shore.stale {
        return;
    }
    shore.stale = false;
    let (map, centre) = (map.clone(), (loc.q, loc.r));
    shore.reading = Some(AsyncComputeTaskPool::get().spawn(async move {
        let height = |q: i32, r: i32| map.ground_at(q, r).map(|ground| ground.height() as f32);
        (centre, shore_texels(&height, centre))
    }));
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
        app.add_systems(Update, (follow_origin.run_if(resource_changed::<RenderOrigin>), read_shore, read_bed));
    }
}

fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<Water>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Far out to sea everywhere until the first reading lands.
    let size = Extent3d { width: SHORE_TILES as u32, height: SHORE_TILES as u32, depth_or_array_layers: 1 };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[255], TextureFormat::R8Unorm, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    image.sampler = ImageSampler::linear();
    let shore = images.add(image);
    commands.insert_resource(Shore { map: shore.clone(), centre: None, reading: None, stale: false });
    // Deep everywhere until the first reading lands.
    let size = Extent3d { width: BED_TEXELS as u32, height: BED_TEXELS as u32, depth_or_array_layers: 1 };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[255], TextureFormat::R8Unorm, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    image.sampler = ImageSampler::linear();
    let bed = images.add(image);
    commands.insert_resource(Bed { map: bed.clone(), centre: None, reading: None, read: 0.0 });

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
    let sea = materials.add(Water { base: clear.clone(), extension: Level::surfed(1.0, 1.0, shore.clone(), bed.clone()) });
    // A river is shallow from bank to bank, and breakers would cover it.
    let inland = materials.add(Water { base: clear, extension: Level::surfed(1.0, 0.0, shore.clone(), bed.clone()) });
    commands.insert_resource(WaterMaterial(inland));
    let solid = materials.add(Water {
        base: StandardMaterial { base_color: Color::srgb(0.07, 0.26, 0.42), double_sided: true, cull_mode: None, ..default() },
        extension: Level::surfed(0.0, 0.0, shore, bed),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The bed map tells the sea's depth over each texel's tile, none
    /// over land, deeper the lower the ground, and the deepest where the
    /// ground is not held.
    #[test]
    fn the_bed_map_tells_the_depth_over_its_tiles() {
        let centre = (40, -12);
        let height = |q: i32, _: i32| (q < centre.0 + 200).then_some(-((q - centre.0) as f32) / 10.0);
        let texels = bed_texels(&height, centre);
        let at = |i: i32| texels[(BED_TEXELS / 2 + i + (BED_TEXELS / 2) * BED_TEXELS) as usize];
        assert_eq!(at(-10), 0, "land stands under no sea");
        assert!(at(5) < at(20) && at(20) < at(40), "the lower the ground the deeper: {} {} {}", at(5), at(20), at(40));
        assert_eq!(at(60), 255, "ground no one holds reads as the deepest");
    }

    /// Off a straight shore the map tells every tile how far it lies from
    /// the waterline, further the further out to sea, the same along the
    /// shore, and less than nothing inland.
    #[test]
    fn the_shore_map_tells_distance_from_a_straight_shore() {
        // The ground falls a quarter level a world unit along x and
        // crosses the waterline three units east of the centre.
        let height = |q: i32, _: i32| Some(WATERLINE - 0.25 * (1.5 * q as f32 - 3.0));
        let texels = shore_texels(&height, (0, 0));
        let half = SHORE_TILES / 2;
        let told = |q: i32, r: i32| texels[((q + half) + (r + half) * SHORE_TILES) as usize] as f32 / 255.0 * (SHORE_LAND + SHORE_SEA) - SHORE_LAND;
        for q in 3..20 {
            // The shore is found where it crosses the lines between tiles,
            // so a tile beside it is told a little far: within half a unit.
            assert!((told(q, 0) - (1.5 * q as f32 - 3.0)).abs() <= 0.5, "tile {q} lies {} out, told {}", 1.5 * q as f32 - 3.0, told(q, 0));
            assert!(told(q + 1, 0) > told(q, 0), "further out to sea is further from the shore");
            assert!((told(q, 0) - told(q, 9)).abs() <= 0.3, "the same all along the shore: {} and {}", told(q, 0), told(q, 9));
        }
        assert!(told(0, 0) < 0.0 && told(-2, 0) < told(0, 0), "inland lies under nothing, and further inland further");
    }
}
