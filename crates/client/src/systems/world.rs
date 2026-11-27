use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::{
    color::{ColorToComponents, Luminance},
    image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    math::ops::*,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{DistanceFog, FogFalloff, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
    tasks::{block_on, futures_lite::future},
};
use bevy_light::{CascadeShadowConfig, CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};

/// Illuminance of the sun at noon, in lux. The moon's share of it is what
/// the haze dims by at night.
const SUN_ILLUMINANCE: f32 = 10_000.;
/// Apparent diameters of the sun's and the moon's discs. Earth's are both
/// about half a degree; the sun's is four times that so it is more than
/// a dot in the frame, and the moon's is what its light asks for.
const SUN_ANGULAR_DIAMETER: f32 = 2_f32.to_radians();
const MOON_ANGULAR_DIAMETER: f32 = 6_f32.to_radians();
/// Illuminance of the moon when full, in lux: Earth's full moon gives a
/// quarter lux from half a degree, and light goes with apparent area, so
/// a moon so many times wider gives that many squared times as much.
const EARTH_MOON_LUX: f32 = 0.25;
const EARTH_MOON_ANGULAR_DIAMETER: f32 = 0.5_f32.to_radians();
const MOON_ILLUMINANCE: f32 = EARTH_MOON_LUX
    * (MOON_ANGULAR_DIAMETER / EARTH_MOON_ANGULAR_DIAMETER)
    * (MOON_ANGULAR_DIAMETER / EARTH_MOON_ANGULAR_DIAMETER);

/// The sky: a dome about the camera, past the discs and inside the far
/// plane, one colour everywhere but toward the sun, where a glow of the
/// sun's tint sits low under its bearing, and the haze at the horizon,
/// behind the frontier where the drawn world ends. Shaded by
/// `shaders/sky.wesl`. The clouds, and the leap season's fireballs in front
/// of them, are a deck of their own in front of the discs
/// (`plugins::weather::CloudDeck`).
#[derive(Component)]
pub struct SkyDome;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SkyMaterial {
    /// To the sun, a unit vector, and in `w` the fire of a sunrise or a
    /// sunset ([`fire`]).
    #[uniform(0)]
    pub sun: Vec4,
    #[uniform(0)]
    pub base: LinearRgba,
    /// The fire's colour at the horizon and high on the dome.
    #[uniform(0)]
    pub ember: LinearRgba,
    #[uniform(0)]
    pub rose: LinearRgba,
    /// The sun's halo.
    #[uniform(0)]
    pub halo: LinearRgba,
    /// The camera's fog colour, which the sky is at the horizon.
    #[uniform(0)]
    pub haze: LinearRgba,
    /// In `x`, how far out the stars are; in `y`, the share of the sun
    /// the visitor covers; in `z`, the sine of elevation the haze clears
    /// at; in `w`, how far the air about the camera hides the stars low
    /// down.
    #[uniform(0)]
    pub night: Vec4,
    /// The columns of what takes a direction in the world to its place
    /// on the stars' sphere (`common_bevy::sky::turning`, undone).
    #[uniform(0)]
    pub turn_east: Vec4,
    #[uniform(0)]
    pub turn_up: Vec4,
    #[uniform(0)]
    pub turn_south: Vec4,
    /// The guide stars (`common_bevy::sky::guides`).
    #[uniform(0)]
    pub guides: [Vec4; common_bevy::sky::GUIDES],
    /// The pale band: its great circle's pole and its core
    /// (`common_bevy::sky::band`, `core`), and its glowing clouds
    /// (`nebulae`).
    #[uniform(0)]
    pub band: Vec4,
    #[uniform(0)]
    pub core: Vec4,
    #[uniform(0)]
    pub nebulae: [Vec4; 2],
    /// To the visitor, a unit vector in the world, and in `w` its radius
    /// as a chord, none while there is no visitor.
    #[uniform(0)]
    pub visitor: Vec4,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sky.wesl".into()
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

/// The dome's radius: past the discs, inside the culling far plane.
fn sky_radius_wu() -> f32 {
    common_bevy::summary::reach_wu() * 1.4
}

/// The sun's and the moon's discs in the sky, unlit and unfogged, held
/// `disc_distance_wu` from the camera in their light's direction.
#[derive(Component)]
pub enum Disc { Sun, Moon }

/// A disc's material: the lit hemisphere of a sphere facing the camera,
/// lit from `sun` in the disc's own frame, +z toward the camera, so the
/// moon's phase shows with its lit limb toward the sun and the sun is
/// full; the rest is the sky. The body's `face`, if it has one, is shown
/// once and whole, inscribed. Under the horizontal through the eye there is
/// no disc, so it sets into the sea instead of showing through it. Shaded by
/// `shaders/disc.wesl`.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct DiscMaterial {
    #[uniform(0)]
    pub sun: Vec4,
    #[uniform(0)]
    pub color: LinearRgba,
    /// A nearer body across this one, which hides it: its centre and, in
    /// `z`, its radius, in this disc's radii.
    #[uniform(0)]
    pub bite: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub face: Option<Handle<Image>>,
}

/// No body across a disc: one of no size, far off it.
const NO_BITE: Vec4 = Vec4::new(10., 0., 0., 0.);

impl DiscMaterial {
    fn new(face: Option<Handle<Image>>) -> Self {
        Self { sun: Vec4::Z, color: LinearRgba::WHITE, bite: NO_BITE, face }
    }
}

impl Material for DiscMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/disc.wesl".into()
    }

    /// The unlit part is the sky: nothing of a body is darker than it.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

/// Past the frontier, so the ground hides a disc as it sets, and inside
/// the culling far plane.
fn disc_distance_wu() -> f32 {
    common_bevy::summary::reach_wu() * 1.25
}
/// The sun's disc over the sky's brightness: the sky is the same light
/// scattered, so at one the disc would sink into it. The moon's disc is
/// its face at the albedo painted, sunlit: about a clear day sky's
/// brightness, so it is faint by day and bright at dusk, and brighter
/// would flatten the face's relief against the tonemapper's shoulder.
const SUN_DISC_BRIGHTNESS: f32 = 4.0;
const MOON_DISC_BRIGHTNESS: f32 = 1.0;

/// When the sun rises, stands highest and sets, as fractions of the day
/// from midnight: a sixteen-hour day, its morning shorter than its
/// afternoon.
const SUNRISE: f32 = common_bevy::systems::NIGHT.1 as f32 / 24.;
const NOON: f32 = common_bevy::systems::NOON as f32 / 24.;
const SUNSET: f32 = common_bevy::systems::NIGHT.0 as f32 / 24.;
/// When the moon rises and sets: up from an hour before the sun sets
/// to an hour after it rises, so some light is always up. It stands highest
/// the same share of the way through its time up as the sun does through
/// its own, quick to rise and slow to set as the sun is: a quarter to one
/// in the morning, counted on from its rise.
const MOONRISE: f32 = 21. / 24.;
const MOONSET: f32 = 7. / 24.;
const MOON_HIGH: f32 = MOONRISE + (1. + MOONSET - MOONRISE) * (NOON - SUNRISE) / (SUNSET - SUNRISE);
/// The visitor, the dark thing the world goes once round in a leap season.
/// It gives no light and takes none: across it the sky is a deep violet
/// by day and by night, the stars and the sun behind it hidden, and the
/// light bent round it a pale rim. Where it stands from the sun, how
/// near it is, and what the sun does meanwhile are
/// `common_bevy::sky::visit`'s; it is drawn on the sun's own track that
/// far behind the sun, so it meets the sun where the sun is.
///
/// How wide it is at its nearest, all the while it holds the world.
const VISITOR_WIDE: f32 = 10_f32.to_radians();
/// The share of the sky's light left with the sun wholly covered, and of
/// the sun's halo, which stands about the dark disc.
const ECLIPSE_SKY: f32 = 0.02;
const ECLIPSE_HALO: f32 = 0.3;
/// The sun's light with the sun all but covered, linear: what comes round
/// the visitor comes cold and violet. The halo and what is left of the
/// disc are the light's colour, so they turn with it.
const ECLIPSE_LIGHT: Vec3 = Vec3::new(0.6, 0.5, 1.0);

/// The share of a disc of radius `r` covered by one of radius `over`
/// whose centre is `apart` from its own, all in one measure.
fn covered(apart: f32, r: f32, over: f32) -> f32 {
    if over <= 0. || apart >= r + over {
        return 0.;
    }
    if apart <= (over - r).abs() {
        return (over / r).min(1.).powi(2);
    }
    let lens = r * r * ((apart * apart + r * r - over * over) / (2. * apart * r)).acos()
        + over * over * ((apart * apart + over * over - r * r) / (2. * apart * over)).acos()
        - 0.5 * ((r + over - apart) * (apart + r - over) * (apart - r + over) * (apart + r + over)).sqrt();
    lens / (PI * r * r)
}

/// How far the sun's rise and set swing from east and west at the
/// season's height: its whole circle leans that far south and back.
const SEASON_SWING: f32 = 25_f32.to_radians();

/// A body's orbit angle at day fraction `t`: zero as it rises at `rise`,
/// π/2 at its zenith at `high`, π as it sets at `set`, 3π/2 at its nadir.
/// One smooth curve through the three: a cubic over each of the three
/// stretches between them, its pace at each of the three the harmonic
/// mean of the stretches' own either side, which keeps it turning one
/// way. So the sun eases from its quick morning into its slow afternoon
/// and its quicker night, and no body changes pace at once.
fn orbit(t: f32, rise: f32, high: f32, set: f32) -> f32 {
    // Each stretch: how long it lasts and how far it turns
    let stretches = [((high - rise).rem_euclid(1.), PI / 2.), ((set - high).rem_euclid(1.), PI / 2.), ((rise - set).rem_euclid(1.), PI)];
    let pace = |k: usize| stretches[k % 3].1 / stretches[k % 3].0;
    let at_start = |k: usize| 2. * pace(k) * pace(k + 2) / (pace(k) + pace(k + 2));
    let since_rise = (t - rise).rem_euclid(1.);
    let (mut from, mut turned) = (0., 0.);
    for (k, &(long, turns)) in stretches.iter().enumerate() {
        if since_rise < from + long || k == 2 {
            let u = (since_rise - from) / long;
            let (rise, eased_in, eased_out) = (u * u * (3. - 2. * u), u * (1. - u) * (1. - u), u * u * (u - 1.));
            return turned + turns * rise + long * (at_start(k) * eased_in + at_start(k + 1) * eased_out);
        }
        (from, turned) = (from + long, turned + turns);
    }
    unreachable!("the third stretch takes what is left of the day")
}

/// Unit direction to a body at orbit angle `a`: rising in the east (+x,
/// north being −z as `Heading` has it), setting in the west, its whole
/// circle leaning `tilt` toward the south.
fn toward(a: f32, tilt: f32) -> Vec3 {
    Vec3::new(cos(a), sin(a), tilt).normalize()
}

/// A body's light through the air, by its elevation: none at the horizon,
/// full and white this high. The sun climbs it in three quarters of an
/// hour of the day, which is how long a sunrise or a sunset lasts.
const CLEAR_ABOVE: f32 = 8_f32.to_radians();
/// The sky stays lit by a sun this far below the horizon, and reddens
/// from `GLOW_BELOW` up to `CLEAR_ABOVE`, so the glow is deepest around
/// the crossing: before the sun crests and after it sinks.
const TWILIGHT_BELOW: f32 = 10_f32.to_radians();
const GLOW_BELOW: f32 = 6_f32.to_radians();
/// The share of the sky's tint the haze carries. The haze is the whole
/// sky's light scattered on the way, mostly the pale overhead, so a red
/// dawn sky stands over a pale land and not a red mist.
const HAZE_TINT_SHARE: f32 = 0.3;

/// What the weather over the camera does to the light. A full deck takes
/// this share of the sun's and the moon's direct light and gives this much
/// more back as the sky's; rain and a storm darken both, and the clouds
/// grey the haze.
const CLOUD_SHADE: f32 = 0.85;
const CLOUD_AMBIENT: f32 = 0.6;
const RAIN_GLOOM: f32 = 0.3;
const STORM_GLOOM: f32 = 0.3;
const CLOUD_GREY: f32 = 0.7;
/// How far the haze closes in under full mist, rain and storm, as a share
/// of every distance the clear day's haze reaches: mist to a few hundred
/// units.
const MIST_VISIBILITY: f32 = 0.03;
const RAIN_VISIBILITY: f32 = 0.4;
const STORM_VISIBILITY: f32 = 0.7;
/// How much of the haze's veil full mist and full rain draw over the deck.
const MIST_VEIL: f32 = 0.95;
const RAIN_VEIL: f32 = 0.3;
/// The lightning's flash on the ground and in the haze, at full.
const FLASH_AMBIENT: f32 = 1_500.;
const FLASH_HAZE: f32 = 0.3;
/// A fireball's light, at its steady brightness, on the ground, in the
/// haze and over the clear sky.
const BLAZE_AMBIENT: f32 = 600.;
const BLAZE_HAZE: f32 = 0.06;
const BLAZE_SKY: f32 = 0.12;
/// The sunlit cloud against the sun's light, and the shaded underside
/// against the sky's colour.
const CLOUD_LIGHT: f32 = 0.95;
const CLOUD_UNDERSIDE: f32 = 0.6;
/// The leap season's pall by day: cloud over at least this much of the
/// sky whatever the fronts, and the light dimmed by this share, so noon is
/// dusky and the lightning shows. It lifts with the sky's light.
const OMEN_PALL: f32 = 0.9;
const OMEN_DUSK: f32 = 0.5;

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// The lowest the sun's light is cast from, in radians above the
/// horizon. A shadow is its caster's height over the tangent of the
/// light's elevation, and its tip moves as the inverse square of the
/// sine: under this a hill's shadow crosses the map and its tip runs
/// faster than a player as the sun climbs. The disc and the light's
/// strength and colour keep to the true sun.
const CAST_ABOVE: f32 = 14_f32.to_radians();

/// The direction the sun's light is cast from while the sun stands at
/// `toward`: its own while it is above [`CAST_ABOVE`], and under that the
/// same bearing raised to it.
fn cast_from(toward: Vec3) -> Vec3 {
    let flat = Vec2::new(toward.x, toward.z);
    if elevation(toward) >= CAST_ABOVE || flat.length_squared() < 1e-9 {
        return toward;
    }
    let flat = flat.normalize() * cos(CAST_ABOVE);
    Vec3::new(flat.x, sin(CAST_ABOVE), flat.y)
}

/// The fire of a sunrise or a sunset with the sun at elevation `e`, 0 to
/// 1: it builds from the first of twilight, burns fullest as the sun
/// crosses the horizon and a little above it, and is bleached out as the
/// sun's light turns white.
fn fire(e: f32) -> f32 {
    smoothstep(-TWILIGHT_BELOW, -1_f32.to_radians(), e) * (1. - smoothstep(2_f32.to_radians(), CLEAR_ABOVE, e))
}

/// How far out the stars are with the sun at elevation `e`, 0 to 1: none
/// until it is well under the horizon, all by the end of twilight.
fn starlit(e: f32) -> f32 {
    1. - smoothstep(-TWILIGHT_BELOW, -3_f32.to_radians(), e)
}

/// The fire's colours at their fullest, linear: the ember at the horizon,
/// brighter than white so it glares, and the rose it turns to higher up.
/// They go by the air the light comes through (`weather::Air`): gold and
/// peach through clean air to a deep red through dust, softened to rose
/// by damp, and paled where rain has lately washed it.
const EMBER_CLEAN: Vec3 = Vec3::new(1.5, 0.72, 0.28);
const ROSE_CLEAN: Vec3 = Vec3::new(0.95, 0.55, 0.45);
const EMBER_DUST: Vec3 = Vec3::new(1.7, 0.3, 0.03);
const ROSE_DUST: Vec3 = Vec3::new(0.8, 0.18, 0.24);
const EMBER_DAMP: Vec3 = Vec3::new(1.3, 0.45, 0.32);
const ROSE_DAMP: Vec3 = Vec3::new(0.9, 0.4, 0.58);
const DAMP_SHARE: f32 = 0.6;
const WASHED_SHARE: f32 = 0.7;
/// How much of the fire is left with the far sky toward the sun shut by
/// cloud, through air with no dust in it, and after rain: the three
/// multiply, so the whole of it burns only through dust under an open
/// far sky.
const FIRE_SHUT: f32 = 0.12;
const FIRE_CLEAN: f32 = 0.55;
const FIRE_WASHED: f32 = 0.65;

/// The fire's ember and rose through `air`, and the share of it that
/// burns.
fn crossing(air: common_bevy::weather::Air) -> (Vec3, Vec3, f32) {
    let through = |clean: Vec3, dust: Vec3, damp: Vec3| {
        clean.lerp(dust, air.dust).lerp(damp, DAMP_SHARE * air.damp).lerp(clean, WASHED_SHARE * air.washed)
    };
    let share = (FIRE_SHUT + (1. - FIRE_SHUT) * air.open)
        * (FIRE_CLEAN + (1. - FIRE_CLEAN) * air.dust)
        * (1. - (1. - FIRE_WASHED) * air.washed);
    (through(EMBER_CLEAN, EMBER_DUST, EMBER_DAMP), through(ROSE_CLEAN, ROSE_DUST, ROSE_DAMP), share)
}
/// The clouds the low sun has lit longest, and a cloud's hem against the
/// sun at the crossing; by day a hem is white. None glares: a cloud
/// brighter than white draws as a white blot about the sun.
const GOLD: Vec3 = Vec3::new(1.1, 0.66, 0.27);
const GILT: Vec3 = Vec3::new(1.15, 0.95, 0.68);
const HEM_BY_DAY: f32 = 1.05;
/// The share of the horizon's ember a cloud's underside burns at.
const CLOUD_EMBER: f32 = 0.65;
/// The share of the crossing's rays a high sun still sends through the
/// gaps in the clouds.
const RAYS_BY_DAY: f32 = 0.3;

/// Where the low sun's light has reached on the cloud deck with the sun
/// at elevation `e`, along the sun's bearing from the camera as a share
/// of the way to the horizon: the deck is lit from there toward the
/// sun. The sun under the horizon lights the deck from beneath, the far
/// clouds on its own side first and the whole deck as it crosses.
fn caught_from(e: f32) -> f32 {
    0.95 - 2.1 * smoothstep(-9_f32.to_radians(), 0.5_f32.to_radians(), e)
}

/// The clear sky overhead while the fire burns: the day's blue gone deep.
const DUSK_SKY: Vec3 = Vec3::new(0.09, 0.16, 0.5);
/// The sun's halo, the same by day and at the crossing: a brighter one
/// whitens the sky further out, and the sun grows as it nears the horizon.
const HALO: f32 = 0.5;
/// The share of it left as the sun stands on the horizon, where its direct
/// light is spent, and how far under the sun is when the last of it goes.
const HALO_LOW: f32 = 0.8;
/// How high the haze stands on the sky, as sines of elevation: through
/// clear air, and through the thickest, which is mist, rain, a storm or
/// `HAZE_DUST` of the air's dust.
const HAZE_TOP_CLEAR: f32 = 0.14;
const HAZE_TOP_THICK: f32 = 0.25;
const HAZE_DUST: f32 = 0.6;
const HALO_UNDER: f32 = 3_f32.to_radians();
/// The clear sky on a moonless night: never black, so the stars stand on
/// a deep blue.
const NIGHT_SKY: Vec3 = Vec3::new(0.0035, 0.006, 0.016);
/// How much of the stars a full moon high in the sky washes out.
const MOON_WASH: f32 = 0.5;

/// Elevation of a direction above the horizontal, in radians.
fn elevation(toward: Vec3) -> f32 {
    toward.y.asin()
}

/// A body's direct light at elevation `e`, as a share of its full.
fn direct(e: f32) -> f32 {
    smoothstep(0., CLEAR_ABOVE, e)
}

/// The colour of a `share` of a body's light: the air takes the blue
/// first, so it is red as the share goes and white at full.
fn tint(share: f32) -> Color {
    Color::linear_rgb(1., share, share)
}

use crate::{
    components::{Moon, Sun},
    plugins::weather::{CloudDeck, CloudMaterial, SkyWeather, DECK_HEIGHT},
    systems::camera::HAZE_COLOR,
    resources::{
        LodTriangleStats, LoadedChunks,
        Server, SummaryMesh, SummaryMeshBuildResult, SummaryMeshState, SummaryMeshes,
        TerrainMaterial,
    },
};
use common_bevy::{
    chunk::{self, loc_to_chunk, CHUNK_EXTENT_WU},
    components::{ *,
        entity_type::*,
    },
    message::{Event, *},
    systems::*,
};

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut disc_materials: ResMut<Assets<DiscMaterial>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    assets: Res<AssetServer>,
) {
    commands.insert_resource(
        GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 10.,
            ..default()});

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_depth_bias: 0.02,
            shadow_normal_bias: 0.6,
            ..default()},
        Transform::default(),
        Sun::default()));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: false,
            color: Color::WHITE,
            ..default()},
        Transform::default(),
        Moon::default()));

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(sky_radius_wu()).mesh().ico(3).expect("a sphere subdivides"))),
        MeshMaterial3d(sky_materials.add(SkyMaterial {
            sun: Vec4::X,
            base: LinearRgba::BLACK,
            ember: LinearRgba::BLACK,
            rose: LinearRgba::BLACK,
            halo: LinearRgba::BLACK,
            haze: LinearRgba::BLACK,
            night: Vec4::new(0., 0., HAZE_TOP_CLEAR, 0.),
            turn_east: Vec4::X,
            turn_up: Vec4::Y,
            turn_south: Vec4::Z,
            guides: common_bevy::sky::guides(),
            band: common_bevy::sky::band().extend(0.),
            core: common_bevy::sky::core().extend(0.),
            nebulae: common_bevy::sky::nebulae(),
            visitor: Vec4::X,
        })),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        SkyDome));

    // The moon's face declares its mip chain; the sampler reads it,
    // trilinear, so the disc small in the frame is the face's mean.
    let moon_face = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        })
        .load("textures/moon.dds");
    let disc = |diameter: f32| Circle::new(disc_distance_wu() * (diameter / 2.).tan()).mesh().resolution(48);
    for (which, diameter, face) in [(Disc::Sun, SUN_ANGULAR_DIAMETER, None), (Disc::Moon, MOON_ANGULAR_DIAMETER, Some(moon_face))] {
        commands.spawn((
            Mesh3d(meshes.add(disc(diameter))),
            MeshMaterial3d(disc_materials.add(DiscMaterial::new(face))),
            Transform::default(),
            NotShadowCaster,
            NotShadowReceiver,
            which));
    }
}

// ─────────────────────────────────────────────────────────
// SYSTEM 1: Server-authoritative chunk eviction
// ─────────────────────────────────────────────────────────
// Server sends EvictChunks when chunks leave the player's visibility.
// Client removes tiles, meshes, and actors on those chunks.

/// Process EvictChunks messages from the server.
/// Removes tiles and loaded_chunks tracking. Mesh lifecycle is handled
/// by dispatch_summary_tasks (auto-evicts regions when chunks disappear).
pub fn evict_data(
    mut commands: Commands,
    mut reader: MessageReader<Do>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut l2r: ResMut<crate::resources::EntityMap>,
    map: Res<common_bevy::resources::map::Map>,
    mut changes: ResMut<crate::systems::gathering::CoverChanges>,
    actor_query: Query<(Entity, &Loc, &EntityType)>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let mut all_evicted = Vec::new();

    for message in reader.read() {
        let Do { event: Event::EvictChunks { chunks, .. } } = message else { continue };
        all_evicted.extend_from_slice(chunks);
    }

    if all_evicted.is_empty() { return; }
    let _t = client_timers.scope("evict");

    // Despawn actors on evicted chunks
    for (entity, loc, entity_type) in actor_query.iter() {
        let actor_chunk = loc_to_chunk(**loc);
        if all_evicted.contains(&actor_chunk) {
            let is_player = matches!(
                entity_type,
                EntityType::Actor(actor_impl) if matches!(
                    actor_impl.identity,
                    common_bevy::components::entity_type::actor::ActorIdentity::Player
                )
            );
            if !is_player {
                l2r.remove_by_left(&entity);
                commands.entity(entity).despawn();
            }
        }
    }

    // Remove tiles from map (triggers mesh rebuild via changed flag)
    for &chunk_id in &all_evicted {
        map.remove_chunk(chunk_id);
    }
    // A chunk sent again arrives with the server's changes already in it.
    changes.0.retain(|&(q, r), _| !all_evicted.contains(&loc_to_chunk(qrz::Qrz { q, r, z: 0 })));
    loaded_chunks.evict(&all_evicted);
}

pub fn do_init(
    mut reader: MessageReader<Do>,
    mut server: ResMut<Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Do { event: Event::Init { dt, calendar, .. } } = message else { continue };
        let dt = *dt;
        let client_now = time.elapsed().as_millis();
        server.sync(dt, client_now);
        server.calendar = *calendar;
        server.last_ping_time = client_now;
    }
}

/// Takes the world's calendar as an admin changed it.
pub fn do_calendar(
    mut reader: MessageReader<Do>,
    mut server: ResMut<Server>,
) {
    for message in reader.read() {
        let Do { event: Event::Calendar { calendar } } = message else { continue };
        server.calendar = *calendar;
    }
}

/// Takes the server's settings as it tells of them.
pub fn do_settings(mut reader: MessageReader<Do>, mut settings: ResMut<common_bevy::settings::Settings>) {
    for message in reader.read() {
        let Do { event: Event::Settings { settings: told } } = message else { continue };
        settings.set_if_neq(*told);
    }
}

pub fn do_spawn(
    mut reader: MessageReader<Do>,
    map: Res<common_bevy::resources::map::Map>,
    changes: Res<crate::systems::gathering::CoverChanges>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.scope("do_spawn");
    for message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };
        map.insert(*qrz, changes.laid_over(*qrz, EntityType::Decorator(*decorator)));
    }
}


#[allow(clippy::type_complexity)]
pub fn update(
    time: Res<Time>,
    mut q_sun: Query<(&mut DirectionalLight, &mut Transform, &mut CascadeShadowConfig), (With<Sun>,Without<Moon>)>,
    mut q_moon: Query<(&mut DirectionalLight, &mut Transform), (With<Moon>,Without<Sun>)>,
    mut a_light: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut q_camera: Query<(&GlobalTransform, &mut DistanceFog)>,
    mut q_discs: Query<(&Disc, &mut Transform, &MeshMaterial3d<DiscMaterial>), (Without<Sun>, Without<Moon>)>,
    mut disc_materials: ResMut<Assets<DiscMaterial>>,
    q_sky: Query<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    (weather, q_clouds, mut cloud_materials): (Res<SkyWeather>, Query<&MeshMaterial3d<CloudMaterial>, With<CloudDeck>>, ResMut<Assets<CloudMaterial>>),
    server: Res<Server>,
    player_query: Query<&Loc, With<crate::components::Viewed>>,
) {
    // The sky keeps wall-clock time (`Server::wall`), whole milliseconds
    // until each fraction below. The sun keeps the clock's hour, noon at
    // 12:00; its path swings with the year's phase, and the moon's phase
    // runs once per season. About a leap season the year the sky keeps
    // is the visit's: where the visitor took the world, and as far on as
    // the sun has moved among the stars since.
    let wall = server.wall(server.now(time.elapsed().as_millis()));
    let dtd = (wall % DAY_MS) as f32 / DAY_MS as f32;
    let dtm = (wall - Date::season_start(wall)) as f32 / SEASON_MS as f32;
    let visit = Date::visitor(wall).map(|t| (t as f32, common_bevy::sky::visit(t as f32)));
    let taken = 1. + VISIT_FROM as f32 / (4 * SEASON_MS) as f32;
    let dty = visit.map_or(Date::turned(wall) as f32, |(_, visit)| taken + visit.sun);
    // The sun's disc, as a share of its own, by how far off the sun is.
    let s_near = visit.map_or(1., |(_, visit)| 1. / visit.far);

    // sun
    let (mut s_light, mut s_transform, mut cascade_config) = q_sun.single_mut().expect("no result in q_sun");
    let s_orbit = orbit(dtd, SUNRISE, NOON, SUNSET);
    let s_tilt = tan(SEASON_SWING) * cos(dty * 2. * PI);
    let s_toward = toward(s_orbit, s_tilt);
    let s_elevation = elevation(s_toward);

    // The visitor: behind the sun on the sun's track by as much of a day
    // as it stands east of it, and as wide as it is near. It fades in
    // over the first tenth of the season before and out over the last
    // of the season after, where it is a speck.
    let (v_behind, v_wide) = visit.map_or((0., 0.), |(t, visit)| {
        (visit.east, VISITOR_WIDE * visit.near * smoothstep(-1., -0.9, t) * (1. - smoothstep(1.9, 2., t)))
    });
    let v_toward = toward(orbit(dtd - v_behind, SUNRISE, NOON, SUNSET), s_tilt);
    let open = 1. - covered(s_toward.angle_between(v_toward), s_near * SUN_ANGULAR_DIAMETER / 2., v_wide / 2.);

    let s_direct = direct(s_elevation);
    let sky_bright = smoothstep(-TWILIGHT_BELOW, CLEAR_ABOVE, s_elevation) * (ECLIPSE_SKY + (1. - ECLIPSE_SKY) * open);
    // The stars' dark: the night's, or the sun wholly covered.
    let dark = starlit(s_elevation).max(1. - smoothstep(0., 0.05, open));
    // The sky is the sun's light scattered, so it turns with it: under a
    // deck the sky's light is most of what reaches the ground.
    let eclipsed = Vec3::ONE.lerp(ECLIPSE_LIGHT, 1. - open);
    let sky_tint = tint(smoothstep(-GLOW_BELOW, CLEAR_ABOVE, s_elevation));
    // The weather over the camera: the deck takes the direct light and
    // gives some back as the sky's, and rain, a storm and the leap
    // season's pall darken both.
    let here = weather.here;
    let pall = weather.omen * sky_bright;
    let cover = here.cover.max(OMEN_PALL * pall);
    let dusk = 1. - OMEN_DUSK * pall;
    let shaded = 1. - CLOUD_SHADE * cover;
    let gloom = (1. - RAIN_GLOOM * here.rain - STORM_GLOOM * here.storm) * dusk;
    let s_color = tint(s_direct).to_linear().to_vec3().lerp(ECLIPSE_LIGHT, 1. - open);
    s_light.color = Color::linear_rgb(s_color.x, s_color.y, s_color.z);
    s_light.illuminance = SUN_ILLUMINANCE * s_direct * open * shaded * gloom;
    *s_transform = Transform::from_translation(cast_from(s_toward) * 1_000.).looking_at(Vec3::ZERO, Vec3::Y);

    // The sky: lit through twilight, reddened around the crossing, and
    // the ambient is its light on the ground, blue as it is overhead, and
    // the lightning's and the fireballs' while they burn.
    let sky_light = Vec3::new(0.7 + 0.3 * sky_bright, 0.8 + 0.2 * sky_bright, 1.0) * eclipsed
        * (800. * sky_bright * (1. + CLOUD_AMBIENT * cover) * gloom + FLASH_AMBIENT * weather.flash)
        + BLAZE_AMBIENT * weather.blaze;
    a_light.brightness = sky_light.max_element();
    let hue = sky_light / a_light.brightness.max(1e-6);
    a_light.color = Color::linear_rgb(hue.x, hue.y, hue.z);

    // moon
    let (mut m_light, mut m_transform) = q_moon.single_mut().expect("no result in q_moon");
    let m_orbit = orbit(dtd, MOONRISE, MOON_HIGH, MOONSET);
    let m_toward = toward(m_orbit, 0.);
    // The month turns the moon from full to new and back: its light is
    // its lit fraction, and its disc is lit from that far round from the
    // camera, on the sun's side.
    let m_elongation = dtm * 2. * PI;
    let m_lit = (1. + cos(m_elongation)) / 2.;
    let m_phase = 0.1 + 0.9 * m_lit;
    let m_clear = MOON_ILLUMINANCE * m_phase * direct(elevation(m_toward));
    m_light.illuminance = m_clear * shaded * gloom;
    *m_transform = Transform::from_translation(m_toward * 1_000.).looking_at(Vec3::ZERO, Vec3::Y);

    // The sky is the sun's light scattered, so it carries the sky's
    // brightness and the moon's share at night, with the sun's tint as a
    // glow toward the sun; the haze carries the brightness and a share
    // of the tint. Neither stays daylight-grey over a dark ground. The
    // clear sky is the same over any weather, seen through its gaps; the
    // haze is the air about the camera, greyed under the deck, darkened
    // by the rain and lit by the flash.
    let moon = m_light.color.to_linear().to_vec3() * (m_clear / SUN_ILLUMINANCE);
    let sky_tint = sky_tint.to_linear().to_vec3();
    let lit = |tint: Vec3| LinearRgba::from_vec3(HAZE_COLOR.to_linear().to_vec3() * (tint * eclipsed * sky_bright + moon));
    let haze = lit(Vec3::ONE.lerp(sky_tint, HAZE_TINT_SHARE)).to_vec3();
    let grey = LinearRgba::from_vec3(haze).luminance();
    let haze = LinearRgba::from_vec3(
        haze.lerp(Vec3::splat(grey), CLOUD_GREY * cover) * gloom
            + Vec3::splat(FLASH_HAZE * weather.flash * grey.max(0.05))
            + BLAZE_HAZE * weather.blaze,
    );
    clear.0 = haze.into();
    let (camera, mut fog) = q_camera.single_mut().expect("no result in q_camera");
    fog.color = haze.into();
    let visibility = (1. - (1. - MIST_VISIBILITY) * here.mist)
        * (1. - (1. - RAIN_VISIBILITY) * here.rain)
        * (1. - (1. - STORM_VISIBILITY) * here.storm);
    fog.falloff = FogFalloff::Exponential { density: crate::systems::camera::haze_density() / visibility };
    // The fire of the crossing: lit by what light is left, dimmed by the
    // weather, and made of the air its light comes through; the stars
    // come out as it dies, less under a bright moon.
    // A covered sun lights no air past the horizon, so it raises no
    // fire, and what a part-covered one raises is its light's colour.
    let sunward = s_toward.xz().try_normalize().unwrap_or(Vec2::X);
    let air = weather.forecast.air(wall, weather.at.0, weather.at.1, (sunward.x as f64, sunward.y as f64));
    let (ember, rose, share) = crossing(air);
    // The halo is the sun's own glare, not the fire's: it stands about
    // the disc until the disc is under, through whatever far cloud lets.
    let s_glare = s_direct.max(HALO_LOW * air.open.max(FIRE_SHUT) * smoothstep(-HALO_UNDER, 1_f32.to_radians(), s_elevation));
    // Its colour low down is the fire's, not the light's: the light on
    // the ground goes to pure red, too dark a colour to glare.
    let s_glow = (ember / ember.max_element()).lerp(Vec3::ONE, s_direct).lerp(ECLIPSE_LIGHT, 1. - open);
    let burning = fire(s_elevation) * open * share;
    let left = (0.3 + 0.7 * sky_bright) * gloom;
    let (ember, rose) = (ember * left * eclipsed, rose * left * eclipsed);
    let stars = dark * (1. - MOON_WASH * m_lit * direct(elevation(m_toward))) * gloom;
    let bearing = s_toward.xz().try_normalize().unwrap_or(Vec2::X).extend(burning).extend(s_toward.y);
    if let Some(mut sky) = q_sky.single().ok().and_then(|m| sky_materials.get_mut(&m.0)) {
        let sphere = Mat3::from_quat(common_bevy::sky::turning(dtd, dty).inverse());
        sky.sun = s_toward.extend(burning);
        let day = lit(Vec3::ONE).to_vec3().lerp(DUSK_SKY * left, 0.9 * burning);
        sky.base = LinearRgba::from_vec3(day + BLAZE_SKY * weather.blaze + NIGHT_SKY * dark);
        sky.ember = LinearRgba::from_vec3(ember);
        sky.rose = LinearRgba::from_vec3(rose);
        sky.halo = LinearRgba::from_vec3(s_glow * HALO * s_glare * (ECLIPSE_HALO + (1. - ECLIPSE_HALO) * open) * gloom * (1. - 0.6 * cover));
        sky.haze = haze;
        let thick = (1. - visibility).max(HAZE_DUST * air.dust);
        sky.night = Vec4::new(stars, 1. - open, HAZE_TOP_CLEAR + (HAZE_TOP_THICK - HAZE_TOP_CLEAR) * thick, 1. - visibility);
        sky.visitor = v_toward.extend(2. * sin(v_wide / 4.));
        (sky.turn_east, sky.turn_up, sky.turn_south) = (sphere.x_axis.extend(0.), sphere.y_axis.extend(0.), sphere.z_axis.extend(0.));
    }

    // The deck: lit white as the sky is bright, its underside the sky's
    // own colour shaded, so a cloud the low sun has not reached stands
    // dark; about the crossing the sun's light caught from beneath, on
    // the hems and through the gaps; the weather over each place is the
    // map's. A far cloud fades by the fog's rule.
    if let Some(mut clouds) = q_clouds.single().ok().and_then(|m| cloud_materials.get_mut(&m.0)) {
        let light = eclipsed * sky_bright + moon;
        let reaching = smoothstep(-7_f32.to_radians(), -2_f32.to_radians(), s_elevation) * gloom * open;
        clouds.light = LinearRgba::from_vec3(light * CLOUD_LIGHT * dusk);
        clouds.shade = LinearRgba::from_vec3(lit(Vec3::ONE).to_vec3().lerp(light, 0.3) * CLOUD_UNDERSIDE * dusk);
        clouds.glow = LinearRgba::from_vec3(ember * CLOUD_EMBER);
        clouds.rose = LinearRgba::from_vec3(rose);
        clouds.gold = LinearRgba::from_vec3(GOLD * left * eclipsed);
        clouds.lining = LinearRgba::from_vec3(GILT.lerp(Vec3::splat(HEM_BY_DAY), s_direct)).with_alpha(reaching);
        clouds.bearing = bearing;
        clouds.dawn = Vec4::new(caught_from(s_elevation), (1.5 * burning).min(1.), s_direct * open, burning.max(RAYS_BY_DAY * s_direct * open));
        clouds.deck = Vec4::new(DECK_HEIGHT, crate::systems::camera::haze_density() / visibility, weather.flash, OMEN_PALL * pall);
        clouds.frame = weather.frame;
        clouds.drift = weather.drift.extend(0.).extend(0.);
        clouds.veil = LinearRgba { alpha: (MIST_VEIL * here.mist + RAIN_VEIL * here.rain).min(1.), ..haze };
        clouds.bolts = weather.bolts;
        clouds.bolt_looks = weather.bolt_looks;
        clouds.meteor_tails = weather.meteor_tails;
        clouds.meteor_paths = weather.meteor_paths;
        clouds.blaze = weather.blaze.extend(0.);
    }

    // Each disc faces the camera from its body's direction, in its light's
    // colour; the ground occludes it as it sets and its shader cuts it at
    // the horizontal.
    let camera = camera.translation();
    for (disc, mut transform, material) in &mut q_discs {
        let toward = match disc { Disc::Sun => s_toward, Disc::Moon => m_toward };
        transform.translation = camera + toward * disc_distance_wu();
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, -toward);
        let Some(mut material) = disc_materials.get_mut(&material.0) else { continue };
        match disc {
            Disc::Sun => {
                material.color = s_light.color.to_linear() * SUN_DISC_BRIGHTNESS;
                // Where the visitor stands on the sun's disc, in its radii.
                transform.scale = Vec3::splat(s_near);
                let across = transform.rotation.inverse() * v_toward;
                let radius = s_near * tan(SUN_ANGULAR_DIAMETER / 2.);
                material.bite = if v_wide > 0. && across.z < 0. {
                    (across.xy() / -across.z / radius).extend(tan(v_wide / 2.) / radius).extend(0.)
                } else {
                    NO_BITE
                };
            }
            Disc::Moon => {
                let facing = -m_toward;
                let sunward = (s_toward - s_toward.dot(facing) * facing).try_normalize().unwrap_or(Vec3::Y);
                let light = facing * cos(m_elongation) + sunward * sin(m_elongation).abs();
                material.sun = (transform.rotation.inverse() * light).extend(0.);
                material.color = m_light.color.to_linear() * MOON_DISC_BRIGHTNESS;
            }
        }
    }

    // Shadows reach 80% of the streamed tile radius, inside the tiles: the
    // summaries beyond are too coarse to shadow. The haze is slight there
    // and does not hide the seam. maximum_distance is measured from the
    // camera, not the player, so add the camera-to-player distance.
    if let Ok(player_loc) = player_query.single() {
        use crate::systems::camera::{CAMERA_DISTANCE, MAX_GAMEPLAY_FOV, camera_height};
        let height = camera_height(MAX_GAMEPLAY_FOV);
        let loading_r = chunk::terrain_chunk_radius(player_loc.z) as f32;
        let camera_to_player = (CAMERA_DISTANCE * CAMERA_DISTANCE + height * height).sqrt();
        let max_dist = camera_to_player + loading_r * 0.8 * CHUNK_EXTENT_WU;
        let current_max = cascade_config.bounds.last().copied().unwrap_or(0.0);
        if (max_dist - current_max).abs() > 1.0 {
            *cascade_config = CascadeShadowConfigBuilder {
                maximum_distance: max_dist,
                // Bounds are distances from the camera, and the boom keeps
                // it this far from the player: a nearer bound spends the
                // finest map on the air between them.
                first_cascade_far_bound: camera_to_player * FINE_SHADOWS_PAST_PLAYER,
                ..default()
            }.into();
        }
    }
}

/// How far the finest shadow map reaches, in the resting camera's distances
/// to the player: past the player, so its far seam never lies at their feet
/// and the ground they act on is all in it.
const FINE_SHADOWS_PAST_PLAYER: f32 = 1.3;

/// Keeps the sky dome centred on the camera. After camera movement, so
/// it never lags a frame and shows its own horizon.
pub fn follow_camera(
    camera: Query<&Transform, (With<Camera3d>, Without<SkyDome>, Without<crate::systems::closeup::CloseupCamera>)>,
    mut sky: Query<&mut Transform, With<SkyDome>>,
) {
    let Ok(cam) = camera.single() else { return };
    for mut transform in &mut sky {
        transform.translation = cam.translation;
    }
}

/// Camera movement (WU) that forces a region re-evaluation even without new
/// map or cache data. Band edges are player-centric — without this, bands
/// only advance on chunk/summary arrival and then snap in bursts.
const REEVAL_MOVE_WU: f32 = 16.0;

/// Hysteresis margin on band edges: a region refines/coarsens only when it
/// is clearly past the threshold. Existing meshes within the margin survive
/// (keep set); new meshes only build inside the crisp band (needed set).
/// Prevents threshold flapping as the player oscillates near a band edge.
const BAND_HYSTERESIS_MARGIN: f32 = 0.08;

/// World-space center of a mesh region.
fn region_center_world(key: &common_bevy::summary_mesh::MeshRegionKey) -> (f32, f32) {
    let origin = key.origin();
    common_bevy::geometry::flat_top_tile_center(origin.q, origin.r, 1.0)
}

/// Dispatches async builds of the summary mesh regions the bands want,
/// every level from the tiles out through one pipeline.
pub fn dispatch_summary_tasks(
    mut commands: Commands,
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut summary_meshes: ResMut<SummaryMeshes>,
    summary_cache: Res<crate::resources::SummaryCache>,
    dens: Res<crate::systems::den::Dens>,
    client_timers: Res<crate::resources::ClientTimers>,
    edges: Res<crate::resources::EdgeCenters>,
    origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, With<crate::components::Viewed>>,
    mut last_eval_pos: Local<Option<Vec3>>,
    mut last_eval_edges: Local<HashMap<u32, Vec2>>,
    mut backlog: Local<bool>,
) {
    // The camera's world position, for the regions: the player is drawn
    // about the render origin.
    let camera_pos = player_query.single().ok().map(|t| origin.world(t.translation));

    let map_changed = map.take_changed();
    let cache_changed = summary_cache.take_new_data();
    for key in summary_cache.take_revised() {
        if let Some(state) = summary_meshes.states.get_mut(&key) {
            state.stale = true;
        }
    }
    let moved = match (*last_eval_pos, camera_pos) {
        (Some(prev), Some(pos)) => prev.distance_squared(pos) >= REEVAL_MOVE_WU * REEVAL_MOVE_WU,
        (None, Some(_)) => true,
        _ => false,
    };
    // A lagging edge catching up sweeps its own sets across the ground, so
    // it re-evaluates as the player does.
    let edge_moved = edges.0.iter().any(|(r, c)| {
        last_eval_edges.get(r).map_or(true, |p| p.distance_squared(*c) >= REEVAL_MOVE_WU * REEVAL_MOVE_WU)
    });
    // A run that exhausted its budget leaves a backlog, picked up next frame.
    let data_changed = map_changed || cache_changed;
    if !data_changed && !moved && !edge_moved && !*backlog {
        return;
    }
    *backlog = false;
    if data_changed {
        summary_meshes.epoch += 1;
    }
    let epoch = summary_meshes.epoch;
    if let Some(pos) = camera_pos {
        *last_eval_pos = Some(pos);
    }
    *last_eval_edges = edges.0.clone();
    let _t = client_timers.scope("sum_disp");

    // Local-data boundary: the largest circle inside guaranteed chunk
    // coverage (the hexagonal chunk set's APOTHEM — the circumradius
    // over-claims by ~80 WU in edge directions): the server streams chunks
    // to FIXED_STREAM_RADIUS.
    let local_boundary = common_bevy::chunk::FIXED_STREAM_APOTHEM_WU;

    // Local regions: camera-dependent (bands within streaming radius).
    // `needed` is the crisp band assignment (what to build); `keep` widens
    // each band edge by the hysteresis margin (what may survive).
    let (needed, keep): (
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
    ) = match camera_pos {
        Some(pos) => {
            let regions = |at: Vec2, margin: f32| {
                compute_band_regions(at, &loaded_chunks.chunks, margin, local_boundary)
            };
            let mut n = regions(pos.xz(), 0.0);
            let mut k = regions(pos.xz(), BAND_HYSTERESIS_MARGIN);
            // An edge lagging the player draws both its levels around its
            // own centre until it catches up: what it waits on is built,
            // and what it shows survives.
            for centre in edges.0.values() {
                if centre.distance_squared(pos.xz()) < 1e-4 {
                    continue;
                }
                n.extend(regions(*centre, 0.0));
                k.extend(regions(*centre, BAND_HYSTERESIS_MARGIN));
            }
            k.extend(n.iter().copied());
            (n, k)
        }
        None => {
            // No camera yet — can't compute local regions.
            // Re-arm map changed so we retry once the player spawns.
            if map_changed { map.force_changed(); }
            (std::collections::HashSet::new(), std::collections::HashSet::new())
        }
    };

    // Evict mesh regions outside the keep set — but only once every needed
    // region overlapping the stale region's footprint has a live entity.
    // Coverage never drops during a band transition (build-before-evict):
    // the old level's plate stays until its replacement is on screen.
    let stale: Vec<common_bevy::summary_mesh::MeshRegionKey> = summary_meshes
        .states
        .keys()
        .filter(|k| !keep.contains(k))
        .copied()
        .collect();
    for key in stale {
        let (kx, kz) = region_center_world(&key);
        let k_half = 0.5 * common_bevy::summary::mesh_region_extent_wu(key.r);
        let covered = needed.iter().all(|n| {
            let (nx, nz) = region_center_world(n);
            let n_half = 0.5 * common_bevy::summary::mesh_region_extent_wu(n.r);
            let dx = nx - kx;
            let dz = nz - kz;
            let reach = k_half + n_half;
            if dx * dx + dz * dz > reach * reach {
                return true; // doesn't overlap the stale region
            }
            summary_meshes
                .states
                .get(n)
                .map_or(false, |s| s.entity.is_some())
        });
        if !covered {
            continue; // replacement not on screen yet — hold the old mesh
        }
        summary_meshes.drop_region(&key, &mut commands);
    }

    // Dispatch build tasks for needed regions, nearest-first so the terrain
    // in front of the camera fills before the horizon (matches the server's
    // dispatch order).
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
    const MAX_MESH_TASKS: usize = 16;
    let mut mesh_dispatched = 0;
    let mut queued = 0;

    let mut ordered: Vec<(f32, common_bevy::summary_mesh::MeshRegionKey)> = needed
        .iter()
        .map(|region_key| {
            let (wx, wz) = region_center_world(region_key);
            let d2 = camera_pos.map_or(0.0, |pos| {
                let dx = wx - pos.x;
                let dz = wz - pos.z;
                dx * dx + dz * dz
            });
            (d2, *region_key)
        })
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (d2, region_key) in ordered {
        let radius = region_key.r;

        // Built regions are final: their heights are durable. A region
        // still waiting for data is retried once data has arrived since its
        // build was dispatched, judged by epoch and never by this run's
        // flag alone: the flag is consumed whether or not the region could
        // act on it, and it cannot while a build is in flight or once the
        // task budget is spent. The one level standing the tiles' cover is
        // final only once it holds the tiles too: its ground may have been
        // built from the summaries, which arrive first.
        let tiles_loaded = radius != common_bevy::summary::LOD_LEVELS[1]
            || common_bevy::summary_mesh::region_tiles_loaded(region_key, &loaded_chunks.chunks);
        if summary_meshes.states.get(&region_key).is_some_and(|state| !state.wants_build(epoch, tiles_loaded)) {
            continue;
        }

        // r>0 regions beyond the local-data boundary are server-owned:
        // without cache data their build would produce nothing — wait for it.
        // Regions within the boundary are Map-built and dispatch regardless
        // of cache state.
        if radius > 0 && !summary_cache.contains_region(&region_key) {
            let reach = local_boundary
                + 0.5 * common_bevy::summary::mesh_region_extent_wu(radius);
            let is_local = camera_pos.is_none() || d2 <= reach * reach;
            if !is_local {
                continue;
            }
        }

        if mesh_dispatched >= MAX_MESH_TASKS {
            queued += 1;
            continue;
        }

        let rk = region_key;
        let map_snap = map.clone();
        let cache_snap = summary_cache.clone();
        let dens_snap = dens.0.clone();

        let task = pool.spawn(async move {
            collect_and_build_summary_mesh(radius, rk, &map_snap, &cache_snap, &dens_snap)
        });

        if let Some(state) = summary_meshes.states.get_mut(&region_key) {
            state.task = Some(task);
            state.epoch = epoch;
            state.tiles_loaded = tiles_loaded;
            state.stale = false;
        } else {
            summary_meshes.states.insert(
                region_key,
                SummaryMeshState {
                    task: Some(task),
                    entity: None,
                    mesh_handle: None,
                    tri_count: 0,
                    cover: Vec::new(),
                    models_spawned: false,
                    cards_spawned: false,
                    waiting: false,
                    tiles_loaded,
                    stale: false,
                    epoch,
                },
            );
        }
        mesh_dispatched += 1;
    }

    // Regions left undispatched with the budget spent are picked up next frame.
    summary_meshes.queued = queued;
    *backlog = queued > 0;
}

/// Active bands out to the reach, the same bound the server's producer
/// covers to. `margin` widens it for the keep set.
fn horizon_bands(margin: f32) -> Vec<common_bevy::summary::Band> {
    common_bevy::summary::compute_active_bands(common_bevy::summary::reach_wu() * (1.0 + margin))
}

/// Time constant of an edge's easing toward the player: a jump — a
/// teleport, or the catch-up after a wait for data — settles within about
/// half a second, sweeping the strip across the ground, and a walk lags by
/// a step.
const EDGE_EASE_S: f32 = 0.15;

/// Points around an edge's circle at which both levels must be on screen
/// before the edge moves there. Two degrees apart: closer than any region
/// is wide at the distance of any edge.
const EDGE_SAMPLES: u32 = 180;

/// How far behind the player an edge may fall, as a fraction of its
/// radius, before it snaps to the player: a teleport, or a wait for data
/// that outlasts a walk. Beyond it the plate the edge holds is far from
/// where its level is due, and for the finest level its tiles may be gone.
const EDGE_MAX_LAG: f32 = 0.5;

/// Upload the band cut: each edge's centre, each level's band, and the
/// morph strip inside its outer edge. Regions are built whole and the
/// shaders drop fragments outside the band, so band edges follow the
/// player with no rebuild. An edge moves only where both its levels are
/// drawn, and eases when it does.
/// The edges live in world coordinates, the frame of the region lattice;
/// the uniform carries them as rendered, the frame the shaders see.
pub fn update_terrain_cut(
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    terrain_material: Res<TerrainMaterial>,
    summary_meshes: Res<SummaryMeshes>,
    mut edges: ResMut<crate::resources::EdgeCenters>,
    mut card_band: ResMut<crate::resources::CardBand>,
    time: Res<Time>,
    render_origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, With<crate::components::Viewed>>,
) {
    let Some(origin) = player_query.single().ok().map(|t| render_origin.world(t.translation)) else { return };

    let bands = horizon_bands(0.0);
    let target = origin.xz();
    advance_edges(&mut edges.0, &bands, target, time.delta_secs(), &summary_meshes);

    // A material is written only where its cut, a switch or the render
    // origin has changed: a changed material is prepared again and every
    // region wearing it specialised again, which for the terrain is every
    // mesh it has.
    let terrain_origin = crate::resources::TerrainOrigin::of(&render_origin);
    let mut band = *card_band;
    for (&r, handle) in &terrain_material.by_level {
        let cut = level_cut(r, &bands, &edges.0, target).rendered(render_origin.world_vec().xz());
        let Some(material) = materials.get(handle) else { continue };
        let on = 1;
        let look = terrain_material.canopy_for(r);
        let extension = &material.extension;
        if extension.cut != cut || extension.lattice.on != on || extension.canopy != look || extension.origin != terrain_origin {
            if let Some(mut material) = materials.get_mut(handle) {
                material.extension.cut = cut;
                material.extension.lattice.on = on;
                material.extension.canopy = look;
                material.extension.origin = terrain_origin;
            }
        }
        // The ring is the tiles' own outer edge, with their ground's
        // overlap; the cards dissolve out across the outer half of the
        // first summary level's band, never narrower than its morph.
        if r == 0 {
            band.center = cut.outer_center;
            band.inner = if cut.fade > 0.0 { cut.outer } else { 0.0 };
            band.overlap = cut.fade;
        }
        if r == common_bevy::summary::LOD_LEVELS[1] {
            band.dissolve = cut.outer_center.extend(cut.outer).extend(((cut.outer - cut.inner) / 2.0).max(cut.fade));
            band.far_in = cut.outer_center.extend(cut.outer).extend(cut.fade);
        }
        if r == common_bevy::summary::LOD_LEVELS[2] {
            band.far_out = cut.outer_center.extend(cut.outer).extend(cut.fade);
        }
    }
    card_band.set_if_neq(band);
}

/// Ease each active edge's centre toward `target`, as far as both its
/// levels are on screen around where it would go: an edge never cuts a
/// plate before the plate replacing it is drawn. The step is the distance
/// left, scaled by the frame over `EDGE_EASE_S`, so a jump plays out as a
/// sweep. An edge is keyed by its finer level; the outermost band's outer
/// edge is the horizon and has none.
fn advance_edges(
    edges: &mut HashMap<u32, Vec2>,
    bands: &[common_bevy::summary::Band],
    target: Vec2,
    dt: f32,
    meshes: &SummaryMeshes,
) {
    let paired = bands.len().saturating_sub(1);
    edges.retain(|r, _| bands[..paired].iter().any(|b| b.r == *r));
    for pair in bands.windows(2) {
        let (fine, coarse) = (&pair[0], &pair[1]);
        let radius = fine.outer_wu;
        let centre = edges.entry(fine.r).or_insert(target);
        let lag = centre.distance(target);
        if lag < 1e-2 || lag > EDGE_MAX_LAG * radius {
            *centre = target;
            continue;
        }
        let next = centre.lerp(target, (dt / EDGE_EASE_S).min(1.0));
        if edge_is_drawn(next, radius, fine.r, coarse.r, meshes) {
            *centre = next;
        }
    }
}

/// Whether both levels are on screen all around the circle: under each of
/// `EDGE_SAMPLES` points on it, the region at the finer level and the
/// region at the coarser level have entities.
fn edge_is_drawn(centre: Vec2, radius: f32, fine: u32, coarse: u32, meshes: &SummaryMeshes) -> bool {
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let lattices = [fine, coarse].map(common_bevy::summary::summary_lattice);
    (0..EDGE_SAMPLES).all(|i| {
        let a = i as f32 / EDGE_SAMPLES as f32 * std::f32::consts::TAU;
        let p = centre + Vec2::new(a.cos(), a.sin()) * radius;
        lattices.iter().all(|lat| {
            let (sq, sr) = lat.cell_at(p);
            let (mn, mm) = region_lat.cell_id(sq, sr);
            let key = common_bevy::summary_mesh::MeshRegionKey { r: lat.radius, mn, mm };
            meshes.states.get(&key).is_some_and(|s| s.entity.is_some())
        })
    })
}

/// Level `r`'s cut: its band, with the morph strip inside its outer edge,
/// complete a cell short of it. The outermost band ends at the horizon and
/// morphs nowhere. The level
/// begins exactly where the finer one ends, since the finer surface has
/// become this one there; anything drawn under the finer level's strip
/// could only show through where it stands higher. Each circle is centred
/// where its edge is (`edges`), or on the player where the edge has not
/// been placed.
fn level_cut(
    r: u32,
    bands: &[common_bevy::summary::Band],
    edges: &HashMap<u32, Vec2>,
    target: Vec2,
) -> crate::resources::TerrainCut {
    use common_bevy::summary::{finer_level, level_band, summary_outer_radius_wu, transition_wu};
    let (band, outermost) = level_band(r, bands);
    let (outer, fade, settle) =
        if outermost { (f32::MAX, 0.0, 0.0) } else { (band.outer_wu, transition_wu(r), summary_outer_radius_wu(r)) };
    let centre_of = |edge: Option<u32>| edge.and_then(|e| edges.get(&e)).copied().unwrap_or(target);
    crate::resources::TerrainCut {
        inner_center: centre_of(finer_level(r)),
        outer_center: centre_of(Some(r)),
        inner: band.inner_wu,
        outer,
        fade,
        settle,
    }
}

/// The mesh regions every band wants around `at`. Local bands (within
/// `local_boundary_wu`, the extent the Map can serve) are gated on loaded
/// chunks; remote bands beyond it are not, their data being the server's
/// summaries. `margin` widens each band's annulus: 0.0 is the crisp band
/// assignment for building, more the widened keep set for eviction.
fn compute_band_regions(
    at: Vec2,
    loaded_chunks: &std::collections::HashSet<common_bevy::chunk::ChunkId>,
    margin: f32,
    local_boundary_wu: f32,
) -> std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey> {
    use common_bevy::summary_mesh::{visible_mesh_regions_in_band, visible_mesh_regions_in_band_ungated};

    let bands = horizon_bands(margin);
    let mut all_regions = std::collections::HashSet::new();

    // Bands are split at the stream-radius boundary, not assigned to one
    // side: a band straddling it contributes a gated segment (client-owned,
    // chunk-fed) AND an ungated segment (server-fed). Assigning the
    // whole band to one side left its other segment with no regions at all.
    for band in &bands {
        // Footprint-overlap enumeration over the band: a region is built
        // if its FOOTPRINT overlaps the band, not just its center, so every
        // fragment the cut keeps has geometry behind it. Regions are up to
        // mesh_region_extent_wu(r) across — center-only membership left
        // crescents near every band boundary covered by neither level.
        // Regions are built whole; the shader cut (`update_terrain_cut`)
        // is what confines a level to its band.
        let half_extent = 0.5 * common_bevy::summary::mesh_region_extent_wu(band.r);
        let (win_inner, win_outer) = (band.inner_wu, band.outer_wu);
        let band_inner = (win_inner * (1.0 - margin) - half_extent).max(0.0);
        let band_outer = win_outer * (1.0 + margin) + half_extent;

        if band_inner < local_boundary_wu {
            // Segment within the local-data boundary: gate on loaded chunks
            let gated_outer = band_outer.min(local_boundary_wu);
            let regions = visible_mesh_regions_in_band(
                band.r,
                at.x,
                at.y,
                band_inner,
                gated_outer,
                loaded_chunks,
            );
            all_regions.extend(regions);
        }
        if band_outer > local_boundary_wu {
            // Segment beyond the boundary: ungated (SummaryCache-fed from
            // the server)
            let inner = band_inner.max(local_boundary_wu);
            let regions = visible_mesh_regions_in_band_ungated(
                band.r,
                at.x,
                at.y,
                inner,
                band_outer,
            );
            all_regions.extend(regions);
        }
    }

    all_regions
}

/// Build a mesh region (runs off main thread).

/// One builder for every level; only the height lookup differs. r=0 reads
/// tile z straight from the Map. r>0 reads the SummaryCache (server-fed)
/// and falls back to sampling the Map with the same 7-sample
/// rule every producer uses — Map z and server elevation agree by
/// construction, so the value is the same whichever side computed it. The
/// next coarser level, which the morph targets read, comes through the
/// same lookup at its own radius. Until every cell and ring cell has data
/// the build yields nothing, and the region is re-dispatched as data
/// streams in.
fn collect_and_build_summary_mesh(
    radius: u32,
    region_key: common_bevy::summary_mesh::MeshRegionKey,
    map: &common_bevy::resources::map::Map,
    cache: &crate::resources::SummaryCache,
    dens: &HashMap<qrz::Qrz, common_bevy::den::DenLook>,
) -> SummaryMeshBuildResult {
    let empty = SummaryMeshBuildResult {
        positions: Vec::new(),
        normals: Vec::new(),
        coarse: Vec::new(),
        canopy: Vec::new(),
        parts: Vec::new(),
        indices: Vec::new(),
        tri_count: 0,
        water: Default::default(),
        cover: Vec::new(),
    };

    let tile_z = |q: i32, r: i32| -> Option<common::grid::Ground> { map.ground_at(q, r) };
    // The same seven samples over the map's tiles, where every producer's
    // rule reads them; nothing until the tiles are all there.
    let sampled = |level: u32, sq: i32, sr: i32| common_bevy::summary::summarize(level, sq, sr, map);

    // The builder also reads the ring of cells around the region, which
    // belong to neighbouring regions, and the coarser level's cells under
    // it: cache lookups are per region, memoised across the build.
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let regions: std::cell::RefCell<
        std::collections::HashMap<common_bevy::summary_mesh::MeshRegionKey, Option<std::sync::Arc<crate::resources::RegionData>>>,
    > = Default::default();
    let cached = |level: u32, sq: i32, sr: i32| -> Option<common_bevy::summary::SummaryCell> {
        let (mn, mm) = region_lat.cell_id(sq, sr);
        let key = common_bevy::summary_mesh::MeshRegionKey { r: level, mn, mm };
        regions
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| cache.get_region(&key))
            .as_ref()
            .and_then(|d| d.cells.get(&(sq, sr)).copied())
    };
    // A level's heights: the tile's own at r = 0, else the cached summary
    // or the same seven samples over the map's tiles.
    let level_z = |level: u32| {
        move |sq: i32, sr: i32| -> Option<common::grid::Ground> {
            if level == 0 {
                return tile_z(sq, sr);
            }
            if let Some(cell) = cached(level, sq, sr) {
                return Some(cell.ground());
            }
            // A summary is the ladder's cell, which the tiles do not hold:
            // it is only ever the server's.
            if level >= common_bevy::summary::LOD_LEVELS[1] {
                return None;
            }
            sampled(level, sq, sr).map(|c| c.ground())
        }
    };
    let height = level_z(radius);
    let coarse = common_bevy::summary::coarser_level(radius).map(level_z);
    let coarse: Option<&dyn Fn(i32, i32) -> Option<common::grid::Ground>> = coarse.as_ref().map(|c| c as &dyn Fn(i32, i32) -> Option<common::grid::Ground>);

    // The cover stands with the ground: placed here from the map's covers
    // at every level the map reaches, spawned with the ground once the kit
    // is loaded. The tiles' water is a surface of its own over each tile;
    // a summary's is painted on its ground from its parts.
    if radius == 0 {
        let tile_water = |q: i32, r: i32| -> Option<i32> { map.water_at(q, r) };
        return common_bevy::summary_mesh::build_summary_mesh_region(0, region_key, &height, coarse, None)
            .map_or(empty, |smr| {
                let mut result = smr_to_result(smr);
                let w = common_bevy::summary_mesh::build_water_mesh_region(0, region_key, &tile_water, &height);
                result.water = crate::resources::WaterGeometry { positions: w.positions, normals: w.normals, indices: w.indices };
                result.cover = crate::plugins::cover::place_cover(0, region_key, map, &height);
                result.cover.extend(crate::plugins::cover::place_dens(0, region_key, dens, map, &height));
                result
            });
    }

    // The level's parts follow the height's provenance: the cached summary
    // where it was sent, else the same reading over the map's tiles. Every
    // level above the tiles carries its canopy and, but the last, its water
    // on the ground as a colour — the first stands its trees on that
    // ground as well, from the map's own covers, which reach exactly as
    // far as that level does, so the ground under the trees is the ground
    // past them and the trees are the tiles' own — and the material lays
    // crowns on it or not by level.
    let summary = |sq: i32, sr: i32| cached(radius, sq, sr).or_else(|| sampled(radius, sq, sr));
    let summary_canopy = |sq: i32, sr: i32| -> Option<[common::Canopy; common_bevy::summary::PARTS]> { summary(sq, sr).map(|c| c.canopy) };
    let canopied: Option<&dyn Fn(i32, i32) -> Option<[common::Canopy; common_bevy::summary::PARTS]>> =
        TerrainMaterial::canopied(radius).then_some(&summary_canopy);

    common_bevy::summary_mesh::build_summary_mesh_region(radius, region_key, &height, coarse, canopied)
        .map_or(empty, |smr| {
            let mut result = smr_to_result(smr);
            if canopied.is_some() {
                result.parts = common_bevy::summary_mesh::parts_layer(radius, region_key, &summary).unwrap_or_default();
            }
            if radius == common_bevy::summary::LOD_LEVELS[1] {
                result.cover = crate::plugins::cover::place_cover(radius, region_key, map, &height);
            }
            // Past the tiles a crag stands from the summaries' own reading
            // of it, the level after the first.
            if radius == common_bevy::summary::LOD_LEVELS[2] {
                let outcrop = |sq: i32, sr: i32| cached(radius, sq, sr).or_else(|| sampled(radius, sq, sr)).map(|c| c.outcrop);
                result.cover = crate::plugins::cover::place_crags(radius, region_key, &outcrop, &height);
            }
            if crate::systems::den::DRAWN_AT.contains(&radius) {
                result.cover.extend(crate::plugins::cover::place_dens(radius, region_key, dens, map, &height));
            }
            result
        })
}

/// The built geometry as the main thread takes it, its buffers moved in.
fn smr_to_result(smr: common_bevy::summary_mesh::SummaryMeshResult) -> SummaryMeshBuildResult {
    SummaryMeshBuildResult {
        positions: smr.positions,
        normals: smr.normals,
        coarse: smr.coarse,
        canopy: smr.canopy,
        parts: Vec::new(),
        indices: smr.indices,
        tri_count: smr.tri_count,
        water: Default::default(),
        cover: Vec::new(),
    }
}

/// Build a Bevy Mesh from raw geometry buffers. `coarse` is the ground's
/// morph target per vertex; the water carries none. `canopy` is the
/// canopy per vertex, as the vertex colour, where the level colours its
/// ground by it.
fn build_bevy_mesh(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    coarse: Option<&[[f32; 4]]>,
    canopy: &[[f32; 4]],
    indices: &[u32],
) -> Mesh {
    use bevy::render::render_resource::PrimitiveTopology;
    use bevy::asset::RenderAssetUsages;
    use bevy_mesh::Indices;

    let verts: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let norms: Vec<Vec3> = normals.iter().map(|n| Vec3::from_array(*n)).collect();
    let vert_count = verts.len();

    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..vert_count).map(|_| [0.0f32, 0.0]).collect::<Vec<[f32; 2]>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
    .with_inserted_indices(Indices::U32(indices.to_vec()));
    let mesh = match coarse {
        Some(coarse) => mesh.with_inserted_attribute(crate::resources::ATTRIBUTE_COARSE_SURFACE, coarse.to_vec()),
        None => mesh,
    };
    if canopy.is_empty() {
        mesh
    } else {
        mesh.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, canopy.to_vec())
    }
}

/// Poll completed summary mesh tasks, upload their meshes, spawn/update
/// entities. A region's geometry is final when its task completes: regions
/// share corners by construction, so nothing is stitched afterwards.
pub fn poll_summary_meshes(
    mut commands: Commands,
    mut summary_meshes: ResMut<SummaryMeshes>,
    origin: Res<crate::resources::RenderOrigin>,
    map: Res<common_bevy::resources::map::Map>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    mut terrain_material: ResMut<TerrainMaterial>,
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    water_material: Res<crate::plugins::water::WaterMaterial>,
    client_timers: Res<crate::resources::ClientTimers>,
    mut canopy_parts: ResMut<crate::plugins::canopy::CanopyParts>,
    mut images: ResMut<Assets<Image>>,
) {
    let _t = client_timers.scope("sum_poll");

    // A layer outlives its region by a frame at most: the regions gone
    // since the last run give theirs back before any is claimed.
    canopy_parts.release(|key| summary_meshes.states.get(key).is_some_and(|state| state.entity.is_some()));

    // Poll async tasks; the region keeps its cover, to spawn once the kit
    // has loaded.
    let mut builds = Vec::new();
    for (&key, state) in summary_meshes.states.iter_mut() {
        let Some(task) = &mut state.task else { continue };
        let Some(mut result) = block_on(future::poll_once(task)) else { continue };
        state.task = None;
        state.cover = std::mem::take(&mut result.cover);
        state.waiting = result.tri_count == 0;
        if result.tri_count > 0 {
            builds.push((key, result));
        }
    }

    // Upload meshes, spawn/update entities.
    for (key, build) in builds {

        let mesh = build_bevy_mesh(&build.positions, &build.normals, Some(&build.coarse), &build.canopy, &build.indices);
        let mesh_handle = meshes.add(mesh);

        let state = summary_meshes.states.get_mut(&key).unwrap();
        state.mesh_handle = Some(mesh_handle.clone());
        state.tri_count = build.tri_count;
        let canopied = crate::resources::TerrainMaterial::canopied(key.r);
        let parts = canopy_parts.image(key.r, canopied, &mut images);

        let entity = match state.entity {
            Some(entity) => {
                commands.entity(entity).insert(Mesh3d(mesh_handle));
                // The water and the cover are the ground's children:
                // rebuilt with it.
                commands.entity(entity).despawn_related::<Children>();
                entity
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(mesh_handle),
                        MeshMaterial3d(terrain_material.for_level(key.r, &mut materials, parts)),
                        Transform::from_translation(origin.render_tile(&map, key.origin())),
                        SummaryMesh,
                    ))
                    .id();
                state.entity = Some(entity);
                entity
            }
        };
        state.models_spawned = false;
        state.cards_spawned = false;
        let tag = match build.parts.is_empty() {
            true => crate::plugins::canopy::NO_LAYER,
            false => canopy_parts.write(key, build.parts),
        };
        commands.entity(entity).insert(bevy::mesh::MeshTag::new(tag));

        if !build.water.indices.is_empty() {
            let water = build_bevy_mesh(&build.water.positions, &build.water.normals, None, &[], &build.water.indices);
            commands.entity(entity).with_child((
                Mesh3d(meshes.add(water)),
                MeshMaterial3d(water_material.0.clone()),
                Transform::IDENTITY,
                bevy_light::NotShadowCaster,
                crate::resources::WaterMesh,
            ));
        }
    }

    // Diagnostics.
    let mut total_tris = 0u64;
    let mut mesh_count = 0u32;
    tri_stats.per_band.clear();
    for (&region_key, state) in summary_meshes.states.iter() {
        if state.entity.is_some() {
            total_tris += state.tri_count as u64;
            mesh_count += 1;
            let entry = tri_stats.per_band.entry(region_key.r).or_insert((0, 0));
            entry.0 += state.tri_count as u64;
            entry.1 += 1;
        }
    }
    tri_stats.total_tris = total_tris;
    tri_stats.mesh_count = mesh_count;
    tri_stats.async_mesh = summary_meshes.states.values().filter(|s| s.task.is_some()).count() as u32;
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A body across the sun covers none of it apart, all of it over it
    /// when it is the wider, and more the nearer it comes.
    #[test]
    fn a_body_covers_more_of_the_sun_the_nearer_it_comes() {
        let (r, over) = (1., 1.5);
        assert_eq!((covered(2.5, r, over), covered(0.4, r, over), covered(1., r, 0.)), (0., 1., 0.));
        assert!((covered(0., r, 0.5) - 0.25).abs() < 1e-6, "a smaller one covers its own area");
        let mut last = 0.;
        for step in (0..=50).rev() {
            let share = covered(step as f32 * 0.05, r, over);
            assert!(share >= last - 1e-5 && (0. ..=1.).contains(&share));
            last = share;
        }
    }

    /// The sun and the moon are each above the horizon exactly from
    /// their rise to their set and highest at their own hour; each orbit
    /// runs on one way without a break, and its pace never changes at
    /// once. The sun is quick through its short morning, slow through
    /// its long afternoon and quicker again by night.
    #[test]
    fn the_sun_and_the_moon_keep_their_hours_at_an_even_changing_pace() {
        // Half-minute samples, so none lands on a rise or a set.
        let step = 1. / 1440.;
        for (rise, high, set) in [(SUNRISE, NOON, SUNSET), (MOONRISE, MOON_HIGH, MOONSET)] {
            let orbit = |t: f32| orbit(t, rise, high, set);
            let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
            assert!(near(orbit(rise + 1e-5), 0.) && near(orbit(high), PI / 2.) && near(orbit(set), PI));
            let advance = |t: f32| (orbit(t + step) - orbit(t)).rem_euclid(2. * PI);
            let mut pace = advance(1. - step / 2.);
            for minute in 0..1440 {
                let t = (minute as f32 + 0.5) * step;
                let a = orbit(t);
                let up = (t - rise).rem_euclid(1.) < (set - rise).rem_euclid(1.);
                assert_eq!(toward(a, 0.).y > 0., up, "t={t} a={a}");
                assert_eq!(direct(elevation(toward(a, 0.))) > 0., up, "t={t}");
                let next = advance(t);
                assert!(next > 0. && next < 0.1, "t={t} advance={next}");
                assert!((next - pace).abs() < 0.03 * next, "the pace jumps at t={t}: {pace} to {next}");
                pace = next;
            }
        }
        let advance = |t: f32| (orbit(t + step, SUNRISE, NOON, SUNSET) - orbit(t, SUNRISE, NOON, SUNSET)).rem_euclid(2. * PI);
        let (morning, afternoon, night) = (advance((SUNRISE + NOON) / 2.), advance((NOON + SUNSET) / 2.), advance((SUNSET + 1. + SUNRISE) / 2.));
        assert!(afternoon < morning && afternoon < night, "{morning} {afternoon} {night}");
        assert!((MOON_HIGH.rem_euclid(1.) - 0.75 / 24.).abs() < 1e-5, "the moon is as quick to rise and slow to set as the sun");
    }

    /// The fire burns only about the crossing, fullest with the sun on
    /// the horizon, and the stars are out only once it has died: neither
    /// by day.
    #[test]
    fn the_fire_burns_at_the_crossing_and_the_stars_follow_it() {
        let at = |degrees: f32| degrees.to_radians();
        assert_eq!((fire(at(40.)), fire(at(-20.))), (0., 0.));
        assert!(fire(at(0.)) > 0.9 && fire(at(2.)) > 0.9);
        assert!(fire(at(-6.)) < fire(at(-2.)) && fire(at(12.)) < fire(at(5.)));
        assert_eq!((starlit(at(5.)), starlit(at(-15.))), (0., 1.));
        assert!(starlit(at(-6.)) > 0. && starlit(at(-6.)) < 1.);
        for degrees in -20..40 {
            assert!(starlit(at(degrees as f32)) <= starlit(at(degrees as f32 - 1.)), "the lower the sun, the more stars");
        }
    }

    /// The sun's light is cast from no lower than its floor, on the sun's
    /// own bearing, and from the sun itself above that, with no step
    /// between: so a shadow is never longer than the floor makes it.
    #[test]
    fn the_light_is_cast_from_no_lower_than_its_floor() {
        for minute in 0..1440 {
            let sun = toward(orbit(minute as f32 / 1440., SUNRISE, NOON, SUNSET), 0.3);
            let cast = cast_from(sun);
            assert!((cast.length() - 1.).abs() < 1e-4);
            if elevation(sun) >= CAST_ABOVE {
                assert_eq!(cast, sun);
            } else {
                assert!((elevation(cast) - CAST_ABOVE).abs() < 1e-4, "minute {minute}");
                let (a, b) = (Vec2::new(sun.x, sun.z).normalize(), Vec2::new(cast.x, cast.z).normalize());
                assert!(a.dot(b) > 0.9999, "its bearing is the sun's");
            }
        }
        let at = toward(CAST_ABOVE, 0.);
        assert!((cast_from(toward(CAST_ABOVE - 1e-4, 0.)) - at).length() < 1e-3, "no step at the floor");
    }

    /// The deck catches the sun's light on the sun's side first, while
    /// the sun is still under the horizon, and whole by the time it is up.
    #[test]
    fn the_clouds_catch_the_sun_from_its_own_side_first() {
        let at = |degrees: f32| caught_from(degrees.to_radians());
        assert!(at(-10.) > 0.6, "nothing the deck draws is lit in the dark");
        assert!(at(-6.) < at(-10.) && at(-6.) > 0., "the sun's side is lit under the horizon, the camera's not yet");
        assert!((-9..=1).map(|d| at(d as f32)).is_sorted_by(|a, b| a >= b));
        assert!(at(1.) < -1., "and the whole deck with the sun up");
    }

    /// The sun's light and the sky's glow both turn within a band about
    /// the horizon: white and full clear above it, and the sky's red
    /// deepest just below, where the sun gives no direct light at all.
    #[test]
    fn the_reddening_keeps_to_the_horizon() {
        let (above, below) = (CLEAR_ABOVE + 1e-4, -GLOW_BELOW);
        assert_eq!(direct(above), 1.);
        assert_eq!(tint(direct(above)), Color::WHITE);
        assert_eq!(direct(0.), 0.);
        assert_eq!(direct(below), 0.);
        let glow = smoothstep(-GLOW_BELOW, CLEAR_ABOVE, below);
        assert_eq!(glow, 0., "the sky is reddest with the sun just below the horizon");
        assert!(smoothstep(-TWILIGHT_BELOW, CLEAR_ABOVE, below) > 0., "and still lit there");
        assert_eq!(smoothstep(-TWILIGHT_BELOW, CLEAR_ABOVE, -TWILIGHT_BELOW), 0., "and dark past twilight");
    }

    /// At the season's height the sun still sets across the sky from
    /// where it rose, each end swung from east and west by no more than
    /// the season's swing.
    #[test]
    fn the_sun_sets_across_the_sky_from_its_rise() {
        let lean = tan(SEASON_SWING);
        let (rise, set) = (toward(0., lean), toward(PI, lean));
        assert!(rise.x > 0. && set.x < 0., "rises in the east, sets in the west");
        assert!(rise.angle_between(set) >= PI - 2. * SEASON_SWING - 1e-4, "{}", rise.angle_between(set).to_degrees());
    }

    /// A level begins exactly where the finer one ends, with the morph
    /// strip inside the finer one's edge and complete a cell of the finer
    /// level short of it, so every finer triangle reaching the edge lies on
    /// the coarser surface. The finest level begins at the player and the
    /// outermost band ends at the horizon, morphing nowhere.
    #[test]
    fn level_cut_meets_the_finer_level_at_its_edge() {
        use common_bevy::summary::{compute_active_bands, summary_outer_radius_wu, transition_wu};
        let bands = compute_active_bands(25_000.0);
        let (edges, target) = (HashMap::new(), Vec2::ZERO);
        for pair in bands.windows(2) {
            let (fine, coarse) = (&pair[0], &pair[1]);
            let f = level_cut(fine.r, &bands, &edges, target);
            let c = level_cut(coarse.r, &bands, &edges, target);
            assert_eq!(f.outer, fine.outer_wu);
            assert!((f.fade - transition_wu(fine.r)).abs() < 0.01);
            assert!(f.fade < f.outer - f.inner);
            assert_eq!(f.settle, summary_outer_radius_wu(fine.r));
            assert!(f.settle < f.fade, "r={} settles before its strip begins", fine.r);
            assert_eq!(c.inner, f.outer, "r={} begins off r={}'s edge", coarse.r, fine.r);
        }
        assert_eq!(level_cut(bands[0].r, &bands, &edges, target).inner, 0.0);
        let last = level_cut(bands.last().unwrap().r, &bands, &edges, target);
        assert_eq!((last.outer, last.fade, last.settle), (f32::MAX, 0.0, 0.0));
    }

    /// The cut as the frame computes it at a fresh spawn: no edges placed,
    /// nothing drawn. The finest level shows from the player out to its
    /// cut, centred on the player.
    #[test]
    fn fresh_spawn_cut_shows_the_finest_level_around_the_player() {
        let origin = Vec3::new(5000.0, 12.0, -3000.0);
        let bands = horizon_bands(0.0);
        let mut edges = HashMap::new();
        advance_edges(&mut edges, &bands, origin.xz(), 0.016, &SummaryMeshes::default());
        let c = level_cut(0, &bands, &edges, origin.xz());
        assert_eq!(c.inner_center, origin.xz());
        assert_eq!(c.outer_center, origin.xz());
        assert_eq!(c.inner, 0.0);
        assert!(c.fade > 0.0 && c.fade < c.outer);
        let c1 = level_cut(1, &bands, &edges, origin.xz());
        assert_eq!(c1.inner_center, origin.xz());
        assert_eq!(c1.inner, c.outer);
    }

    /// An edge's circle is centred where the edge is, on both the level
    /// leaving across it and the level arriving; a level's two circles can
    /// have different centres.
    #[test]
    fn level_cut_centres_each_circle_on_its_edge() {
        use common_bevy::summary::compute_active_bands;
        let bands = compute_active_bands(25_000.0);
        let target = Vec2::new(100.0, 50.0);
        let mut edges = HashMap::new();
        edges.insert(bands[1].r, Vec2::new(90.0, 50.0));
        let fine = level_cut(bands[1].r, &bands, &edges, target);
        let coarse = level_cut(bands[2].r, &bands, &edges, target);
        assert_eq!(fine.inner_center, target, "an edge not yet placed sits on the player");
        assert_eq!(fine.outer_center, Vec2::new(90.0, 50.0));
        assert_eq!(coarse.inner_center, fine.outer_center, "the two levels share the edge's circle");
        assert_eq!(coarse.outer_center, target);
    }

    /// An edge moves toward the player only where both levels are drawn
    /// around its next position, and by a step that eases in.
    #[test]
    fn edge_advances_only_where_both_levels_are_drawn() {
        use common_bevy::summary::compute_active_bands;
        let bands = compute_active_bands(25_000.0);
        let (fine, coarse) = (bands[0].r, bands[1].r);
        let radius = bands[0].outer_wu;
        let mut meshes = SummaryMeshes::default();
        let target = Vec2::new(40.0, 0.0);
        let mut edges = HashMap::new();
        edges.insert(fine, Vec2::ZERO);

        // Nothing drawn: the edge holds.
        advance_edges(&mut edges, &bands, target, 0.05, &meshes);
        assert_eq!(edges[&fine], Vec2::ZERO);

        // Every region of both levels the circle could touch is drawn: the
        // edge steps toward the player, part of the way.
        let region_lat = common_bevy::summary::mesh_region_lattice();
        // Sampled finer than a mesh region, out to twice the circle's
        // radius so the circle keeps its margin wherever it moves.
        let step = 3.0;
        let reach = (radius * 2.0 / step).ceil() as i32;
        for r in [fine, coarse] {
            let lat = common_bevy::summary::summary_lattice(r);
            for x in -reach..=reach {
                for z in -reach..=reach {
                    let p = Vec2::new(x as f32, z as f32) * step;
                    let (sq, sr) = lat.cell_at(p);
                    let (mn, mm) = region_lat.cell_id(sq, sr);
                    let key = common_bevy::summary_mesh::MeshRegionKey { r, mn, mm };
                    meshes.states.entry(key).or_insert_with(|| SummaryMeshState {
                        task: None,
                        entity: Some(Entity::from_raw_u32(1).unwrap()),
                        mesh_handle: None,
                        tri_count: 0,
                        cover: Vec::new(),
                        models_spawned: false,
                        cards_spawned: false,
                        waiting: false,
                        tiles_loaded: true,
                        stale: false,
                        epoch: 0,
                    });
                }
            }
        }
        assert!(edge_is_drawn(Vec2::ZERO, radius, fine, coarse, &meshes));
        advance_edges(&mut edges, &bands, target, 0.05, &meshes);
        let moved = edges[&fine];
        assert!(moved.x > 0.0 && moved.x < target.x, "eased partway: {moved:?}");

        // One coarse region under the circle gone: the edge holds again.
        let p = Vec2::new(radius, 0.0) + moved;
        let lat = common_bevy::summary::summary_lattice(coarse);
        let (sq, sr) = lat.cell_at(p);
        let (mn, mm) = region_lat.cell_id(sq, sr);
        meshes.states.get_mut(&common_bevy::summary_mesh::MeshRegionKey { r: coarse, mn, mm }).unwrap().entity = None;
        advance_edges(&mut edges, &bands, target, 0.05, &meshes);
        assert_eq!(edges[&fine], moved);

        // Left too far behind, it snaps to the player whatever is drawn.
        let far = Vec2::new(radius, 0.0);
        advance_edges(&mut edges, &bands, far, 0.05, &meshes);
        assert_eq!(edges[&fine], far);
    }

    /// A region whose build found no data is retried once data has arrived
    /// since that build was dispatched, however many runs later, and not
    /// before: neither a build in flight nor a spent budget loses the data.
    #[test]
    fn a_waiting_region_is_retried_once_data_has_arrived_since_its_build() {
        let mut state = SummaryMeshState {
            task: None,
            entity: None,
            mesh_handle: None,
            tri_count: 0,
            cover: Vec::new(),
            models_spawned: false,
            cards_spawned: false,
            waiting: true,
            tiles_loaded: true,
            stale: false,
            epoch: 3,
        };
        assert!(!state.wants_build(3, true), "nothing new to build from");
        assert!(state.wants_build(4, true), "data arrived after the build was dispatched");
        assert!(state.wants_build(7, true), "and stays wanted until a build is dispatched");

        state.waiting = false;
        assert!(state.wants_build(3, true), "a fresh region builds at any epoch");

        state.entity = Some(Entity::from_raw_u32(1).unwrap());
        assert!(!state.wants_build(9, true), "a built region is final");

        // Built from the summaries before its tiles arrived, it stands no
        // trees: it is built once more when they are all there, and once.
        state.tiles_loaded = false;
        assert!(!state.wants_build(9, false), "the tiles are still coming");
        assert!(state.wants_build(9, true), "the tiles are all there");
        state.tiles_loaded = true;
        assert!(!state.wants_build(10, true), "and it is final again");

        // A tile under it changed: built once more, whatever the epoch.
        state.stale = true;
        assert!(state.wants_build(10, true), "a changed tile rebuilds its region");
    }

    /// Coverage invariant for the LoD band system: every ground point inside
    /// the horizon (a) lies within at least one needed region at its band's
    /// level, and (b) if that region's footprint extends past the local-data
    /// boundary, the producer enumerates it (so its data will exist).
    #[test]
    fn lod_bands_cover_horizon_without_gaps() {
        use common_bevy::chunk::{calculate_visible_chunks, ChunkId, FIXED_STREAM_APOTHEM_WU, FIXED_STREAM_RADIUS};
        use common_bevy::summary::{compute_active_bands, mesh_region_extent_wu};
        use common_bevy::summary_mesh::visible_lod_regions;

        // (fov, camera ground y, loaded chunk ring, local boundary): the
        // regions are chosen on the ground plane, so neither the lens nor
        // the camera's height reaches the computation
        let cases: &[(f32, f32, u8, f32)] = &[
            (common::camera::MAX_GAMEPLAY_FOV, 0.0, FIXED_STREAM_RADIUS, FIXED_STREAM_APOTHEM_WU),
        ];

        for &(fov, cam_y, chunk_ring, boundary) in cases {
            // Chunks loaded exactly as the game does (hexagonal coverage —
            // the boundary is its inscribed circle).
            let loaded: std::collections::HashSet<ChunkId> =
                calculate_visible_chunks(ChunkId(0, 0), chunk_ring).into_iter().collect();

            let cam = Vec3::new(0.0, cam_y, 0.0);
            let needed = compute_band_regions(cam.xz(), &loaded, 0.0, boundary);

            // Producer set, to the same reach as the consumer.
            let bands = compute_active_bands(common_bevy::summary::reach_wu());
            let produced = visible_lod_regions(&bands, 0.0, 0.0, boundary, 0);

            // (b) Data coverage: needed r>0 regions reaching past the
            // boundary must be produced.
            for k in &needed {
                if k.r == 0 {
                    continue;
                }
                let (kx, kz) = region_center_world(k);
                let reach = (kx * kx + kz * kz).sqrt() + 0.5 * mesh_region_extent_wu(k.r);
                if reach > boundary {
                    assert!(
                        produced.contains(k),
                        "[fov={fov:.2} y={cam_y} b={boundary}] needed region {k:?} \
                         (reach {reach:.1}) is beyond the boundary but not produced"
                    );
                }
                // (c) A region is built only once its ring has heights, so
                // every neighbour of a needed region must be produced or lie
                // wholly inside the loaded tiles (its cells sample within a
                // summary of their centre).
                for (dn, dm) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
                    let n = common_bevy::summary_mesh::MeshRegionKey { r: k.r, mn: k.mn + dn, mm: k.mm + dm };
                    let (nx, nz) = region_center_world(&n);
                    let n_reach = (nx * nx + nz * nz).sqrt()
                        + 0.5 * mesh_region_extent_wu(n.r)
                        + common_bevy::summary::summary_width_wu(n.r);
                    assert!(
                        produced.contains(&n) || n_reach <= boundary,
                        "[fov={fov:.2} y={cam_y} b={boundary}] ring region {n:?} of needed \
                         {k:?} (reach {n_reach:.1}) is neither produced nor inside the Map \
                         — the region could never be built"
                    );
                }
            }

            // (d) A region's morph targets read the coarser cell under each
            // vertex and its ring, so every coarser region whose cells can
            // lie within that reach of a needed region must be produced or
            // sample wholly inside the loaded tiles.
            for k in &needed {
                let Some(c) = common_bevy::summary::coarser_level(k.r) else { continue };
                let (kx, kz) = region_center_world(k);
                let circum = |r: u32| mesh_region_extent_wu(r) / 3.0_f32.sqrt();
                let cell_reach = common_bevy::summary::summary_lattice(c).scale as f32
                    + common_bevy::summary::summary_width_wu(c);
                let reach = circum(k.r) + cell_reach;
                let coarse_lat = common_bevy::summary::mesh_region_lattice();
                let coarse_sum = common_bevy::summary::summary_lattice(c);
                // Every coarser region whose circumcircle meets the reach.
                let span = (reach + circum(c)) / common_bevy::summary::mesh_region_spacing_wu(c);
                let n = span.ceil() as i32 + 1;
                let (ksq, ksr) = coarse_sum.cell_id(
                    (kx as f64 / 1.5).round() as i32,
                    ((kz as f64 - (kx as f64 / 1.5) * 3.0_f64.sqrt() / 2.0) / 3.0_f64.sqrt()).round() as i32,
                );
                let (kmn, kmm) = coarse_lat.cell_id(ksq, ksr);
                for dn in -n..=n {
                    for dm in -n..=n {
                        let cr = common_bevy::summary_mesh::MeshRegionKey { r: c, mn: kmn + dn, mm: kmm + dm };
                        let (cx, cz) = region_center_world(&cr);
                        let dist = ((cx - kx).powi(2) + (cz - kz).powi(2)).sqrt();
                        if dist > reach + circum(c) {
                            continue;
                        }
                        let c_reach = (cx * cx + cz * cz).sqrt() + circum(c)
                            + common_bevy::summary::summary_width_wu(c);
                        assert!(
                            produced.contains(&cr) || c_reach <= boundary,
                            "[fov={fov:.2} y={cam_y} b={boundary}] coarser region {cr:?} under \
                             needed {k:?} (reach {c_reach:.1}) is neither produced nor inside \
                             the Map — the region could never be built"
                        );
                    }
                }
            }

            // (a) Geometric coverage: every ground point inside a level's
            // window — what the cut lets that level show — must lie within
            // some needed region of that level (region circumradius =
            // extent/sqrt(3)).
            for (az_deg, band) in (0..360).step_by(5).flat_map(|az| bands.iter().map(move |b| (az, b))) {
                let azr = (az_deg as f32).to_radians();
                let (win_inner, win_outer) = (band.inner_wu, band.outer_wu);
                let mut d = win_inner.max(2.0);
                while d < win_outer.min(common_bevy::summary::reach_wu() - 1.0) {
                    let px = d * azr.cos();
                    let pz = d * azr.sin();
                    let circum = mesh_region_extent_wu(band.r) / 3.0_f32.sqrt();
                    let covered = needed.iter().any(|k| {
                        if k.r != band.r {
                            return false;
                        }
                        let (kx, kz) = region_center_world(k);
                        let dx = kx - px;
                        let dz = kz - pz;
                        dx * dx + dz * dz <= circum * circum
                    });
                    if !covered {
                        // Owning region of this point at the band's level
                        let tile_q = (px as f64 / 1.5).round() as i32;
                        let tile_r = ((pz as f64 - (px as f64 / 1.5) * 3.0_f64.sqrt() / 2.0)
                            / 3.0_f64.sqrt())
                        .round() as i32;
                        let slat = common_bevy::summary::summary_lattice(band.r);
                        let (sq, sr) = slat.cell_id(tile_q, tile_r);
                        let rlat = common_bevy::summary::mesh_region_lattice();
                        let (mn, mm) = rlat.cell_id(sq, sr);
                        let owner = common_bevy::summary_mesh::MeshRegionKey { r: band.r, mn, mm };
                        let (ox, oz) = region_center_world(&owner);
                        let owner_dist = (ox * ox + oz * oz).sqrt();
                        // Probe the enumerators directly with the same args
                        // compute_band_regions uses for this band.
                        let h = 0.5 * mesh_region_extent_wu(band.r);
                        let b_inner = (win_inner - h).max(0.0);
                        let b_outer = win_outer + h;
                        let gated = common_bevy::summary_mesh::visible_mesh_regions_in_band(
                            band.r, 0.0, 0.0, b_inner, b_outer.min(boundary), &loaded,
                        );
                        let ungated = common_bevy::summary_mesh::visible_mesh_regions_in_band_ungated(
                            band.r, 0.0, 0.0, b_inner.max(boundary), b_outer,
                        );
                        panic!(
                            "[fov={fov:.2} y={cam_y} b={boundary}] uncovered ground at \
                             d={d:.1} az={az_deg} (band r={}, circum={circum:.1}); \
                             owner {owner:?} center_dist={owner_dist:.1} \
                             in_needed={} in_produced={} in_gated={} in_ungated={} \
                             gated_range=[{b_inner:.1},{:.1}] owner_summary=({sq},{sr})",
                            band.r,
                            needed.contains(&owner),
                            produced.contains(&owner),
                            gated.contains(&owner),
                            ungated.contains(&owner),
                            b_outer.min(boundary),
                        );
                    }
                    d += 7.0;
                }
            }
        }
    }
}
