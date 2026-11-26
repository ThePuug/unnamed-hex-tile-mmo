//! Reading a map: M opens the map in the player's main hand, north up,
//! and M again puts it away. What a map shows comes with the map: the
//! server sends each chunk of every map the player carries drawn down to
//! a sketch (`common_bevy::picture`), and each chunk the map charts
//! after, so a map opens with nothing asked.
//!
//! The map is read at one scale whatever its reach, through a window on
//! it. The numpad is a compass laid as its keys lie: 8 moves the window
//! north, 6 east, 9 between them, and so round; 5, at its middle, brings
//! the window back to the player, which it follows until it is moved.
//! + and - bring the ground nearer and further, and 0 puts the map away.
//! Each map keeps the window it was last read through while the client
//! runs.
//!
//! A wagon is sent on the map too: its settler, asked to send it, has
//! the map opened to choose where (`Reading::choose`). The hubs it may go
//! to stand out, the one nearest the window's middle is picked, * moves
//! the window to the next of them, and Enter sends the wagon there.
//!
//! The picture is coloured here: the ground by its height, lit from the
//! north-west, darker the more of it is full, the sea apart. The way
//! between every two hubs the map joins is found here too, from how the
//! map's chunks join (`picture::Atlas`), and inked chunk to chunk: the
//! chunks a wagon sent by the map would cross, though not its steps
//! inside each. A mark stands at each hub, and one where the player is,
//! held at the window's edge while the player is off it. That last is
//! the one thing shown of the world as it is now.

use std::{collections::HashMap, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task},
};
use common_bevy::{
    chunk::ChunkId,
    components::{
        equipment::{Equipment, Item, Slot},
        position::Position,
        Actor,
    },
    message::{Do, Event, Try},
    picture::{chunk_of, Atlas, Frame, Sketch},
};

use crate::systems::{
    focus::{NumpadFocus, Panel},
    gathering::KEYCODE_CLOSE,
};

/// The key that opens the map in hand, and puts it away.
pub const KEYCODE_MAP: KeyCode = KeyCode::KeyM;

/// The keys that move the window on the map, in rows as they lie on the
/// numpad, each with the way it moves it: across and down the picture.
/// The one at the middle brings the window back to the player.
const COMPASS: [[(crate::systems::keycap::Cap, IVec2, &str); 3]; 3] = [
    [(cap(KeyCode::Numpad7), IVec2::new(-1, -1), "north-west"), (cap(KeyCode::Numpad8), IVec2::new(0, -1), "north"), (cap(KeyCode::Numpad9), IVec2::new(1, -1), "north-east")],
    [(cap(KeyCode::Numpad4), IVec2::new(-1, 0), "west"), (cap(KEYCODE_HERE), IVec2::ZERO, "where you stand"), (cap(KeyCode::Numpad6), IVec2::new(1, 0), "east")],
    [(cap(KeyCode::Numpad1), IVec2::new(-1, 1), "south-west"), (cap(KeyCode::Numpad2), IVec2::new(0, 1), "south"), (cap(KeyCode::Numpad3), IVec2::new(1, 1), "south-east")],
];
const KEYCODE_HERE: KeyCode = KeyCode::Numpad5;
use crate::systems::keycap::Cap;
/// A cap in the compass table, which the compiler evaluates.
const fn cap(key: KeyCode) -> Cap {
    Cap::of(key)
}
/// The keys that move the window to the next hub a wagon may be sent to,
/// and send it to the one picked.
const KEYCODE_NEXT: KeyCode = KeyCode::NumpadMultiply;
const KEYCODE_SEND: KeyCode = KeyCode::NumpadEnter;
/// The keys that bring the ground nearer and further.
const KEYCODE_NEARER: KeyCode = KeyCode::NumpadAdd;
const KEYCODE_FURTHER: KeyCode = KeyCode::NumpadSubtract;

/// How many tiles the window is across at each scale, and the scale a
/// map opens at.
const SCALES: [f32; 6] = [128., 256., 512., 1024., 2048., 4096.];
const OPENS_AT: usize = 2;
/// How much of the window a press moves it by, and how far from its
/// middle the player may walk before a window that follows is moved.
const MOVED: f32 = 0.25;
const FOLLOWED: f32 = 0.25;

/// The picture's side, in texels and in hundredths of the screen's height.
const SIDE: u32 = 512;
const SIDE_VH: f32 = 70.;

const PAPER: Color = Color::srgb(0.50, 0.44, 0.33);
const HEARTH: Color = Color::srgb(1.0, 0.5, 0.12);
const HERE: Color = Color::srgb(0.85, 0.1, 0.08);
/// A hub a wagon may be sent to, the one picked, and the rim of both.
const OFFERED: Color = Color::srgb(1.0, 0.72, 0.3);
const PICKED: Color = Color::srgb(1.0, 0.95, 0.55);
const RIM: Color = Color::srgb(1.0, 1.0, 1.0);
const EDGE: Color = Color::srgb(0.08, 0.06, 0.04);
const TEXT: Color = Color::srgb(0.85, 0.85, 0.85);

/// The ground's colours, as sRGB: the sea, the lowest land and the
/// land `HIGHEST` levels up or more, and what full ground is drawn
/// toward.
const SEA: [f32; 3] = [0.52, 0.62, 0.66];
const LOW: [f32; 3] = [0.76, 0.78, 0.58];
const HIGH: [f32; 3] = [0.90, 0.80, 0.62];
const FULL: [f32; 3] = [0.30, 0.27, 0.18];
const HIGHEST: f32 = 600.;
/// The ink a way is drawn in, and how many texels wide.
const WAY: [u8; 4] = [122, 28, 20, 255];
const WAY_TEXELS: i32 = 2;
/// How much of wholly full ground's colour is `FULL`, and how much
/// lighter or darker a level's rise or fall toward the light draws it.
const FULLNESS: f32 = 0.55;
const RELIEF: f32 = 0.05;
/// The most levels of rise or fall the light tells apart.
const STEEPEST: i32 = 3;

type Tile = (i32, i32);

/// The maps the player carries, by number, each as the server has sent
/// it.
#[derive(Resource, Default)]
pub struct Maps(HashMap<u32, Arc<Atlas>>);

/// A window on a map: the tile at its middle, its scale, and whether it
/// follows the player. A map not yet read is opened on the player.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Window {
    at: Tile,
    scale: usize,
    follows: bool,
}

impl Default for Window {
    fn default() -> Self {
        Window { at: (0, 0), scale: OPENS_AT, follows: true }
    }
}

/// The map the player reads: whether it is open, the number of the one
/// in hand and the window on it, the window each other map was last read
/// through, and while a wagon's way is chosen on it, the hubs it may go
/// to and the window to come back to.
#[derive(Resource, Default)]
pub struct Reading {
    open: bool,
    map: Option<u32>,
    window: Window,
    left: HashMap<u32, Window>,
    sending: Option<(Vec<Tile>, Window)>,
}

impl Reading {
    pub fn open(&self) -> bool {
        self.open
    }

    fn frame(&self) -> Frame {
        Frame { centre: self.window.at, across: SCALES[self.window.scale] }
    }

    /// Opens the map to choose which of `offered`, hubs it holds, a wagon
    /// is sent to, the window on the first of them. Nothing where none
    /// is offered.
    pub fn choose(&mut self, offered: Vec<Tile>) {
        let Some(&first) = offered.first() else { return };
        self.sending = Some((offered, self.window));
        self.window = Window { at: first, follows: false, ..self.window };
        self.open = true;
    }

    /// The hub a wagon would be sent to: of those offered, the one
    /// nearest the middle of `frame`.
    fn picked(&self, frame: Frame) -> Option<Tile> {
        let off = |hub: &Tile| (frame.spot(*hub, Vec3::ZERO) - 0.5).length_squared();
        self.sending.as_ref()?.0.iter().copied().min_by(|a, b| off(a).total_cmp(&off(b)))
    }

    /// Puts the map away: a window moved to choose a wagon's way goes
    /// back where it was.
    fn close(&mut self) {
        self.open = false;
        if let Some((_, before)) = self.sending.take() {
            self.window = before;
        }
    }

    /// Takes map `held` in hand: the window on the one it had is kept,
    /// and the one `held` was last read through is before it again.
    fn take(&mut self, held: Option<u32>) {
        if self.sending.is_some() {
            self.close();
        }
        if let Some(was) = self.map {
            self.left.insert(was, self.window);
        }
        self.window = held.and_then(|number| self.left.get(&number).copied()).unwrap_or_default();
        self.map = held;
    }
}

/// Where the window each map was last read through is kept between runs.
fn windows_path() -> Option<std::path::PathBuf> {
    user_data::folder("client").map(|folder| folder.join("maps.json"))
}

/// Has each map read in an earlier run opened as it was left then.
pub fn recall(mut reading: ResMut<Reading>) {
    let Some(text) = windows_path().and_then(|path| std::fs::read_to_string(path).ok()) else { return };
    match serde_json::from_str::<HashMap<u32, Window>>(&text) {
        // A scale this build has no more reads as the widest it has
        Ok(left) => reading.left = left.into_iter().map(|(map, window)| (map, Window { scale: window.scale.min(SCALES.len() - 1), ..window })).collect(),
        Err(e) => warn!("map: the windows kept cannot be read: {e}"),
    }
}

/// Keeps the window every map was last read through as the map is put
/// away, for the next run. Written beside the file and renamed over it,
/// so a run that stops mid-write leaves the last whole one.
pub fn keep(reading: Res<Reading>, mut was_open: Local<bool>) {
    let closed = *was_open && !reading.open;
    *was_open = reading.open;
    if !closed {
        return;
    }
    let Some(path) = windows_path() else { return };
    let mut windows = reading.left.clone();
    if let Some(map) = reading.map {
        windows.insert(map, reading.window);
    }
    let written = (|| {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, serde_json::to_string_pretty(&windows).map_err(std::io::Error::other)?)?;
        std::fs::rename(&partial, &path)
    })();
    if let Err(e) = written {
        warn!("map: {}: {e}", path.display());
    }
}

/// The ground of `atlas` under `frame` coloured, `side` texels a side,
/// as RGBA rows from the north: clear where no sketch lies, each of
/// `ways` inked over it.
pub fn texels(atlas: &Atlas, ways: &[Vec<Tile>], frame: Frame, side: u32) -> Vec<u8> {
    let mut texels = Vec::with_capacity((side * side * 4) as usize);
    // The level of each texel of the row above, where it has ground
    let mut above: Vec<Option<i32>> = vec![None; side as usize];
    let mut last: Option<(ChunkId, Option<&Sketch>)> = None;
    for down in 0..side {
        let mut lit = None;
        for across in 0..side {
            let tile = frame.tile((UVec2::new(across, down).as_vec2() + 0.5) / side as f32);
            let chunk = chunk_of(tile);
            let sketch = match last {
                Some((held, sketch)) if held == chunk => sketch,
                _ => {
                    let sketch = atlas.sketch(chunk);
                    last = Some((chunk, sketch));
                    sketch
                }
            };
            let ground = sketch.and_then(|sketch| sketch.at(chunk, tile));
            // The texel toward the light is the one up and to the left
            let toward = std::mem::replace(&mut lit, std::mem::replace(&mut above[across as usize], ground.map(|(level, _)| level)));
            let Some((level, full)) = ground else {
                texels.extend([0; 4]);
                continue;
            };
            let mut colour = if level < 0 {
                SEA
            } else {
                let height = (level as f32 / HIGHEST).min(1.);
                std::array::from_fn(|k| LOW[k] + (HIGH[k] - LOW[k]) * height)
            };
            let rise = toward.map_or(0, |lit| (level.max(0) - lit.max(0)).clamp(-STEEPEST, STEEPEST));
            for (k, part) in colour.iter_mut().enumerate() {
                *part += rise as f32 * RELIEF;
                *part += (FULL[k] - *part) * FULLNESS * full;
            }
            texels.extend(colour.map(|part| (part.clamp(0., 1.) * 255.) as u8));
            texels.push(255);
        }
    }
    let within = -(side as f32)..2. * side as f32;
    for step in ways.iter().flat_map(|way| way.windows(2)) {
        let (from, to) = (frame.spot(step[0], Vec3::ZERO) * side as f32, frame.spot(step[1], Vec3::ZERO) * side as f32);
        if ![from.x, from.y, to.x, to.y].iter().all(|part| within.contains(part)) {
            continue;
        }
        let steps = (to - from).abs().max_element().ceil().max(1.) as u32;
        for step in 0..=steps {
            let middle = from.lerp(to, step as f32 / steps as f32).floor().as_ivec2();
            for (across, down) in (0..WAY_TEXELS).flat_map(|across| (0..WAY_TEXELS).map(move |down| (middle.x + across, middle.y + down))) {
                if (0..side as i32).contains(&across) && (0..side as i32).contains(&down) {
                    let k = ((down as u32 * side + across as u32) * 4) as usize;
                    texels[k..k + 4].copy_from_slice(&WAY);
                }
            }
        }
    }
    texels
}

/// Opens the map in hand on M and puts it away on M again, on the close
/// key, or when the hand holds none; and moves the window on it.
pub fn request(
    mut keys: crate::systems::help::Keys,
    menu: Res<crate::plugins::shell::menu::GameMenu>,
    focus: Res<NumpadFocus>,
    mut reading: ResMut<Reading>,
    player: Query<(Entity, &Equipment, &Position), With<Actor>>,
    mut writer: MessageWriter<Try>,
) {
    let player = player.single().ok();
    let held = player.and_then(|(_, worn, _)| worn.worn(Slot::MainHand)).and_then(Item::map);
    let stood = player.map(|(.., position)| (position.tile.q, position.tile.r));
    if reading.map != held {
        reading.take(held);
    }
    if reading.open && held.is_none() {
        reading.close();
    }
    // A window that follows the player moves once it has walked well off its middle
    if let Some(stood) = stood.filter(|_| reading.open && reading.window.follows) {
        if (reading.frame().spot(stood, Vec3::ZERO) - 0.5).abs().max_element() > FOLLOWED {
            reading.window.at = stood;
        }
    }
    if menu.open {
        return;
    }
    if keys.pressed(KEYCODE_MAP, "Read the map in your hand, or put it away") {
        match (reading.open, held) {
            (true, _) => reading.close(),
            (false, Some(_)) => reading.open = true,
            (false, None) => info!("chart: no map is in hand to read"),
        }
    }
    if !reading.open || !focus.has(Panel::Chart) {
        return;
    }
    if keys.take(KEYCODE_CLOSE, "Put the map away") {
        reading.close();
        return;
    }
    if let Some((offered, _)) = reading.sending.clone() {
        let picked = reading.picked(reading.frame());
        if keys.take(KEYCODE_SEND, "Send the wagon to the hub picked, by this map") {
            if let (Some((ent, ..)), Some(to)) = (player, picked) {
                writer.write(Try { event: Event::Send { ent, to } });
            }
            reading.close();
            return;
        }
        if keys.take(KEYCODE_NEXT, "Look at the next hub the wagon may be sent to") {
            let at = picked.and_then(|picked| offered.iter().position(|&hub| hub == picked)).map_or(0, |at| (at + 1) % offered.len());
            (reading.window.at, reading.window.follows) = (offered[at], false);
        }
    }
    for (key, by, way) in COMPASS.into_iter().flatten().map(|(cap, by, way)| (cap.key(), by, way)) {
        if !keys.take(key, if key == KEYCODE_HERE { "Look where you stand on the map".to_owned() } else { format!("Look {way} on the map") }) {
            continue;
        }
        if key == KEYCODE_HERE {
            if let Some(stood) = stood {
                (reading.window.at, reading.window.follows) = (stood, true);
            }
        } else {
            reading.window.at = reading.frame().tile(Vec2::splat(0.5) + by.as_vec2() * MOVED);
            reading.window.follows = false;
        }
    }
    if keys.take(KEYCODE_NEARER, "Look closer at the map") && reading.window.scale > 0 {
        reading.window.scale -= 1;
    }
    if keys.take(KEYCODE_FURTHER, "Look at more of the map") && reading.window.scale + 1 < SCALES.len() {
        reading.window.scale += 1;
    }
}

/// Keeps each map carried as the server says it. A client entering the
/// world holds none: the server sends its maps anew.
pub fn do_charted(mut reader: MessageReader<Do>, mut maps: ResMut<Maps>) {
    for message in reader.read() {
        match &message.event {
            Event::Init { .. } if !maps.0.is_empty() => maps.0.clear(),
            Event::Charted { map, sketches, links, hubs, .. } => {
                let atlas = Arc::make_mut(maps.0.entry(*map).or_default());
                for (at, (chunk, sketch)) in sketches.iter().enumerate() {
                    let links = links.iter().filter(|link| link.0 as usize == at).map(|link| link.1).collect();
                    let hubs = hubs.iter().filter(|hub| chunk_of(hub.0) == *chunk).copied().collect();
                    atlas.lay(*chunk, *sketch, links, hubs);
                }
            }
            _ => {}
        }
    }
}

/// The picture as it is drawn: the one being made off the main thread,
/// the window the one shown is of, and whether the map or the window has
/// changed since the last was begun.
#[derive(Resource, Default)]
pub struct Drawing {
    task: Option<Task<(Frame, Vec<u8>)>>,
    shown: Option<Frame>,
    stale: bool,
}

/// The panel's root, its heading, the node its picture is drawn on, and
/// the mark that stands where the player is.
#[derive(Component)]
pub struct ChartPanel;
#[derive(Component)]
pub struct Heading;
#[derive(Component)]
pub struct Drawn;
#[derive(Component)]
pub struct HereMark;

/// A mark `size` px across, centred on `spot` of the picture, rimmed in
/// `rim`.
fn mark(spot: Vec2, size: f32, colour: Color, rim: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(spot.x * 100.),
            top: Val::Percent(spot.y * 100.),
            width: Val::Px(size),
            height: Val::Px(size),
            margin: UiRect { left: Val::Px(-size / 2.), top: Val::Px(-size / 2.), ..default() },
            border: UiRect::all(Val::Px(2.)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(colour),
        BorderColor::all(rim),
    )
}

/// Shows the panel while a map is open, draws the window on the map in
/// hand again as the map or the window changes, and keeps the marks
/// where the hubs and the player are.
pub fn update(
    mut commands: Commands,
    (reading, maps): (Res<Reading>, Res<Maps>),
    mut drawing: ResMut<Drawing>,
    mut images: ResMut<Assets<Image>>,
    mut roots: Query<(Entity, &mut Node), (With<ChartPanel>, Without<HereMark>)>,
    drawn: Query<Entity, With<Drawn>>,
    player: Query<&Position, With<Actor>>,
    mut here: Query<&mut Node, With<HereMark>>,
    mut heading: Query<&mut Text, With<Heading>>,
) {
    if reading.is_changed() {
        let says = if reading.sending.is_some() { "Map: where does the wagon go?" } else { "Map" };
        if let Ok(mut heading) = heading.single_mut() {
            if heading.0 != says {
                heading.0 = says.to_owned();
            }
        }
        match roots.single_mut() {
            Ok((_, mut node)) => node.display = if reading.open { Display::Flex } else { Display::None },
            Err(_) if reading.open => spawn(&mut commands),
            Err(_) => {}
        }
    }
    if !reading.open {
        return;
    }
    if reading.is_changed() || maps.is_changed() {
        drawing.stale = true;
    }
    let Ok(node) = drawn.single() else { return };
    let none = Arc::new(Atlas::default());
    let atlas = reading.map.and_then(|number| maps.0.get(&number)).unwrap_or(&none);

    if drawing.stale && drawing.task.is_none() {
        drawing.stale = false;
        let (atlas, frame) = (atlas.clone(), reading.frame());
        drawing.task = Some(AsyncComputeTaskPool::get().spawn(async move { (frame, texels(&atlas, &atlas.ways(), frame, SIDE)) }));
    }
    let mut marks = false;
    if let Some((frame, texels)) = drawing.task.as_mut().and_then(|task| block_on(future::poll_once(task))) {
        drawing.task = None;
        drawing.shown = Some(frame);
        let size = Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 };
        let image = Image::new(size, TextureDimension::D2, texels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        commands.entity(node).insert(ImageNode::new(images.add(image)));
        marks = true;
    }
    let Some(frame) = drawing.shown else { return };
    let stood = player.single().ok().map(|position| frame.spot((position.tile.q, position.tile.r), position.offset).clamp(Vec2::ZERO, Vec2::ONE));
    if marks {
        commands.entity(node).despawn_related::<Children>().with_children(|marks| {
            let (offered, picked) = (reading.sending.as_ref().map_or(&[][..], |sending| sending.0.as_slice()), reading.picked(frame));
            for hub in atlas.hubs() {
                let spot = frame.spot(hub, Vec3::ZERO);
                if spot.min_element() < 0. || spot.max_element() > 1. {
                    continue;
                }
                let (size, colour, rim) = match (Some(hub) == picked, offered.contains(&hub)) {
                    (true, _) => (22., PICKED, RIM),
                    (false, true) => (16., OFFERED, RIM),
                    (false, false) => (12., HEARTH, EDGE),
                };
                marks.spawn(mark(spot, size, colour, rim));
            }
            marks.spawn((mark(stood.unwrap_or(Vec2::splat(0.5)), 12., HERE, EDGE), HereMark));
        });
    } else if let (Some(stood), Ok(mut node)) = (stood, here.single_mut()) {
        let (left, top) = (Val::Percent(stood.x * 100.), Val::Percent(stood.y * 100.));
        if node.left != left || node.top != top {
            (node.left, node.top) = (left, top);
        }
    }
}

/// Spawns the panel: the picture in the middle of the screen, and under
/// it what its marks stand for and the keys that work it.
fn spawn(commands: &mut Commands) {
    use crate::systems::keycap::{hint_row, Hint};
    let line = |text: &str| (Text::new(text), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(TEXT));
    commands
        .spawn((
            ChartPanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.), padding: UiRect::all(Val::Px(10.)), border_radius: BorderRadius::all(Val::Px(6.)), ..default() },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.95)),
            ))
            .with_children(|panel| {
                panel.spawn((line("Map"), Heading));
                panel.spawn((Drawn, Node { width: Val::Vh(SIDE_VH), height: Val::Vh(SIDE_VH), overflow: Overflow::clip(), ..default() }, BackgroundColor(PAPER)));
                panel.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.), ..default() }).with_children(|legend| {
                    for (colour, round, stands) in [(HERE, true, "you"), (HEARTH, true, "a hub"), (Color::srgb_u8(WAY[0], WAY[1], WAY[2]), false, "a way between hubs")] {
                        legend.spawn((
                            Node { width: Val::Px(12.), height: Val::Px(12.), border: UiRect::all(Val::Px(2.)), border_radius: if round { BorderRadius::MAX } else { BorderRadius::ZERO }, ..default() },
                            BackgroundColor(colour),
                            BorderColor::all(EDGE),
                        ));
                        legend.spawn((line(stands), Node { margin: UiRect::right(Val::Px(10.)), ..default() }));
                    }
                });
                // The compass keys as they lie on the numpad, beside what the rest do
                panel.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(16.), ..default() }).with_children(|keys| {
                    keys.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(2.), ..default() }).with_children(|compass| {
                        for row in COMPASS {
                            compass.spawn(Node { column_gap: Val::Px(2.), ..default() }).with_children(|caps| {
                                for (key, ..) in row {
                                    crate::systems::keycap::keycap(caps, key);
                                }
                            });
                        }
                    });
                    keys.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.), ..default() }).with_children(|rest| {
                        rest.spawn(line("move the map, north up; the middle key finds you"));
                        hint_row(rest, None, &[Hint::key(crate::cap!(KEYCODE_NEARER), "nearer"), Hint::key(crate::cap!(KEYCODE_FURTHER), "further"), Hint::key(crate::cap!(KEYCODE_CLOSE), "close")]);
                        hint_row(rest, Some("sending a wagon"), &[Hint::key(crate::cap!(KEYCODE_NEXT), "next hub"), Hint::key(crate::cap!(KEYCODE_SEND), "send it to the hub picked")]);
                    });
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_bevy::{
        chart::{Chart, Leaf},
        chunk::{chunk_tiles, CHUNK_TILES},
    };

    /// What a reader holds of `chunks`, every tile of each at `level`
    /// and full or not.
    fn sketched(chunks: &[ChunkId], level: i32, full: bool) -> Atlas {
        let mut chart = Chart::default();
        for &chunk in chunks {
            chart.lay(Arc::new(Leaf::of(chunk, vec![(level, full); CHUNK_TILES], Vec::new()).unwrap()));
        }
        let mut atlas = Atlas::default();
        for leaf in chart.leaves() {
            let (sketch, links): (Sketch, _) = Sketch::of(&chart, leaf);
            atlas.lay(leaf.chunk(), sketch, links, Vec::new());
        }
        atlas
    }

    fn texel(texels: &[u8], frame: Frame, tile: Tile, side: u32) -> [u8; 4] {
        let at = (frame.spot(tile, Vec3::ZERO) * side as f32).as_uvec2();
        let k = ((at.y * side + at.x) * 4) as usize;
        texels[k..k + 4].try_into().unwrap()
    }

    /// The window is coloured where a sketch lies under it and clear
    /// where none does; full ground is darker than open, and the sea is
    /// not the land.
    #[test]
    fn a_window_is_coloured_where_the_map_is_charted() {
        let (side, frame) = (64, Frame { centre: (0, 0), across: 64. });
        let chunks = [ChunkId(0, 0)];
        let off = chunk_tiles(ChunkId(1, 1)).next().unwrap();
        let open = texels(&sketched(&chunks, 4, false), &[], frame, side);
        let land = texel(&open, frame, (0, 0), side);
        assert_eq!((open.len(), land[3]), ((side * side * 4) as usize, 255));
        assert_eq!(texel(&open, frame, off, side), [0; 4]);
        let full = texel(&texels(&sketched(&chunks, 4, true), &[], frame, side), frame, (0, 0), side);
        assert!(full[0] < land[0] && full[1] < land[1]);
        assert_ne!(texel(&texels(&sketched(&chunks, -2, false), &[], frame, side), frame, (0, 0), side), land);
    }

    /// A way is inked from end to end where it lies in the window, a way
    /// far off it leaves no ink, and the same ground far out in the world
    /// is drawn as it is at the origin.
    #[test]
    fn a_way_is_inked_in_the_window() {
        let (side, frame) = (64, Frame { centre: (0, 0), across: 64. });
        let ground = sketched(&[ChunkId(0, 0)], 0, false);
        let inked = |ways: &[Vec<Tile>]| texels(&ground, ways, frame, side).chunks(4).filter(|texel| *texel == WAY).count();
        assert!(inked(&[vec![(-6, 3), (6, -3)]]) >= 12);
        assert_eq!(inked(&[]), 0);
        assert_eq!(inked(&[vec![(5_000, 0), (5_010, 0)]]), 0);

        let far = ChunkId(90_000, -40_000);
        let there = Frame { centre: (far.center().q, far.center().r), across: 64. };
        assert_eq!(texels(&sketched(&[far], 0, false), &[], there, side), texels(&ground, &[], frame, side));
    }
}
