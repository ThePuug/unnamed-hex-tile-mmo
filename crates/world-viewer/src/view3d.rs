//! The 3D view: the world's ground over an area, each ladder level drawn as
//! its own hex cells by the game's own mesh builders
//! (`common_bevy::summary_mesh`), so a cell's corners follow the corner rule
//! and a face between cells out of reach stands as a wall, as the game draws
//! it. The ladder is read through `Composite::ladders`, the viewer's
//! inspection of the stack (INV-009); the ground is the composite's.
//!
//! An area is built when it is first shown and kept per level and surface;
//! the ladder's parameters are read from a file, and reloading it builds
//! the world again, its top with it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use clap::Parser;

use common::summary::summarize;
use common_bevy::geometry::flat_top_tile_center;
use common_bevy::summary::{mesh_region_lattice, summary_lattice};
use common_bevy::summary_mesh::{build_summary_mesh_region, build_water_mesh_region, MeshRegionKey};
use common_bevy::surface::height_y;
use world::events::Composite;
use world::ladder::geom::{cube_round, pos3};
use world::ladder::{Params, STAGES};

/// The levels a view shows, finest first, as their width in tiles.
const LEVELS: [i64; 5] = [1, 3, 9, 27, 81];

#[derive(Parser)]
#[command(name = "world-viewer 3d", about = "The world's ground in 3D, a ladder level at a time")]
struct Cli {
    /// The tile the view is centred on, as q,r: the haven by default, which
    /// `HAVEN=<q>,<r>` moves as it moves the server's.
    #[arg(long, allow_hyphen_values = true)]
    centre: Option<String>,
    /// How far the view reaches either way of its centre, in tiles.
    #[arg(long, default_value_t = 384)]
    radius: i32,
    /// The ladder's parameters, reloaded with R.
    #[arg(long, default_value = "crates/world-viewer/ladder.params")]
    params: PathBuf,
    /// The level shown first, as its width: 1, 3, 9, 27 or 81.
    #[arg(long, default_value_t = 1)]
    level: i64,
    /// The surface shown first: ladder, dry or stack.
    #[arg(long, default_value = "ladder")]
    surface: String,
    /// Water drawn, faces highlighted and lines overlaid from the start.
    #[arg(long)]
    no_water: bool,
    #[arg(long)]
    faces: bool,
    #[arg(long)]
    lines: bool,
    /// The camera: the tile it looks at, as q,r (the centre by default),
    /// its bearing in degrees clockwise from north, its pitch below the
    /// horizon, and its distance in world units.
    #[arg(long, allow_hyphen_values = true)]
    look: Option<String>,
    #[arg(long, default_value_t = 90.0)]
    bearing: f32,
    #[arg(long, default_value_t = 35.0)]
    pitch: f32,
    #[arg(long, default_value_t = 260.0)]
    distance: f32,
    /// Take the first view to this PNG and exit.
    #[arg(long)]
    shot: Option<PathBuf>,
    /// Once the first view is up, reload the parameters from this file, as
    /// R does from `--params`, and take the shot after that view instead:
    /// for timing a reload.
    #[arg(long)]
    then_reload: Option<PathBuf>,
    /// A tile read into the panel once the first view is up, as a click
    /// on it reads it, as q,r.
    #[arg(long, allow_hyphen_values = true)]
    read: Option<String>,
    #[arg(long, default_value_t = world::WORLD_SEED)]
    seed: u64,
}

fn pair(s: &str) -> (i32, i32) {
    let (a, b) = s.split_once(',').expect("a pair as q,r");
    (a.trim().parse().expect("q"), b.trim().parse().expect("r"))
}

/// The ladder's parameters from a file of `name = value` lines, `#` a
/// comment; a name the file leaves out keeps its default.
fn read_params(path: &PathBuf) -> Params {
    let mut p = Params::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        warn!("no parameters at {}: the defaults", path.display());
        return p;
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((name, value)) = line.split_once('=') else { continue };
        let (name, value) = (name.trim(), value.trim());
        let num = || value.parse::<f64>().unwrap_or_else(|_| panic!("{name} = {value}: a number"));
        match name {
            "catchment_full" => p.catchment_full = num(),
            "cut_from" => p.cut_from = num(),
            "wall" => p.wall = num(),
            "ring" => p.ring = num(),
            "water" => p.water = value == "true",
            _ => warn!("{}: no parameter {name}", path.display()),
        }
    }
    p
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Surface {
    Ladder,
    Dry,
    Stack,
}

impl Surface {
    fn parse(s: &str) -> Surface {
        match s {
            "dry" => Surface::Dry,
            "stack" => Surface::Stack,
            _ => Surface::Ladder,
        }
    }

    fn next(self) -> Surface {
        match self {
            Surface::Ladder => Surface::Dry,
            Surface::Dry => Surface::Stack,
            Surface::Stack => Surface::Ladder,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Surface::Ladder => "ladder",
            Surface::Dry => "ladder dry",
            Surface::Stack => "stack",
        }
    }
}

/// The world a view reads: the stack with its ladder, and the stack alone.
struct Worlds {
    world: Composite,
    stack: Composite,
    params: Params,
}

/// One region's meshes as built, in the region's frame.
struct Region {
    offset: Vec3,
    ground: (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>),
    water: Option<(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>)>,
}

/// A level's area built over one surface: its regions, every cell's height,
/// its lines as segments in the view's frame, and how long it took.
struct Built {
    level: i64,
    surface: Surface,
    generation: u32,
    regions: Vec<Region>,
    heights: HashMap<(i32, i32), i32>,
    lines: Vec<[Vec3; 2]>,
    seconds: f64,
}

#[derive(Resource)]
struct View {
    centre: (i32, i32),
    radius: i32,
    params_path: PathBuf,
    worlds: Arc<Worlds>,
    /// Raised by each reload: what was built before it is stale.
    generation: u32,
    level: i64,
    surface: Surface,
    water: bool,
    faces: bool,
    lines: bool,
    built: HashMap<(i64, Surface), Built>,
    pending: Option<(i64, Surface)>,
    /// When the asked-for build was asked for, and why: the launch or a
    /// reload, for the timings.
    asked: Option<(Instant, &'static str)>,
    shown: Option<(i64, Surface, u32, bool, bool, bool)>,
    inspect: String,
    status: String,
    shot: Option<PathBuf>,
    shot_wait: u32,
    then_reload: Option<PathBuf>,
    read: Option<(i32, i32)>,
}

#[derive(Resource)]
struct Jobs {
    tx: Sender<Built>,
    rx: Mutex<Receiver<Built>>,
}

#[derive(Resource)]
struct Orbit {
    focus: Vec3,
    bearing: f32,
    pitch: f32,
    distance: f32,
}

#[derive(Component)]
struct Shown;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct InspectText;

#[derive(Resource)]
struct Launched(Instant);

pub fn run(args: Vec<String>) {
    let launched = Instant::now();
    let cli = Cli::parse_from(args);
    let haven = common_bevy::haven::launch();
    let centre = cli.centre.as_deref().map_or((haven.q, haven.r), pair);
    let params = read_params(&cli.params);
    let worlds = Arc::new(Worlds { world: Composite::standard_with(cli.seed, params), stack: Composite::stack_alone(cli.seed), params });
    let (tx, rx) = channel();
    let look = cli.look.as_deref().map(pair).unwrap_or(centre);
    let (fx, fz) = flat_top_tile_center(look.0 - centre.0, look.1 - centre.1, 1.0);
    let view = View {
        centre,
        radius: cli.radius,
        params_path: cli.params.clone(),
        worlds,
        generation: 0,
        level: if LEVELS.contains(&cli.level) { cli.level } else { 1 },
        surface: Surface::parse(&cli.surface),
        water: !cli.no_water,
        faces: cli.faces,
        lines: cli.lines,
        built: HashMap::new(),
        pending: None,
        asked: Some((launched, "launch")),
        shown: None,
        inspect: String::from("click a tile to read it"),
        status: String::new(),
        shot: cli.shot.clone(),
        shot_wait: 0,
        then_reload: cli.then_reload.clone(),
        read: cli.read.as_deref().map(pair),
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "world-viewer 3d".into(), resolution: (1600u32, 900u32).into(), ..default() }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.62, 0.74, 0.86)))
        .insert_resource(GlobalAmbientLight { brightness: 150.0, ..default() })
        .insert_resource(view)
        .insert_resource(Jobs { tx, rx: Mutex::new(rx) })
        .insert_resource(Orbit { focus: Vec3::new(fx, 0.0, fz), bearing: cli.bearing, pitch: cli.pitch, distance: cli.distance })
        .insert_resource(Launched(launched))
        .add_systems(Startup, setup)
        .add_systems(Update, (keys, orbit, ask, receive, show, inspect, text, shoot).chain())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::default()));
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-400.0, 900.0, -300.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let style = TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() };
    commands.spawn((
        Text::new(""),
        style.clone(),
        TextColor(Color::BLACK),
        Node { position_type: PositionType::Absolute, top: Val::Px(8.0), left: Val::Px(10.0), ..default() },
        StatusText,
    ));
    commands.spawn((
        Text::new(""),
        style,
        TextColor(Color::BLACK),
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(8.0), left: Val::Px(10.0), padding: UiRect::all(Val::Px(6.0)), ..default() },
        InspectText,
    ));
}

/// The switches: 1–5 the level, Z the surface, X water, C faces, L lines,
/// R the parameters reloaded and the world built again, P a screenshot.
fn keys(input: Res<ButtonInput<KeyCode>>, mut view: ResMut<View>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    for (key, level) in [(KeyCode::Digit1, 1), (KeyCode::Digit2, 3), (KeyCode::Digit3, 9), (KeyCode::Digit4, 27), (KeyCode::Digit5, 81)] {
        if input.just_pressed(key) {
            view.level = level;
        }
    }
    if input.just_pressed(KeyCode::KeyZ) {
        view.surface = view.surface.next();
    }
    if input.just_pressed(KeyCode::KeyX) {
        view.water = !view.water;
    }
    if input.just_pressed(KeyCode::KeyC) {
        view.faces = !view.faces;
    }
    if input.just_pressed(KeyCode::KeyL) {
        view.lines = !view.lines;
    }
    if input.just_pressed(KeyCode::KeyR) {
        let path = view.params_path.clone();
        reload(&mut view, &path);
    }
    if input.just_pressed(KeyCode::KeyP) {
        let path = PathBuf::from(format!("proofs/world-viewer/view3d-{}.png", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())));
        info!("screenshot to {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if input.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

/// The world built again under the parameters at `path`, everything built
/// before it dropped.
fn reload(view: &mut View, path: &PathBuf) {
    let asked = Instant::now();
    let params = read_params(path);
    let seed = world::WORLD_SEED;
    view.worlds = Arc::new(Worlds { world: Composite::standard_with(seed, params), stack: Composite::stack_alone(seed), params });
    view.generation += 1;
    view.built.clear();
    view.pending = None;
    view.shown = None;
    view.asked = Some((asked, "reload"));
    info!("reloaded {}: {:?}", path.display(), params);
}

/// Right drag orbits, middle drag pans, the wheel zooms; WASD pans, Q and E
/// turn.
fn orbit(
    mut orbit: ResMut<Orbit>,
    buttons: Res<ButtonInput<MouseButton>>,
    input: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    view: Res<View>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    let d = motion.delta;
    if buttons.pressed(MouseButton::Right) {
        orbit.bearing += d.x * 0.3;
        orbit.pitch = (orbit.pitch + d.y * 0.3).clamp(2.0, 89.0);
    }
    let yaw = orbit.bearing.to_radians();
    // Bearing 0 looks north, toward -z; east is +x.
    let ahead = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let span = orbit.distance;
    if buttons.pressed(MouseButton::Middle) {
        orbit.focus += (-right * d.x + ahead * d.y) * span * 0.002;
    }
    let step = span * time.delta_secs();
    for (key, dir) in [(KeyCode::KeyW, ahead), (KeyCode::KeyS, -ahead), (KeyCode::KeyD, right), (KeyCode::KeyA, -right)] {
        if input.pressed(key) {
            orbit.focus += dir * step;
        }
    }
    if input.pressed(KeyCode::KeyQ) {
        orbit.bearing -= 60.0 * time.delta_secs();
    }
    if input.pressed(KeyCode::KeyE) {
        orbit.bearing += 60.0 * time.delta_secs();
    }
    if scroll.delta.y != 0.0 {
        orbit.distance = (orbit.distance * (1.0 - scroll.delta.y * 0.1)).clamp(5.0, 20_000.0);
    }
    // The focus rides the ground shown under it.
    if let Some(b) = view.built.get(&(view.level, view.surface)) {
        if let Some(z) = height_under(b, view.centre, orbit.focus.xz()) {
            orbit.focus.y = height_y(z as f32);
        }
    }
    let pitch = orbit.pitch.to_radians();
    let eye = orbit.focus - ahead * orbit.distance * pitch.cos() + Vec3::Y * orbit.distance * pitch.sin();
    if let Ok(mut t) = camera.single_mut() {
        *t = Transform::from_translation(eye).looking_at(orbit.focus, Vec3::Y);
    }
}

/// The height of the shown level's cell under a point of the view's frame.
fn height_under(b: &Built, centre: (i32, i32), xz: Vec2) -> Option<i32> {
    let r = ((b.level - 1) / 2) as u32;
    let lat = summary_lattice(r);
    let cell = lat.cell_from(centre, xz);
    b.heights.get(&cell).copied()
}

/// A build asked for where the shown level and surface have none.
fn ask(mut view: ResMut<View>, jobs: Res<Jobs>) {
    let want = (view.level, view.surface);
    if view.built.contains_key(&want) || view.pending.is_some() {
        return;
    }
    view.pending = Some(want);
    if view.asked.is_none() {
        view.asked = Some((Instant::now(), "switch"));
    }
    let (worlds, centre, radius, generation, tx) = (view.worlds.clone(), view.centre, view.radius, view.generation, jobs.tx.clone());
    std::thread::spawn(move || {
        let built = build(&worlds, centre, radius, want.0, want.1, generation);
        let _ = tx.send(built);
    });
}

fn receive(mut view: ResMut<View>, jobs: Res<Jobs>, launched: Res<Launched>) {
    let Ok(rx) = jobs.rx.lock() else { return };
    while let Ok(built) = rx.try_recv() {
        if built.generation != view.generation {
            continue;
        }
        let key = (built.level, built.surface);
        if view.pending == Some(key) {
            view.pending = None;
        }
        let line = match view.asked.take() {
            Some((at, why)) => format!(
                "level {} {}: built in {:.1} s; {} {:.1} s ago (from launch {:.1} s)",
                built.level,
                built.surface.name(),
                built.seconds,
                why,
                at.elapsed().as_secs_f64(),
                launched.0.elapsed().as_secs_f64()
            ),
            None => format!("level {} {}: built in {:.1} s", built.level, built.surface.name(), built.seconds),
        };
        info!("{line}");
        view.status = line;
        view.built.insert(key, built);
    }
}

/// Everything shown replaced when the level, the surface or a switch
/// changes, or a build lands.
fn show(
    mut view: ResMut<View>,
    mut commands: Commands,
    shown: Query<Entity, With<Shown>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let key = (view.level, view.surface, view.generation, view.water, view.faces, view.lines);
    let Some(b) = view.built.get(&(view.level, view.surface)) else { return };
    if view.shown == Some(key) {
        return;
    }
    for e in &shown {
        commands.entity(e).despawn();
    }
    let ground = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() });
    let water = materials.add(StandardMaterial { base_color: Color::srgba(0.15, 0.4, 0.85, 0.75), alpha_mode: AlphaMode::Blend, ..default() });
    // The ramp spans the heights shown.
    let mut zs: Vec<i32> = b.heights.values().copied().collect();
    zs.sort_unstable();
    let pick = |f: f64| zs.get(((zs.len().max(1) - 1) as f64 * f) as usize).copied().unwrap_or(0);
    let (lo, hi) = (pick(0.02), pick(0.98));
    let span = (height_y(lo as f32), height_y(hi as f32).max(height_y(lo as f32) + 1.0));
    for region in &b.regions {
        let (pos, nor, idx) = &region.ground;
        let colors: Vec<[f32; 4]> = pos.iter().zip(nor).map(|(p, n)| shade(p[1], *n, view.faces, span)).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(idx.clone()));
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(ground.clone()), Transform::from_translation(region.offset), Shown));
        if view.water {
            if let Some((pos, nor, idx)) = &region.water {
                let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor.clone());
                mesh.insert_indices(Indices::U32(idx.clone()));
                commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(water.clone()), Transform::from_translation(region.offset), Shown));
            }
        }
    }
    if view.lines && !b.lines.is_empty() {
        let pos: Vec<[f32; 3]> = b.lines.iter().flat_map(|s| [s[0].to_array(), s[1].to_array()]).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        let line = materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.1, 0.6), unlit: true, ..default() });
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(line), Transform::default(), Shown));
    }
    view.shown = Some(key);
}

/// A ground vertex's colour: by its height over the span shown, red on a
/// face where faces are highlighted, grey on a wall otherwise.
fn shade(y: f32, n: [f32; 3], faces: bool, span: (f32, f32)) -> [f32; 4] {
    let wall = n[1].abs() < 0.2;
    if wall {
        return if faces { [0.85, 0.12, 0.1, 1.0] } else { [0.45, 0.43, 0.4, 1.0] };
    }
    let t = ((y - span.0) / (span.1 - span.0)).clamp(0.0, 1.0);
    let (low, high) = ([0.25, 0.42, 0.2], [0.9, 0.85, 0.72]);
    [low[0] + (high[0] - low[0]) * t, low[1] + (high[1] - low[1]) * t, low[2] + (high[2] - low[2]) * t, 1.0]
}

/// The area round `centre` at level `w` over a surface: every region whose
/// cells lie within `radius`, its cells read in one request, its ground and
/// water meshed by the game's builders, and the level's lines.
fn build(worlds: &Worlds, centre: (i32, i32), radius: i32, w: i64, surface: Surface, generation: u32) -> Built {
    let started = Instant::now();
    let r = ((w - 1) / 2) as u32;
    let lat = summary_lattice(r);
    let rl = mesh_region_lattice();
    let home = lat.cell_id(centre.0, centre.1);
    let reach = (radius / (w as i32 * (2 * common_bevy::summary::MESH_REGION_RADIUS as i32 + 1))) as u32 + 1;
    let regions: Vec<(i32, i32)> = rl.cells_within_distance(rl.cell_id(home.0, home.1), reach);
    let mut cells: Vec<(i32, i32)> = Vec::new();
    for &region in &regions {
        for c in rl.tiles_in_cell(region) {
            for (dq, dr) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
                cells.push((c.0 + dq, c.1 + dr));
            }
        }
    }
    cells.sort_unstable();
    cells.dedup();
    let mut heights: HashMap<(i32, i32), i32> = HashMap::new();
    let mut water: HashMap<(i32, i32), i32> = HashMap::new();
    match surface {
        Surface::Stack => {
            for &(sq, sr) in &cells {
                let z = if w == 1 {
                    Some(worlds.stack.tile_at(sq, sr).elevation.round() as i32)
                } else {
                    summarize(r, sq, sr, &worlds.stack).map(|s| s.z)
                };
                if let Some(z) = z {
                    heights.insert((sq, sr), z);
                }
            }
        }
        _ => {
            let ladders = worlds.world.ladders().expect("the standard stack's ladders");
            let at: Vec<(i64, i64)> = cells.iter().map(|&(a, b)| (a as i64, b as i64)).collect();
            for (&c, cell) in cells.iter().zip(ladders.cells(w, &at)) {
                let Some(cell) = cell else { continue };
                heights.insert(c, if surface == Surface::Dry { cell.zd } else { cell.z } as i32);
                if surface == Surface::Ladder {
                    if let Some(s) = cell.water {
                        water.insert(c, s.round() as i32);
                    }
                }
            }
            if let Some(site) = ladders.site_of(centre.0, centre.1) {
                info!("{}", ladders.with(site, |l| timings(&l.terrain().ladder())));
            }
        }
    }
    let mut built = Vec::new();
    for &(mn, mm) in &regions {
        let key = MeshRegionKey { r, mn, mm };
        let height = |a: i32, b: i32| heights.get(&(a, b)).copied();
        let Some(ground) = build_summary_mesh_region(r, key, &height, None, None) else { continue };
        let origin = key.origin();
        let (x, z) = flat_top_tile_center(origin.q - centre.0, origin.r - centre.1, 1.0);
        let wet = |a: i32, b: i32| water.get(&(a, b)).copied();
        let water = (!water.is_empty()).then(|| build_water_mesh_region(r, key, &wet, &height)).filter(|m| !m.indices.is_empty());
        built.push(Region {
            offset: Vec3::new(x, 0.0, z),
            ground: (ground.positions, ground.normals, ground.indices),
            water: water.map(|m| (m.positions, m.normals, m.indices)),
        });
    }
    let lines = if surface == Surface::Stack { Vec::new() } else { lines(worlds, centre, radius, w) };
    Built { level: w, surface, generation, regions: built, heights, lines, seconds: started.elapsed().as_secs_f64() }
}

/// The seconds each level of the site's ladder has spent in each stage,
/// summed over every build since it was made, the cells each stage has
/// filled, and a digest of every routed vertex's values: two builds that
/// route alike digest alike.
fn timings<G: world::ladder::Ground>(ladder: &world::ladder::Ladder<'_, G>) -> String {
    use world::ladder::paged::{Atom, Paged, Progress, PAGE};
    fn count<T: Atom>(of: &Paged<T>, set: impl Fn(T) -> bool) -> usize {
        of.written().flat_map(|first| first..first + PAGE).filter(|&c| set(of.get(c))).count()
    }
    fn done(of: &Progress) -> usize {
        of.written().flat_map(|first| first..first + PAGE).filter(|&c| of.done(c)).count()
    }
    let mut out = format!(
        "ladder timings, s, cells read, routed, watered, and the routing's digest:\n{:>6}{}",
        "w",
        STAGES.iter().map(|s| format!("{s:>9}")).collect::<String>()
    );
    for (lv, t) in ladder.levels.iter().zip(&ladder.timing()) {
        let cells = [count(&lv.erod, |v| !v.is_nan()), done(&lv.routed), done(&lv.watered)];
        let nn = lv.cap();
        let mut digest = 0xcbf2_9ce4_8422_2325u64;
        for c in lv.routed.written().flat_map(|first| first..first + PAGE).filter(|&c| lv.routed.done(c)) {
            for v in [c, nn + c, 2 * nn + c] {
                for word in [v as u64, lv.rec.get(v) as u64, lv.trough.get(v) as u64, lv.acc.get(v).to_bits(), lv.flow.get(v).to_bits(), lv.graded.get(v).to_bits(), lv.floor.get(v).to_bits(), lv.born.get(v) as u64] {
                    digest = (digest ^ word).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        out += &format!(
            "\n{:>6}{}{}  {digest:016x}",
            lv.w,
            t.iter().map(|s| format!("{s:>9.1}")).collect::<String>(),
            cells.iter().map(|n| format!("{n:>10}")).collect::<String>()
        );
    }
    out
}

/// The level's lines within the view, each a segment from a vertex to its
/// receiver at the line's floor, in the view's frame.
fn lines(worlds: &Worlds, centre: (i32, i32), radius: i32, w: i64) -> Vec<[Vec3; 2]> {
    let ladders = worlds.world.ladders().expect("the standard stack's ladders");
    let Some(site) = ladders.site_of(centre.0, centre.1) else { return Vec::new() };
    ladders.with(site, |l| {
        let ladder = l.terrain().ladder();
        let Some(lv) = ladder.levels.iter().find(|lv| lv.w == w) else { return Vec::new() };
        let (ci, cj) = cube_round(centre.0 as f64 / w as f64, centre.1 as f64 / w as f64);
        let h = radius as i64 / w + 1;
        let place = |v: usize, fl: f64| -> Vec3 {
            let (t, i, j) = lv.vertex(v);
            let (q3, r3) = pos3(t, i, j, w);
            let (q, r) = (q3 as f32 / 3.0 - centre.0 as f32, r3 as f32 / 3.0 - centre.1 as f32);
            Vec3::new(1.5 * q, height_y(fl as f32) + 0.3, 3f32.sqrt() * (r + q / 2.0))
        };
        let nn = lv.cap();
        let mut out = Vec::new();
        for a in -h..=h {
            for b in -h..=h {
                let Some(c) = lv.idx(ci + a, cj + b) else { continue };
                for t in 0..3 {
                    let v = t * nn + c;
                    if lv.trough.get(v) && lv.rec.get(v) >= 0 {
                        let u = lv.rec.get(v) as usize;
                        out.push([place(v, lv.floor.get(v)), place(u, lv.floor.get(u))]);
                    }
                }
            }
        }
        out
    })
}

/// A left click reads the tile under the cursor: its cell at every level,
/// the height, dry height and cut each made, the governing line and its
/// floor, and the water's surface, into the panel and the log.
fn inspect(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut view: ResMut<View>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (Ok(window), Ok((camera, at))) = (windows.single(), camera.single()) else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(ray) = camera.viewport_to_world(at, cursor) else { return };
    let Some(b) = view.built.get(&(view.level, view.surface)) else { return };
    let mut hit = None;
    let mut t = 0.0;
    while t < 30_000.0 {
        let p = ray.origin + *ray.direction * t;
        if let Some(z) = height_under(b, view.centre, p.xz()) {
            if p.y <= height_y(z as f32) {
                hit = Some(p);
                break;
            }
        }
        t += 0.25;
    }
    let Some(p) = hit else { return };
    let tile = summary_lattice(0).cell_from(view.centre, p.xz());
    view.inspect = read_tile(&view.worlds, tile);
    info!("{}", view.inspect);
}

fn read_tile(worlds: &Worlds, (q, r): (i32, i32)) -> String {
    let stack = worlds.stack.tile_at(q, r).elevation;
    let mut out = format!("tile ({q}, {r}); the stack alone {stack:.0}\n  w      cell        z    dry    cut  water  governing line: read at, vertex, floor, catchment, born at\n");
    let Some(ladders) = worlds.world.ladders() else { return out };
    let Some(site) = ladders.site_of(q, r) else { return out + "  open sea: no ladder holds it\n" };
    ladders.with(site, |l| {
        let ladder = l.terrain().ladder();
        for lv in &ladder.levels {
            let w = lv.w;
            let (i, j) = cube_round(q as f64 / w as f64, r as f64 / w as f64);
            let Some(c) = lv.idx(i, j).filter(|&c| lv.built.done(c)) else {
                out.push_str(&format!("{w:>4} ({i}, {j}) not built\n"));
                continue;
            };
            let water = if lv.water.get(c).is_nan() { "-".to_string() } else { format!("{:.0}", lv.water.get(c)) };
            // The governing river is a vertex of the level it was read from.
            let source = ladder.levels.iter().find(|s| s.w == lv.wsrc.get(c));
            let line = match (lv.wgov.get(c), source) {
                (g, Some(s)) if g >= 0 => {
                    let v = g as usize;
                    let (acc, born) = (s.acc.get(v), s.born.get(v));
                    let (t, vi, vj) = s.vertex(v);
                    format!("{}: {t}:({vi}, {vj}) {:.1}, {acc:.0}, {born}", s.w, s.floor.get(v))
                }
                _ => "-".to_string(),
            };
            out.push_str(&format!(
                "{w:>4} {:>11} {:>6.0} {:>6.0} {:>6.1} {:>6}  {line}\n",
                format!("({i}, {j})"),
                lv.z.get(c),
                lv.zd.get(c),
                lv.cut.get(c),
                water
            ));
        }
    });
    out
}

fn text(view: Res<View>, mut status: Query<&mut Text, (With<StatusText>, Without<InspectText>)>, mut panel: Query<&mut Text, (With<InspectText>, Without<StatusText>)>) {
    let on = |b: bool| if b { "on" } else { "off" };
    let building = match view.pending {
        Some((w, s)) => format!("  building level {w} {} ...", s.name()),
        None => String::new(),
    };
    let p = view.worlds.params;
    let line = format!(
        "level {} (1-5)  surface {} (Z)  water {} (X)  faces {} (C)  lines {} (L)  reload R  shot P\nparams: cut {}, wall {}, ring {:.2}, water {}  [{}]\n{}{}",
        view.level,
        view.surface.name(),
        on(view.water),
        on(view.faces),
        on(view.lines),
        p.cut_from,
        p.wall,
        p.ring,
        p.water,
        view.params_path.display(),
        view.status,
        building
    );
    if let Ok(mut t) = status.single_mut() {
        **t = line;
    }
    if let Ok(mut t) = panel.single_mut() {
        **t = view.inspect.clone();
    }
}

/// With `--shot`, the first view taken once it has drawn a few frames, and
/// the viewer closed.
fn shoot(mut view: ResMut<View>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    if view.shown.is_some() {
        if let Some(tile) = view.read.take() {
            view.inspect = read_tile(&view.worlds, tile);
            info!("{}", view.inspect);
        }
    }
    let Some(path) = view.shot.clone() else { return };
    if view.shown.is_none() {
        return;
    }
    if let Some(next) = view.then_reload.take() {
        reload(&mut view, &next);
        return;
    }
    view.shot_wait += 1;
    if view.shot_wait == 30 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if view.shot_wait == 60 {
        exit.write(AppExit::Success);
    }
}
