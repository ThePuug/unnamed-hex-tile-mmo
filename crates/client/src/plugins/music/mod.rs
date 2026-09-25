//! The overworld's music. A loop from the pool fades in, plays through once
//! or twice, and fades out to nothing on its seam; a rest of silence
//! follows, then a different loop. Leaving play fades whatever is playing
//! out at once, and the rest waits until play resumes.
//!
//! A file declares itself in its Vorbis comments — `POOL` names its pool and
//! `LOOP=true` says its end runs into its head — so the pool is whatever
//! `music/` holds that says so, never a list of names here. A loop plays at
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
const POOL: &str = "overworld";
/// Silence from the start of play to the first loop.
const FIRST_REST: Duration = Duration::from_secs(5);
/// Silence between two loops, in seconds, drawn evenly.
const REST: RangeInclusive<f32> = 60.0..=180.0;
/// Times through a loop before it fades.
const PASSES: RangeInclusive<u32> = 1..=2;
const FADE_IN: f32 = 3.0;
/// Ends on the seam, so a loop leaves on the hold its end and head share.
const FADE_OUT: f32 = 10.0;
/// How quickly a loop leaves when play is left under it.
const FADE_LEAVING: f32 = 2.0;

#[derive(Resource, Default)]
struct Music {
    listing: Option<Task<Result<Vec<PathBuf>, String>>>,
    /// Every OGG in the folder, held until all have loaded or failed; then
    /// the pool keeps its own and the rest are dropped.
    loading: Vec<Handle<AudioSource>>,
    pool: Vec<Loop>,
    last: Option<usize>,
    phase: Phase,
}

struct Loop {
    source: Handle<AudioSource>,
    length: Duration,
}

enum Phase {
    Resting { until: Duration },
    Playing { entity: Entity, started: Duration, ends: Duration, fade_out: f32 },
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Resting { until: Duration::ZERO }
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
/// every loop tagged with the pool joins it.
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
    let mut pool = Vec::new();
    for source in std::mem::take(&mut music.loading) {
        let Some(bytes) = sources.get(&source).map(|s| s.bytes.clone()) else { continue };
        let tags = ogg::comments(&bytes).unwrap_or_default();
        let tag = |key: &str| tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        if tag("POOL") != Some(POOL) || tag("LOOP") != Some("true") {
            continue;
        }
        let Some(length) = ogg::length(&bytes) else {
            warn!("music: {:?} has no length to read", source.path());
            continue;
        };
        pool.push(Loop { source, length });
    }
    let lengths: Vec<String> = pool.iter().map(|l| format!("{:.1} s", l.length.as_secs_f32())).collect();
    info!("music: {} loops in the {POOL} pool ({})", pool.len(), lengths.join(", "));
    music.pool = pool;
}

fn play(
    mut commands: Commands,
    mut music: ResMut<Music>,
    mut sinks: Query<&mut AudioSink>,
    stage: Res<State<Stage>>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed();
    let playing = *stage.get() == Stage::Playing;
    let music = &mut *music;
    match &mut music.phase {
        Phase::Resting { until } => {
            if !playing {
                *until = (*until).max(now + FIRST_REST);
                return;
            }
            if now < *until || music.pool.is_empty() {
                return;
            }
            let mut rng = rand::rng();
            let index = match music.last {
                Some(last) if music.pool.len() > 1 => {
                    let drawn = rng.random_range(0..music.pool.len() - 1);
                    if drawn >= last { drawn + 1 } else { drawn }
                }
                _ => rng.random_range(0..music.pool.len()),
            };
            let chosen = &music.pool[index];
            let entity = commands.spawn((
                AudioPlayer(chosen.source.clone()),
                PlaybackSettings::LOOP.with_volume(Volume::SILENT),
            )).id();
            music.last = Some(index);
            music.phase = Phase::Playing {
                entity,
                started: now,
                ends: now + chosen.length * rng.random_range(PASSES),
                fade_out: FADE_OUT,
            };
        }
        Phase::Playing { entity, started, ends, fade_out } => {
            if !playing && *ends > now + Duration::from_secs_f32(FADE_LEAVING) {
                *ends = now + Duration::from_secs_f32(FADE_LEAVING);
                *fade_out = FADE_LEAVING;
            }
            if now >= *ends {
                commands.entity(*entity).despawn();
                let rest = Duration::from_secs_f32(rand::rng().random_range(REST));
                music.phase = Phase::Resting { until: now + rest };
                return;
            }
            // The sink arrives the frame after the spawn; until then the
            // player is silent by its settings.
            if let Ok(mut sink) = sinks.get_mut(*entity) {
                let gain = envelope((now - *started).as_secs_f32(), (*ends - *started).as_secs_f32(), *fade_out);
                sink.set_volume(Volume::Linear(gain));
            }
        }
    }
}

/// The gain `t` seconds into a play `span` seconds long: up from silence
/// over `FADE_IN`, down to silence at `span` over `fade_out`. Squared, so
/// each fade moves evenly to the ear rather than lingering near full.
fn envelope(t: f32, span: f32, fade_out: f32) -> f32 {
    let rising = (t / FADE_IN).clamp(0.0, 1.0);
    let falling = ((span - t) / fade_out).clamp(0.0, 1.0);
    rising.min(falling).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_play_starts_and_ends_silent_and_holds_full_between() {
        let span = 120.0;
        assert_eq!(envelope(0.0, span, FADE_OUT), 0.0);
        assert_eq!(envelope(span, span, FADE_OUT), 0.0);
        assert_eq!(envelope(span / 2.0, span, FADE_OUT), 1.0);
    }

    #[test]
    fn each_fade_moves_one_way() {
        let span = 120.0;
        let gains: Vec<f32> = (0..=1200).map(|i| envelope(i as f32 / 10.0, span, FADE_OUT)).collect();
        let peak = gains.iter().position(|&g| g == 1.0).unwrap();
        assert!(gains[..=peak].windows(2).all(|w| w[0] <= w[1]));
        assert!(gains[peak..].windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn leaving_mid_fade_in_never_lifts_the_gain() {
        // Leaving 1 s in: the play is cut to end FADE_LEAVING later.
        let before = envelope(1.0, 120.0, FADE_OUT);
        let after = envelope(1.0, 1.0 + FADE_LEAVING, FADE_LEAVING);
        assert!(after <= before);
    }
}
