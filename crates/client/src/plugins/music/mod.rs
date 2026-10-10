//! Music, a style and a setting to a stage: in play the Bulgarian tracks
//! the overworld's wandering is played in, on character select the
//! teaser as it is. A track plays once, from its head to its own
//! ending; a rest of silence follows, then another play. Leaving the
//! stage fades whatever is playing out at once, and a stage arrived on
//! rests before its first play.
//!
//! Nothing is recorded. A stage plays the tracks of its style that are
//! played in its setting (`music::pieces::TRACKS`), by one of the
//! style's bands (`music::band::BANDS`) for the session, and the client composes each play as the music
//! player does: a track at a seed of its own, rendered on a thread of its own
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
    band::{self, Band},
    banks,
    pieces::{Params, Setting, Style, TRACKS},
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
        let mut rng = rand::rng();
        let bands = Style::ALL
            .iter()
            .map(|style| {
                let roster: Vec<&'static Band> = band::of_style(*style).collect();
                roster[rng.random_range(0..roster.len())]
            })
            .collect();
        app.insert_resource(Music { composer: Some(Composer::spawn()), next: None, phase: Phase::default(), bands });
        app.add_systems(Update, (gather, play).chain());
    }
}

/// What a stage plays and how: the tracks of `style` played in
/// `setting`, or the one `track` where the stage names one.
struct Kind {
    style: Style,
    setting: Setting,
    track: Option<&'static str>,
    stage: Stage,
    /// Silence from arriving on the stage to the first piece.
    first_rest: Duration,
    /// Silence between two pieces, in seconds, drawn evenly.
    rest: RangeInclusive<f32>,
}

// A piece has a head and an ending of its own, so it plays whole.
const KINDS: [Kind; 2] = [
    Kind {
        style: Style::Bulgarian,
        setting: Setting::Ambient,
        track: None,
        stage: Stage::Playing,
        first_rest: Duration::from_secs(5),
        rest: 60.0..=180.0,
    },
    Kind {
        style: Style::Bulgarian,
        // The teaser as it is, its cue cut to picture.
        setting: Setting::None,
        track: Some("teaser"),
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
    /// Each style's band for the session, one of its roster: its plays
    /// differ, the band does not.
    bands: Vec<&'static Band>,
}

struct Composed {
    take: Handle<Take>,
    length: Duration,
}

enum Phase {
    /// Silent until `until`, counted for the stage it was set on, or for
    /// none yet.
    Resting { stage: Option<Stage>, until: Duration },
    Playing { entity: Entity, kind: usize, started: Duration, ends: Duration, fade_out: f32 },
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
    asks: Sender<(usize, Params)>,
    /// Behind a lock only because a resource is shared between threads.
    done: Mutex<Receiver<Result<Vec<[f32; 2]>, String>>>,
}

impl Composer {
    fn spawn() -> Self {
        let (asks, asked) = mpsc::channel::<(usize, Params)>();
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
                for (piece, params) in asked {
                    let seed = params.seed;
                    if let Some(folder) = fetched.as_ref().and_then(|f| f.try_recv().ok()) {
                        bank = bank.with_banks(&folder);
                        info!("music: GeneralUser GS and {} sampled banks", bank.sampled());
                    }
                    let started = Instant::now();
                    let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&TRACKS[piece], &params, &bank).1))
                        .map_err(|_| format!("{} at seed {seed} panicked as it composed", TRACKS[piece].name));
                    if taken.is_ok() {
                        info!("music: composed {} at seed {seed} in {:.1} s", TRACKS[piece].name, started.elapsed().as_secs_f32());
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
    let Kind { style, setting, track, .. } = KINDS[kind];
    let pool: Vec<usize> = (0..TRACKS.len()).filter(|&p| TRACKS[p].style == style && TRACKS[p].plays_in(setting) && track.is_none_or(|t| TRACKS[p].name == t)).collect();
    if pool.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    let piece = pool[rng.random_range(0..pool.len())];
    let seed = rng.random_range(0..SEEDS);
    let band = *music.bands.iter().find(|b| b.style == style).unwrap();
    info!("music: composing {} in {} at seed {seed} by {}", TRACKS[piece].name, setting.name(), band.name);
    if composer.asks.send((piece, Params { seed, band, setting })).is_ok() {
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
                fade_out: 0.0,
            };
        }
        Phase::Playing { entity, kind, started, ends, fade_out } => {
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
                let gain = envelope((now - *started).as_secs_f32(), (*ends - *started).as_secs_f32(), *fade_out);
                sink.set_volume(Volume::Linear(gain * audio.music.gain()));
            }
        }
    }
}

/// The gain `t` seconds into a play `span` seconds long: full, then down
/// to silence at `span` over `fade_out`, with no fade where that is zero.
/// Squared, so the fade moves evenly to the ear rather than lingering
/// near full.
fn envelope(t: f32, span: f32, fade_out: f32) -> f32 {
    let ramp = if fade_out > 0.0 { ((span - t) / fade_out).clamp(0.0, 1.0) } else { 1.0 };
    ramp.powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FADE_OUT: f32 = 10.0;

    #[test]
    fn a_play_holds_full_and_ends_silent() {
        let span = 120.0;
        assert_eq!(envelope(span / 2.0, span, FADE_OUT), 1.0);
        assert_eq!(envelope(span, span, FADE_OUT), 0.0);
    }

    #[test]
    fn a_cue_plays_whole_at_full() {
        assert!((0..=1200).all(|i| envelope(i as f32 / 10.0, 120.0, 0.0) == 1.0));
    }

    #[test]
    fn the_fade_moves_one_way() {
        let span = 120.0;
        let gains: Vec<f32> = (0..=1200).map(|i| envelope(i as f32 / 10.0, span, FADE_OUT)).collect();
        assert!(gains.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn a_cue_cut_short_by_leaving_still_fades() {
        assert!(envelope(119.0, 120.0, FADE_LEAVING) < 1.0);
        assert_eq!(envelope(120.0, 120.0, FADE_LEAVING), 0.0);
    }

    /// Every stage plays one kind of music.
    #[test]
    fn a_stage_has_one_kind() {
        for (i, a) in KINDS.iter().enumerate() {
            assert!(KINDS[i + 1..].iter().all(|b| b.stage != a.stage), "{} in {} shares its stage", a.style.name(), a.setting.name());
        }
    }

    /// Every stage has a track to compose: one of its style played in its
    /// setting.
    #[test]
    fn every_stage_has_a_track() {
        for kind in &KINDS {
            assert!(TRACKS.iter().any(|t| t.style == kind.style && t.plays_in(kind.setting) && kind.track.is_none_or(|n| t.name == n)), "no {} track is played in {}", kind.style.name(), kind.setting.name());
        }
    }
}
