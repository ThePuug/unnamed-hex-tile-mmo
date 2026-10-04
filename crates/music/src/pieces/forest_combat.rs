//! The forest's fight: the overworld's dances taken at a run. What
//! makes a fight's music hit is its rhythm before its tune: a driving
//! ostinato under everything, the drum as its floor, low brass striking
//! the accents and pushing off the beat, a minor second in the harmony,
//! and a piercing lead over it; its intensity is layers, not tempo, and
//! a breakdown is the calm the next blow lands out of. So the bed here
//! is an ostinato of strings on every eighth, the davul's strokes on
//! the dance's groups, trombone and tuba stabbing the strokes, a choir
//! and a string pad where it thickens and a pizzicato echo of the tune,
//! with no drone under it: the harmony moves, to the minor's flat sixth
//! or Hijaz's flat second, and the tune is told by one lead, mostly as
//! a riff. Every choice is a draw from the seed's stream, one fork per
//! purpose; the seed picks which shape at every level, never the next
//! note.

use crate::ladder::{self, turn, Bed, Story, Walk};
use crate::pieces::Params;
use crate::rng::Rng;
use crate::score::{Instrument, Note, Role, Score, TICKS_PER_EIGHTH as E};
use crate::teller::{self, Hold, Teller, Telling};
use crate::theory::groove::{Groove, FIGHT};
use crate::theory::melody::Theme;
use crate::theory::phrase::{self, Form as PhraseForm, FORMS};
use crate::theory::schema::{fight_schemata, Schema};
use crate::theory::{Chord, Key, Mode};
use crate::variation::{self, Role as Bar};
use crate::tune::{self, Tune};

/// General MIDI programs, 0-based.
const CELLO: u8 = 42;
const CONTRABASS: u8 = 43;
const TREMOLO: u8 = 44;
const PIZZICATO: u8 = 45;
const STRINGS: u8 = 48;
const CHOIR_AAHS: u8 = 52;
const TRUMPET: u8 = 56;
const TROMBONE: u8 = 57;
const TUBA: u8 = 58;
const FRENCH_HORN: u8 = 60;
const CLARINET: u8 = 71;
const FIDDLE: u8 = 110;
const SHANAI: u8 = 111;
const TAIKO: u8 = 116;
/// The standard kit, and the keys the davul's strokes are played on.
const KIT: u8 = 0;
const KICK: u8 = 36;
const SIDE_STICK: u8 = 37;
const FLOOR_TOM: u8 = 41;
const CRASH: u8 = 49;
/// The toms a fill falls down through, high to low.
const FILL: [u8; 4] = [50, 47, 45, 43];

const CH_BASS: u8 = 0;
const CH_FIGURE: u8 = 1;
const CH_LEAD: u8 = 2;
const CH_TROMBONE: u8 = 3;
const CH_SECOND: u8 = 4;
const CH_TUBA: u8 = 5;
const CH_CHOIR: u8 = 6;
const CH_DOUBLE: u8 = 7;
const CH_TAIKO: u8 = 8;
const CH_KIT: u8 = 9;
const CH_WEAVE: u8 = 10;
const CH_HORN: u8 = 11;
const CH_PAD: u8 = 12;

/// How long the forest rings: open air and trees, shorter than the
/// overworld's hall, so a stroke lands and is gone.
const ROOM_S: f32 = 1.6;

/// Who may lead: the players that cut through a band, the zurna first
/// among them. The pool's files each have their own.
const LEADS: [u8; 4] = [SHANAI, FIDDLE, CLARINET, TRUMPET];

/// Each lead's level, dB, so that wherever it plays it sits three dB
/// under the band: the bank's samples of them are not one
/// loudness. Measured against the band on the pool's seeds.
fn lead_level(program: u8) -> f32 {
    match program {
        SHANAI => 2.0,
        FIDDLE => 5.6,
        CLARINET => 0.0,
        TRUMPET => 2.2,
        _ => 0.0,
    }
}

/// Every other player's level, dB: the band is dense, rhythm first, and
/// sits under the lead together so its own balance holds; the lead
/// sits some three dB under it where it plays.
const BAND: f32 = -3.0;

/// The velocity every voice strikes at before its own accent: a fight
/// strikes harder than the overworld's bed.
const VEL: i32 = 90;

/// What each layer at each notch lifts the loudness by, LU, and what
/// each telling does, which the pedal takes back through the part, so
/// a full fight is no louder than a sparse one: the story is told in
/// what plays. Fitted to the render, every section of forty seeds
/// against what was on in it; the range check holds them true.
const LIFT_OSTINATO: [f32; 3] = [0.0, 1.2, 1.35];
/// The drum is under every part of every story, so only its step up
/// is measured; its strokes are the floor the rest is measured on.
const LIFT_DRUM: [f32; 3] = [0.0, 0.0, 0.3];
const LIFT_BRASS: [f32; 3] = [0.0, 0.4, 0.6];
const LIFT_PAD: [f32; 3] = [0.0, 0.4, 1.8];
const LIFT_WEAVE: [f32; 2] = [0.0, 0.1];
/// By telling, in `Telling`'s order. A part with the lead silent sits a
/// LU and a half under the fight: raised the whole way, the band alone
/// is lifted by all the lead gave, and its strokes are the loop's
/// peaks.
const LIFT_LEAD: [f32; 6] = [1.5, 6.8, 6.5, 6.1, 5.65, 6.95];
/// The drum alone is let sit a LU and a half further under the fight:
/// held as loud as all of it, its strokes are the loop's peaks.
const LIFT_ALONE: f32 = 1.5;

/// The tune's register; the lead takes it here, the echo an octave
/// under. Its middle puts every key's home between E4 and D5, so no key
/// throws the tune an octave up into a shriek, and it is wide enough
/// over home for the climb's third on a phrase pair's step.
const TUNE: (u8, u8) = (60, 89);
/// The sequence a phrase pair takes, in degrees: the theme, a step up.
const PAIRS: [i32; 2] = [0, 1];
/// The tune a third up on the ladder's top rung.
const CLIMB: i32 = 2;

/// The strings on every eighth, the fight's engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ostinato {
    Off,
    /// The low strings on the root, every eighth, the group's first
    /// struck.
    Low,
    /// The upper strings too, a figure through the chord each group.
    Full,
}

/// The davul.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drum {
    Off,
    /// The low stroke with the taiko under it, and the thin stick on the
    /// other groups' first beats: every group struck.
    Strokes,
    /// The stick's pickups into the groups too, and the cymbal at every
    /// phrase.
    Full,
}

/// The low brass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Brass {
    Off,
    /// Trombone and tuba on the drum's strokes.
    Stabs,
    /// And on the off-beats between, pushing.
    Full,
}

/// Held chords where the fight thickens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pad {
    Off,
    /// The choir on root and fifth.
    Thin,
    /// The strings' tremolo on the chord and the horn on its third.
    Full,
}

/// The tune echoed under the fight, on the pizzicato.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weave {
    Off,
    /// A bar behind, an octave under.
    On,
}

/// A layer of the bed, the thing a rung of the ladder moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Ostinato,
    Drum,
    Brass,
    Pad,
    Weave,
}

/// What the bed is: each layer at its notch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Texture {
    ostinato: Ostinato,
    drum: Drum,
    brass: Brass,
    pad: Pad,
    weave: Weave,
}

impl Texture {
    const BARE: Texture = Texture { ostinato: Ostinato::Off, drum: Drum::Off, brass: Brass::Off, pad: Pad::Off, weave: Weave::Off };

    /// The pedal's gain under this texture and `lead`: what they lift
    /// the loudness by, given back.
    fn trim(&self, lead: Telling) -> f32 {
        let alone = if self.ostinato == Ostinato::Off && self.brass == Brass::Off && self.pad == Pad::Off { LIFT_ALONE } else { 0.0 };
        ladder::gain(LIFT_OSTINATO[self.ostinato as usize] + LIFT_DRUM[self.drum as usize] + LIFT_BRASS[self.brass as usize] + LIFT_PAD[self.pad as usize] + LIFT_WEAVE[self.weave as usize] + LIFT_LEAD[lead as usize] + alone)
    }
}

impl Bed for Texture {
    type Layer = Layer;

    fn up(self, layer: Layer) -> Texture {
        match layer {
            Layer::Ostinato => Texture { ostinato: if self.ostinato == Ostinato::Off { Ostinato::Low } else { Ostinato::Full }, ..self },
            Layer::Drum => Texture { drum: if self.drum == Drum::Off { Drum::Strokes } else { Drum::Full }, ..self },
            Layer::Brass => Texture { brass: if self.brass == Brass::Off { Brass::Stabs } else { Brass::Full }, ..self },
            Layer::Pad => Texture { pad: if self.pad == Pad::Off { Pad::Thin } else { Pad::Full }, ..self },
            Layer::Weave => Texture { weave: Weave::On, ..self },
        }
    }

    fn step_from(self, from: Texture) -> &'static str {
        let to = self;
        let moved = |a: u8, b: u8, names: [&'static str; 4]| match (a, b) {
            (0, _) => names[0],
            (_, 0) => names[3],
            _ if b > a => names[1],
            _ => names[2],
        };
        if from.ostinato != to.ostinato {
            moved(from.ostinato as u8, to.ostinato as u8, ["ostinato in", "ostinato up", "ostinato down", "ostinato out"])
        } else if from.drum != to.drum {
            moved(from.drum as u8, to.drum as u8, ["drum in", "drum up", "drum down", "drum out"])
        } else if from.brass != to.brass {
            moved(from.brass as u8, to.brass as u8, ["brass in", "brass up", "brass down", "brass out"])
        } else if from.pad != to.pad {
            moved(from.pad as u8, to.pad as u8, ["pad in", "pad up", "pad down", "pad out"])
        } else if from.weave != to.weave {
            if to.weave == Weave::On { "echo in" } else { "echo out" }
        } else {
            "held"
        }
    }
}

use Layer::{Brass as BrassLayer, Drum as DrumLayer, Ostinato as OstinatoLayer, Pad as PadLayer, Weave as WeaveLayer};
use Telling::{Long, Off, Phrases, Riff, RiffAndLong, Trading};

/// Every story's parts are two or three half-phrases: the dances' bars
/// are short at a run, and a part must stand long enough to be one. At
/// its crest the lead sings the tune, or holds long tones over all of
/// it, the wail a riff never gives.
const STORIES: [Story<Texture, Telling>; 4] = [
    // The fight joined on the drum and the low strings, built to all of
    // it, and let back down.
    Story {
        name: "clash",
        weight: 3.0,
        base: Texture { ostinato: Ostinato::Low, drum: Drum::Strokes, ..Texture::BARE },
        ladder: &[DrumLayer, BrassLayer, OstinatoLayer, PadLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Riff, Trading, Riff, Phrases, Trading, RiffAndLong, RiffAndLong, Long],
        turns: &[turn((7, 7), (1, 1))],
        halves: (2, 3),
        pace: (0.2, 0.8),
    },
    // In at full stride and never letting up.
    Story {
        name: "onslaught",
        weight: 2.0,
        base: Texture { ostinato: Ostinato::Full, drum: Drum::Full, brass: Brass::Stabs, ..Texture::BARE },
        ladder: &[PadLayer, BrassLayer, WeaveLayer, PadLayer],
        leads: &[Trading, Riff, Trading, RiffAndLong, Phrases],
        turns: &[turn((4, 4), (1, 2))],
        halves: (2, 3),
        pace: (0.6, 1.0),
    },
    // The drum alone, then the fight snaps in around it, the lead a
    // part after the engine.
    Story {
        name: "ambush",
        weight: 2.0,
        base: Texture { drum: Drum::Strokes, ..Texture::BARE },
        ladder: &[OstinatoLayer, DrumLayer, OstinatoLayer, BrassLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Off, Off, Phrases, Trading, Riff, RiffAndLong, Trading, Phrases],
        turns: &[turn((3, 3), (0, 0)), turn((2, 2), (0, 0)), turn((7, 7), (1, 1))],
        halves: (2, 3),
        pace: (0.3, 0.9),
    },
    // Up, a breakdown to the engine alone — the low strings and the
    // drum, the lead silent — the calm the next blow lands out of, and
    // up again.
    Story {
        name: "storm",
        weight: 2.0,
        base: Texture { ostinato: Ostinato::Low, drum: Drum::Full, ..Texture::BARE },
        ladder: &[OstinatoLayer, BrassLayer, PadLayer, BrassLayer, PadLayer, WeaveLayer],
        leads: &[Off, Trading, Riff, RiffAndLong, Trading, RiffAndLong, Long],
        turns: &[turn((6, 6), (0, 1)), turn((0, 0), (0, 0)), turn((6, 6), (1, 1))],
        halves: (2, 3),
        pace: (0.3, 1.0),
    },
];

/// What a seed's piece is.
struct Design {
    /// The one player who tells the tune.
    lead: u8,
    bass: u8,
    groove: &'static Groove,
    form: PhraseForm,
    theme: Theme,
    open: &'static Schema,
    closed: &'static Schema,
}

/// The band's velocity with an accent and a little jitter so no two
/// notes strike alike.
fn vel(accent: i32, rng: &mut Rng) -> u8 {
    (VEL + accent + rng.range(-4, 4)).clamp(1, 127) as u8
}

struct Form {
    bar: u32,
    walk: Walk<Texture, Telling>,
    tune: Tune,
    design: Design,
}

impl Form {
    fn texture_at(&self, bar: u32) -> Texture {
        self.walk.bed_at(bar)
    }
    fn chord(&self, bar: u32) -> Chord {
        self.tune.chords[bar as usize]
    }
    fn bars(&self) -> u32 {
        self.walk.bars()
    }
}

pub fn build(params: &Params) -> Score {
    let rng = Rng::new(params.seed);
    let mut skeleton = rng.fork(0);
    let story = ladder::draw(&STORIES, &mut skeleton);
    let mode = [Mode::Aeolian, Mode::Hijaz][skeleton.weighted(&[3.0, 3.0])];
    let key = Key::new(["D", "E", "G", "A", "C"][skeleton.weighted(&[3.0, 3.0, 2.0, 2.0, 1.0])], mode);
    // The five-eight least: at a run its bar is under a second, and its
    // loop a short one.
    let groove = &FIGHT[skeleton.weighted(&[3.0, 1.0, 2.0, 2.0])];
    let tempo = story.tempo(groove.tempo, &mut skeleton);
    let (open, closed) = fight_schemata(mode);
    let design = Design {
        lead: LEADS[skeleton.below(LEADS.len())],
        bass: [CELLO, CONTRABASS][skeleton.below(2)],
        groove,
        form: FORMS[skeleton.below(FORMS.len())],
        theme: Theme::draw(groove, &mut skeleton),
        open: open[skeleton.below(open.len())],
        closed: closed[skeleton.below(closed.len())],
    };
    let instruments = vec![
        Instrument { name: "bass", program: design.bass, channel: CH_BASS, role: Role::Pluck, low: 36, high: 55, reverb: 20, pan: 0, level: BAND },
        Instrument { name: "figure", program: STRINGS, channel: CH_FIGURE, role: Role::Pluck, low: 45, high: 64, reverb: 35, pan: -26, level: BAND },
        Instrument { name: "lead", program: design.lead, channel: CH_LEAD, role: Role::Melody, low: 58, high: 92, reverb: 30, pan: 0, level: lead_level(design.lead) },
        Instrument { name: "trombone", program: TROMBONE, channel: CH_TROMBONE, role: Role::Pluck, low: 45, high: 65, reverb: 30, pan: 22, level: BAND },
        Instrument { name: "horn, the second", program: FRENCH_HORN, channel: CH_SECOND, role: Role::Melody, low: 53, high: 76, reverb: 45, pan: -30, level: BAND },
        Instrument { name: "tuba", program: TUBA, channel: CH_TUBA, role: Role::Pluck, low: 28, high: 45, reverb: 20, pan: 8, level: BAND },
        Instrument { name: "choir", program: CHOIR_AAHS, channel: CH_CHOIR, role: Role::Sustain, low: 52, high: 71, reverb: 70, pan: 30, level: BAND },
        Instrument { name: "strings, the riff", program: STRINGS, channel: CH_DOUBLE, role: Role::Doubling, low: 58, high: 92, reverb: 40, pan: -36, level: BAND },
        Instrument { name: "taiko", program: TAIKO, channel: CH_TAIKO, role: Role::Percussion, low: 36, high: 36, reverb: 30, pan: -12, level: BAND },
        Instrument { name: "kit", program: KIT, channel: CH_KIT, role: Role::Percussion, low: KICK, high: 50, reverb: 25, pan: 0, level: BAND },
        Instrument { name: "echo", program: PIZZICATO, channel: CH_WEAVE, role: Role::Pluck, low: 46, high: 80, reverb: 35, pan: 40, level: BAND },
        Instrument { name: "horn", program: FRENCH_HORN, channel: CH_HORN, role: Role::Sustain, low: 53, high: 65, reverb: 50, pan: -42, level: BAND },
        Instrument { name: "pad", program: TREMOLO, channel: CH_PAD, role: Role::Sustain, low: 57, high: 74, reverb: 60, pan: -18, level: BAND },
    ];
    let mut score = Score::new(key, groove.meter(), tempo, instruments, ROOM_S);
    score.loops = true;
    score.lead = Some(CH_LEAD);
    let name = |program: u8| match program {
        SHANAI => "zurna",
        FIDDLE => "fiddle",
        CLARINET => "clarinet",
        TRUMPET => "trumpet",
        CELLO => "cello",
        CONTRABASS => "contrabass",
        _ => "?",
    };
    score.summary = format!(
        "{} on the {}: a {:?} in a {:?}, {} to ask and {} to answer; {} lead, {} ostinato",
        story.name,
        groove.name,
        design.theme.shape,
        design.form,
        design.open.name,
        design.closed.name,
        name(design.lead),
        name(design.bass),
    );
    let bar = score.bar();

    // The walk, in half-phrases, and whole question-and-answer pairs
    // of them, so the loop closes on an answer.
    let walk = story.place(&mut skeleton, &mut score, 4, |texture, lead| texture.trim(lead));

    let upper = story.ladder.len().max(1);
    let shifts: Vec<i32> = (0..walk.bars()).map(|b| if walk.at(b).rung >= upper { CLIMB } else { PAIRS[(b / phrase::BARS / 2) as usize % PAIRS.len()] }).collect();
    let tune = Tune::compose(&design.theme, &score.meter, design.form, &[design.open, design.closed], 3, shifts);
    score.harmony = tune.chords.clone();
    let form = Form { bar, walk, tune, design };

    ostinato(&mut score, &form, &mut rng.fork(1));
    drum(&mut score, &form, &mut rng.fork(2));
    brass(&mut score, &form, &mut rng.fork(3));
    pad(&mut score, &form, &mut rng.fork(4));
    weave(&mut score, &form, &mut rng.fork(5));
    let teller = Teller {
        lead: CH_LEAD,
        second: CH_SECOND,
        double: Some((CH_DOUBLE, -18)),
        echo: CH_WEAVE,
        register: TUNE,
        grace: teller::above,
        sung: (-6, -16),
        riff: (10, 0, -12),
        long: (72, 84, -6),
        under: (55, 67, -14),
        hold: Hold::Breathing,
        breathes: true,
        vel,
    };
    teller::tell(&mut score, &teller, &form.tune, &form.walk.runs(), &mut rng.fork(6));
    score.finish();
    score
}

/// The chord's tone at `degree` nearest `to` within `lo..=hi`.
fn at_degree(key: &Key, chord: Chord, degree: i32, lo: u8, hi: u8, to: u8) -> u8 {
    *chord.pitches_within(key, lo, hi).iter().filter(|p| key.degree_of(**p) == Some(degree.rem_euclid(7) as usize)).min_by_key(|p| ((**p as i32 - to as i32).abs(), **p)).unwrap()
}

/// The engine: the low strings on the chord's root every eighth, the
/// group's first struck hard and the rest pressed, short enough that
/// each is a stroke; at full, the strings an octave over them through
/// the chord each group — root, fifth, third — a sixteenth shorter, so
/// the two sound as one bow, and under the lead's register, where a
/// figure on its pitches masks it. A phrase's variant bar kicks the low
/// strings up to the fifth on the eighth into the dance's long group;
/// its cadence climbs both through the chord across the last group into
/// the next phrase.
fn ostinato(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let groups = form.design.groove.groups.to_vec();
    let eighths = score.meter.eighths();
    let last_group = eighths - *groups.last().unwrap() as u32;
    let kick = form.design.groove.dum.get(1).map(|d| d - 1).filter(|e| !score.meter.strong(*e));
    for b in 0..form.bars() {
        let mode = form.texture_at(b).ostinato;
        if mode == Ostinato::Off {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 38, 50, 43);
        let role = variation::role(b);
        // The cadence's last group climbs the chord into the next phrase;
        // the variant kicks up to the fifth into the dance's long group.
        let climb = chord.pitches_within(&key, root, 55);
        let fifth = chord.pitches_within(&key, 36, 55).into_iter().filter(|p| key.degree_of(*p) == Some((chord.root + 4).rem_euclid(7) as usize)).min_by_key(|p| (*p as i32 - root as i32 - 7).abs());
        let sweep = chord.pitches_within(&key, 45, 64);
        let mut at = 0;
        for g in &groups {
            for i in 0..*g as u32 {
                let e = at + i;
                let start = b * form.bar + e * E;
                let accent = if i == 0 { 6 } else { -8 };
                let pitch = match role {
                    Bar::Cadence if e >= last_group => climb[((e - last_group) as usize).min(climb.len() - 1)],
                    Bar::Variant if Some(e) == kick => fifth.unwrap_or(root),
                    _ => root,
                };
                score.add(Note { start, len: E * 3 / 4, pitch, vel: vel(accent, rng), channel: CH_BASS });
                if mode == Ostinato::Full {
                    let pitch = if role == Bar::Cadence && e >= last_group {
                        sweep[((e - last_group) as usize * 2).min(sweep.len() - 1)]
                    } else {
                        let degree = [chord.root, chord.root + 4, chord.root + 2][(i as usize) % 3];
                        at_degree(&key, chord, degree, 45, 60, 52)
                    };
                    score.add(Note { start, len: E / 2, pitch, vel: vel(accent - 6, rng), channel: CH_FIGURE });
                }
            }
            at += *g as u32;
        }
    }
}

/// The davul: its low stroke on the bar's first beat and on the dance's
/// long group, the kick and the floor tom together with the taiko under
/// them, the bar's the harder, and its thin stick on the other groups'
/// first beats, so every group is struck and the limp is heard; at
/// full, the stick's pickups into the groups, softer, the cymbal on
/// every phrase's first beat, and a fill down the toms through the last
/// group of every phrase, rising in strength, into the next. A phrase's
/// variant bar adds the stick's pickup into the long group and the
/// taiko under the stick's first stroke; its cadence rolls the taiko
/// into the next phrase.
fn drum(score: &mut Score, form: &Form, rng: &mut Rng) {
    for b in 0..form.bars() {
        let mode = form.texture_at(b).drum;
        if mode == Drum::Off {
            continue;
        }
        let groove = form.design.groove;
        for i in groove.dum {
            let start = b * form.bar + i * E;
            let accent = if *i == 0 { 2 } else { -3 };
            score.add(Note { start, len: E - 20, pitch: KICK, vel: vel(accent, rng), channel: CH_KIT });
            // Under the kick: three drums struck at once are the fight's
            // peak, and the kick is the one the ear counts.
            score.add(Note { start, len: E - 20, pitch: FLOOR_TOM, vel: vel(accent - 14, rng), channel: CH_KIT });
            score.add(Note { start, len: E - 20, pitch: 36, vel: vel(accent - 10, rng), channel: CH_TAIKO });
        }
        let role = variation::role(b);
        let last_eighth = groove.eighths() - 1;
        for i in groove.tek {
            let group = score.meter.strong(*i);
            if group || mode == Drum::Full {
                score.add(Note { start: b * form.bar + i * E, len: E / 2, pitch: SIDE_STICK, vel: vel(if group { -4 } else { -14 }, rng), channel: CH_KIT });
            }
        }
        // The variant's stick picks up into the long group; the taiko
        // answers the stick's first stroke there, and on the cadence rolls
        // two sixteenths into the next phrase.
        // The eighths neither hand plays, where a pickup can go.
        let free: Vec<u32> = (1..groove.eighths()).filter(|e| !groove.dum.contains(e) && !groove.tek.contains(e)).collect();
        if role == Bar::Variant {
            let into_long = groove.dum.get(1).map(|d| d - 1).filter(|e| free.contains(e));
            if let Some(e) = into_long.or(free.first().copied()) {
                score.add(Note { start: b * form.bar + e * E, len: E / 2, pitch: SIDE_STICK, vel: vel(-12, rng), channel: CH_KIT });
            }
            if let Some(i) = groove.tek.first() {
                score.add(Note { start: b * form.bar + i * E, len: E - 20, pitch: 36, vel: vel(-16, rng), channel: CH_TAIKO });
            }
        }
        if role == Bar::Cadence && b + 1 < form.bars() && !groove.dum.contains(&last_eighth) {
            variation::roll(score, CH_TAIKO, 36, b * form.bar + last_eighth * E, 2, |x| vel(-14 + (8.0 * x) as i32, rng));
            if mode != Drum::Full {
                score.add(Note { start: b * form.bar + last_eighth * E, len: E / 2, pitch: FLOOR_TOM, vel: vel(-10, rng), channel: CH_KIT });
            }
        }
        if mode != Drum::Full {
            continue;
        }
        if b % phrase::BARS == 0 {
            score.add(Note { start: b * form.bar, len: 2 * E, pitch: CRASH, vel: vel(-10, rng), channel: CH_KIT });
        }
        if role == Bar::Cadence || form.walk.parts.iter().any(|p| p.b == b + 1) {
            let groups = groove.groups;
            let last = *groups.last().unwrap() as u32;
            let from = groove.eighths() - last;
            for k in 0..last {
                let pitch = FILL[(k as usize * FILL.len() / last as usize).min(FILL.len() - 1)];
                score.add(Note { start: b * form.bar + (from + k) * E, len: E - 20, pitch, vel: vel(-10 + 5 * k as i32, rng), channel: CH_KIT });
            }
        }
    }
}

/// The low brass: trombone on the chord's root and fifth and tuba on
/// its root, short, on the dance's strokes, the bar's first the hardest;
/// at full, the trombones' fifth on the accompaniment's off-beats too,
/// softer, the push between the blows. A phrase's variant bar anticipates
/// its second stroke by an eighth; its cadence rips up the chord through
/// the last group, growing, into the next phrase.
fn brass(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).brass;
        if mode == Brass::Off {
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 45, 57, 50);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 65, root + 7);
        let low = at_degree(&key, chord, chord.root, 28, 40, 34);
        let groove = form.design.groove;
        let role = variation::role(b);
        let last_group = groove.eighths() - *groove.groups.last().unwrap() as u32;
        let rips = role == Bar::Cadence && b + 1 < form.bars();
        for (k, i) in groove.dum.iter().enumerate() {
            if rips && *i >= last_group {
                continue;
            }
            // The variant anticipates the second stroke by an eighth.
            let i = if role == Bar::Variant && k == 1 { i - 1 } else { *i };
            let start = b * form.bar + i * E;
            let accent = if i == 0 { 8 } else { -2 };
            for p in [root, fifth] {
                score.add(Note { start, len: E * 2 / 3, pitch: p, vel: vel(accent, rng), channel: CH_TROMBONE });
            }
            score.add(Note { start, len: E * 2 / 3, pitch: low, vel: vel(accent - 4, rng), channel: CH_TUBA });
        }
        if rips {
            // The cadence rips up the chord through its last group.
            let up = chord.pitches_within(&key, root, 65);
            for (k, e) in (last_group..groove.eighths()).enumerate() {
                let pitch = up[(k * 2).min(up.len() - 1)];
                score.add(Note { start: b * form.bar + e * E, len: E / 2, pitch, vel: vel(-6 + 5 * k as i32, rng), channel: CH_TROMBONE });
            }
            score.add(Note { start: b * form.bar + last_group * E, len: E * 2 / 3, pitch: low, vel: vel(-4, rng), channel: CH_TUBA });
        }
        if mode != Brass::Full {
            continue;
        }
        for i in groove.chord.iter().filter(|i| !rips || **i < last_group) {
            score.add(Note { start: b * form.bar + i * E, len: E / 2, pitch: fifth, vel: vel(-12, rng), channel: CH_TROMBONE });
        }
    }
}

/// Held chords where the fight thickens: the choir on root and fifth a
/// bar at a time, carried across the bar where the next chord keeps
/// them; at full, the strings' tremolo on three voices of the chord,
/// each led to the nearest tone of the next, and the horn on the third.
fn pad(score: &mut Score, form: &Form, rng: &mut Rng) {
    let key = score.key;
    let mut voices: Option<Vec<u8>> = None;
    for b in 0..form.bars() {
        let mode = form.texture_at(b).pad;
        if mode == Pad::Off {
            voices = None;
            continue;
        }
        let chord = form.chord(b);
        let root = at_degree(&key, chord, chord.root, 52, 64, 57);
        let fifth = at_degree(&key, chord, chord.root + 4, root + 1, 71, root + 7);
        for p in [root, fifth] {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: p, vel: vel(-22, rng), channel: CH_CHOIR });
        }
        if mode != Pad::Full {
            voices = None;
            continue;
        }
        let third = at_degree(&key, chord, chord.root + 2, 53, 65, 59);
        score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: third, vel: vel(-18, rng), channel: CH_HORN });
        let mut candidates = chord.pitches_within(&key, 57, 72);
        let next: Vec<u8> = match &voices {
            Some(prev) => prev
                .iter()
                .map(|v| {
                    let (i, p) = candidates.iter().enumerate().min_by_key(|(_, c)| (**c as i32 - *v as i32).abs()).map(|(i, c)| (i, *c)).unwrap();
                    candidates.remove(i);
                    p
                })
                .collect(),
            None => candidates.iter().copied().take(3).collect(),
        };
        for p in &next {
            score.hold(Note { start: b * form.bar, len: form.bar + E / 8, pitch: *p, vel: vel(-24, rng), channel: CH_PAD });
        }
        voices = Some(next);
    }
}

/// The tune echoed under the fight on the pizzicato: each bar the
/// tune of the bar before, detached, an octave under the tune's
/// register, its strong beats bent to this bar's chord.
fn weave(score: &mut Score, form: &Form, rng: &mut Rng) {
    let (lo, hi) = (TUNE.0 - 12, TUNE.1 - 12);
    let bars = form.bars();
    for b in 0..bars {
        if form.texture_at(b).weave == Weave::Off {
            continue;
        }
        let behind = (b + bars - 1) % bars;
        let chord = form.chord(b);
        for (start, len, pitch) in form.tune.bar(score, behind, lo, hi, false) {
            let onset = start - behind * form.bar;
            let held = if len >= 3 * E { 2 * E } else { len - E / 2 };
            let pitch = if score.meter.strong(onset / E) && !chord.holds(&score.key, pitch) { tune::nearest_chord_tone(&score.key, chord, pitch, lo, hi) } else { pitch };
            score.add(Note { start: b * form.bar + onset, len: held, pitch, vel: vel(-10, rng), channel: CH_WEAVE });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every story is a ladder.
    #[test]
    fn every_story_is_a_ladder() {
        for story in &STORIES {
            assert_eq!(story.fault(), None);
        }
    }

    /// The piece is a loop of whole question-and-answer pairs, its parts
    /// whole half-phrases, its harmony opening and closing on the tonic,
    /// its tune in its register; nothing struck ends past the end, only
    /// what is held, which the loop continues.
    #[test]
    fn a_fight_is_whole_phrases() {
        for seed in 0..24 {
            let score = build(&Params { seed });
            assert!(score.loops);
            let bars = score.end() / score.bar();
            assert_eq!(bars % (2 * phrase::BARS), 0, "seed {seed}: {bars} bars");
            for s in &score.sections {
                assert_eq!((s.end - s.start) % (phrase::BARS / 2 * score.bar()), 0, "seed {seed}: a part of broken half-phrases");
            }
            assert_eq!(score.harmony[0].root, 0);
            assert_eq!(score.harmony.last().unwrap().root, 0);
            for n in score.notes.iter().filter(|n| n.channel == CH_LEAD && n.len >= E / 2) {
                assert!((TUNE.0..=TUNE.1).contains(&n.pitch), "seed {seed}: the tune at {} leaves its register", n.pitch);
            }
            let end = score.end();
            for n in &score.notes {
                let role = score.instrument(n.channel).role;
                assert!(n.end() <= end || matches!(role, Role::Drone | Role::Sustain), "seed {seed}: {} past the end", score.instrument(n.channel).name);
            }
        }
    }
}
