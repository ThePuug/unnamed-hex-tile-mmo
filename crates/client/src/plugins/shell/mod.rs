//! Everything around the world: connecting to the server, the character
//! screen, the loading screen, and the menu Esc opens over the world.
//!
//! The client runs through `Stage`s. It connects, retrying on its own with
//! a growing wait, and waits at the character screen; Play asks the server
//! to put the character in the world, and the world loads until the
//! terrain the server streams has arrived and been built. Leaving the world,
//! by the menu or by losing the server, takes everything the world put on
//! the client down again, so the next entry starts from nothing.

pub mod menu;
mod screens;

use std::time::Duration;

use bevy::prelude::*;

use common_bevy::{
    chunk::{calculate_visible_chunks, loc_to_chunk, FIXED_STREAM_RADIUS},
    components::{
        entity_type::{actor::*, EntityType},
        equipment::Equipment,
        Actor, Loc,
    },
    message::{Event, Try},
    resources::InputQueues,
};

use crate::{
    components::{CombatLogEntry, FloatingText, PoppingThreatIcon, ResolvedThreatEntry},
    network::Link,
    resources::{EntityMap, LoadedChunks, RenderOrigin, Server, SummaryCache, SummaryMesh, SummaryMeshes},
    systems::{
        attack_telegraph::{AttackBall, HitLine},
        character_panel::{self, CharacterPanel, CharacterPanelState},
        closeup::Figure,
        threat_icons::{OverflowCounter, ThreatIcon},
    },
};

/// Where the client is, from connecting to playing.
#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Stage {
    /// No connection yet, or the last one failed.
    #[default]
    Connecting,
    /// Connected, the character out of the world.
    CharacterSelect,
    /// The character is in the world and its terrain is arriving.
    Loading,
    Playing,
}

/// Loading and playing: the stretch the world's state belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InWorld;

impl ComputedStates for InWorld {
    type SourceStates = Stage;

    // Loading to playing recomputes the same value; were that a transition,
    // leaving the world would run as play begins.
    const ALLOW_SAME_STATE_TRANSITIONS: bool = false;

    fn compute(stage: Stage) -> Option<Self> {
        matches!(stage, Stage::Loading | Stage::Playing).then_some(InWorld)
    }
}

/// Whether the client has asked to be in the world and not since left it.
/// Set as the request goes out, not when the stage follows a frame later:
/// on a near server the answer can arrive before the stage changes, and
/// what reads the wire must not drop it.
#[derive(Resource, Default)]
pub struct Entered(pub bool);

/// How long the loading screen waits for terrain before showing the world
/// anyway: a chunk the server never sends must not hold the player out.
const LOADING_LIMIT: Duration = Duration::from_secs(60);
/// How many frames in a row no terrain build may be in flight before the
/// terrain counts as settled: a build is dispatched the frame after its
/// data lands, so one quiet frame proves nothing.
const QUIET_FRAMES: u32 = 10;

/// How far the world has loaded.
#[derive(Resource, Default)]
pub struct Loading {
    /// Chunks streamed of those the server streams around the player.
    pub chunks: (usize, usize),
    /// Terrain builds in flight.
    pub building: usize,
    /// The most builds seen in flight at once, which `building` counts down from.
    pub building_peak: usize,
    quiet: u32,
    since: Duration,
}

impl Loading {
    /// The share loaded, 0 to 1: the chunks for most of the bar, the builds
    /// for the rest.
    pub fn progress(&self) -> f32 {
        let (have, want) = self.chunks;
        let chunks = if want == 0 { 0.0 } else { have as f32 / want as f32 };
        let built = if self.building_peak == 0 { 1.0 } else { 1.0 - self.building as f32 / self.building_peak as f32 };
        if have < want || want == 0 {
            0.85 * chunks
        } else {
            0.85 + 0.15 * built
        }
    }
}

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Stage>();
        app.add_computed_state::<InWorld>();
        app.init_resource::<menu::GameMenu>();
        app.init_resource::<Loading>();
        app.init_resource::<Entered>();
        app.init_resource::<screens::RttSamples>();
        app.add_systems(Startup, (menu::setup, screens::setup));
        app.add_systems(Update, (follow_link, menu::route_keys, menu::draw, screens::draw, screens::show_rtt));
        app.add_systems(Update, track_loading.run_if(in_state(Stage::Loading)));
        app.add_systems(OnEnter(Stage::Loading), start_loading);
        app.add_systems(OnEnter(Stage::CharacterSelect), dress_preview);
        app.add_systems(OnExit(InWorld), leave_world);
    }
}

/// Follows the connection: to the character screen once it is made, back
/// to connecting whenever it is lost, whatever the stage.
fn follow_link(link: Res<Link>, stage: Res<State<Stage>>, mut next: ResMut<NextState<Stage>>) {
    match (link.is_connected(), stage.get()) {
        (true, Stage::Connecting) => next.set(Stage::CharacterSelect),
        (false, Stage::Connecting) => {}
        (false, _) => next.set(Stage::Connecting),
        _ => {}
    }
}

/// Asks the server to put the character in the world.
pub fn play(writer: &mut MessageWriter<Try>, next: &mut NextState<Stage>, entered: &mut Entered) {
    entered.0 = true;
    writer.write(Try { event: Event::Play });
    next.set(Stage::Loading);
}

/// Asks the server to take the character out of the world, keeping the
/// connection.
pub fn leave(writer: &mut MessageWriter<Try>, next: &mut NextState<Stage>, entered: &mut Entered) {
    entered.0 = false;
    writer.write(Try { event: Event::Leave });
    next.set(Stage::CharacterSelect);
}

/// The body the character screen shows, the one the server gives a player.
pub fn preview_body() -> EntityType {
    EntityType::Actor(ActorImpl::new(Origin::Evolved, Approach::Direct, Resilience::Vital, ActorIdentity::Player))
}

fn dress_preview(mut figure: Query<&mut Equipment, With<Figure>>) {
    if let Ok(mut worn) = figure.single_mut() {
        worn.set_if_neq(Equipment::starting_outfit());
    }
}

fn start_loading(mut loading: ResMut<Loading>, time: Res<Time<Real>>) {
    *loading = Loading { since: time.elapsed(), ..default() };
}

/// Counts what has arrived of the terrain around the player and what is
/// still being built, and plays once it has settled.
fn track_loading(
    mut loading: ResMut<Loading>,
    mut next: ResMut<NextState<Stage>>,
    buffers: Res<InputQueues>,
    locs: Query<&Loc>,
    loaded: Res<LoadedChunks>,
    meshes: Res<SummaryMeshes>,
    time: Res<Time<Real>>,
) {
    // The local player is the one entity with an input queue.
    let player = buffers.entities().find_map(|&ent| locs.get(ent).ok());
    if let Some(loc) = player {
        let wanted = calculate_visible_chunks(loc_to_chunk(**loc), FIXED_STREAM_RADIUS);
        let have = wanted.iter().filter(|c| loaded.chunks.contains(c)).count();
        loading.chunks = (have, wanted.len());
    }
    loading.building = meshes.states.values().filter(|s| s.task.is_some()).count();
    loading.building_peak = loading.building_peak.max(loading.building);

    let (have, want) = loading.chunks;
    let streamed = want > 0 && have == want;
    loading.quiet = if streamed && loading.building == 0 { loading.quiet + 1 } else { 0 };
    if loading.quiet >= QUIET_FRAMES {
        next.set(Stage::Playing);
    } else if time.elapsed() - loading.since > LOADING_LIMIT {
        warn!("Terrain not settled after {:?}: {have}/{want} chunks, {} builds; playing anyway", LOADING_LIMIT, loading.building);
        next.set(Stage::Playing);
    }
}

/// Takes down everything the world put on the client: its actors, its
/// terrain and what hangs from them, and the state that tracked them, so
/// entering again starts as the first entry did.
#[allow(clippy::too_many_arguments)]
fn leave_world(
    mut commands: Commands,
    actors: Query<Entity, (Or<(With<EntityType>, With<Actor>)>, Without<Figure>, Without<Camera>)>,
    world: Query<
        Entity,
        Or<(
            With<SummaryMesh>,
            With<AttackBall>,
            With<HitLine>,
            With<FloatingText>,
            With<ResolvedThreatEntry>,
            With<CombatLogEntry>,
            With<ThreatIcon>,
            With<PoppingThreatIcon>,
            With<OverflowCounter>,
        )>,
    >,
    l2r: Res<EntityMap>,
    mut menu: ResMut<menu::GameMenu>,
    mut character: ResMut<CharacterPanelState>,
    mut character_view: Query<&mut Visibility, With<CharacterPanel>>,
    mut entered: ResMut<Entered>,
) {
    entered.0 = false;
    for entity in actors.iter().chain(world.iter()).chain(l2r.left_values().copied()) {
        commands.entity(entity).try_despawn();
    }
    commands.insert_resource(crate::resources::world_map());
    commands.insert_resource(EntityMap::default());
    commands.insert_resource(InputQueues::default());
    commands.insert_resource(LoadedChunks::default());
    commands.insert_resource(SummaryMeshes::default());
    commands.insert_resource(SummaryCache::default());
    commands.insert_resource(RenderOrigin::default());
    commands.insert_resource(Server::default());
    menu.close();
    if let Ok(mut visibility) = character_view.single_mut() {
        character_panel::close(&mut character, &mut visibility);
    }
}
