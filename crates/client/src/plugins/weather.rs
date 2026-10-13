//! The weather as the camera sees it. `common_bevy::weather` says what the
//! sky does over a place; this reads it about the camera each frame into
//! [`SkyWeather`] for `world::update` to light the sky, the clouds and the
//! ground by, and draws the rain or snow falling round the view.
//!
//! Everything here reads the world's calendar (`Server::wall`), which an
//! admin may shift or hold for every player at once: held, the fronts, the
//! strikes, the fireballs, the wind and the fall all stand still.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};
use bevy_camera::visibility::NoFrustumCulling;
use bevy_light::{NotShadowCaster, NotShadowReceiver};

use common::grid::RISE;
use common_bevy::weather::{self, Forecast, Weather};

use common_bevy::components::Loc;

use crate::{
    components::Viewed,
    plugins::diagnostics::{DiagnosticsState, WeatherHold},
    resources::{RenderOrigin, Server},
    systems::closeup::CloseupCamera,
};

/// Bolts and fireballs the sky draws at once.
pub const BOLTS: usize = 4;
pub const METEORS: usize = 3;

/// The weather map: a square of cells about the camera, each the weather
/// at its centre, out past where the cloud deck meets the reach.
const MAP_CELLS: u32 = 64;
const MAP_CELL_WU: f64 = 750.0;
/// How often the map is read again while the camera stays in its cell.
/// The fronts change over a period of hours of game time; this is only
/// how smoothly the change shows.
const MAP_REFRESH_MS: u128 = 250;

/// The cloud deck's height over the camera, wherever the camera stands:
/// the sky is the same from a valley and from a peak, and climbing never
/// reaches the clouds. Unbuilt: low cloud lying on the ground.
pub const DECK_HEIGHT: f32 = 900.0;

/// How quickly the weather over the camera follows the camera's move into
/// another, in seconds: crossing a front's edge takes a few seconds.
const EASE_SECS: f32 = 3.0;

/// How far a strike is seen, and how far its flash lights the ground at
/// half strength.
const STRIKE_RADIUS_WU: f64 = 16_000.0;
const FLASH_REACH_WU: f32 = 3_000.0;
/// A bolt's apparent width at its distance: wide enough to read in the
/// drawn look, as the ink is.
const BOLT_WIDTH_WU: f32 = 10.0;
/// How long a strike's strokes last.
const STRIKE_MS: u128 = 600;
/// How quickly the scene's light follows the strikes up, and down: slow
/// enough down to fill the gaps between a strike's strokes, so the view
/// swells once a strike, well under the three flashes a second that can
/// set off a photosensitive seizure.
const FLASH_RISE_SECS: f32 = 0.03;
const FLASH_FALL_SECS: f32 = 0.35;

/// What the sky shows over the camera this frame.
#[derive(Resource, Clone)]
pub struct SkyWeather {
    /// The weather over the camera, eased as it moves.
    pub here: Weather,
    /// How much of the leap season holds, the same everywhere.
    pub omen: f32,
    /// How much the lightning lights the scene now.
    pub flash: f32,
    /// Each bolt in sight: its bearing from the camera in xz, its top's
    /// elevation, its brightness.
    pub bolts: [Vec4; BOLTS],
    /// Each bolt's path seed and its apparent width in radians.
    pub bolt_looks: [Vec4; BOLTS],
    /// Each fireball alight: its trail's end and its brightness, and the
    /// pole of the great circle it runs on and the trail's angle along it.
    pub meteor_tails: [Vec4; METEORS],
    pub meteor_paths: [Vec4; METEORS],
    /// The fireballs' light on the sky, the clouds and the ground.
    pub blaze: Vec3,
    /// The weather map: cover, rain, storm and mist about the camera.
    pub map: Handle<Image>,
    /// The map's corner, rendered, in xz, and its side.
    pub frame: Vec4,
    /// What carries a rendered xz to the cloud detail's lattice: the world
    /// wound back by the wind, within one cell of `WIND_LATTICE_WU`.
    pub drift: Vec2,
    /// What the map was last read for: the sky's moment, its centre cell,
    /// the hold.
    read: Option<(u128, (i64, i64), WeatherHold)>,
}

impl FromWorld for SkyWeather {
    fn from_world(world: &mut World) -> Self {
        let size = Extent3d { width: MAP_CELLS, height: MAP_CELLS, depth_or_array_layers: 1 };
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        image.sampler = ImageSampler::linear();
        let map = world.resource_mut::<Assets<Image>>().add(image);
        Self {
            here: Weather::default(),
            omen: 0.0,
            flash: 0.0,
            bolts: [Vec4::ZERO; BOLTS],
            bolt_looks: [Vec4::ZERO; BOLTS],
            meteor_tails: [Vec4::ZERO; METEORS],
            meteor_paths: [Vec4::ZERO; METEORS],
            blaze: Vec3::ZERO,
            map,
            frame: Vec4::new(0.0, 0.0, MAP_CELLS as f32 * MAP_CELL_WU as f32, 0.0),
            drift: Vec2::ZERO,
            read: None,
        }
    }
}

/// The cloud deck: a dome about the camera inside the discs, so a cloud
/// passes in front of the sun and the moon, and past the reach, so the
/// ground stands in front of it. Transparent, so it writes no depth and
/// the ink draws no line round a cloud. Shaded by `shaders/clouds.wgsl`.
#[derive(Component)]
pub struct CloudDeck;

/// The deck's radius, inside `world::disc_distance_wu`.
fn deck_radius_wu() -> f32 {
    common_bevy::summary::reach_wu() * 1.2
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct CloudMaterial {
    /// The sunlit cloud and its shaded underside.
    #[uniform(0)]
    pub light: LinearRgba,
    #[uniform(0)]
    pub shade: LinearRgba,
    /// The sky's glow toward the sun, which lights the clouds there too,
    /// and the sun's bearing in xz.
    #[uniform(0)]
    pub glow: LinearRgba,
    #[uniform(0)]
    pub bearing: Vec4,
    /// The deck's height over the camera, the haze's visibility, the
    /// lightning's flash, the leap season's pall.
    #[uniform(0)]
    pub deck: Vec4,
    /// `SkyWeather::frame` and `SkyWeather::drift`.
    #[uniform(0)]
    pub frame: Vec4,
    #[uniform(0)]
    pub drift: Vec4,
    /// The haze's colour, which a far cloud fades to, and the mist's veil
    /// over the whole sky.
    #[uniform(0)]
    pub veil: LinearRgba,
    #[uniform(0)]
    pub bolts: [Vec4; BOLTS],
    #[uniform(0)]
    pub bolt_looks: [Vec4; BOLTS],
    /// `SkyWeather::meteor_tails`, `meteor_paths` and `blaze`.
    #[uniform(0)]
    pub meteor_tails: [Vec4; METEORS],
    #[uniform(0)]
    pub meteor_paths: [Vec4; METEORS],
    #[uniform(0)]
    pub blaze: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub weather: Handle<Image>,
}

impl Material for CloudMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/clouds.wgsl".into()
    }

    /// A cloud covers what is behind it by its density, and a bolt adds
    /// its light without covering anything.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// The camera is inside the dome, so its inner faces must draw.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The rain or snow round the view: a box of drops ahead of the camera,
/// each fixed on a lattice in the world the box's size, so the drops stay
/// where they fall as the camera moves and the box wraps them round it.
#[derive(Component)]
struct Precipitation;

/// The box's width and height, how far ahead of the camera its centre
/// stands and how far below the camera its floor: the camera stands a
/// boom's length behind and above the player, and the rain falls round
/// the player as much as round the lens.
const BOX: Vec3 = Vec3::new(80.0, 50.0, 80.0);
const BOX_AHEAD: f32 = 30.0;
const BOX_BELOW: f32 = 35.0;
const DROPS: u32 = 6_000;
/// How fast rain and snow fall, in units a second, and the length of a
/// raindrop's streak.
const RAIN_FALL: f32 = 14.0;
const SNOW_FALL: f32 = 1.5;
const STREAK: f32 = 0.9;
/// The wind carries the drops this many boxes along x, and half that along
/// z, each minute: a whole number, so the drift wraps without a seam. West-
/// south-west, as the fronts go.
const DROP_DRIFT_MS: u128 = 60_000;

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct PrecipitationMaterial {
    /// The box's lowest corner, rendered, and its size and the drops in it.
    #[uniform(0)]
    pub corner: Vec4,
    #[uniform(0)]
    pub size: Vec4,
    /// What a drop's place in the box is wound back by: the box's corner
    /// in the world, less the wind's drift, plus the fall, each within the
    /// box.
    #[uniform(0)]
    pub anchor: Vec4,
    /// The direction a drop falls, and its streak's length.
    #[uniform(0)]
    pub fall: Vec4,
    /// A drop's width, its share of the drops shown, and one for snow.
    #[uniform(0)]
    pub look: Vec4,
    #[uniform(0)]
    pub color: LinearRgba,
}

impl Material for PrecipitationMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/precipitation.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/precipitation.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    /// A drop's seed and its quad's corner, and nothing of Bevy's layout.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
        ])?];
        Ok(())
    }
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MaterialPlugin::<CloudMaterial>::default(), MaterialPlugin::<PrecipitationMaterial>::default()));
        app.init_resource::<SkyWeather>();
        app.add_systems(Startup, setup);
        app.add_systems(Update, (read, fall));
        // After camera movement, so the deck never lags a frame and shows
        // its own horizon.
        app.add_systems(PostUpdate, follow_camera);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut clouds: ResMut<Assets<CloudMaterial>>,
    mut drops: ResMut<Assets<PrecipitationMaterial>>,
    weather: Res<SkyWeather>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(deck_radius_wu()).mesh().ico(3).expect("a sphere subdivides"))),
        MeshMaterial3d(clouds.add(CloudMaterial { weather: weather.map.clone(), ..default() })),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        CloudDeck,
    ));
    commands.spawn((
        Mesh3d(meshes.add(drops_mesh())),
        MeshMaterial3d(drops.add(PrecipitationMaterial::default())),
        Transform::default(),
        // Placed by its shader about the camera, never by its transform.
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
        Precipitation,
    ));
}

/// `DROPS` quads, each four corners at one seed in the unit cube: the
/// shader places the seed in the box and opens the quad about it. The
/// seeds are scattered, so the first of them are any share of the box.
fn drops_mesh() -> Mesh {
    let mut seeds = Vec::with_capacity(DROPS as usize * 4);
    let mut corners = Vec::with_capacity(DROPS as usize * 4);
    let mut indices = Vec::with_capacity(DROPS as usize * 6);
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    for drop in 0..DROPS {
        let seed = [next(), next(), next()];
        for corner in [[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]] {
            seeds.push(seed);
            corners.push(corner);
        }
        let base = drop * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, seeds)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, corners)
        .with_inserted_indices(Indices::U32(indices))
}

/// The forecast the sky shows: the fronts at the sky's moment, or one
/// weather everywhere where the console holds it. Whether what falls is
/// snow, where the hold says.
fn forecast(hold: WeatherHold, wall_ms: u128) -> (Forecast, Option<bool>) {
    let steady = |cover: f32, rain: f32, mist: f32, storm: f32, omen: f32| {
        Forecast::steady(wall_ms, Weather { cover, rain, mist, storm }, omen)
    };
    match hold {
        WeatherHold::Forecast => (Forecast::at(wall_ms), None),
        WeatherHold::Clear => (steady(0.0, 0.0, 0.0, 0.0, 0.0), None),
        WeatherHold::Overcast => (steady(1.0, 0.0, 0.0, 0.0, 0.0), None),
        WeatherHold::Rain => (steady(1.0, 1.0, 0.0, 0.0, 0.0), Some(false)),
        WeatherHold::Snow => (steady(1.0, 0.7, 0.0, 0.0, 0.0), Some(true)),
        WeatherHold::Mist => (steady(0.3, 0.0, 1.0, 0.0, 0.0), None),
        WeatherHold::Storm => (steady(1.0, 1.0, 0.0, 1.0, 1.0), Some(false)),
        WeatherHold::Meteors => (steady(0.0, 0.0, 0.0, 0.0, 1.0), None),
    }
}

/// Reads the weather about the camera: the weather over it, the map when
/// it is due, the strikes and their flash, the meteors alight.
#[allow(clippy::too_many_arguments)]
fn read(
    time: Res<Time>,
    server: Res<Server>,
    diagnostics: Res<DiagnosticsState>,
    origin: Res<RenderOrigin>,
    camera: Query<&Transform, (With<Camera3d>, Without<CloseupCamera>)>,
    mut weather: ResMut<SkyWeather>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok(camera) = camera.single() else { return };
    let wall_ms = server.wall(server.now(time.elapsed().as_millis()));
    let (forecast, _) = forecast(diagnostics.weather, wall_ms);
    let world = origin.world(camera.translation);
    let (x, z) = (world.x as f64, world.z as f64);

    let target = forecast.over(x, z);
    let ease = 1.0 - (-time.delta_secs() / EASE_SECS).exp();
    let here = &mut weather.here;
    for (now, to) in [(&mut here.cover, target.cover), (&mut here.rain, target.rain), (&mut here.mist, target.mist), (&mut here.storm, target.storm)] {
        *now += (to - *now) * ease;
    }
    weather.omen = forecast.omen();

    // The map, about the camera's cell.
    let centre = ((x / MAP_CELL_WU).floor() as i64, (z / MAP_CELL_WU).floor() as i64);
    let due = match weather.read {
        Some((read_ms, read_centre, hold)) => read_centre != centre || hold != diagnostics.weather || wall_ms.abs_diff(read_ms) >= MAP_REFRESH_MS,
        None => true,
    };
    if due {
        let half = MAP_CELLS as i64 / 2;
        let (i0, j0) = (centre.0 - half, centre.1 - half);
        let mut texels = Vec::with_capacity((MAP_CELLS * MAP_CELLS * 4) as usize);
        for j in 0..MAP_CELLS as i64 {
            for i in 0..MAP_CELLS as i64 {
                let w = forecast.over((i0 + i) as f64 * MAP_CELL_WU + MAP_CELL_WU / 2.0, (j0 + j) as f64 * MAP_CELL_WU + MAP_CELL_WU / 2.0);
                texels.extend([w.cover, w.rain, w.storm, w.mist].map(|share| (share.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
        }
        if let Some(mut image) = images.get_mut(&weather.map) {
            image.data = Some(texels);
        }
        let corner_world = Vec3::new((i0 as f64 * MAP_CELL_WU) as f32, 0.0, (j0 as f64 * MAP_CELL_WU) as f32);
        let corner = corner_world - origin.world_vec();
        weather.frame = Vec4::new(corner.x, corner.z, MAP_CELLS as f32 * MAP_CELL_WU as f32, 0.0);
        weather.read = Some((wall_ms, centre, diagnostics.weather));
    }

    let (dx, dz) = weather::drift(wall_ms);
    let lattice = weather::WIND_LATTICE_WU;
    let origin_world = origin.world_vec();
    weather.drift = Vec2::new(
        (origin_world.x as f64 - dx).rem_euclid(lattice) as f32,
        (origin_world.z as f64 - dz).rem_euclid(lattice) as f32,
    );

    // Strikes: each bolt by its strokes' light, the nearest and brightest
    // drawn, and the flash every one in reach casts. The bolt flickers
    // with its strokes; the scene's light follows them slowly down, so a
    // strike is one swell of light and never a strobe over the whole view.
    let mut flash = 0.0;
    let mut bolts: Vec<(Vec4, Vec4)> = Vec::new();
    for strike in weather::strikes(&forecast, wall_ms.saturating_sub(STRIKE_MS), wall_ms, x, z, STRIKE_RADIUS_WU) {
        let light = strokes((wall_ms - strike.at) as f32, strike.seed);
        if light <= 0.01 {
            continue;
        }
        let toward = Vec2::new((strike.x - x) as f32, (strike.z - z) as f32);
        let distance = toward.length().max(1.0);
        flash += light / (1.0 + (distance / FLASH_REACH_WU).powi(2));
        let haze = (-crate::systems::camera::haze_density() * distance).exp();
        bolts.push((
            (toward / distance).extend(DECK_HEIGHT.atan2(distance)).extend(light * (0.3 + 0.7 * haze)),
            Vec4::new((strike.seed % 1024) as f32, BOLT_WIDTH_WU / distance, 0.0, 0.0),
        ));
    }
    bolts.sort_by(|a, b| b.0.w.total_cmp(&a.0.w));
    let flash = flash.min(1.0);
    let follow = if flash > weather.flash { FLASH_RISE_SECS } else { FLASH_FALL_SECS };
    weather.flash += (flash - weather.flash) * (1.0 - (-time.delta_secs() / follow).exp());
    weather.bolts = [Vec4::ZERO; BOLTS];
    weather.bolt_looks = [Vec4::ZERO; BOLTS];
    for (n, (bolt, look)) in bolts.into_iter().take(BOLTS).enumerate() {
        weather.bolts[n] = bolt;
        weather.bolt_looks[n] = look;
    }

    // Fireballs: each from its trail's end, a part of its run behind the
    // head, along its arc to its head, and the light of all of them.
    weather.meteor_tails = [Vec4::ZERO; METEORS];
    weather.meteor_paths = [Vec4::ZERO; METEORS];
    weather.blaze = Vec3::ZERO;
    let alight = weather::meteors(&forecast, wall_ms.saturating_sub(weather::METEOR_BURNS_MOST_MS), wall_ms)
        .into_iter()
        .filter(|m| wall_ms < m.at + m.burns_ms);
    for (n, meteor) in alight.take(METEORS).enumerate() {
        let burnt = (wall_ms - meteor.at) as f32 / meteor.burns_ms as f32;
        let trailing = (burnt - METEOR_TRAIL).max(0.0);
        let bright = blazing(burnt);
        weather.meteor_tails[n] = meteor.along(trailing).extend(bright);
        weather.meteor_paths[n] = meteor.pole.extend(meteor.span * (burnt - trailing));
        weather.blaze += BLAZE_COLOR * bright;
    }
}

/// The share of a fireball's run its trail stands behind the head.
const METEOR_TRAIL: f32 = 0.25;
/// The colour a fireball lights the world in.
const BLAZE_COLOR: Vec3 = Vec3::new(1.0, 0.55, 0.2);

/// A fireball's brightness at `burnt` of its run: quick to catch, steady
/// as it crosses, flaring as it breaks up near its end, then out.
fn blazing(burnt: f32) -> f32 {
    let catch = (burnt / 0.1).clamp(0.0, 1.0);
    let flare = 1.0 + 1.2 * (-((burnt - 0.8) / 0.07).powi(2)).exp();
    let out = 1.0 - ((burnt - 0.9) / 0.1).clamp(0.0, 1.0);
    catch * flare * out
}

/// A strike's light `age_ms` after it: the first stroke and the return
/// strokes down its channel, two or three in the first quarter second,
/// each dying fast.
fn strokes(age_ms: f32, seed: u32) -> f32 {
    let second = 60.0 + (seed % 61) as f32;
    let third = 170.0 + (seed / 61 % 91) as f32;
    let third_strength = if seed / 5_551 % 2 == 0 { 0.45 } else { 0.0 };
    [(0.0, 1.0), (second, 0.7), (third, third_strength)]
        .iter()
        .filter(|(at, _)| age_ms >= *at)
        .map(|(at, strength)| strength * (-(age_ms - at) / 45.0).exp())
        .sum()
}

/// Lays the box of drops about the view and lets it fall: rain or snow as
/// the ground the viewed actor stands on is over the snow line or under
/// it, as many drops as the weather over the camera rains.
#[allow(clippy::too_many_arguments)]
fn fall(
    time: Res<Time>,
    server: Res<Server>,
    diagnostics: Res<DiagnosticsState>,
    origin: Res<RenderOrigin>,
    weather: Res<SkyWeather>,
    camera: Query<(&Transform, &DistanceFog), (With<Camera3d>, Without<CloseupCamera>)>,
    viewed: Query<&Loc, With<Viewed>>,
    precipitation: Query<&MeshMaterial3d<PrecipitationMaterial>, With<Precipitation>>,
    mut materials: ResMut<Assets<PrecipitationMaterial>>,
) {
    let Ok((camera, fog)) = camera.single() else { return };
    let Some(mut material) = precipitation.single().ok().and_then(|m| materials.get_mut(&m.0)) else { return };
    let wall_ms = server.wall(server.now(time.elapsed().as_millis()));
    let snow = forecast(diagnostics.weather, wall_ms).1
        .unwrap_or_else(|| {
            let ground = viewed.single().map_or(camera.translation.y / RISE, |loc| loc.z as f32);
            ground > weather::snow_line(wall_ms)
        });

    let ahead = camera.forward().as_vec3().with_y(0.0).normalize_or_zero() * BOX_AHEAD;
    let corner = camera.translation + ahead - Vec3::new(BOX.x / 2.0, BOX_BELOW, BOX.z / 2.0);
    let corner_world = origin.world(corner);
    let speed = if snow { SNOW_FALL } else { RAIN_FALL };
    let fall_ms = (BOX.y / speed * 1_000.0) as u128;
    let fallen = (wall_ms % fall_ms) as f64 / fall_ms as f64 * BOX.y as f64;
    let drifted = (wall_ms % DROP_DRIFT_MS) as f64 / DROP_DRIFT_MS as f64;
    let wrap = |v: f64, side: f32| v.rem_euclid(side as f64) as f32;
    material.corner = corner.extend(0.0);
    material.size = BOX.extend(DROPS as f32);
    material.anchor = Vec4::new(
        wrap(corner_world.x as f64 - 2.0 * drifted * BOX.x as f64, BOX.x),
        wrap(corner_world.y as f64 + fallen, BOX.y),
        wrap(corner_world.z as f64 - drifted * BOX.z as f64, BOX.z),
        0.0,
    );
    let wind = Vec3::new(2.0 * BOX.x, 0.0, BOX.z) / (DROP_DRIFT_MS as f32 / 1_000.0);
    material.fall = (wind - Vec3::Y * speed).normalize().extend(STREAK);
    material.look = Vec4::new(if snow { 0.07 } else { 0.02 }, weather.here.rain, f32::from(snow), 0.0);
    let haze = fog.color.to_linear();
    material.color = if snow {
        LinearRgba::new(0.9, 0.92, 0.95, 0.9) * (haze.luminance() * 2.0).clamp(0.05, 1.0)
    } else {
        LinearRgba::new(haze.red * 1.3, haze.green * 1.3, haze.blue * 1.3, 0.35)
    };
}

/// Keeps the cloud deck centred on the camera, and the drops' origin just
/// behind it. The drops' shader places them; their origin only orders them
/// among the transparent draws, which run from the furthest origin in, so
/// they draw after the deck and no cloud covers a drop against the sky.
fn follow_camera(
    camera: Query<&Transform, (With<Camera3d>, Without<CloudDeck>, Without<Precipitation>, Without<CloseupCamera>)>,
    mut deck: Query<&mut Transform, (With<CloudDeck>, Without<Precipitation>)>,
    mut drops: Query<&mut Transform, (With<Precipitation>, Without<CloudDeck>)>,
) {
    let Ok(cam) = camera.single() else { return };
    for mut transform in &mut deck {
        transform.translation = cam.translation;
    }
    for mut transform in &mut drops {
        transform.translation = cam.translation - cam.forward().as_vec3();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fireball catches, flares near its end, and is out at its end.
    #[test]
    fn a_fireball_flares_before_it_goes_out() {
        assert_eq!(blazing(0.0), 0.0);
        assert!(blazing(0.8) > blazing(0.5));
        assert_eq!(blazing(1.0), 0.0);
    }

    /// A strike's light is its first stroke at full and dies within the
    /// strike's time.
    #[test]
    fn a_strike_flashes_and_dies() {
        for seed in [0, 7, 5_551, 123_456_789] {
            assert!((strokes(0.0, seed) - 1.0).abs() < 1e-6);
            assert!(strokes(STRIKE_MS as f32, seed) < 0.01);
        }
    }
}
