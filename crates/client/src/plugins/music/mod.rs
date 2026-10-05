//! Music, a pool to a stage: in play the overworld's pieces, on character
//! select the teaser's cues. A piece plays once, from its head to its own
//! ending; a rest of silence follows, then another piece of the pool.
//! Leaving the stage fades whatever is playing out at once, and a stage
//! arrived on rests before its first piece.
//!
//! Nothing is recorded. A pool is the pieces of `music::pieces::PIECES`
//! that name it, and the client composes each play as the music player
//! does: a piece at a seed of its own, rendered on a thread of its own
//! while the last one rests. GeneralUser GS ships in the game's assets;
//! the sampled banks are fetched on the first run that lacks them
//! (`fetch`).
//! A play waits for its rest and for its render, whichever is longer, so
//! the rest is never cut short. A piece plays at the loudness its render
//! sets, scaled by the player's music volume: the loudness the piece
//! declares is the mix.
//!
//! No GeneralUser, no music: the client says so once and stays silent.

mod fetch;
mod take;

use std::{
    ops::RangeInclusive,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Mutex,
    },
    time::{Duration, Instant},
};

use bevy::{
    audio::{AddAudioSource, Volume},
    prelude::*,
};
use music::{
    banks,
    pieces::PIECES,
    render::{self, Bank, SAMPLE_RATE},
    SEEDS,
};
use rand::Rng;

use crate::plugins::{settings::AudioSettings, shell::Stage};
use take::Take;

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Take>();
        app.insert_resource(Music { composer: Some(Composer::spawn()), next: None, phase: Phase::default() });
        app.add_systems(Update, (gather, play).chain());
    }
}

/// A pool and how it plays on its stage.
struct Kind {
    /// What its pieces name as their `pool`.
    pool: &'static str,
    stage: Stage,
    /// Silence from arriving on the stage to the first piece.
    first_rest: Duration,
    /// Silence between two pieces, in seconds, drawn evenly.
    rest: RangeInclusive<f32>,
}

// A piece has a head and an ending of its own, so it plays whole.
const KINDS: [Kind; 2] = [
    Kind {
        pool: "overworld",
        stage: Stage::Playing,
        first_rest: Duration::from_secs(5),
        rest: 60.0..=180.0,
    },
    Kind {
        pool: "teaser",
        stage: Stage::CharacterSelect,
        first_rest: Duration::from_secs(1),
        rest: 60.0..=120.0,
    },
];

/// GeneralUser GS, in the game's assets.
const DEFAULT_BANK: &str = "soundfonts/GeneralUser.sf2";

/// How quickly a piece leaves when its stage is left under it.
const FADE_LEAVING: f32 = 2.0;

#[derive(Resource)]
struct Music {
    /// Gone once it has no bank to play.
    composer: Option<Composer>,
    /// The next piece, of the kind at its index: composing, or composed.
    next: Option<(usize, Option<Composed>)>,
    phase: Phase,
}

struct Composed {
    take: Handle<Take>,
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

/// The thread that composes, one piece at a time, each as it is asked
/// for. It reads the bank first; a render runs for seconds and holds the
/// sampled banks its score seats while it does. Where the bank lacks a
/// sampled bank `voices` names, a second thread fetches them, and the
/// pieces composed meanwhile play those programs on GeneralUser GS.
struct Composer {
    asks: Sender<(usize, u64)>,
    /// Behind a lock only because a resource is shared between threads.
    done: Mutex<Receiver<Result<Vec<[f32; 2]>, String>>>,
}

impl Composer {
    fn spawn() -> Self {
        let (asks, asked) = mpsc::channel::<(usize, u64)>();
        let (tell, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("music".into())
            .spawn(move || {
                let mut bank = match banks::open(Some(Path::new(crate::ASSETS).join(DEFAULT_BANK))) {
                    Ok(bank) => bank,
                    Err(error) => {
                        let _ = tell.send(Err(error));
                        return;
                    }
                };
                info!("music: GeneralUser GS and {} sampled banks", bank.sampled());
                let fetched = fetch_lacking(&bank);
                for (piece, seed) in asked {
                    if let Some(folder) = fetched.as_ref().and_then(|f| f.try_recv().ok()) {
                        bank = bank.with_banks(&folder);
                        info!("music: GeneralUser GS and {} sampled banks", bank.sampled());
                    }
                    let started = Instant::now();
                    let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&PIECES[piece], seed, &bank).1))
                        .map_err(|_| format!("{} at seed {seed} panicked as it composed", PIECES[piece].name));
                    if taken.is_ok() {
                        info!("music: composed {} at seed {seed} in {:.1} s", PIECES[piece].name, started.elapsed().as_secs_f32());
                    }
                    if tell.send(taken).is_err() {
                        return;
                    }
                }
            })
            .expect("a thread to compose on");
        Composer { asks, done: Mutex::new(done) }
    }
}

/// Where `bank` lacks a file `voices` names, fetches the banks on a thread
/// of their own; the folder they went into arrives once they are in.
fn fetch_lacking(bank: &Bank) -> Option<Receiver<PathBuf>> {
    let lacking = bank.lacking();
    if lacking.is_empty() {
        return None;
    }
    let Some(folder) = banks::folder() else {
        warn!("music: no data folder to fetch the banks into; {} play on GeneralUser GS", lacking.join(", "));
        return None;
    };
    info!("music: fetching {} into {}, lacking {}", banks::RELEASE, folder.display(), lacking.join(", "));
    let (tell, fetched) = mpsc::channel();
    std::thread::Builder::new()
        .name("music banks".into())
        .spawn(move || match fetch::fetch(&folder) {
            Ok(()) => {
                let _ = tell.send(folder);
            }
            Err(error) => warn!("music: the banks could not be fetched: {error}"),
        })
        .expect("a thread to fetch on");
    Some(fetched)
}

/// Takes what the composer finished, and while resting asks it for the
/// stage's next piece where none is composing or composed for it: one
/// render is held at a time, a minute or more of samples. A piece composed
/// for a stage since left is dropped.
fn gather(mut music: ResMut<Music>, mut takes: ResMut<Assets<Take>>, stage: Res<State<Stage>>) {
    let music = &mut *music;
    let Some(composer) = &music.composer else { return };
    let done = composer.done.lock().unwrap().try_recv();
    match done {
        Ok(Ok(audio)) => {
            if let Some((_, composed @ None)) = &mut music.next {
                let length = Duration::from_secs_f64(audio.len() as f64 / SAMPLE_RATE as f64);
                *composed = Some(Composed { take: takes.add(Take::new(audio)), length });
            }
        }
        // A bank that failed to load ends the thread after this; a piece
        // that failed is asked for again at another seed.
        Ok(Err(error)) => {
            warn!("music: {error}");
            music.next = None;
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            music.composer = None;
            return;
        }
    }
    if matches!(music.phase, Phase::Playing { .. }) {
        return;
    }
    let Some(kind) = KINDS.iter().position(|k| k.stage == *stage.get()) else { return };
    match &music.next {
        Some((_, None)) => return,
        Some((k, Some(_))) if *k == kind => return,
        _ => {}
    }
    let pool: Vec<usize> = (0..PIECES.len()).filter(|&p| PIECES[p].pool == KINDS[kind].pool).collect();
    if pool.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    let piece = pool[rng.random_range(0..pool.len())];
    let seed = rng.random_range(0..SEEDS);
    info!("music: composing {} at seed {seed}", PIECES[piece].name);
    if composer.asks.send((piece, seed)).is_ok() {
        music.next = Some((kind, None));
    }
}

fn play(
    mut commands: Commands,
    mut music: ResMut<Music>,
    mut sinks: Query<&mut AudioSink>,
    stage: Res<State<Stage>>,
    time: Res<Time<Real>>,
    audio: Res<AudioSettings>,
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
            if now < *until || !matches!(music.next, Some((kind, Some(_))) if kind == k) {
                return;
            }
            let Some((_, Some(next))) = music.next.take() else { return };
            let entity = commands.spawn((AudioPlayer(next.take), PlaybackSettings::ONCE.with_volume(Volume::SILENT))).id();
            music.phase = Phase::Playing {
                entity,
                kind: k,
                started: now,
                ends: now + next.length,
                fade_in: 0.0,
                fade_out: 0.0,
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
                sink.set_volume(Volume::Linear(gain * audio.music.gain()));
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

    /// Every pool a stage plays has a piece to compose.
    #[test]
    fn every_pool_has_a_piece() {
        for kind in &KINDS {
            assert!(PIECES.iter().any(|p| p.pool == kind.pool), "no piece names the {} pool", kind.pool);
        }
    }
}
