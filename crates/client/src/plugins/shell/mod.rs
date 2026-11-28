//! Everything around the world: connecting to the server, the character
//! screen and its credits, the loading screen, and the menu Esc opens
//! over the world.
//!
//! The client runs through `Stage`s. It connects, retrying on its own with
//! a growing wait, and waits at the character screen; Play asks the server
//! to put the character in the world, and the world loads until the
//! terrain the server streams, the tiles and the whole far view, has
//! arrived and been built. Leaving the world,
//! by the menu or by losing the server, takes everything the world put on
//! the client down again, so the next entry starts from nothing.

mod credits;
pub mod menu;
mod screens;
pub mod view;

use std::time::Duration;

use bevy::prelude::*;

use common_bevy::{
    chunk::{calculate_visible_chunks, loc_to_chunk, FIXED_STREAM_RADIUS},
    geometry::flat_top_tile_center,
    summary_mesh::{streamed_regions, MeshRegionKey},
    components::{
        entity_type::{actor::*, EntityType},
        Actor, Loc,
    },
    message::{Event, Try},
    resources::InputQueues,
};

use crate::{
    components::{CombatLogEntry, FloatingText},
    network::Link,
    resources::{EntityMap, LoadedChunks, RenderOrigin, Server, SummaryCache, SummaryMesh, SummaryMeshes},
    systems::{
        character_panel::{self, CharacterPanel, CharacterPanelState},
        closeup::Figure,
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

/// How long the loading screen waits with nothing arriving and nothing
/// built before showing the world anyway: terrain the server never sends
/// must not hold the player out. The far view takes minutes where the
/// server has not made it before, so the wait is on a stall, never on the
/// whole.
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
    /// Far regions streamed of those the server streams around the player,
    /// every level's out to the far view's reach.
    pub regions: (usize, usize),
    /// The same, level by level, the finest first: a level's radius, and
    /// its regions streamed of those the server streams.
    pub levels: Vec<(u32, usize, usize)>,
    /// Terrain builds in flight or waiting their turn.
    pub building: usize,
    /// Regions of terrain standing built, of those drawn round the player
    /// once everything streamed has arrived.
    pub built: (usize, usize),
    /// The far regions the server streams a player where this one entered,
    /// found once: the server sends for the tile it entered on. And the
    /// regions drawn round it there.
    wanted: Option<Vec<MeshRegionKey>>,
    drawn: Option<Vec<MeshRegionKey>>,
    quiet: u32,
    /// When something last arrived or a build last finished.
    since: Duration,
}

impl Loading {
    /// What the server streams, chunks and far regions together: how much
    /// has arrived, and how much there is.
    pub fn streamed(&self) -> (usize, usize) {
        (self.chunks.0 + self.regions.0, self.chunks.1 + self.regions.1)
    }

    /// Each thing the world loads, in the order it comes: the tiles, the
    /// far view a level at a time from the finest, and the terrain built
    /// of them.
    pub fn lines(&self) -> Vec<Line> {
        let mut lines = vec![Line { name: "Tiles".to_string(), have: self.chunks.0, want: self.chunks.1 }];
        lines.extend(self.levels.iter().map(|&(r, have, want)| Line {
            name: format!("Far view, {}-tile cells", common::summary::scale(r)),
            have,
            want,
        }));
        lines.push(Line { name: "Terrain built".to_string(), have: self.built.0, want: self.built.1 });
        lines
    }
}

/// One thing the world loads: what it is, and how much of it is there.
pub struct Line {
    pub name: String,
    pub have: usize,
    pub want: usize,
}

/// How many lines a loading world shows ([`Loading::lines`]): the tiles,
/// each far level, and the terrain built.
pub const LINES: usize = common_bevy::summary::LOD_LEVELS.len() + 1;

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Stage>();
        app.add_computed_state::<InWorld>();
        app.init_resource::<menu::GameMenu>();
        app.init_resource::<credits::CreditsPanel>();
        app.init_resource::<Loading>();
        app.init_resource::<Entered>();
        app.init_resource::<screens::RttSamples>();
        app.init_resource::<view::Viewing>();
        app.init_resource::<view::Rejoin>();
        app.add_systems(Update, (view::do_view, view::end_when_gone).chain().run_if(in_state(InWorld)));
        app.add_systems(Update, view::rejoin.run_if(in_state(Stage::CharacterSelect)));
        app.add_systems(Startup, (menu::setup, screens::setup, credits::setup));
        app.add_systems(Update, (follow_link, menu::route_keys, menu::draw, credits::draw, screens::draw, screens::draw_lines, screens::show_rtt));
        app.add_systems(Update, track_loading.run_if(in_state(Stage::Loading)));
        app.add_systems(OnEnter(Stage::Loading), start_loading);
        app.add_systems(OnEnter(Stage::Playing), arrive);
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

/// Tells the server the world has loaded: the character it held out of
/// play while the loading screen stood steps into the world.
fn arrive(mut writer: MessageWriter<Try>) {
    writer.write(Try { event: Event::Arrive });
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

fn start_loading(mut loading: ResMut<Loading>, time: Res<Time<Real>>) {
    *loading = Loading { since: time.elapsed(), ..default() };
}

/// Of the chunks the server streams around a player at `loc`, how many
/// have arrived, and how many it streams.
pub fn chunks_streamed(loc: &Loc, loaded: &LoadedChunks) -> (usize, usize) {
    let wanted = calculate_visible_chunks(loc_to_chunk(**loc), FIXED_STREAM_RADIUS);
    let have = wanted.iter().filter(|c| loaded.chunks.contains(c)).count();
    (have, wanted.len())
}

/// Counts what has arrived of the terrain around the player, the tiles
/// and the whole far view, and what is still being built, and plays once
/// it has all arrived and settled.
#[allow(clippy::too_many_arguments)]
fn track_loading(
    mut loading: ResMut<Loading>,
    mut next: ResMut<NextState<Stage>>,
    buffers: Res<InputQueues>,
    locs: Query<&Loc>,
    loaded: Res<LoadedChunks>,
    summaries: Res<SummaryCache>,
    meshes: Res<SummaryMeshes>,
    time: Res<Time<Real>>,
) {
    let before = (loading.streamed().0, loading.building);
    // The local player is the one entity with an input queue.
    let player = buffers.entities().find_map(|&ent| locs.get(ent).ok());
    if let Some(loc) = player {
        loading.chunks = chunks_streamed(loc, &loaded);
        let wanted = loading.wanted.get_or_insert_with(|| {
            let (wx, wz) = flat_top_tile_center(loc.q, loc.r, 1.0);
            streamed_regions(wx, wz, 0).into_iter().collect()
        });
        let levels: Vec<(u32, usize, usize)> = common_bevy::summary::LOD_LEVELS[1..]
            .iter()
            .map(|&r| {
                let of_level = || wanted.iter().filter(move |key| key.r == r);
                (r, of_level().filter(|key| summaries.contains_region(key)).count(), of_level().count())
            })
            .collect();
        loading.regions = levels.iter().fold((0, 0), |(have, want), level| (have + level.1, want + level.2));
        loading.levels = levels;
        let drawn = loading.drawn.get_or_insert_with(|| crate::systems::world::regions_when_streamed(loc).into_iter().collect());
        let standing = drawn.iter().filter(|key| meshes.states.get(key).is_some_and(|state| state.entity.is_some())).count();
        let to_stand = drawn.len();
        loading.built = (standing, to_stand);
    }
    loading.building = meshes.states.values().filter(|s| s.task.is_some()).count() + meshes.queued;
    if (loading.streamed().0, loading.building) != before {
        loading.since = time.elapsed();
    }

    let (have, want) = loading.streamed();
    let streamed = want > 0 && have == want;
    loading.quiet = if streamed && loading.building == 0 { loading.quiet + 1 } else { 0 };
    if loading.quiet >= QUIET_FRAMES {
        next.set(Stage::Playing);
    } else if time.elapsed() - loading.since > LOADING_LIMIT {
        let ((chunks, of_chunks), (regions, of_regions)) = (loading.chunks, loading.regions);
        warn!(
            "Nothing of the terrain arrived or was built for {:?}: {chunks}/{of_chunks} chunks, {regions}/{of_regions} far regions, {} builds; playing anyway",
            LOADING_LIMIT, loading.building
        );
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
            With<FloatingText>,
            With<CombatLogEntry>,
        )>,
    >,
    l2r: Res<EntityMap>,
    mut menu: ResMut<menu::GameMenu>,
    mut character: ResMut<CharacterPanelState>,
    mut character_view: Query<&mut Visibility, With<CharacterPanel>>,
    mut entered: ResMut<Entered>,
    mut viewing: ResMut<view::Viewing>,
) {
    entered.0 = false;
    viewing.0 = None;
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
