//! A score to samples through a SoundFont, as `perform` plays it. Events
//! land on the block they fall in — a block is a couple of milliseconds.
//! The synthesizer's own
//! effects are off: the score is played twice, once dry and once with
//! each channel at its reverb send, and the second pass rings the
//! score's `hall` under the first. Pure arithmetic over a buffer: the
//! same score and bank give the same samples.
//!
//! The bank is evened. Its samples are not one loudness across an
//! instrument's range — a horn steps up three decibels where one sample
//! hands over to the next — and where a lone player carries a part that
//! step is the part's loudness. Each note is struck at the velocity that
//! brings its pitch to the level its instrument has across its range, as
//! the bank sounds it: a short tone of every pitch, measured once.
//!
//! A loop is played through twice and the second pass kept, so its head
//! already carries the tail's reverb and held tones; its last blocks are
//! blended into the first pass's last blocks, which the head follows
//! sample for sample, so the seam is no sharper than the render itself.
//! A one-shot is played once and rings on for its room's time. Where its
//! sections declare levels, `set_levels` sets their trims from the
//! render before the render that ships.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::hall;
use crate::perform::{perform, Msg};
use crate::pieces::{Params, Piece};
use crate::score::{Instrument, Role, Score};

pub use audio::SAMPLE_RATE;
const BLOCK: usize = 64;


/// The room's level under the dry pass, for a send of 127.
const WET: f32 = 0.25;

/// A player's channel volume at level 0: under the top, so a quiet
/// sample can be raised some six dB before the volume reaches 127.
const VOLUME: f32 = 90.0;

/// The seam's blend, in samples: long enough that the two passes' phases
/// meet without a click, short enough that neither pass is heard twice.
const SEAM: usize = SAMPLE_RATE as usize * 4 / 100;

/// The channel volume a player's `level` is sent as: the synthesizer
/// squares volume into gain, so a gain in dB is a quarter of it in
/// forty on the volume.
pub fn volume(level: f32) -> u8 {
    (VOLUME * 10f32.powf(level / 40.0)).round().min(127.0) as u8
}

/// The velocity a pitch is measured at, and how long its tone sounds.
const MEASURED_AT: i32 = 100;
const MEASURED_S: f64 = 0.6;

/// The most a note is evened by, dB either way: past it a pitch is a
/// sample the bank does not mean to be played.
const EVEN_DB: f32 = 6.0;

/// A SoundFont, and the loudness of every tone it has been asked for.
pub struct Bank {
    font: Arc<SoundFont>,
    levels: Mutex<HashMap<(u8, u8), f32>>,
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

    pub fn load(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let sf = SoundFont::new(&mut file).map_err(|e| format!("{}: {e:?}", path.display()))?;
        Ok(Bank { font: Arc::new(sf), levels: Mutex::new(HashMap::new()) })
    }

    /// The loudness of `program`'s `pitch`, LUFS at its loudest moment:
    /// one tone at `MEASURED_AT`, dry and centred.
    fn level(&self, program: u8, pitch: u8) -> f32 {
        if let Some(l) = self.levels.lock().unwrap().get(&(program, pitch)) {
            return *l;
        }
        let mut settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
        settings.block_size = BLOCK;
        settings.enable_reverb_and_chorus = false;
        let mut synth = Synthesizer::new(&self.font, &settings).expect("a synthesizer");
        synth.process_midi_message(0, 0xC0, program as i32, 0);
        synth.note_on(0, pitch as i32, MEASURED_AT);
        let n = (MEASURED_S * SAMPLE_RATE as f64) as usize / BLOCK;
        let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        let mut tone = Vec::with_capacity(n * BLOCK);
        for _ in 0..n {
            synth.render(&mut left, &mut right);
            tone.extend(left.iter().zip(&right).map(|(l, r)| [*l, *r]));
        }
        let l = audio::measure::loudest_moment(&tone);
        self.levels.lock().unwrap().insert((program, pitch), l);
        l
    }

    /// For each pitch in `inst`'s range, the dB that brings it to the
    /// middle of the range's levels, within `EVEN_DB`; none for a drum
    /// kit, whose keys are different drums.
    fn evening(&self, inst: &Instrument) -> HashMap<u8, f32> {
        if inst.role == Role::Percussion {
            return HashMap::new();
        }
        let levels: Vec<(u8, f32)> = (inst.low..=inst.high).map(|p| (p, self.level(inst.program, p))).filter(|(_, l)| l.is_finite()).collect();
        let mut sorted: Vec<f32> = levels.iter().map(|(_, l)| *l).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let Some(middle) = sorted.get(sorted.len() / 2).copied() else {
            return HashMap::new();
        };
        levels.into_iter().map(|(p, l)| (p, (middle - l).clamp(-EVEN_DB, EVEN_DB))).collect()
    }
}

/// How long a one-shot runs on past the last sample over the silence
/// floor, seconds.
const RING_KEPT_S: f32 = 0.5;

/// `piece` at `seed` as its file sounds: composed, its sections set to
/// their levels, rendered, set to the piece's loudness, and a one-shot
/// cut `RING_KEPT_S` past the last sample over the silence floor — the
/// room's ring under that is no sound anyone hears.
pub fn take(piece: &Piece, seed: u64, bank: &Bank) -> (Score, Vec<[f32; 2]>) {
    let mut score = (piece.build)(&Params { seed });
    set_levels(&mut score, bank);
    let mut audio = render(&score, bank);
    audio::encode::set_loudness(&mut audio, piece.lufs);
    if !score.loops {
        let (_, tail) = audio::measure::silence(&audio);
        if tail > RING_KEPT_S {
            let cut = ((tail - RING_KEPT_S) * SAMPLE_RATE as f32) as usize;
            audio.truncate(audio.len() - cut);
        }
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

/// The block that carries `tick`'s events: the first that starts at or
/// after its sample.
fn block_at(score: &Score, tick: u32) -> usize {
    ((score.seconds(tick) * SAMPLE_RATE as f64).round() as usize).div_ceil(BLOCK) * BLOCK
}

/// Interleaved stereo f32 at `SAMPLE_RATE`: the score once with its
/// tail, or, for a loop, exactly one pass whose end runs into its head.
pub fn render(score: &Score, bank: &Bank) -> Vec<[f32; 2]> {
    // A one-shot rings on past its last section for the room's time, the
    // fall of 60 dB, past which is silence; a loop's tails ring into its
    // head.
    if !score.loops {
        return mixed(score, bank, score.end(), (score.room as f64 * SAMPLE_RATE as f64) as usize);
    }
    let twice = score.unrolled(2);
    let all = mixed(&twice, bank, score.end(), BLOCK);
    let (a, b) = (block_at(score, score.end()), block_at(&twice, twice.end()));
    let mut out = all[a..b].to_vec();
    let n = SEAM.min(a).min(out.len());
    let into = &all[a - n..a];
    let len = out.len();
    for i in 0..n {
        let theta = (i + 1) as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
        let (keep, take) = (theta.cos(), theta.sin());
        for ch in 0..2 {
            out[len - n + i][ch] = out[len - n + i][ch] * keep + into[i][ch] * take;
        }
    }
    out
}

/// The score once in its room, then `tail` samples more; its playing
/// repeats every `period` ticks.
fn mixed(score: &Score, bank: &Bank, period: u32, tail: usize) -> Vec<[f32; 2]> {
    let played = perform(score, period);
    let mut out = through(score, &played, bank, tail, false);
    let room = hall::ring(&through(score, &played, bank, tail, true), score.room);
    for (o, r) in out.iter_mut().zip(room) {
        o[0] += WET * r[0];
        o[1] += WET * r[1];
    }
    out
}

/// The score as `played`, then `tail` samples more: dry, or as the
/// reverb send hears it.
fn through(score: &Score, played: &[crate::perform::Played], bank: &Bank, tail: usize, send: bool) -> Vec<[f32; 2]> {
    let mut settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
    settings.block_size = BLOCK;
    settings.enable_reverb_and_chorus = false;
    let mut synth = Synthesizer::new(&bank.font, &settings).expect("a synthesizer");
    let evening: HashMap<u8, HashMap<u8, f32>> = score.instruments.iter().map(|i| (i.channel, bank.evening(i))).collect();
    // The synthesizer gains a velocity as its square, forty log ten of
    // it in dB.
    let even = |channel: u8, pitch: u8, vel: u8| -> i32 {
        let db = evening.get(&channel).and_then(|e| e.get(&pitch)).copied().unwrap_or(0.0);
        (vel as f32 * 10f32.powf(db / 40.0)).round().clamp(1.0, 127.0) as i32
    };
    for inst in &score.instruments {
        synth.process_midi_message(inst.channel as i32, 0xC0, inst.program as i32, 0);
        synth.process_midi_message(inst.channel as i32, 0xB0, 10, 64 + inst.pan as i32);
        // The synthesizer squares volume into gain, so the send's gain
        // is its root on the volume.
        let send = if send { (inst.reverb as f32 / 127.0).sqrt() } else { 1.0 };
        synth.process_midi_message(inst.channel as i32, 0xB0, 7, (volume(inst.level) as f32 * send).round() as i32);
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
            match played[next].msg {
                Msg::On { channel, pitch, vel } => synth.note_on(channel as i32, pitch as i32, even(channel, pitch, vel)),
                Msg::Off { channel, pitch } => synth.note_off(channel as i32, pitch as i32),
                Msg::Control { channel, number, value } => synth.process_midi_message(channel as i32, 0xB0, number as i32, value as i32),
                Msg::Bend { channel, value } => synth.process_midi_message(channel as i32, 0xE0, (value & 0x7F) as i32, (value >> 7) as i32),
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
