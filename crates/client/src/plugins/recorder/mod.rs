//! Records scripted shots to video: `cargo run --bin client -- --record
//! <script.json>` (admin builds). The client enters the world on its own,
//! then for each shot puts the player on its tile facing its bearing,
//! waits until the world there has arrived and been built, and records.
//!
//! While a shot rolls the game is stepped exactly one frame's time per
//! frame and never faster than the wall clock, so the video runs at game
//! speed and what the server times — a felling's work — keeps its length
//! on screen. Frames are read back from the window and piped to ffmpeg,
//! which must be on the PATH. No HUD, console or gizmo is drawn from the
//! moment the recorder starts until the client quits.

mod script;
mod writer;

use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    app::AppExit,
    gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigStore},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    time::TimeUpdateStrategy,
    window::PrimaryWindow,
};
use common_bevy::{
    components::{
        heading::{Heading, HEADING_SLOTS},
        position::{Position, VisualPosition},
        Loc,
    },
    message::{Event, Try},
    resources::{map::Map, InputQueues},
    systems::HOUR_MS,
};

use crate::{
    plugins::{
        diagnostics::{metrics_overlay::OverlayCameraEntity, DiagnosticsState},
        shell::{self, Entered, Stage},
    },
    resources::{LoadedChunks, RenderOrigin, SummaryMeshes},
    systems::{closeup::CloseupCamera, input},
};
use script::{camera_at, clock_at, handover, key_code, Anchor, CameraPath, Press, Script, Shot};
use writer::{Frame, Writer};

pub struct RecorderPlugin;

impl Plugin for RecorderPlugin {
    fn build(&self, app: &mut App) {
        let mut args = std::env::args().skip_while(|a| a != "--record").skip(1);
        let Some(path) = args.next() else { return };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("recorder: {path}: {e}"));
        let script: Script = serde_json::from_str(&text).unwrap_or_else(|e| panic!("recorder: {path}: {e}"));
        std::fs::create_dir_all(&script.out).unwrap_or_else(|e| panic!("recorder: {}: {e}", script.out.display()));
        let shots: Vec<Shot> = script.shots.iter()
            .filter(|s| script.only.as_ref().is_none_or(|only| only.contains(&s.name)))
            .cloned()
            .collect();
        info!("recorder: {} of {} shots from {path}", shots.len(), script.shots.len());

        app.insert_resource(Recorder { script, shots, index: 0, phase: Phase::Enter, keys: Keys::default(), sink: None });
        app.init_resource::<Captured>();
        app.add_systems(PreUpdate, press_keys.after(bevy::input::InputSystems).before(input::update_keybits));
        // The path is laid over what the gameplay camera placed, so a shot
        // that hands over to it blends from the path into this frame's pose.
        app.add_systems(Update, (
            enter,
            hide_hud,
            advance,
            drive_camera.after(advance).after(crate::systems::camera::update),
            capture.after(drive_camera),
        ));
        app.add_systems(Last, end_frame);
    }
}

/// Whether the recorder holds the camera alone: through every phase of a
/// shot whose camera runs a path and never hands over. The gameplay camera
/// stands down meanwhile; for a path that hands over it keeps following the
/// player, so it is where play would have it when it takes over.
pub fn camera_free(recorder: Option<Res<Recorder>>) -> bool {
    recorder.is_some_and(|r| matches!(r.shot().map(|s| &s.camera), Some(CameraPath::Path { follow_at: None, .. })))
}

#[derive(Resource)]
pub struct Recorder {
    script: Script,
    shots: Vec<Shot>,
    index: usize,
    phase: Phase,
    keys: Keys,
    /// An inactive camera every UI root is pointed at.
    sink: Option<Entity>,
}

impl Recorder {
    fn shot(&self) -> Option<&Shot> {
        self.shots.get(self.index)
    }

    /// Seconds into the rolling shot, or zero before it rolls.
    fn t(&self) -> f32 {
        match &self.phase {
            Phase::Roll(roll) | Phase::Drain(roll) => roll.frame as f32 / self.script.fps as f32,
            _ => 0.0,
        }
    }

    /// Moves on to the next shot, letting go of every key.
    fn next_shot(&mut self) {
        let held: Vec<KeyCode> = self.keys.held.drain().collect();
        self.keys.released.extend(held);
        self.index += 1;
        self.phase = Phase::Arrive { since: Instant::now(), asked: None };
    }
}

enum Phase {
    /// Waiting to be in the world.
    Enter,
    /// Sending the player to the shot's tile, last asked at `asked`.
    Arrive { since: Instant, asked: Option<Instant> },
    /// Turning the player to the shot's bearing.
    Turn { since: Instant },
    /// Waiting for the world to settle, then for the shot's `settle` more.
    Settle { since: Instant, quiet: u32, epoch: u64, quiet_since: Option<Instant> },
    Roll(Roll),
    /// The last frame is taken; waiting for the readbacks still out.
    Drain(Roll),
    Done,
}

struct Roll {
    /// Frames taken so far: the number of the next.
    frame: u64,
    frames: u64,
    /// Whether the game's clock is stepped by frame yet: from the frame
    /// after the shot is armed, so every frame taken is a whole step.
    armed: bool,
    start: Instant,
    /// The player where the shot rolled, for a path anchored there.
    anchor: Position,
    writer: Option<Writer>,
    received: u64,
    fps: u32,
}

impl Roll {
    /// The shot's length on screen.
    fn length(&self) -> Duration {
        Duration::from_secs_f64(self.frames as f64 / self.fps as f64)
    }
}

/// The keys the script holds down, those tapped this frame, which come up
/// the next, and those to let go.
#[derive(Default)]
struct Keys {
    held: HashSet<KeyCode>,
    tapped: Vec<KeyCode>,
    released: Vec<KeyCode>,
}

/// Frames read back and not yet handed to the writer.
#[derive(Resource, Default)]
struct Captured(Vec<Frame>);

/// How long the player may take to arrive or turn, and the world to
/// settle, before the recorder gives up on the shot or rolls anyway; and
/// how often a teleport not answered is asked for again.
const ARRIVE_LIMIT: Duration = Duration::from_secs(20);
const ASK_AGAIN: Duration = Duration::from_secs(5);
const TURN_LIMIT: Duration = Duration::from_secs(10);
const SETTLE_LIMIT: Duration = Duration::from_secs(120);
/// How long past a shot's length its last readbacks may take before the
/// file is closed without them.
const DRAIN_LIMIT: Duration = Duration::from_secs(10);
/// Frames in a row without a build in flight or new data before the world
/// counts as settled, as the loading screen counts them.
const QUIET_FRAMES: u32 = 10;

/// Enters the world from the character screen and sizes the window.
fn enter(
    mut recorder: ResMut<Recorder>,
    stage: Res<State<Stage>>,
    mut entered: ResMut<Entered>,
    mut writer: MessageWriter<Try>,
    mut next: ResMut<NextState<Stage>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !matches!(recorder.phase, Phase::Enter) {
        return;
    }
    if let Ok(mut window) = window.single_mut() {
        let [w, h] = recorder.script.size;
        if window.resolution.physical_width() != w || window.resolution.physical_height() != h {
            window.resolution.set_physical_resolution(w, h);
        }
    }
    match stage.get() {
        Stage::CharacterSelect if !entered.0 => shell::play(&mut writer, &mut next, &mut entered),
        Stage::Playing => {
            info!("milestone: recorder in the world");
            recorder.phase = Phase::Arrive { since: Instant::now(), asked: None };
        }
        _ => {}
    }
}

/// Points every UI root at a camera that never draws, stops the overlay's
/// camera, and turns gizmos off.
fn hide_hud(
    mut recorder: ResMut<Recorder>,
    mut commands: Commands,
    roots: Query<(Entity, Option<&UiTargetCamera>), (With<Node>, Without<ChildOf>)>,
    overlay: Option<Res<OverlayCameraEntity>>,
    mut cameras: Query<&mut Camera>,
    mut gizmos: ResMut<GizmoConfigStore>,
) {
    let sink = *recorder.sink.get_or_insert_with(|| {
        commands.spawn((Camera2d, Camera { is_active: false, order: -100, ..default() })).id()
    });
    for (entity, target) in &roots {
        if target.is_none_or(|t| t.0 != sink) {
            commands.entity(entity).insert(UiTargetCamera(sink));
        }
    }
    if let Some(mut camera) = overlay.and_then(|o| cameras.get_mut(o.0).ok()) {
        camera.is_active = false;
    }
    gizmos.config_mut::<DefaultGizmoConfigGroup>().0.enabled = false;
}

/// Moves each shot through its phases: arrive, turn, settle, roll.
#[allow(clippy::too_many_arguments)]
fn advance(
    mut recorder: ResMut<Recorder>,
    mut writer: MessageWriter<Try>,
    buffers: Res<InputQueues>,
    mut player: Query<(&Loc, &Heading, &Position, &mut Visibility)>,
    loaded: Res<LoadedChunks>,
    meshes: Res<SummaryMeshes>,
    mut diagnostics: ResMut<DiagnosticsState>,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(recorder.phase, Phase::Enter | Phase::Done) {
        return;
    }
    // The local player is the one entity with an input queue.
    let Some(&ent) = buffers.entities().next() else { return };
    let Ok((loc, heading, position, mut visibility)) = player.get_mut(ent) else { return };
    let Some(shot) = recorder.shot().cloned() else {
        recorder.phase = Phase::Done;
        info!("milestone: recorder done");
        if recorder.script.exit {
            exit.write(AppExit::Success);
        }
        return;
    };

    // The light and the player's visibility are the shot's through all of
    // it, so the world settles under the light it is recorded in.
    let hours = clock_at(&shot.clock, recorder.t());
    diagnostics.lighting.hold_at((hours * HOUR_MS as f64) as u128);
    visibility.set_if_neq(if shot.hide_player { Visibility::Hidden } else { Visibility::Inherited });

    let now = Instant::now();
    let recorder = &mut *recorder;
    let mut skip = false;
    match &mut recorder.phase {
        Phase::Enter | Phase::Roll(_) | Phase::Drain(_) | Phase::Done => {}
        Phase::Arrive { since, asked } => {
            if (loc.q, loc.r) == (shot.at[0], shot.at[1]) {
                info!("recorder: shot {} at ({}, {})", shot.name, shot.at[0], shot.at[1]);
                recorder.phase = Phase::Turn { since: now };
            } else if now - *since > ARRIVE_LIMIT {
                error!("recorder: shot {} never arrived at ({}, {}); skipped", shot.name, shot.at[0], shot.at[1]);
                skip = true;
            } else if asked.is_none_or(|a| now - a > ASK_AGAIN) {
                *asked = Some(now);
                writer.write(Try { event: Event::Teleport { ent, q: shot.at[0], r: shot.at[1] } });
            }
        }
        Phase::Turn { since } => {
            let wanted = Heading::from_degrees(shot.face);
            let keys = &mut recorder.keys;
            keys.held.remove(&KeyCode::ArrowLeft);
            keys.held.remove(&KeyCode::ArrowRight);
            let late = now - *since > TURN_LIMIT;
            if *heading == wanted || late {
                if late {
                    warn!("recorder: shot {} could not turn to {}; rolling as it faces", shot.name, shot.face);
                }
                keys.released.extend([KeyCode::ArrowLeft, KeyCode::ArrowRight]);
                recorder.phase = Phase::Settle { since: now, quiet: 0, epoch: meshes.epoch, quiet_since: None };
            } else {
                // Right turns clockwise, the way bearings count.
                let ahead = (wanted.slot() as i32 - heading.slot() as i32).rem_euclid(HEADING_SLOTS as i32);
                let clockwise = ahead <= HEADING_SLOTS as i32 / 2;
                keys.held.insert(if clockwise { KeyCode::ArrowRight } else { KeyCode::ArrowLeft });
            }
        }
        Phase::Settle { since, quiet, epoch, quiet_since } => {
            let (have, want) = shell::chunks_streamed(loc, &loaded);
            let building = meshes.states.values().any(|s| s.task.is_some());
            let still = want > 0 && have == want && !building && meshes.epoch == *epoch;
            *epoch = meshes.epoch;
            *quiet = if still { *quiet + 1 } else { 0 };
            if *quiet < QUIET_FRAMES {
                *quiet_since = None;
            } else if quiet_since.is_none() {
                *quiet_since = Some(now);
            }
            let settled = quiet_since.is_some_and(|q| now - q >= Duration::from_secs_f32(shot.settle));
            let late = now - *since > SETTLE_LIMIT;
            if settled || late {
                if !settled {
                    warn!("recorder: shot {} not settled after {SETTLE_LIMIT:?} ({have}/{want} chunks, building {building}); rolling", shot.name);
                }
                let fps = recorder.script.fps;
                let frames = ((shot.seconds * fps as f32).ceil() as u64).max(1);
                let writer = (shot.seconds > 0.0).then(|| {
                    Writer::start(recorder.script.out.join(format!("{}.mp4", shot.name)), fps, recorder.script.size)
                });
                info!("milestone: recorder shot {} rolling ({frames} frames, settled in {:.1}s)", shot.name, (now - *since).as_secs_f32());
                recorder.phase = Phase::Roll(Roll {
                    frame: 0, frames, armed: false, start: now, anchor: position.clone(), writer, received: 0, fps,
                });
            }
        }
    }
    if skip {
        recorder.next_shot();
    }
}

/// Presses and lets go of the keys the script holds, fires and taps.
fn press_keys(mut recorder: ResMut<Recorder>, mut keyboard: ResMut<ButtonInput<KeyCode>>) {
    let t = recorder.t();
    let step = 1.0 / recorder.script.fps as f32;
    let rolling = matches!(&recorder.phase, Phase::Roll(roll) if roll.armed);
    let presses: Vec<(Press, KeyCode)> = match (rolling, recorder.shot()) {
        (true, Some(shot)) => shot.input.iter()
            .filter(|(at, ..)| *at >= t && *at < t + step)
            .filter_map(|(_, press, name)| {
                let code = key_code(name);
                if code.is_none() {
                    warn!("recorder: no key named {name}");
                }
                code.map(|c| (*press, c))
            })
            .collect(),
        _ => Vec::new(),
    };
    let keys = &mut recorder.keys;
    for key in keys.tapped.drain(..).chain(keys.released.drain(..)) {
        keyboard.release(key);
    }
    for (press, key) in presses {
        match press {
            Press::Hold => { keys.held.insert(key); }
            Press::Release => { keys.held.remove(&key); keyboard.release(key); }
            Press::Tap => { keyboard.press(key); keys.tapped.push(key); }
        }
    }
    for &key in &keys.held {
        keyboard.press(key);
    }
}

/// Places the camera on the shot's path.
fn drive_camera(
    recorder: Res<Recorder>,
    mut camera: Query<(&mut Projection, &mut Transform), (With<Camera3d>, Without<CloseupCamera>)>,
    buffers: Res<InputQueues>,
    player: Query<(&VisualPosition, &Position, &Heading)>,
    map: Res<Map>,
    origin: Res<RenderOrigin>,
) {
    let Some(CameraPath::Path { anchor, ease, keys, follow_at, blend }) = recorder.shot().map(|s| &s.camera) else { return };
    let taken = handover(*follow_at, *blend, recorder.t());
    if taken >= 1.0 {
        return;
    }
    let Some((visual, position, heading)) = buffers.entities().next().and_then(|&e| player.get(e).ok()) else { return };
    let Ok((mut projection, mut transform)) = camera.single_mut() else { return };
    let base = match (anchor, &recorder.phase) {
        (Anchor::Player | Anchor::Heading, _) => visual.current(),
        (Anchor::Start, Phase::Roll(roll) | Phase::Drain(roll)) => origin.render(&map, &roll.anchor),
        (Anchor::Start, _) => origin.render(&map, position),
    };
    // Bearings run clockwise seen from above, a turn about +y the other way.
    let turn = match anchor {
        Anchor::Heading => Quat::from_rotation_y(-heading.degrees().to_radians()),
        _ => Quat::IDENTITY,
    };
    let (from, look, fov) = camera_at(keys, *ease, recorder.t());
    let on_path = Transform::from_translation(base + turn * from).looking_at(base + turn * look, Vec3::Y);
    // Blended toward the pose the gameplay camera set this frame.
    let followed = *transform;
    *transform = Transform {
        translation: on_path.translation + (followed.translation - on_path.translation) * taken,
        rotation: on_path.rotation.slerp(followed.rotation, taken),
        ..on_path
    };
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = fov + (p.fov - fov) * taken;
    }
}

/// Takes this frame, once the shot's clock is stepped by frame.
fn capture(recorder: Res<Recorder>, mut commands: Commands) {
    let Some(shot) = recorder.shot() else { return };
    let Phase::Roll(roll) = &recorder.phase else { return };
    if !roll.armed {
        return;
    }
    let index = roll.frame;
    if shot.seconds <= 0.0 {
        let path: PathBuf = recorder.script.out.join(format!("{}.png", shot.name));
        commands.spawn(Screenshot::primary_window()).observe(
            move |captured: On<ScreenshotCaptured>, mut frames: ResMut<Captured>| {
                match captured.image.clone().try_into_dynamic() {
                    Ok(image) => if let Err(e) = image.to_rgb8().save(&path) {
                        error!("recorder: {}: {e}", path.display());
                    },
                    Err(e) => error!("recorder: {}: {e}", path.display()),
                }
                frames.0.push(Frame { index, width: 0, height: 0, format: captured.image.texture_descriptor.format, data: Vec::new() });
            },
        );
    } else {
        commands.spawn(Screenshot::primary_window()).observe(
            move |captured: On<ScreenshotCaptured>, mut frames: ResMut<Captured>| {
                let image = &captured.image;
                let Some(data) = image.data.clone() else {
                    error!("recorder: frame {index} came back empty");
                    return;
                };
                frames.0.push(Frame { index, width: image.width(), height: image.height(), format: image.texture_descriptor.format, data });
            },
        );
    }
}

/// Hands the frames read back to the writer, steps the shot's frame and
/// holds it to the wall clock, and finishes a shot once its last frame is
/// written.
fn end_frame(
    mut recorder: ResMut<Recorder>,
    mut captured: ResMut<Captured>,
    mut strategy: ResMut<TimeUpdateStrategy>,
) {
    let fps = recorder.script.fps;
    let name = recorder.shot().map(|s| s.name.clone()).unwrap_or_default();
    let recorder = &mut *recorder;
    let (Phase::Roll(roll) | Phase::Drain(roll)) = &mut recorder.phase else { return };
    for frame in captured.0.drain(..) {
        roll.received += 1;
        if let Some(writer) = &roll.writer {
            if writer.sender().send(frame).is_err() {
                error!("recorder: the writer for {name} stopped");
            }
        }
    }

    match &mut recorder.phase {
        Phase::Roll(roll) if !roll.armed => {
            roll.armed = true;
            roll.start = Instant::now();
            *strategy = TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps as f64));
        }
        Phase::Roll(roll) => {
            roll.frame += 1;
            let due = roll.start + Duration::from_secs_f64(roll.frame as f64 / fps as f64);
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            if roll.frame >= roll.frames {
                *strategy = TimeUpdateStrategy::Automatic;
                let took = (Instant::now() - roll.start).as_secs_f32();
                info!("recorder: shot {name} took {took:.1}s for {:.1}s of video", roll.frames as f32 / fps as f32);
                let Phase::Roll(roll) = std::mem::replace(&mut recorder.phase, Phase::Done) else { unreachable!() };
                recorder.phase = Phase::Drain(roll);
            }
        }
        Phase::Drain(roll) if roll.received >= roll.frames || roll.start.elapsed() > roll.length() + DRAIN_LIMIT => {
            if roll.received < roll.frames {
                warn!("recorder: shot {name}: {} of {} frames came back", roll.received, roll.frames);
            }
            match roll.writer.take().map(Writer::finish) {
                Some(Ok(written)) => info!("milestone: recorder shot {name} written ({written} frames)"),
                Some(Err(e)) => error!("recorder: shot {name}: {e}"),
                None => info!("milestone: recorder shot {name} written (still)"),
            }
            recorder.next_shot();
        }
        _ => {}
    }
}
