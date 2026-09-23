use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::{
    color::ColorToComponents,
    image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    math::ops::*,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{DistanceFog, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
    tasks::{block_on, futures_lite::future},
};
use bevy_light::{CascadeShadowConfig, CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};

pub const TILE_SIZE: f32 = 1.;

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
/// sun's tint sits on the horizon under its bearing. Shaded by
/// `shaders/sky.wgsl`.
#[derive(Component)]
pub struct SkyDome;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SkyMaterial {
    /// The sun's bearing on the ground, a unit vector in xz.
    #[uniform(0)]
    pub bearing: Vec4,
    #[uniform(0)]
    pub base: LinearRgba,
    #[uniform(0)]
    pub glow: LinearRgba,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sky.wgsl".into()
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
/// once and whole, inscribed. Below `waterline` there is no disc, so it
/// sets into the sea instead of showing through it. Shaded by
/// `shaders/disc.wgsl`.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct DiscMaterial {
    #[uniform(0)]
    pub sun: Vec4,
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub waterline: f32,
    #[texture(1)]
    #[sampler(2)]
    pub face: Option<Handle<Image>>,
}

impl DiscMaterial {
    fn new(face: Option<Handle<Image>>) -> Self {
        Self { sun: Vec4::Z, color: LinearRgba::WHITE, waterline: crate::plugins::water::SEA_LEVEL_Y, face }
    }
}

impl Material for DiscMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/disc.wgsl".into()
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

/// When the sun and the moon rise and set, as fractions of the day from
/// midnight: a sixteen-hour day, and the moon up while the sun is down,
/// opposite it. The twilight sky lights the ground while the moon's light
/// climbs, and again after it goes.
const SUNRISE: f32 = 4. / 24.;
const SUNSET: f32 = 20. / 24.;
const MOONRISE: f32 = SUNSET;
const MOONSET: f32 = SUNRISE;
/// How far the sun's rise and set swing from east and west at the
/// season's height: its whole circle leans that far south and back.
const SEASON_SWING: f32 = 25_f32.to_radians();

/// A body's orbit angle at day fraction `t`: zero as it rises, π/2 at
/// its zenith, π as it sets, 3π/2 at its nadir. The half above the
/// horizon takes the time from `rise` to `set` and the half below the
/// rest, so the sun's long day is its slow sweep.
fn orbit(t: f32, rise: f32, set: f32) -> f32 {
    let up = (set - rise).rem_euclid(1.);
    let since_rise = (t - rise).rem_euclid(1.);
    if since_rise < up { PI * since_rise / up } else { PI + PI * (since_rise - up) / (1. - up) }
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

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

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
    plugins::diagnostics::DiagnosticsState,
    systems::camera::HAZE_COLOR,
    resources::{
        ForcedSummaryRadius, LodTriangleStats, LoadedChunks,
        Server, SummaryMesh, SummaryMeshBuildResult, SummaryMeshState, SummaryMeshes,
        TerrainMaterial,
    },
};
use common_bevy::{
    chunk::{self, loc_to_chunk, CHUNK_EXTENT_WU},
    components::{ *,
        behaviour::PlayerControlled,
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
        MeshMaterial3d(sky_materials.add(SkyMaterial { bearing: Vec4::X, base: LinearRgba::BLACK, glow: LinearRgba::BLACK })),
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
    actor_query: Query<(Entity, &Loc, &EntityType)>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let mut all_evicted = Vec::new();

    for message in reader.read() {
        let Do { event: Event::EvictChunks { chunks, .. } } = message else { continue };
        all_evicted.extend_from_slice(chunks);
    }

    if all_evicted.is_empty() { return; }
    let _t = client_timers.0.scope("evict");

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
    loaded_chunks.evict(&all_evicted);
}

pub fn do_init(
    mut reader: MessageReader<Do>,
    mut try_writer: MessageWriter<Try>,
    mut server: ResMut<Server>,
    time: Res<Time>,
) {
    for message in reader.read() {
        let Do { event: Event::Init { dt, .. } } = message else { continue };
        let dt = *dt;
        let client_now = time.elapsed().as_millis();

        // CRITICAL: The server captured dt when it SENT Init, but we're receiving it now
        // During the client startup time (client_now ms), the server's clock also advanced
        // We need to add that startup time to server_time_at_init to compensate
        server.server_time_at_init = dt.saturating_add(server.smoothed_latency).saturating_add(client_now);
        server.client_time_at_init = client_now;
        server.last_ping_time = client_now; // Track when we sent initial ping

        // Send initial Ping to measure actual network latency
        try_writer.write(Try { event: Event::Ping { client_time: client_now } });
    }
}

pub fn do_spawn(
    mut reader: MessageReader<Do>,
    map: Res<common_bevy::resources::map::Map>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("do_spawn");
    for message in reader.read() {
        let Do { event: Event::Spawn { typ: EntityType::Decorator(decorator), qrz, .. } } = message else { continue };
        map.insert(*qrz, EntityType::Decorator(*decorator));
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
    server: Res<Server>,
    diagnostics_state: Res<DiagnosticsState>,
    player_query: Query<&Loc, (With<PlayerControlled>, With<common_bevy::components::Actor>)>,
) {
    let dt = diagnostics_state.lighting.at(server.current_time(time.elapsed().as_millis()));
    let dtd = (dt % DAY_MS) as f32 / DAY_MS as f32;
    let dtm = (dt % SEASON_MS) as f32 / SEASON_MS as f32;
    let dty = (dt % YEAR_MS) as f32 / YEAR_MS as f32;

    // sun
    let (mut s_light, mut s_transform, mut cascade_config) = q_sun.single_mut().expect("no result in q_sun");
    let s_orbit = orbit(dtd, SUNRISE, SUNSET);
    let s_toward = toward(s_orbit, tan(SEASON_SWING) * cos(dty * 2. * PI));
    let s_elevation = elevation(s_toward);
    let s_direct = direct(s_elevation);
    s_light.color = tint(s_direct);
    s_light.illuminance = SUN_ILLUMINANCE * s_direct;
    *s_transform = Transform::from_translation(s_toward * 1_000.).looking_at(Vec3::ZERO, Vec3::Y);

    // The sky: lit through twilight, reddened around the crossing, and
    // the ambient is its light on the ground, blue as it is overhead.
    let sky_bright = smoothstep(-TWILIGHT_BELOW, CLEAR_ABOVE, s_elevation);
    let sky_tint = tint(smoothstep(-GLOW_BELOW, CLEAR_ABOVE, s_elevation));
    a_light.brightness = 800. * sky_bright;
    a_light.color = Color::linear_rgb(0.7 + 0.3 * sky_bright, 0.8 + 0.2 * sky_bright, 1.0);

    // moon
    let (mut m_light, mut m_transform) = q_moon.single_mut().expect("no result in q_moon");
    let m_orbit = orbit(dtd, MOONRISE, MOONSET);
    let m_toward = toward(m_orbit, 0.);
    // The month turns the moon from full to new and back: its light is
    // its lit fraction, and its disc is lit from that far round from the
    // camera, on the sun's side.
    let m_elongation = dtm * 2. * PI;
    let m_lit = (1. + cos(m_elongation)) / 2.;
    let m_phase = 0.1 + 0.9 * m_lit;
    m_light.illuminance = MOON_ILLUMINANCE * m_phase * direct(elevation(m_toward));
    *m_transform = Transform::from_translation(m_toward * 1_000.).looking_at(Vec3::ZERO, Vec3::Y);

    // The sky is the sun's light scattered, so it carries the sky's
    // brightness and the moon's share at night, with the sun's tint as a
    // glow toward the sun; the haze carries the brightness and a share
    // of the tint. Neither stays daylight-grey over a dark ground.
    let moon = m_light.color.to_linear().to_vec3() * (m_light.illuminance / SUN_ILLUMINANCE);
    let sky_tint = sky_tint.to_linear().to_vec3();
    let lit = |tint: Vec3| LinearRgba::from_vec3(HAZE_COLOR.to_linear().to_vec3() * (tint * sky_bright + moon));
    clear.0 = lit(Vec3::ONE).into();
    let (camera, mut fog) = q_camera.single_mut().expect("no result in q_camera");
    fog.color = lit(Vec3::ONE.lerp(sky_tint, HAZE_TINT_SHARE)).into();
    if let Some(mut sky) = q_sky.single().ok().and_then(|m| sky_materials.get_mut(&m.0)) {
        sky.bearing = s_toward.xz().try_normalize().unwrap_or(Vec2::X).extend(0.).extend(0.);
        sky.base = lit(Vec3::ONE);
        sky.glow = lit(sky_tint) - lit(Vec3::ONE);
    }

    // Each disc faces the camera from its body's direction, in its light's
    // colour; the ground occludes it as it sets and its shader cuts it at
    // the waterline.
    let camera = camera.translation();
    for (disc, mut transform, material) in &mut q_discs {
        let toward = match disc { Disc::Sun => s_toward, Disc::Moon => m_toward };
        transform.translation = camera + toward * disc_distance_wu();
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, -toward);
        let Some(mut material) = disc_materials.get_mut(&material.0) else { continue };
        match disc {
            Disc::Sun => material.color = s_light.color.to_linear() * SUN_DISC_BRIGHTNESS,
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
        use crate::systems::camera::{CAMERA_DISTANCE, gameplay_camera_height};
        let height = gameplay_camera_height();
        let loading_r = chunk::terrain_chunk_radius(player_loc.z) as f32;
        let camera_to_player = (CAMERA_DISTANCE * CAMERA_DISTANCE + height * height).sqrt();
        let max_dist = camera_to_player + loading_r * 0.8 * CHUNK_EXTENT_WU;
        let current_max = cascade_config.bounds.last().copied().unwrap_or(0.0);
        if (max_dist - current_max).abs() > 1.0 {
            *cascade_config = CascadeShadowConfigBuilder {
                maximum_distance: max_dist,
                ..default()
            }.into();
        }
    }
}

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

// ── Summary Mesh Pipeline ──
// All terrain rendering — r=0 tiles through r=N summaries — goes through
// this unified pipeline. No separate chunk mesh path.

/// Dispatch async tasks to build summary mesh regions.

/// **Forced mode** (`Some(r)`): single band at that radius, all visible regions.
/// **Auto mode** (`None`): multiple bands from `compute_active_bands`, with overlap.
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
    let summary_lat = common_bevy::summary::summary_lattice(key.r);
    let region_lat = common_bevy::summary::mesh_region_lattice();
    let region_center = region_lat.cell_center((key.mn, key.mm));
    let (cq, cr) = summary_lat.cell_center(region_center);
    common_bevy::geometry::flat_top_tile_center(cq, cr, 1.0)
}

pub fn dispatch_summary_tasks(
    mut commands: Commands,
    loaded_chunks: Res<LoadedChunks>,
    map: Res<common_bevy::resources::map::Map>,
    mut summary_meshes: ResMut<SummaryMeshes>,
    forced_radius: Res<ForcedSummaryRadius>,
    summary_cache: Res<crate::resources::SummaryCache>,
    client_timers: Res<crate::resources::ClientTimers>,
    edges: Res<crate::resources::EdgeCenters>,
    origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, (With<common_bevy::components::behaviour::PlayerControlled>, With<common_bevy::components::Actor>)>,
    mut last_eval_pos: Local<Option<Vec3>>,
    mut last_eval_edges: Local<HashMap<u32, Vec2>>,
    mut backlog: Local<bool>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    // The camera's world position, for the regions: the player is drawn
    // about the render origin.
    #[cfg(feature = "admin")]
    let camera_pos = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.world_position)
        .or_else(|| player_query.single().ok().map(|t| origin.world(t.translation)));
    #[cfg(not(feature = "admin"))]
    let camera_pos = player_query.single().ok().map(|t| origin.world(t.translation));

    let map_changed = map.take_changed();
    let cache_changed = summary_cache.take_new_data();
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
    let _t = client_timers.0.scope("sum_disp");

    // Local-data boundary: the largest circle inside guaranteed chunk
    // coverage (the hexagonal chunk set's APOTHEM — the circumradius
    // over-claims by ~80 WU in edge directions). In gameplay the server
    // streams chunks to FIXED_STREAM_RADIUS; in flyover only the
    // detail-chunk disc around the camera exists.
    #[cfg(feature = "admin")]
    let local_boundary = flyover
        .as_ref()
        .filter(|f| f.active)
        .map(|f| f.local_boundary_wu())
        .unwrap_or(common_bevy::chunk::FIXED_STREAM_APOTHEM_WU);
    #[cfg(not(feature = "admin"))]
    let local_boundary = common_bevy::chunk::FIXED_STREAM_APOTHEM_WU;

    // Local regions: camera-dependent (bands within streaming radius).
    // `needed` is the crisp band assignment (what to build); `keep` widens
    // each band edge by the hysteresis margin (what may survive).
    let (needed, keep): (
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
        std::collections::HashSet<common_bevy::summary_mesh::MeshRegionKey>,
    ) = match (camera_pos, forced_radius.0) {
        (_, Some(r)) => {
            let n = common_bevy::summary_mesh::visible_mesh_regions(r, &loaded_chunks.chunks);
            let k = n.clone();
            (n, k)
        }
        (Some(pos), None) => {
            let regions = |at: Vec2, margin: f32| {
                compute_auto_mode_regions(at, &loaded_chunks.chunks, margin, local_boundary)
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
        (None, None) => {
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
        if let Some(state) = summary_meshes.states.remove(&key) {
            if let Some(entity) = state.entity {
                commands.entity(entity).despawn();
            }
        }
    }

    // Dispatch build tasks for needed regions, nearest-first so the terrain
    // in front of the camera fills before the horizon (matches server and
    // flyover dispatch order).
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
    const MAX_MESH_TASKS: usize = 16;
    let mut mesh_dispatched = 0;

    let mut ordered: Vec<(f32, common_bevy::summary_mesh::MeshRegionKey, Vec3)> = needed
        .iter()
        .map(|region_key| {
            let summary_lat = common_bevy::summary::summary_lattice(region_key.r);
            let region_lat = common_bevy::summary::mesh_region_lattice();
            let region_center = region_lat.cell_center((region_key.mn, region_key.mm));
            let (cq, cr) = summary_lat.cell_center(region_center);
            let (wx, wz) = common_bevy::geometry::flat_top_tile_center(cq, cr, 1.0);
            let d2 = camera_pos.map_or(0.0, |pos| {
                let dx = wx - pos.x;
                let dz = wz - pos.z;
                dx * dx + dz * dz
            });
            (d2, *region_key, Vec3::new(wx, 0.0, wz))
        })
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (d2, region_key, mesh_origin) in ordered {
        if mesh_dispatched >= MAX_MESH_TASKS { break; }
        let radius = region_key.r;

        // Built regions are final: their heights are durable. A region
        // still waiting for data is retried once data has arrived since its
        // build was dispatched, judged by epoch and never by this run's
        // flag alone: the flag is consumed whether or not the region could
        // act on it, and it cannot while a build is in flight or once the
        // task budget is spent. The one level standing the tiles' trees is
        // final only once it holds the tiles too: its ground may have been
        // built from the summaries, which arrive first.
        let tiles_loaded = radius != common_bevy::summary::LOD_LEVELS[1]
            || common_bevy::summary_mesh::region_tiles_loaded(region_key, &loaded_chunks.chunks);
        if summary_meshes.states.get(&region_key).is_some_and(|state| !state.wants_build(epoch, tiles_loaded)) {
            continue;
        }

        // r>0 regions beyond the local-data boundary are server/flyover-owned:
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

        let rk = region_key;
        let map_snap = map.clone();
        let cache_snap = summary_cache.clone();

        let task = pool.spawn(async move {
            collect_and_build_summary_mesh(radius, rk, &map_snap, &cache_snap)
        });

        if let Some(state) = summary_meshes.states.get_mut(&region_key) {
            state.task = Some(task);
            state.epoch = epoch;
            state.tiles_loaded = tiles_loaded;
        } else {
            summary_meshes.states.insert(
                region_key,
                SummaryMeshState {
                    task: Some(task),
                    entity: None,
                    mesh_handle: None,
                    tri_count: 0,
                    mesh_origin,
                    base_positions: Vec::new(),
                    base_normals: Vec::new(),
                    base_coarse: Vec::new(),
                    base_canopy: Vec::new(),
                    base_indices: Vec::new(),
                    base_tri_count: 0,
                    base_water: Default::default(),
                    base_trees: Vec::new(),
                    trees_spawned: false,
                    cards_spawned: false,
                    waiting: false,
                    tiles_loaded,
                    epoch,
                },
            );
        }
        mesh_dispatched += 1;
    }

    // Budget exhausted with regions possibly still undispatched.
    if mesh_dispatched >= MAX_MESH_TASKS {
        *backlog = true;
    }
}

/// Active bands out to the reach, the same bound the server and flyover
/// producers cover to. `margin` widens it for the keep set.
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
/// drawn, and eases when it does. A forced debug radius lifts the cut.
/// The edges live in world coordinates, the frame of the region lattice;
/// the uniform carries them as rendered, the frame the shaders see.
pub fn update_terrain_cut(
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    terrain_material: Res<TerrainMaterial>,
    forced_radius: Res<ForcedSummaryRadius>,
    summary_meshes: Res<SummaryMeshes>,
    mut edges: ResMut<crate::resources::EdgeCenters>,
    mut card_band: ResMut<crate::resources::CardBand>,
    time: Res<Time>,
    render_origin: Res<crate::resources::RenderOrigin>,
    player_query: Query<&Transform, (With<PlayerControlled>, With<common_bevy::components::Actor>)>,
    #[cfg(feature = "admin")] flyover: Option<Res<crate::plugins::flyover::FlyoverState>>,
) {
    let player = || player_query.single().ok().map(|t| render_origin.world(t.translation));
    #[cfg(feature = "admin")]
    let origin = match flyover.as_ref().filter(|f| f.active) {
        Some(f) => Some(f.world_position),
        None => player(),
    };
    #[cfg(not(feature = "admin"))]
    let origin = player();
    let Some(origin) = origin else { return };

    let bands = horizon_bands(0.0);
    let target = origin.xz();
    if forced_radius.0.is_none() {
        advance_edges(&mut edges.0, &bands, target, time.delta_secs(), &summary_meshes);
    }

    // The span the canopy's ground rises over is the cards' own: from the
    // ring where the models hand over to where the last card is drawn.
    // It falls away again across the whole of the next level's band, so
    // the level past that carries none. Every level is given the same
    // spans, so the ground they draw agrees where their bands meet.
    let (lift, fall) = if forced_radius.0.is_some() {
        (Vec4::ZERO, Vec4::ZERO)
    } else {
        let at = |r: u32| level_cut(r, &bands, &edges.0, target).rendered(render_origin.world_vec().xz());
        let ring = at(0);
        let cards = at(common_bevy::summary::LOD_LEVELS[1]).outer;
        let third = at(common_bevy::summary::LOD_LEVELS[2]);
        (ring.outer_center.extend(ring.outer).extend(cards), third.outer_center.extend(third.inner).extend(third.outer))
    };

    // A material is written only where its cut or lift has moved: a
    // changed material is prepared again and every region wearing it
    // specialised again, which for the terrain is every mesh it has.
    let mut band = *card_band;
    for (&r, handle) in &terrain_material.by_level {
        let cut = if forced_radius.0.is_some() {
            crate::resources::TerrainCut::default()
        } else {
            level_cut(r, &bands, &edges.0, target).rendered(render_origin.world_vec().xz())
        };
        let Some(material) = materials.get(handle) else { continue };
        let canopy = &material.extension.canopy;
        if material.extension.cut != cut || canopy.lift != lift || canopy.fall != fall {
            if let Some(mut material) = materials.get_mut(handle) {
                material.extension.cut = cut;
                material.extension.canopy.lift = lift;
                material.extension.canopy.fall = fall;
            }
        }
        // The ring is the tiles' own outer edge, with their ground's
        // overlap; the cards have sunk away by the first summary level's.
        if r == 0 {
            band.center = cut.outer_center;
            band.inner = if cut.fade > 0.0 { cut.outer } else { 0.0 };
            band.overlap = cut.fade;
        }
        if r == common_bevy::summary::LOD_LEVELS[1] {
            band.sink_to = cut.outer;
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

/// Whether the ground at a point is drawn: the region of the level the cut
/// shows there has an entity. What the camera's envelope reads, and the
/// same test an edge makes before it moves.
pub struct DrawnGround<'a> {
    /// Each level's cut, finest first; the first that holds a point shows it.
    cuts: Vec<(u32, crate::resources::TerrainCut)>,
    region_lat: common::HexLattice,
    meshes: &'a SummaryMeshes,
}

impl<'a> DrawnGround<'a> {
    /// The cut as the frame draws it around `origin`, the player's ground
    /// position, with the edges where they stand.
    pub fn new(origin: Vec2, edges: &HashMap<u32, Vec2>, meshes: &'a SummaryMeshes) -> Self {
        let bands = horizon_bands(0.0);
        let cuts = bands.iter().map(|b| (b.r, level_cut(b.r, &bands, edges, origin))).collect();
        Self { cuts, region_lat: common_bevy::summary::mesh_region_lattice(), meshes }
    }

    pub fn at(&self, p: Vec2) -> bool {
        let shown = self.cuts.iter().find(|(_, c)| {
            p.distance(c.inner_center) >= c.inner && p.distance(c.outer_center) < c.outer
        });
        let Some(&(r, _)) = shown else { return false };
        let (sq, sr) = common_bevy::summary::summary_lattice(r).cell_at(p);
        let (mn, mm) = self.region_lat.cell_id(sq, sr);
        let key = common_bevy::summary_mesh::MeshRegionKey { r, mn, mm };
        self.meshes.states.get(&key).is_some_and(|s| s.entity.is_some())
    }
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

/// Compute visible mesh regions for auto mode (multi-band).

/// Local bands (within `local_boundary_wu`): gated on loaded chunks.
/// Remote bands (beyond it): ungated — data from server/flyover summaries.

/// `local_boundary_wu`: the extent the Map can serve — FIXED_STREAM_RADIUS_WU
/// in gameplay, the flyover's detail-chunk radius while flyover is active.
/// `margin`: hysteresis expansion of each band's annulus (0.0 = crisp band
/// assignment for building; > 0.0 = widened keep set for eviction).
fn compute_auto_mode_regions(
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
    // chunk-fed) AND an ungated segment (server/flyover-fed). Assigning the
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
            // server or flyover producer)
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
/// tile z straight from the Map. r>0 reads the SummaryCache (server- or
/// flyover-fed) and falls back to sampling the Map with the same 7-sample
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
) -> SummaryMeshBuildResult {
    let empty = SummaryMeshBuildResult {
        positions: Vec::new(),
        normals: Vec::new(),
        coarse: Vec::new(),
        canopy: Vec::new(),
        indices: Vec::new(),
        tri_count: 0,
        mesh_origin: Vec3::ZERO,
        water: Default::default(),
        trees: Vec::new(),
    };

    let tile_z = |q: i32, r: i32| -> Option<i32> { map.get_by_qr(q, r).map(|(qrz, _)| qrz.z) };
    // The same seven samples over the map's tiles, where every producer's
    // rule reads them; nothing until the tiles are all there.
    let sampled = |level: u32, sq: i32, sr: i32| common_bevy::summary::summarize(level, sq, sr, map);

    // The water over the region, built once the ground is: at r = 0 the
    // map's per-tile surface, above it the surface each summary carries.
    let with_water = |smr: &common_bevy::summary_mesh::SummaryMeshResult, water: &dyn Fn(i32, i32) -> Option<i32>| {
        let mut result = smr_to_result(smr);
        let w = common_bevy::summary_mesh::build_water_mesh_region(radius, region_key, water);
        result.water = crate::resources::WaterGeometry { positions: w.positions, normals: w.normals, indices: w.indices };
        result
    };

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
        move |sq: i32, sr: i32| -> Option<i32> {
            if level == 0 {
                return tile_z(sq, sr);
            }
            if let Some(cell) = cached(level, sq, sr) {
                return Some(cell.z);
            }
            sampled(level, sq, sr).map(|c| c.z)
        }
    };
    let height = level_z(radius);
    let coarse = common_bevy::summary::coarser_level(radius).map(level_z);
    let coarse: Option<&dyn Fn(i32, i32) -> Option<i32>> = coarse.as_ref().map(|c| c as &dyn Fn(i32, i32) -> Option<i32>);

    // The trees stand with the ground: placed here from the map's covers
    // at every level the map reaches, spawned with the ground once the kit
    // is loaded.
    if radius == 0 {
        let tile_water = |q: i32, r: i32| -> Option<i32> { map.water_at(q, r) };
        return common_bevy::summary_mesh::build_summary_mesh_region(0, region_key, &height, coarse, None)
            .as_ref()
            .map_or(empty, |smr| {
                let mut result = with_water(smr, &tile_water);
                result.trees = crate::plugins::forest::place_trees(0, region_key, smr.mesh_origin, map, &height);
                result
            });
    }

    // Water follows the height's provenance: the cached surface where the
    // cell was sent, else the same seven samples over the map's tiles, and
    // nothing where the tiles are not all there.
    let summary_water = |sq: i32, sr: i32| -> Option<i32> {
        match cached(radius, sq, sr) {
            Some(cell) => cell.water,
            None => sampled(radius, sq, sr)?.water,
        }
    };

    // The level's canopy follows the height's provenance too. Every level
    // above the tiles but the last carries it on the ground as a colour —
    // the first stands its trees on that ground as well, from the map's
    // own covers, which reach exactly as far as that level does, so the
    // ground under the trees is the ground past them and the trees are
    // the tiles' own — and the material lays crowns on it or not by level.
    let summary_canopy = |sq: i32, sr: i32| -> Option<common::Canopy> {
        cached(radius, sq, sr).or_else(|| sampled(radius, sq, sr)).map(|c| c.canopy)
    };
    let last = *common_bevy::summary::LOD_LEVELS.last().expect("a ladder");
    let canopied: Option<&dyn Fn(i32, i32) -> Option<common::Canopy>> = (radius != last).then_some(&summary_canopy);

    common_bevy::summary_mesh::build_summary_mesh_region(radius, region_key, &height, coarse, canopied)
        .as_ref()
        .map_or(empty, |smr| {
            let mut result = with_water(smr, &summary_water);
            if radius == common_bevy::summary::LOD_LEVELS[1] {
                result.trees = crate::plugins::forest::place_trees(radius, region_key, smr.mesh_origin, map, &height);
            }
            // Past the tiles a crag stands from the summaries' own reading
            // of it, the level after the first.
            if radius == common_bevy::summary::LOD_LEVELS[2] {
                let outcrop = |sq: i32, sr: i32| cached(radius, sq, sr).or_else(|| sampled(radius, sq, sr)).map(|c| c.outcrop);
                result.trees = crate::plugins::forest::place_crags(radius, region_key, smr.mesh_origin, &outcrop, &height);
            }
            result
        })
}

fn smr_to_result(smr: &common_bevy::summary_mesh::SummaryMeshResult) -> SummaryMeshBuildResult {
    SummaryMeshBuildResult {
        positions: smr.positions.clone(),
        normals: smr.normals.clone(),
        coarse: smr.coarse.clone(),
        canopy: smr.canopy.clone(),
        indices: smr.indices.clone(),
        tri_count: smr.tri_count,
        mesh_origin: smr.mesh_origin,
        water: Default::default(),
        trees: Vec::new(),
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut tri_stats: ResMut<LodTriangleStats>,
    mut terrain_material: ResMut<TerrainMaterial>,
    mut materials: ResMut<Assets<crate::resources::TerrainMaterialAsset>>,
    water_material: Res<crate::plugins::water::WaterMaterial>,
    client_timers: Res<crate::resources::ClientTimers>,
) {
    let _t = client_timers.0.scope("sum_poll");

    // Poll async tasks, keeping the geometry so the entity can be respawned
    // without a rebuild.
    let mut to_upload: Vec<common_bevy::summary_mesh::MeshRegionKey> = Vec::new();

    for (&region_key, state) in summary_meshes.states.iter_mut() {
        if let Some(task) = &mut state.task {
            if let Some(result) = block_on(future::poll_once(task)) {
                state.task = None;
                state.mesh_origin = result.mesh_origin;
                state.base_positions = result.positions;
                state.base_normals = result.normals;
                state.base_coarse = result.coarse;
                state.base_canopy = result.canopy;
                state.base_indices = result.indices;
                state.base_tri_count = result.tri_count;
                state.base_water = result.water;
                state.base_trees = result.trees;
                state.waiting = result.tri_count == 0;
                to_upload.push(region_key);
            }
        }
    }

    // Orphaned states: geometry but no entity, after a flyover stash restore
    // (entities were despawned on toggle-on). A task may already be pending;
    // the stored geometry goes up now so there is no flash.
    for (&region_key, state) in summary_meshes.states.iter() {
        if !state.base_positions.is_empty() && state.entity.is_none() && !to_upload.contains(&region_key) {
            to_upload.push(region_key);
        }
    }

    struct MeshBuild {
        key: common_bevy::summary_mesh::MeshRegionKey,
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        coarse: Vec<[f32; 4]>,
        canopy: Vec<[f32; 4]>,
        indices: Vec<u32>,
        tri_count: u32,
        water: crate::resources::WaterGeometry,
    }

    let builds: Vec<MeshBuild> = to_upload
        .iter()
        .filter_map(|&key| {
            let state = summary_meshes.states.get(&key)?;
            (!state.base_positions.is_empty()).then(|| MeshBuild {
                key,
                positions: state.base_positions.clone(),
                normals: state.base_normals.clone(),
                coarse: state.base_coarse.clone(),
                canopy: state.base_canopy.clone(),
                indices: state.base_indices.clone(),
                tri_count: state.base_tri_count,
                water: state.base_water.clone(),
            })
        })
        .collect();

    // Upload meshes, spawn/update entities.
    for build in builds {
        if build.tri_count == 0 {
            continue;
        }

        let mesh = build_bevy_mesh(&build.positions, &build.normals, Some(&build.coarse), &build.canopy, &build.indices);
        let mesh_handle = meshes.add(mesh);

        let state = summary_meshes.states.get_mut(&build.key).unwrap();
        state.mesh_handle = Some(mesh_handle.clone());
        state.tri_count = build.tri_count;

        let entity = match state.entity {
            Some(entity) => {
                commands.entity(entity).insert(Mesh3d(mesh_handle));
                // The water and the trees are the ground's children:
                // rebuilt with it.
                commands.entity(entity).despawn_related::<Children>();
                entity
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(mesh_handle),
                        MeshMaterial3d(terrain_material.for_level(build.key.r, &mut materials)),
                        Transform::from_translation(origin.render_world(state.mesh_origin)),
                        SummaryMesh { region_key: build.key },
                    ))
                    .id();
                state.entity = Some(entity);
                entity
            }
        };
        state.trees_spawned = false;
        state.cards_spawned = false;

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

    /// The sun is above the horizon exactly from sunrise to sunset, its
    /// orbit runs on without a break through both, and its day sweep is
    /// the slow one.
    #[test]
    fn the_sun_orbits_slowly_by_day_and_fast_by_night() {
        // Half-minute samples, so none lands on a rise or a set.
        let step = 1. / 1440.;
        let mut prev = orbit(1. - step / 2., SUNRISE, SUNSET);
        for minute in 0..1440 {
            let t = (minute as f32 + 0.5) * step;
            let a = orbit(t, SUNRISE, SUNSET);
            let up = (SUNRISE..SUNSET).contains(&t);
            assert_eq!(toward(a, 0.).y > 0., up, "t={t} a={a}");
            let advance = (a - prev).rem_euclid(2. * PI);
            assert!(advance > 0. && advance < 0.1, "t={t} advance={advance}");
            assert_eq!(direct(elevation(toward(a, 0.))) > 0., up, "t={t}");
            prev = a;
        }
        let day_rate = orbit(SUNRISE + step, SUNRISE, SUNSET) - orbit(SUNRISE, SUNRISE, SUNSET);
        let night_rate = orbit(SUNSET + step, SUNRISE, SUNSET) - orbit(SUNSET, SUNRISE, SUNSET);
        assert!(day_rate < night_rate, "{day_rate} vs {night_rate}");
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
        assert!(c.outer > 100.0 && c.outer < 200.0, "outer {}", c.outer);
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
                        mesh_origin: Vec3::ZERO,
                        base_positions: Vec::new(),
                        base_normals: Vec::new(),
                        base_coarse: Vec::new(),
                        base_canopy: Vec::new(),
                        base_indices: Vec::new(),
                        base_tri_count: 0,
                        base_water: Default::default(),
                        base_trees: Vec::new(),
                        trees_spawned: false,
                        cards_spawned: false,
                        waiting: false,
                        tiles_loaded: true,
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
            mesh_origin: Vec3::ZERO,
            base_positions: Vec::new(),
            base_normals: Vec::new(),
            base_coarse: Vec::new(),
            base_canopy: Vec::new(),
            base_indices: Vec::new(),
            base_tri_count: 0,
            base_water: Default::default(),
            base_trees: Vec::new(),
            trees_spawned: false,
            cards_spawned: false,
            waiting: true,
            tiles_loaded: true,
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
    }

    /// Coverage invariant for the LoD band system: every ground point inside
    /// the horizon (a) lies within at least one needed region at its band's
    /// level, and (b) if that region's footprint extends past the local-data
    /// boundary, the producer enumerates it (so its data will exist).

    /// Run for gameplay (boundary = FIXED_STREAM_RADIUS_WU) and flyover
    /// (small detail-chunk boundary) — the flyover case caught the missing
    /// ring between the flyover's chunks and the first produced band.
    #[test]
    fn lod_bands_cover_horizon_without_gaps() {
        use common_bevy::chunk::{
            calculate_visible_chunks, ChunkId, APOTHEM_FACTOR, CHUNK_EXTENT_WU,
            FIXED_STREAM_APOTHEM_WU, FIXED_STREAM_RADIUS,
        };
        use common_bevy::summary::{compute_active_bands, mesh_region_extent_wu};
        use common_bevy::summary_mesh::visible_lod_regions;

        let cases: &[(f32, f32, u8, f32)] = &[
            // (fov, camera ground y, loaded chunk ring, local boundary)
            (common::camera::MAX_GAMEPLAY_FOV, 0.0, FIXED_STREAM_RADIUS, FIXED_STREAM_APOTHEM_WU),
            (common::camera::MAX_GAMEPLAY_FOV, 80.0, FIXED_STREAM_RADIUS, FIXED_STREAM_APOTHEM_WU),
            (
                crate::systems::camera::MAX_FLYOVER_FOV,
                0.0,
                6,
                6.0 * CHUNK_EXTENT_WU * APOTHEM_FACTOR,
            ),
            (
                crate::systems::camera::MAX_FLYOVER_FOV,
                200.0,
                9,
                9.0 * CHUNK_EXTENT_WU * APOTHEM_FACTOR,
            ),
        ];

        for &(fov, cam_y, chunk_ring, boundary) in cases {
            // Chunks loaded exactly as the game does (hexagonal coverage —
            // the boundary is its inscribed circle).
            let loaded: std::collections::HashSet<ChunkId> =
                calculate_visible_chunks(ChunkId(0, 0), chunk_ring).into_iter().collect();

            let cam = Vec3::new(0.0, cam_y, 0.0);
            let needed = compute_auto_mode_regions(cam.xz(), &loaded, 0.0, boundary);

            // Producer set, to the same reach as the consumer.
            let bands = compute_active_bands(common_bevy::summary::reach_wu());
            let produced = visible_lod_regions(&bands, 0.0, 0.0, boundary);

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
                        // compute_auto_mode_regions uses for this band.
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
