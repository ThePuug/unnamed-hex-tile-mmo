//! A score to samples through a SoundFont, as `perform` plays it. Events
//! land on the block they fall in — a block is a couple of milliseconds.
//! Every player is a synthesizer of its own, its stem placed across the
//! stage here rather than by the synthesizer's pan, and a kit's drums
//! each where they sit (`Stem`). The synthesizer's own effects are off:
//! each stem goes to the dry mix and, at its reverb send, to the send
//! that rings the score's `hall` under it. Pure arithmetic over a
//! buffer: the same score and bank give the same samples.
//!
//! A bank is several SoundFonts: the default, which plays every
//! General MIDI program, and the files `voices` names for the programs
//! another plays better; a player's synthesizer is on the file that
//! seats it, a slurred note on the file's legato preset beside it.
//!
//! An instrument recorded at the jack sounds through its part's rig
//! (`rigs`, `amp`): the stem is gathered as the jack hears it, played
//! through the rig whole, and only then given its volume and pedal, so
//! how hard a note is struck drives the amp and a fade fades it. The
//! rig's sound is made once a take, however often the take renders.
//!
//! The bank is evened. Its samples are not one loudness across an
//! instrument's range — a horn steps up three decibels where one sample
//! hands over to the next — and where a lone player carries a part that
//! step is the part's loudness. Each note is struck at the velocity that
//! brings its pitch to the level its instrument has across its range, as
//! the bank sounds it: a short tone of every pitch, measured once — a
//! struck tone at its loudest moment, a sustained one at what it settles
//! to past its attack, since a bowed or blown tone is heard at its
//! sustain: measured at the bow or the breath, a fiddle and a harmonica
//! sat some four decibels under the band, and a slow pad, measured
//! before it was full, over it.
//!
//! The summed band goes through the mix bus (`master`) before it is
//! kept.
//!
//! A piece is played once and rings on for its room's time. Where its
//! sections declare levels, `take` sets their trims from the render
//! before the render that ships.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::amp;
use crate::hall;
use crate::master;
use crate::perform::{perform, Msg};
use crate::pieces::{Params, Track};
use crate::rigs::Rig;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, TICKS_PER_EIGHTH};
use crate::voices;

pub use audio::SAMPLE_RATE;
const BLOCK: usize = 64;


/// The room's level under the dry pass, for a send of 127.
const WET: f32 = 0.25;

/// A player's channel volume at level 0 in a MIDI file, which plays on
/// a bank nobody has measured: under the top, so a quiet sample can be
/// raised some six dB before the volume reaches 127.
const VOLUME: f32 = 90.0;

/// The channel volume a player's `level` is written as in a MIDI file:
/// the synthesizer squares volume into gain, so a gain in dB is a
/// quarter of it in forty on the volume. A render measures the bank
/// instead (`Bank::volumes`).
pub fn volume(level: f32) -> u8 {
    (VOLUME * 10f32.powf(level / 40.0)).round().min(127.0) as u8
}

fn open(path: &Path) -> Result<Arc<SoundFont>, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Arc::new(SoundFont::new(&mut file).map_err(|e| format!("{}: {e:?}", path.display()))?))
}

/// The velocity a level is stated at. A part is struck at the middle of
/// its own notes' velocities, since a bank's layers differ in level key
/// by key and the layer struck at a hundred is not the one heard at
/// seventy-five, and what it gives is brought to this velocity by the
/// synthesizer's own law (`law`), so a part written softer stays softer.
/// A tone is read over the length the part writes — the middle of its
/// notes' lengths, at least `READ_S` and at most `READ_TO_S` — from
/// `SETTLED_FROM_S` in where its voice sings, its attack past, so a
/// plucked part writing long notes is heard at its mean and not its
/// strike, and a reed is not read in its swell. A drum is read at its
/// loudest moment over `READ_S`. The eighth note a rig's delay is timed
/// by there.
const MEASURED_AT: u8 = 100;

/// The gain the synthesizer gives a note at `vel`, dB: twice the
/// velocity's share of the top in decibels (rustysynth, as SoundFont
/// players do), the one dynamic every bank is played with.
fn law(vel: u8) -> f32 {
    2.0 * 20.0 * (vel as f32 / 127.0).log10()
}
const READ_S: f64 = 0.4;
const READ_TO_S: f64 = 1.2;
const SETTLED_FROM_S: f64 = 0.5;
const MEASURED_EIGHTH_S: f64 = 0.2;

/// The most a note is evened by, dB either way: past it a pitch is a
/// sample the bank does not mean to be played.
const EVEN_DB: f32 = 6.0;

/// The loudness a player is taken to have where none of its keys sounds
/// on its own, LUFS: a General MIDI program's middling tone.
const MEASURED_FLOOR: f32 = -35.0;

/// The SoundFonts that play a score — the default first, then every file
/// `voices` names that sits beside it — and the loudness of every tone
/// each has been asked for. The default stays read; the others are read
/// when a render needs them and let go when none holds them.
pub struct Bank {
    fonts: Vec<Font>,
    /// The default, held for the bank's life so it is never read again.
    _default: Arc<SoundFont>,
    levels: Mutex<HashMap<Measured, f32>>,
}

/// A tone measured: its font, bank, preset, key and velocity, the window
/// it was read over (milliseconds from its strike), and the rig it
/// sounded through, by name.
type Measured = (usize, u8, u8, u8, u8, (u16, u16), &'static str);

/// A SoundFont of the bank, held only while a render holds it: the
/// sampled ones run to hundreds of megabytes each, and a player playing
/// piece after piece would otherwise keep every bank it ever seated.
struct Font {
    file: &'static str,
    path: PathBuf,
    font: Mutex<Weak<SoundFont>>,
}

/// Where an instrument is played: the font, its bank number and preset
/// there, and the voice that put it there, whose key map it plays by.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Seat {
    font: usize,
    bank: u8,
    preset: u8,
    voice: Option<&'static voices::Voice>,
}

impl Bank {
    /// Where the bank is: `given`, else `$SOUNDFONT`, else the first of
    /// these that exists: `GeneralUser.sf2` shipped with the program —
    /// beside the executable, or in the bundle's `Resources` on macOS,
    /// where the executable sits in `MacOS` — and
    /// `~/soundfonts/GeneralUser.sf2`.
    pub fn find(given: Option<PathBuf>) -> Option<PathBuf> {
        if let Some(p) = given {
            return Some(p);
        }
        if let Some(p) = std::env::var_os("SOUNDFONT") {
            return Some(PathBuf::from(p));
        }
        let shipped = std::env::current_exe().ok().and_then(|exe| {
            let dir = exe.parent()?;
            let dir = if cfg!(target_os = "macos") { dir.parent()?.join("Resources") } else { dir.to_path_buf() };
            Some(dir.join("GeneralUser.sf2"))
        });
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| Path::new(&h).join("soundfonts/GeneralUser.sf2"));
        [shipped, home].into_iter().flatten().find(|p| p.is_file())
    }

    /// The default bank at `path`, and every file `voices` names that sits
    /// beside it.
    pub fn load(path: &Path) -> Result<Self, String> {
        let read = open(path)?;
        let fonts = vec![Font { file: "", path: path.to_path_buf(), font: Mutex::new(Arc::downgrade(&read)) }];
        let bank = Bank { fonts, _default: read, levels: Mutex::new(HashMap::new()) };
        Ok(bank.with_banks(path.parent().unwrap_or(Path::new("."))))
    }

    /// The bank playing every file `voices` names that sits in `dir` from
    /// there, in place of where it was found before.
    pub fn with_banks(mut self, dir: &Path) -> Self {
        for v in voices::VOICES {
            let found = dir.join(v.file);
            if !found.is_file() {
                continue;
            }
            match self.fonts.iter_mut().find(|f| f.file == v.file) {
                Some(f) => *f = Font { file: v.file, path: found, font: Mutex::new(Weak::new()) },
                None => self.fonts.push(Font { file: v.file, path: found, font: Mutex::new(Weak::new()) }),
            }
        }
        self.levels.get_mut().unwrap().clear();
        self
    }

    /// How many of the files `voices` names the bank plays from.
    pub fn sampled(&self) -> usize {
        self.fonts.len() - 1
    }

    /// The files `voices` names that the bank found nowhere, whose
    /// programs the default plays.
    pub fn lacking(&self) -> Vec<&'static str> {
        let mut files: Vec<&'static str> = voices::VOICES.iter().map(|v| v.file).filter(|f| !self.fonts.iter().any(|font| font.file == *f)).collect();
        files.sort();
        files.dedup();
        files
    }

    /// The font at `index`, read now if nothing holds it.
    fn font(&self, index: usize) -> Arc<SoundFont> {
        let f = &self.fonts[index];
        let mut held = f.font.lock().unwrap();
        held.upgrade().unwrap_or_else(|| {
            let read = open(&f.path).unwrap_or_else(|e| panic!("{e}"));
            *held = Arc::downgrade(&read);
            read
        })
    }

    /// Every font `score` seats a player on, read and held for as long
    /// as the returned fonts are: a render holds them through its every
    /// pass, so none is read twice in one.
    fn hold(&self, score: &Score) -> Vec<Arc<SoundFont>> {
        let mut seated: Vec<usize> = score.instruments.iter().map(|i| self.seat(i).font).collect();
        seated.sort();
        seated.dedup();
        seated.into_iter().map(|i| self.font(i)).collect()
    }

    /// Where `inst` is played: on the bank `voices` names for its program
    /// where that bank was found, else on the default.
    fn seat(&self, inst: &Instrument) -> Seat {
        voices::voice(inst.program, inst.role == Role::Percussion)
            .and_then(|v| self.fonts.iter().position(|f| f.file == v.file).map(|font| Seat { font, bank: v.bank, preset: v.preset, voice: Some(v) }))
            .unwrap_or(Seat { font: 0, bank: 0, preset: inst.program, voice: None })
    }

    /// The rig `inst` plays through in `score`, where its seat's
    /// instrument was recorded at the jack.
    fn rig(&self, score: &Score, inst: &Instrument) -> Option<&'static Rig> {
        self.seat(inst).voice.filter(|v| v.direct).map(|_| score.rig(inst.channel))
    }

    /// The loudness of `seat`'s `pitch`, LUFS, as at `MEASURED_AT`: one
    /// tone struck at `vel`, dry and centred, on the key the seat strikes
    /// for it, through `rig` where there is one, driven as a stem drives
    /// it — read over `window`, seconds from its strike, or a drum at its
    /// loudest moment — less what the synthesizer's law gave `vel` over
    /// `MEASURED_AT`. A kit the default bank plays is on the drum channel.
    fn level(&self, seat: Seat, drums: bool, window: (f64, f64), pitch: u8, vel: u8, rig: Option<&'static Rig>) -> f32 {
        let struck = seat.voice.map_or(pitch, |v| voices::key(v, pitch, 0));
        let ms = |s: f64| (s * 1000.0).round() as u16;
        let key = (seat.font, seat.bank, seat.preset, struck, vel, (ms(window.0), ms(window.1)), rig.map_or("", |r| r.name));
        if let Some(l) = self.levels.lock().unwrap().get(&key) {
            return *l;
        }
        let mut settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        settings.block_size = BLOCK;
        settings.enable_reverb_and_chorus = false;
        let mut synth = Synthesizer::new(&self.font(seat.font), &settings).expect("a synthesizer");
        let ch = if drums && seat.voice.is_none() { 9 } else { 0 };
        if ch == 0 {
            synth.process_midi_message(ch, 0xB0, 0, seat.bank as i32);
        }
        synth.process_midi_message(ch, 0xC0, seat.preset as i32, 0);
        // At the top of the volume, as `volumes` places the part furthest
        // over its samples and as a rigged stem is driven at the jack: the
        // synthesizer's own default is a hundred, four decibels under.
        synth.process_midi_message(ch, 0xB0, 7, 127);
        synth.note_on(ch, struck as i32, vel as i32);
        let n = (window.1 * SAMPLE_RATE as f64) as usize / BLOCK;
        let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        let mut tone = Vec::with_capacity(n * BLOCK);
        for _ in 0..n {
            synth.render(&mut left, &mut right);
            tone.extend(left.iter().zip(&right).map(|(l, r)| [*l, *r]));
        }
        if let Some(rig) = rig {
            let jack: Vec<f32> = tone.iter().map(|[l, r]| (l + r) / 2.0).collect();
            tone = amp::play(rig, &jack, MEASURED_EIGHTH_S).into_iter().map(|(m, s)| [m + s, m - s]).collect();
        }
        let l = if drums { audio::measure::loudest_moment(&tone) } else { read(&tone, window).unwrap_or_else(|| audio::measure::loudest_moment(&tone)) };
        let l = l - (law(vel) - law(MEASURED_AT));
        self.levels.lock().unwrap().insert(key, l);
        l
    }

    /// The level of each key `inst` plays in `score`, and the middle of
    /// them, LUFS: every pitch of a melodic player's range, each once,
    /// struck at the middle of the velocities the part is written at and
    /// read over the middle of its notes' lengths; the drums a kit
    /// strikes, each a different instrument, counted once a stroke, so
    /// the middle is the drum that keeps the time whichever others a seed
    /// adds.
    fn levels(&self, score: &Score, inst: &Instrument) -> (Vec<(u8, f32)>, Option<f32>) {
        let seat = self.seat(inst);
        let rig = self.rig(score, inst);
        let drums = inst.role == Role::Percussion;
        let notes: Vec<&Note> = score.notes.iter().filter(|n| n.channel == inst.channel).collect();
        let middle = |mut v: Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.get(v.len() / 2).copied()
        };
        let vel = middle(notes.iter().map(|n| n.vel as f64).collect()).map_or(MEASURED_AT, |v| v as u8);
        let held = middle(notes.iter().map(|n| score.seconds(n.end()) - score.seconds(n.start)).collect()).unwrap_or(READ_S);
        let from = if voices::sings(inst.program) { SETTLED_FROM_S } else { 0.0 };
        let window = (from, (from + READ_S).max(held.min(READ_TO_S)));
        let counted: Vec<u8> = if drums { notes.iter().map(|n| n.pitch).collect() } else { (inst.low..=inst.high).collect() };
        let mut keys = counted.clone();
        keys.sort();
        keys.dedup();
        let levels: Vec<(u8, f32)> = keys.into_iter().map(|p| (p, self.level(seat, drums, window, p, vel, rig))).filter(|(_, l)| l.is_finite()).collect();
        let mut sorted: Vec<f32> = counted.iter().filter_map(|p| levels.iter().find(|(k, _)| k == p).map(|(_, l)| *l)).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let middle = sorted.get(sorted.len() / 2).copied();
        (levels, middle)
    }

    /// For each pitch in `inst`'s range, the dB that brings it to the
    /// middle of the range's levels, within `EVEN_DB`; none for a drum
    /// kit, whose keys are different drums.
    fn evening(&self, score: &Score, inst: &Instrument) -> HashMap<u8, f32> {
        let (levels, middle) = self.levels(score, inst);
        match middle {
            Some(middle) if inst.role != Role::Percussion => levels.into_iter().map(|(p, l)| (p, (middle - l).clamp(-EVEN_DB, EVEN_DB))).collect(),
            _ => HashMap::new(),
        }
    }

    /// Each player's channel volume: its `level` over what the bank's
    /// samples give it on their own, so a level is a loudness against the
    /// others' whatever bank plays them. The player furthest over its
    /// samples is at the top of the volume and every other under it.
    fn volumes(&self, score: &Score) -> HashMap<u8, u8> {
        let gains: Vec<(u8, f32)> = score.instruments.iter().map(|i| (i.channel, i.level - self.levels(score, i).1.unwrap_or(MEASURED_FLOOR))).collect();
        let top = gains.iter().map(|(_, g)| *g).fold(f32::NEG_INFINITY, f32::max);
        gains.into_iter().map(|(ch, g)| (ch, (127.0 * 10f32.powf((g - top) / 40.0)).round().max(1.0) as u8)).collect()
    }
}

/// What a tone gives over `window`, LUFS: the mean power of its momentary
/// loudness blocks starting within it; none where none does.
fn read(tone: &[[f32; 2]], window: (f64, f64)) -> Option<f32> {
    let (blocks, starts) = audio::measure::block_loudness(tone);
    let read: Vec<f32> = blocks.into_iter().zip(starts).filter(|(l, s)| (window.0..window.1).contains(&(*s as f64)) && l.is_finite()).map(|(l, _)| l).collect();
    if read.is_empty() {
        return None;
    }
    Some(10.0 * (read.iter().map(|l| 10f32.powf(l / 10.0)).sum::<f32>() / read.len() as f32).log10())
}

/// How long a piece runs on past the last sample over the silence
/// floor, seconds.
const RING_KEPT_S: f32 = 0.5;

/// `piece` as `params` compose it, as it plays: composed, its sections set to
/// their levels, rendered, set to its loudness in the setting and limited under
/// its ceiling, and cut `RING_KEPT_S` past the last sample over the
/// silence floor — the room's ring under that is no sound anyone hears.
pub fn take(piece: &Track, params: &Params, bank: &Bank) -> (Score, Vec<[f32; 2]>) {
    let mut score = (piece.build)(params);
    let _held = bank.hold(&score);
    let mut amped = Amped::default();
    let lufs = piece.lufs_in(params.setting);
    levelled(&mut score, bank, &mut amped, lufs);
    let mut audio = rendered(&score, bank, &mut amped);
    audio::encode::set_loudness(&mut audio, lufs);
    master::limit(&mut audio);
    let (_, tail) = audio::measure::silence(&audio);
    if tail > RING_KEPT_S {
        let cut = ((tail - RING_KEPT_S) * SAMPLE_RATE as f32) as usize;
        audio.truncate(audio.len() - cut);
    }
    (score, audio)
}

/// How near every levelled section must come to its declared level, LU,
/// and the most renders spent bringing it there.
const LEVEL_LU: f32 = 0.25;
const LEVEL_PASSES: usize = 6;

/// Sets the trim of every section that declares a level so the render
/// meets it: renders, brings the render to `lufs`, the loudness it ships
/// at, measures each levelled section's mean momentary loudness against
/// the section declaring the highest level, moves each trim by what it
/// misses, and renders again, until every one is within `LEVEL_LU` or the
/// passes run out; the levelled trims are then scaled together so the
/// highest is the full pedal. Measured at another loudness, a ring's
/// tail falls under the silence gate here and over it in the take, and
/// the take's ending sits under its level. What plays sets how the
/// loudness moves within a section; its level sets where it sits, so an
/// arc holds whatever the seed brings. Nothing moves where no section
/// declares a level.
fn levelled(score: &mut Score, bank: &Bank, amped: &mut Amped, lufs: f32) {
    let levelled: Vec<usize> = (0..score.sections.len()).filter(|i| score.sections[*i].level.is_some()).collect();
    let Some(&crest) = levelled.iter().max_by(|a, b| score.sections[**a].level.partial_cmp(&score.sections[**b].level).unwrap()) else {
        return;
    };
    let _held = bank.hold(score);
    for _ in 0..LEVEL_PASSES {
        let mut audio = rendered(score, bank, amped);
        audio::encode::set_loudness(&mut audio, lufs);
        let spans: Vec<(f32, f32)> = levelled.iter().map(|i| (score.seconds(score.sections[*i].start) as f32, score.seconds(score.sections[*i].end) as f32)).collect();
        let measured = audio::measure::spans(&audio, &spans);
        let at_crest = measured[levelled.iter().position(|i| *i == crest).unwrap()];
        let declared = |i: usize| score.sections[i].level.unwrap() - score.sections[crest].level.unwrap();
        let misses: Vec<f32> = levelled.iter().zip(&measured).map(|(i, m)| (m - at_crest) - declared(*i)).collect();
        if misses.iter().all(|m| m.abs() <= LEVEL_LU) {
            return;
        }
        for (i, miss) in levelled.iter().zip(&misses) {
            score.sections[*i].trim *= 10f32.powf(-miss / 20.0);
        }
        let top = levelled.iter().map(|i| score.sections[*i].trim).fold(0.0f32, f32::max);
        for i in &levelled {
            score.sections[*i].trim /= top;
        }
    }
}

/// Interleaved stereo f32 at `SAMPLE_RATE`: the score once, then on
/// for the room's time, the fall of 60 dB, past which is silence.
pub fn render(score: &Score, bank: &Bank) -> Vec<[f32; 2]> {
    rendered(score, bank, &mut Amped::default())
}

fn rendered(score: &Score, bank: &Bank, amped: &mut Amped) -> Vec<[f32; 2]> {
    let _held = bank.hold(score);
    let mut once = mixed(score, bank, (score.room as f64 * SAMPLE_RATE as f64) as usize, amped);
    master::master(&mut once);
    once
}

/// What each rig made of what reached it, by a hash of that and the rig
/// and the tempo, kept through one take's renders: a rigged stem's
/// volume and pedal act after its rig, so setting the sections' levels
/// changes nothing the rig hears, and its sound is made once.
#[derive(Default)]
struct Amped(HashMap<u64, Vec<(f32, f32)>>);

impl Amped {
    fn play(&mut self, rig: &'static Rig, jack: &[f32], eighth_s: f64) -> &[(f32, f32)] {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        rig.name.hash(&mut h);
        eighth_s.to_bits().hash(&mut h);
        jack.iter().for_each(|v| v.to_bits().hash(&mut h));
        self.0.entry(h.finish()).or_insert_with(|| amp::play(rig, jack, eighth_s))
    }
}

/// The score once in its room, then `tail` samples more.
fn mixed(score: &Score, bank: &Bank, tail: usize, amped: &mut Amped) -> Vec<[f32; 2]> {
    let played = perform(score);
    let (mut out, send) = through(score, &played, bank, tail, amped);
    let room = hall::ring(&send, score.room);
    for (o, r) in out.iter_mut().zip(room) {
        o[0] += WET * r[0];
        o[1] += WET * r[1];
    }
    out
}

/// Where a kit's drums sit across the stage, against the kit's own seat,
/// as `Instrument::pan` counts: from the drummer's stool, the hats on the
/// left, the toms from the high on the left to the floor on the right,
/// a crash either side, the ride and the china on the right; the kick,
/// the snare and every drum not named in the middle. A kit seated whole
/// on one point is one drum the size of the stage.
const DRUM_SEATS: &[(u8, i8)] = &[(42, -22), (44, -22), (46, -22), (50, -28), (48, -16), (47, -6), (45, 8), (43, 22), (41, 34), (49, -34), (55, -20), (57, 34), (51, 26), (53, 26), (59, 26), (52, 42)];

/// Where `inst`'s note on `pitch` sits across the stage, -1 hard left to 1
/// hard right: the player's seat, and a kit's drum its own.
fn seat_of(inst: &Instrument, pitch: u8) -> f32 {
    let drum = if inst.role == Role::Percussion { DRUM_SEATS.iter().find(|(k, _)| *k == pitch).map_or(0, |(_, s)| *s) } else { 0 };
    ((inst.pan as i32 + drum as i32) as f32 / 63.0).clamp(-1.0, 1.0)
}

/// A player as the synthesizer sounds it, a stereo stem — or one drum of
/// a kit, or the drums sharing a seat — placed across the stage: its
/// middle panned at constant power, its width narrowed as it nears a
/// side, so a stem in the middle is as the bank recorded it and one at a
/// side is all there. A bank's stereo sample panned by the synthesizer
/// leaves its far side near the middle, and a guitar meant for one side
/// sounds from both.
struct Stem {
    synth: Synthesizer,
    /// The synthesizer's channel the player is on: the drum channel for a
    /// kit the default bank plays, else the first.
    channel: i32,
    /// The channel after it, on the instrument's legato preset, where it
    /// has one: a slurred note is struck there, every controller and bend
    /// reaching both.
    legato: Option<i32>,
    /// The gains on the stem's middle, left and right, and on its side.
    mid: (f32, f32),
    side: f32,
    /// The reverb send, as a gain.
    send: f32,
    /// Where the player's instrument sounds through a rig: what reaches
    /// it, and the stem's volume and pedal, which act after it.
    rigged: Option<Rigged>,
}

/// A stem played through a rig. The amp hears the player's hands: how
/// hard a note is struck drives it, and the channel's volume and pedal
/// come after it, as a fader and a volume pedal do — a fade is the amp
/// fading, not the guitar played softer into it.
struct Rigged {
    rig: &'static Rig,
    /// The stem's middle at the jack, every block.
    jack: Vec<f32>,
    /// The volume and pedal, controller values, and the gain they make
    /// at each block's end.
    volume: u8,
    pedal: u8,
    gains: Vec<f32>,
}

impl Rigged {
    /// The synthesizer's gain for its volume and pedal: each a share of
    /// its 14-bit top, their product squared.
    fn gain(&self) -> f32 {
        let share = |v: u8| (v as f32 * 128.0) / 16383.0;
        (share(self.volume) * share(self.pedal)).powi(2)
    }
}

impl Stem {
    fn new(font: &Arc<SoundFont>, seat: Seat, drums: bool, place: f32, volume: u8, reverb: u8, rig: Option<&'static Rig>) -> Stem {
        let mut settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        settings.block_size = BLOCK;
        settings.enable_reverb_and_chorus = false;
        let mut synth = Synthesizer::new(font, &settings).expect("a synthesizer");
        let channel = if drums && seat.voice.is_none() { 9 } else { 0 };
        if channel != 9 {
            synth.process_midi_message(channel, 0xB0, 0, seat.bank as i32);
        }
        synth.process_midi_message(channel, 0xC0, seat.preset as i32, 0);
        synth.process_midi_message(channel, 0xB0, 7, if rig.is_some() { 127 } else { volume as i32 });
        let legato = seat.voice.and_then(|v| v.legato).filter(|_| channel != 9).map(|preset| {
            let ch = channel + 1;
            synth.process_midi_message(ch, 0xB0, 0, seat.bank as i32);
            synth.process_midi_message(ch, 0xC0, preset as i32, 0);
            synth.process_midi_message(ch, 0xB0, 7, if rig.is_some() { 127 } else { volume as i32 });
            ch
        });
        let angle = (place + 1.0) * std::f32::consts::FRAC_PI_4;
        Stem {
            synth,
            channel,
            legato,
            mid: (std::f32::consts::SQRT_2 * angle.cos(), std::f32::consts::SQRT_2 * angle.sin()),
            side: 1.0 - place.abs(),
            send: reverb as f32 / 127.0,
            rigged: rig.map(|rig| Rigged { rig, jack: Vec::new(), volume, pedal: 127, gains: Vec::new() }),
        }
    }
}

/// The score as `played`, then `tail` samples more, dry and as the
/// reverb send hears it: every player on a synthesizer of its own, on
/// the font that seats it, each stem placed and summed.
fn through(score: &Score, played: &[crate::perform::Played], bank: &Bank, tail: usize, amped: &mut Amped) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let volumes = bank.volumes(score);
    let mut stems: Vec<Stem> = Vec::new();
    // Each channel's stems, by the key each note sounds on: a kit's drums
    // sharing a seat share a stem, so a hat's foot still closes its open
    // stroke.
    let mut route: HashMap<u8, (Seat, Vec<(u8, usize)>)> = HashMap::new();
    let mut evening: HashMap<u8, HashMap<u8, f32>> = HashMap::new();
    for inst in &score.instruments {
        let seat = bank.seat(inst);
        let rig = bank.rig(score, inst);
        let drums = inst.role == Role::Percussion;
        let font = bank.font(seat.font);
        let mut keys: Vec<u8> = score.notes.iter().filter(|n| n.channel == inst.channel).map(|n| n.pitch).collect();
        keys.sort();
        keys.dedup();
        let mut seats: Vec<(f32, usize)> = Vec::new();
        let mut by_key = Vec::new();
        for key in keys {
            let place = if drums { seat_of(inst, key) } else { seat_of(inst, 0) };
            let stem = match seats.iter().find(|(p, _)| *p == place) {
                Some((_, s)) => *s,
                None => {
                    stems.push(Stem::new(&font, seat, drums, place, volumes[&inst.channel], inst.reverb, rig));
                    seats.push((place, stems.len() - 1));
                    stems.len() - 1
                }
            };
            by_key.push((key, stem));
        }
        route.insert(inst.channel, (seat, by_key));
        evening.insert(inst.channel, bank.evening(score, inst));
    }
    // The synthesizer gains a velocity as its square, forty log ten of
    // it in dB.
    let even = |channel: u8, pitch: u8, vel: u8| -> i32 {
        let db = evening.get(&channel).and_then(|e| e.get(&pitch)).copied().unwrap_or(0.0);
        (vel as f32 * 10f32.powf(db / 40.0)).round().clamp(1.0, 127.0) as i32
    };

    let at = |seconds: f64| (seconds * SAMPLE_RATE as f64).round() as u64;
    let total = at(score.seconds(score.end())) as usize + tail;
    let mut dry = Vec::with_capacity(total);
    let mut wet = Vec::with_capacity(total);
    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    let mut next = 0;
    let mut sample = 0usize;
    // Each note's last recording and the key it was struck on: a stroke
    // takes any other, drawn by when it lands and whose it is, so two
    // players striking together strike two takes; its release goes to the
    // key it struck.
    let mut struck: HashMap<(u8, u8), (u8, i32)> = HashMap::new();
    while sample < total {
        while next < played.len() && at(played[next].at) as usize <= sample {
            let channel = match played[next].msg {
                Msg::On { channel, .. } | Msg::Off { channel, .. } | Msg::Control { channel, .. } | Msg::Bend { channel, .. } => channel,
            };
            let Some((seat, by_key)) = route.get(&channel) else {
                next += 1;
                continue;
            };
            let stem_of = |pitch: u8| by_key.iter().find(|(k, _)| *k == pitch).map(|(_, s)| *s);
            match played[next].msg {
                Msg::On { channel, pitch, vel, legato } => {
                    let vel = even(channel, pitch, vel);
                    let (key, vel) = match seat.voice {
                        Some(v) if v.takes.count() > 1 => {
                            let takes = v.takes.count();
                            let last = struck.get(&(channel, pitch)).map_or(0, |(n, _)| *n);
                            let n = (last + 1 + Rng::new(sample as u64 ^ (channel as u64) << 48 ^ (pitch as u64) << 40).below(takes as usize - 1) as u8) % takes;
                            let key = voices::key(v, pitch, n) as i32;
                            struck.insert((channel, pitch), (n, key));
                            (key, voices::velocity(v, vel as u8, n) as i32)
                        }
                        Some(v) => (voices::key(v, pitch, 0) as i32, vel),
                        None => (pitch as i32, vel),
                    };
                    if let Some(s) = stem_of(pitch) {
                        let stem = &mut stems[s];
                        let on = if legato { stem.legato.unwrap_or(stem.channel) } else { stem.channel };
                        stem.synth.note_on(on, key, vel);
                    }
                }
                Msg::Off { channel, pitch } => {
                    let key = struck.get(&(channel, pitch)).map_or_else(|| seat.voice.map_or(pitch, |v| voices::key(v, pitch, 0)) as i32, |(_, k)| *k);
                    if let Some(s) = stem_of(pitch) {
                        let stem = &mut stems[s];
                        for ch in std::iter::once(stem.channel).chain(stem.legato) {
                            stem.synth.note_off(ch, key);
                        }
                    }
                }
                // A controller reaches every stem of the player; a rigged
                // stem's volume and pedal wait for its rig.
                Msg::Control { number, value, .. } => {
                    for (_, s) in by_key {
                        let stem = &mut stems[*s];
                        match (&mut stem.rigged, number) {
                            (Some(r), 7) => r.volume = value,
                            (Some(r), 11) => r.pedal = value,
                            _ => {
                                for ch in std::iter::once(stem.channel).chain(stem.legato) {
                                    stem.synth.process_midi_message(ch, 0xB0, number as i32, value as i32);
                                }
                            }
                        }
                    }
                }
                Msg::Bend { value, .. } => {
                    for (_, s) in by_key {
                        let stem = &mut stems[*s];
                        for ch in std::iter::once(stem.channel).chain(stem.legato) {
                            stem.synth.process_midi_message(ch, 0xE0, (value & 0x7F) as i32, (value >> 7) as i32);
                        }
                    }
                }
            }
            next += 1;
        }
        let from = dry.len();
        dry.resize(from + BLOCK, [0.0f32; 2]);
        wet.resize(from + BLOCK, [0.0f32; 2]);
        for stem in &mut stems {
            stem.synth.render(&mut left, &mut right);
            if let Some(r) = &mut stem.rigged {
                r.jack.extend(left.iter().zip(&right).map(|(l, r)| (l + r) / 2.0));
                let gain = r.gain();
                r.gains.push(gain);
                continue;
            }
            for i in 0..BLOCK {
                let (m, s) = ((left[i] + right[i]) / 2.0, (left[i] - right[i]) / 2.0);
                let (l, r) = (stem.mid.0 * m + stem.side * s, stem.mid.1 * m - stem.side * s);
                dry[from + i][0] += l;
                dry[from + i][1] += r;
                wet[from + i][0] += stem.send * l;
                wet[from + i][1] += stem.send * r;
            }
        }
        sample += BLOCK;
    }
    // Each rigged stem through its rig, then its volume and pedal, eased
    // across each block as the synthesizer eases its own, then placed.
    let eighth_s = score.seconds(TICKS_PER_EIGHTH) - score.seconds(0);
    for stem in &stems {
        let Some(r) = &stem.rigged else { continue };
        let out = amped.play(r.rig, &r.jack, eighth_s);
        for (b, gain) in r.gains.iter().enumerate() {
            let before = if b == 0 { *gain } else { r.gains[b - 1] };
            for i in 0..BLOCK {
                let at = b * BLOCK + i;
                let g = before + (gain - before) * (i + 1) as f32 / BLOCK as f32;
                let (m, s) = (g * out[at].0, g * out[at].1);
                let (l, r) = (stem.mid.0 * m + stem.side * s, stem.mid.1 * m - stem.side * s);
                dry[at][0] += l;
                dry[at][1] += r;
                wet[at][0] += stem.send * l;
                wet[at][1] += stem.send * r;
            }
        }
    }
    dry.truncate(total);
    wet.truncate(total);
    (dry, wet)
}
