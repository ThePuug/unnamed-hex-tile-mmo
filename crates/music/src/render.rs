//! A score to samples through a SoundFont, as `perform` plays it. Events
//! land on the block they fall in — a block is a couple of milliseconds.
//! The synthesizer's own
//! effects are off: the score is played twice, once dry and once with
//! each channel at its reverb send, and the second pass rings the
//! score's `hall` under the first. Pure arithmetic over a buffer: the
//! same score and bank give the same samples.
//!
//! A bank is several SoundFonts: the default, which plays every
//! General MIDI program, and the files `voices` names for the programs
//! another plays better. Each file is its own synthesizer, playing the
//! channels whose instruments it holds, and their outputs are summed.
//!
//! The bank is evened. Its samples are not one loudness across an
//! instrument's range — a horn steps up three decibels where one sample
//! hands over to the next — and where a lone player carries a part that
//! step is the part's loudness. Each note is struck at the velocity that
//! brings its pitch to the level its instrument has across its range, as
//! the bank sounds it: a short tone of every pitch, measured once.
//!
//! The summed band goes through the mix bus (`master`) before it is
//! kept.
//!
//! A piece is played once and rings on for its room's time. Where its
//! sections declare levels, `set_levels` sets their trims from the
//! render before the render that ships.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::hall;
use crate::master;
use crate::perform::{perform, Msg};
use crate::pieces::{Params, Piece};
use crate::score::{Instrument, Role, Score};
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

/// The velocity a pitch is measured at, and how long its tone sounds.
const MEASURED_AT: i32 = 100;
const MEASURED_S: f64 = 0.6;

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
    levels: Mutex<HashMap<(usize, u8, u8, u8), f32>>,
}

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

    /// The loudness of `seat`'s `pitch`, LUFS at its loudest moment: one
    /// tone at `MEASURED_AT`, dry and centred, on the key the seat strikes
    /// for it. A kit the default bank plays is on the drum channel.
    fn level(&self, seat: Seat, drums: bool, pitch: u8) -> f32 {
        let struck = seat.voice.map_or(pitch, |v| voices::key(v, pitch));
        let key = (seat.font, seat.bank, seat.preset, struck);
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
        synth.note_on(ch, struck as i32, MEASURED_AT);
        let n = (MEASURED_S * SAMPLE_RATE as f64) as usize / BLOCK;
        let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        let mut tone = Vec::with_capacity(n * BLOCK);
        for _ in 0..n {
            synth.render(&mut left, &mut right);
            tone.extend(left.iter().zip(&right).map(|(l, r)| [*l, *r]));
        }
        let l = audio::measure::loudest_moment(&tone);
        self.levels.lock().unwrap().insert(key, l);
        l
    }

    /// The level of each key `inst` plays in `score`, and the middle of
    /// them, LUFS: every pitch of a melodic player's range; the drums a
    /// kit strikes, each a different instrument.
    fn levels(&self, score: &Score, inst: &Instrument) -> (Vec<(u8, f32)>, Option<f32>) {
        let seat = self.seat(inst);
        let drums = inst.role == Role::Percussion;
        let mut keys: Vec<u8> = if drums { score.notes.iter().filter(|n| n.channel == inst.channel).map(|n| n.pitch).collect() } else { (inst.low..=inst.high).collect() };
        keys.sort();
        keys.dedup();
        let levels: Vec<(u8, f32)> = keys.into_iter().map(|p| (p, self.level(seat, drums, p))).filter(|(_, l)| l.is_finite()).collect();
        let mut sorted: Vec<f32> = levels.iter().map(|(_, l)| *l).collect();
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

/// How long a piece runs on past the last sample over the silence
/// floor, seconds.
const RING_KEPT_S: f32 = 0.5;

/// `piece` at `seed` as its file sounds: composed, its sections set to
/// their levels, rendered, set to the piece's loudness and limited under
/// its ceiling, and cut `RING_KEPT_S` past the last sample over the
/// silence floor — the room's ring under that is no sound anyone hears.
pub fn take(piece: &Piece, seed: u64, bank: &Bank) -> (Score, Vec<[f32; 2]>) {
    let mut score = (piece.build)(&Params { seed });
    let _held = bank.hold(&score);
    set_levels(&mut score, bank);
    let mut audio = render(&score, bank);
    audio::encode::set_loudness(&mut audio, piece.lufs);
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
/// meets it: renders, measures each levelled section's mean momentary
/// loudness against the section declaring the highest level, moves each
/// trim by what it misses, and renders again, until every one is within
/// `LEVEL_LU` or the passes run out; the levelled trims are then scaled
/// together so the highest is the full pedal. What plays sets how the
/// loudness moves within a section; its level sets where it sits, so an
/// arc holds whatever the seed brings. Nothing moves where no section
/// declares a level.
pub fn set_levels(score: &mut Score, bank: &Bank) {
    let levelled: Vec<usize> = (0..score.sections.len()).filter(|i| score.sections[*i].level.is_some()).collect();
    let Some(&crest) = levelled.iter().max_by(|a, b| score.sections[**a].level.partial_cmp(&score.sections[**b].level).unwrap()) else {
        return;
    };
    let _held = bank.hold(score);
    for _ in 0..LEVEL_PASSES {
        let audio = render(score, bank);
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
    let _held = bank.hold(score);
    let mut once = mixed(score, bank, (score.room as f64 * SAMPLE_RATE as f64) as usize);
    master::master(&mut once);
    once
}

/// The score once in its room, then `tail` samples more.
fn mixed(score: &Score, bank: &Bank, tail: usize) -> Vec<[f32; 2]> {
    let played = perform(score);
    let mut out = through(score, &played, bank, tail, false);
    let room = hall::ring(&through(score, &played, bank, tail, true), score.room);
    for (o, r) in out.iter_mut().zip(room) {
        o[0] += WET * r[0];
        o[1] += WET * r[1];
    }
    out
}

/// The score as `played`, then `tail` samples more: dry, or as the
/// reverb send hears it; each font of the bank playing the channels it
/// seats, their outputs summed.
fn through(score: &Score, played: &[crate::perform::Played], bank: &Bank, tail: usize, send: bool) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::new();
    for font in 0..bank.fonts.len() {
        let seated: Vec<&Instrument> = score.instruments.iter().filter(|i| bank.seat(i).font == font).collect();
        if seated.is_empty() {
            continue;
        }
        let part = through_font(score, played, bank, font, &seated, tail, send);
        if out.is_empty() {
            out = part;
        } else {
            for (o, p) in out.iter_mut().zip(part) {
                o[0] += p[0];
                o[1] += p[1];
            }
        }
    }
    out
}

/// The channels of `seated` as `played`, on `font`.
#[allow(clippy::too_many_arguments)]
fn through_font(score: &Score, played: &[crate::perform::Played], bank: &Bank, font: usize, seated: &[&Instrument], tail: usize, send: bool) -> Vec<[f32; 2]> {
    let mut settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
    settings.block_size = BLOCK;
    settings.enable_reverb_and_chorus = false;
    let mut synth = Synthesizer::new(&bank.font(font), &settings).expect("a synthesizer");
    // A kit another bank files as a melodic preset plays on a melodic
    // channel of its own synthesizer, which holds no other player there.
    let free = (0..16u8).find(|c| *c != 9 && !seated.iter().any(|i| i.channel == *c)).unwrap_or(15);
    let route: HashMap<u8, (u8, Seat)> = seated
        .iter()
        .map(|i| {
            let seat = bank.seat(i);
            let melodic_kit = i.role == Role::Percussion && seat.voice.is_some_and(|v| v.bank < 128);
            (i.channel, (if melodic_kit { free } else { i.channel }, seat))
        })
        .collect();
    let evening: HashMap<u8, HashMap<u8, f32>> = seated.iter().map(|i| (i.channel, bank.evening(score, i))).collect();
    let volumes = bank.volumes(score);
    // The synthesizer gains a velocity as its square, forty log ten of
    // it in dB.
    let even = |channel: u8, pitch: u8, vel: u8| -> i32 {
        let db = evening.get(&channel).and_then(|e| e.get(&pitch)).copied().unwrap_or(0.0);
        (vel as f32 * 10f32.powf(db / 40.0)).round().clamp(1.0, 127.0) as i32
    };
    for inst in seated {
        let (ch, seat) = route[&inst.channel];
        let ch = ch as i32;
        if seat.font != 0 {
            synth.process_midi_message(ch, 0xB0, 0, seat.bank as i32);
        }
        synth.process_midi_message(ch, 0xC0, seat.preset as i32, 0);
        synth.process_midi_message(ch, 0xB0, 10, 64 + inst.pan as i32);
        // The synthesizer squares volume into gain, so the send's gain
        // is its root on the volume.
        let send = if send { (inst.reverb as f32 / 127.0).sqrt() } else { 1.0 };
        synth.process_midi_message(ch, 0xB0, 7, (volumes[&inst.channel] as f32 * send).round() as i32);
    }

    let at = |seconds: f64| (seconds * SAMPLE_RATE as f64).round() as u64;
    let total = at(score.seconds(score.end())) as usize + tail;
    let mut out = Vec::with_capacity(total);
    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    let mut next = 0;
    let mut sample = 0usize;
    while sample < total {
        while next < played.len() && at(played[next].at) as usize <= sample {
            let channel = match played[next].msg {
                Msg::On { channel, .. } | Msg::Off { channel, .. } | Msg::Control { channel, .. } | Msg::Bend { channel, .. } => channel,
            };
            let Some((ch, seat)) = route.get(&channel) else {
                next += 1;
                continue;
            };
            let (ch, key) = (*ch as i32, |pitch: u8| seat.voice.map_or(pitch, |v| voices::key(v, pitch)) as i32);
            match played[next].msg {
                Msg::On { channel, pitch, vel } => synth.note_on(ch, key(pitch), even(channel, pitch, vel)),
                Msg::Off { pitch, .. } => synth.note_off(ch, key(pitch)),
                Msg::Control { number, value, .. } => synth.process_midi_message(ch, 0xB0, number as i32, value as i32),
                Msg::Bend { value, .. } => synth.process_midi_message(ch, 0xE0, (value & 0x7F) as i32, (value >> 7) as i32),
            }
            next += 1;
        }
        synth.render(&mut left, &mut right);
        for i in 0..BLOCK {
            out.push([left[i], right[i]]);
        }
        sample += BLOCK;
    }
    out.truncate(total);
    out
}
