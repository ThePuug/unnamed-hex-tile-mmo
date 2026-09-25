//! Music, a pool to a stage. In play the overworld's loops play: one fades
//! in, plays through once or twice, and fades out to nothing on its seam.
//! On character select the teaser's cues play: one plays from its head to
//! its own ending. A rest of silence follows either, then a different piece
//! of the pool. Leaving the stage fades whatever is playing out at once,
//! and a stage arrived on rests before its first piece.
//!
//! A file declares itself in its Vorbis comments — `POOL` names its pool and
//! `LOOP` says whether its end runs into its head — so a pool is whatever
//! `music/` holds that says so, never a list of names here. A piece plays at
//! the level it was rendered at: the loudness the generator set is the mix.
//!
//! The folder is listed and its OGGs loaded one by one, not through
//! `load_folder`: that fails the whole folder over one file no loader
//! claims, and the MIDIs ship beside the OGGs.

mod ogg;

use std::{
    ops::RangeInclusive,
    path::{Path, PathBuf},
    time::Duration,
};

use bevy::{
    asset::io::AssetSourceId,
    audio::Volume,
    prelude::*,
    tasks::{block_on, futures_lite::StreamExt, poll_once, IoTaskPool, Task},
};
use rand::Rng;

use crate::plugins::shell::Stage;

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Music>();
        app.add_systems(Startup, list);
        app.add_systems(Update, (gather, play).chain());
    }
}

const FOLDER: &str = "music";

/// A pool and how it plays on its stage.
struct Kind {
    /// What its files name in `POOL`.
    pool: &'static str,
    stage: Stage,
    /// Whether its files are loops, as their `LOOP` says.
    loops: bool,
    /// Silence from arriving on the stage to the first piece.
    first_rest: Duration,
    /// Silence between two pieces, in seconds, drawn evenly.
    rest: RangeInclusive<f32>,
    /// Times through a piece before it ends.
    passes: RangeInclusive<u32>,
    fade_in: f32,
    fade_out: f32,
}

const KINDS: [Kind; 2] = [
    // A loop's fade out ends on the seam, so it leaves on the hold its end
    // and head share.
    Kind {
        pool: "overworld",
        stage: Stage::Playing,
        loops: true,
        first_rest: Duration::from_secs(5),
        rest: 60.0..=180.0,
        passes: 1..=2,
        fade_in: 3.0,
        fade_out: 10.0,
    },
    // A cue has a head and an ending of its own, so it plays whole.
    Kind {
        pool: "teaser",
        stage: Stage::CharacterSelect,
        loops: false,
        first_rest: Duration::from_secs(1),
        rest: 60.0..=120.0,
        passes: 1..=1,
        fade_in: 0.0,
        fade_out: 0.0,
    },
];

/// How quickly a piece leaves when its stage is left under it.
const FADE_LEAVING: f32 = 2.0;

#[derive(Resource, Default)]
struct Music {
    listing: Option<Task<Result<Vec<PathBuf>, String>>>,
    /// Every OGG in the folder, held until all have loaded or failed; then
    /// each pool keeps its own and the rest are dropped.
    loading: Vec<Handle<AudioSource>>,
    /// Each of [`KINDS`]' pools, in its order.
    pools: [Vec<Piece>; KINDS.len()],
    /// The piece of each pool played last.
    last: [Option<usize>; KINDS.len()],
    phase: Phase,
}

struct Piece {
    source: Handle<AudioSource>,
    length: Duration,
}

enum Phase {
    /// Silent until `until`, counted for the stage it was set on, or for
    /// none yet.
    Resting { stage: Option<Stage>, until: Duration },
    Playing { entity: Entity, kind: usize, started: Duration, ends: Duration, fade_in: f32, fade_out: f32 },
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Resting { stage: None, until: Duration::ZERO }
    }
}

fn list(mut music: ResMut<Music>, server: Res<AssetServer>) {
    let server = server.clone();
    music.listing = Some(IoTaskPool::get().spawn(async move {
        let source = server.get_source(AssetSourceId::Default).map_err(|e| e.to_string())?;
        let mut paths = source.reader().read_directory(Path::new(FOLDER)).await.map_err(|e| e.to_string())?;
        let mut oggs = Vec::new();
        while let Some(path) = paths.next().await {
            if path.extension().is_some_and(|e| e == "ogg") {
                oggs.push(path);
            }
        }
        Ok(oggs)
    }));
}

/// Loads what the listing found, then reads each file once all are in:
/// every file tagged with a pool, and looping as the pool's files do,
/// joins it.
fn gather(mut music: ResMut<Music>, server: Res<AssetServer>, sources: Res<Assets<AudioSource>>) {
    if let Some(task) = &mut music.listing {
        let Some(listed) = block_on(poll_once(task)) else { return };
        music.listing = None;
        match listed {
            Ok(paths) => music.loading = paths.into_iter().map(|p| server.load(p)).collect(),
            Err(error) => warn!("music: {FOLDER}/ could not be listed, so none plays: {error}"),
        }
        return;
    }
    if music.loading.is_empty()
        || music.loading.iter().any(|h| !server.is_loaded(h) && !server.load_state(h).is_failed())
    {
        return;
    }
    for source in std::mem::take(&mut music.loading) {
        let Some(bytes) = sources.get(&source).map(|s| s.bytes.clone()) else { continue };
        let tags = ogg::comments(&bytes).unwrap_or_default();
        let tag = |key: &str| tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        let loops = tag("LOOP") == Some("true");
        let Some(kind) = KINDS.iter().position(|k| tag("POOL") == Some(k.pool) && k.loops == loops) else { continue };
        let Some(length) = ogg::length(&bytes) else {
            warn!("music: {:?} has no length to read", source.path());
            continue;
        };
        music.pools[kind].push(Piece { source, length });
    }
    for (kind, pool) in KINDS.iter().zip(&music.pools) {
        let lengths: Vec<String> = pool.iter().map(|p| format!("{:.1} s", p.length.as_secs_f32())).collect();
        info!("music: {} pieces in the {} pool ({})", pool.len(), kind.pool, lengths.join(", "));
    }
}

fn play(
    mut commands: Commands,
    mut music: ResMut<Music>,
    mut sinks: Query<&mut AudioSink>,
    stage: Res<State<Stage>>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed();
    let stage = *stage.get();
    let music = &mut *music;
    match &mut music.phase {
        Phase::Resting { stage: counted, until } => {
            let Some(k) = KINDS.iter().position(|k| k.stage == stage) else {
                *counted = None;
                return;
            };
            if *counted != Some(stage) {
                *counted = Some(stage);
                *until = (*until).max(now + KINDS[k].first_rest);
            }
            let pool = &music.pools[k];
            if now < *until || pool.is_empty() {
                return;
            }
            let mut rng = rand::rng();
            let index = match music.last[k] {
                Some(last) if pool.len() > 1 => {
                    let drawn = rng.random_range(0..pool.len() - 1);
                    if drawn >= last { drawn + 1 } else { drawn }
                }
                _ => rng.random_range(0..pool.len()),
            };
            let kind = &KINDS[k];
            let chosen = &pool[index];
            let settings = if kind.loops { PlaybackSettings::LOOP } else { PlaybackSettings::ONCE };
            let entity = commands.spawn((AudioPlayer(chosen.source.clone()), settings.with_volume(Volume::SILENT))).id();
            music.last[k] = Some(index);
            music.phase = Phase::Playing {
                entity,
                kind: k,
                started: now,
                ends: now + chosen.length * rng.random_range(kind.passes.clone()),
                fade_in: kind.fade_in,
                fade_out: kind.fade_out,
            };
        }
        Phase::Playing { entity, kind, started, ends, fade_in, fade_out } => {
            let kind = &KINDS[*kind];
            if kind.stage != stage && *ends > now + Duration::from_secs_f32(FADE_LEAVING) {
                *ends = now + Duration::from_secs_f32(FADE_LEAVING);
                *fade_out = FADE_LEAVING;
            }
            if now >= *ends {
                commands.entity(*entity).despawn();
                let rest = Duration::from_secs_f32(rand::rng().random_range(kind.rest.clone()));
                music.phase = Phase::Resting { stage: Some(kind.stage), until: now + rest };
                return;
            }
            // The sink arrives the frame after the spawn; until then the
            // player is silent by its settings.
            if let Ok(mut sink) = sinks.get_mut(*entity) {
                let gain = envelope((now - *started).as_secs_f32(), (*ends - *started).as_secs_f32(), *fade_in, *fade_out);
                sink.set_volume(Volume::Linear(gain));
            }
        }
    }
}

/// The gain `t` seconds into a play `span` seconds long: up from silence
/// over `fade_in`, down to silence at `span` over `fade_out`, with no fade
/// where either is zero. Squared, so each fade moves evenly to the ear
/// rather than lingering near full.
fn envelope(t: f32, span: f32, fade_in: f32, fade_out: f32) -> f32 {
    let ramp = |left: f32, fade: f32| if fade > 0.0 { (left / fade).clamp(0.0, 1.0) } else { 1.0 };
    ramp(t, fade_in).min(ramp(span - t, fade_out)).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOP: (f32, f32) = (3.0, 10.0);

    #[test]
    fn a_play_starts_and_ends_silent_and_holds_full_between() {
        let span = 120.0;
        assert_eq!(envelope(0.0, span, LOOP.0, LOOP.1), 0.0);
        assert_eq!(envelope(span, span, LOOP.0, LOOP.1), 0.0);
        assert_eq!(envelope(span / 2.0, span, LOOP.0, LOOP.1), 1.0);
    }

    #[test]
    fn a_cue_plays_whole_at_full() {
        assert!((0..=1200).all(|i| envelope(i as f32 / 10.0, 120.0, 0.0, 0.0) == 1.0));
    }

    #[test]
    fn each_fade_moves_one_way() {
        let span = 120.0;
        let gains: Vec<f32> = (0..=1200).map(|i| envelope(i as f32 / 10.0, span, LOOP.0, LOOP.1)).collect();
        let peak = gains.iter().position(|&g| g == 1.0).unwrap();
        assert!(gains[..=peak].windows(2).all(|w| w[0] <= w[1]));
        assert!(gains[peak..].windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn leaving_mid_fade_in_never_lifts_the_gain() {
        // Leaving 1 s in: the play is cut to end FADE_LEAVING later.
        let before = envelope(1.0, 120.0, LOOP.0, LOOP.1);
        let after = envelope(1.0, 1.0 + FADE_LEAVING, LOOP.0, FADE_LEAVING);
        assert!(after <= before);
    }

    #[test]
    fn a_cue_cut_short_by_leaving_still_fades() {
        assert!(envelope(119.0, 120.0, 0.0, FADE_LEAVING) < 1.0);
        assert_eq!(envelope(120.0, 120.0, 0.0, FADE_LEAVING), 0.0);
    }

    /// Every stage has at most one pool.
    #[test]
    fn a_stage_has_one_pool() {
        for (i, a) in KINDS.iter().enumerate() {
            assert!(KINDS[i + 1..].iter().all(|b| b.stage != a.stage), "{} shares its stage", a.pool);
        }
    }
}
