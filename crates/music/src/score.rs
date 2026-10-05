//! A score: what plays, when, how loud, on what. Time is in ticks of an
//! eighth note so a bar in any meter is whole; the tempo turns ticks
//! into seconds at render.

use crate::theory::{Chord, Key, Meter};

pub const TICKS_PER_EIGHTH: u32 = 240;

/// How a voice sits in the texture, which is what the checks hold it to:
/// a drone is the tonic and nothing else; a sustained voice plays the
/// bar's chord; a melody or a pluck lands on the chord at every strong
/// beat and may walk the mode between, and a melody is a line of its
/// own — its leaps answered, never in parallel perfects with another; a
/// doubling plays another line's notes in unison and is judged with it,
/// not against it; percussion has no pitch to hold to the mode, only a
/// range of drum keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Drone,
    Sustain,
    Melody,
    Doubling,
    Pluck,
    Percussion,
}

#[derive(Clone, Copy, Debug)]
pub struct Instrument {
    pub name: &'static str,
    /// General MIDI program, 0-based.
    pub program: u8,
    pub channel: u8,
    pub role: Role,
    /// Playable range, inclusive.
    pub low: u8,
    pub high: u8,
    /// The reverb send, 0 to 127: how far back in the room the voice
    /// sits. General MIDI opens a channel at 40.
    pub reverb: u8,
    /// Where the player sits across the stage, -63 hard left to 63 hard
    /// right; sent as MIDI pan, 64 + this.
    pub pan: i8,
    /// The player's loudness against the others', dB: the render takes
    /// off what the bank's own samples give the player, so a level holds
    /// on any bank.
    pub level: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub start: u32,
    pub len: u32,
    pub pitch: u8,
    pub vel: u8,
    pub channel: u8,
}

impl Note {
    pub fn end(&self) -> u32 {
        self.start + self.len
    }
}

/// A stretch of the form, named for the sheet, and the trim every
/// channel's expression pedal takes through it, as a gain, reached by
/// its first beat: what a fuller texture gives back so the loudness
/// holds where the thinnest sits.
#[derive(Clone, Debug)]
pub struct Section {
    pub name: &'static str,
    pub start: u32,
    pub end: u32,
    pub trim: f32,
    /// Where a one-shot's arc puts the part, LU against its loudest
    /// levelled part, which declares 0: the trim is then the render's to
    /// set, measured, and not the piece's. None where the trim is as
    /// written.
    pub level: Option<f32>,
    /// Whether the pedal falls away through the section, from its trim
    /// at the first beat to `RING_DB` under it at the end, slowly at first
    /// and faster to the end: a one-shot's close, its held tones ringing
    /// on under the hit and dying as a struck one does.
    pub rings: bool,
}

/// How far a ringing section's pedal falls by its end, dB.
pub const RING_DB: f32 = 40.0;

/// A place in a piece a player of it may need: where it may be left
/// cleanly, and where its ending begins. The composer knows them
/// exactly; a file carries them beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A phrase begins: the bar line a piece may be left at, or jumped
    /// from to its coda, and land on its feet.
    Phrase,
    /// The piece's ending begins: the coda, played through to the last
    /// tone.
    Coda,
}

#[derive(Clone, Debug)]
pub struct Score {
    pub key: Key,
    pub meter: Meter,
    /// Eighth notes per minute: the piece's tempo, where it opens.
    pub eighth_bpm: f32,
    /// Where the tempo moves: from each tick on, eighths a minute, in
    /// order. A ritardando or a dance pressing on is a run of steps; empty
    /// where the piece holds its tempo throughout.
    pub tempo: Vec<(u32, f32)>,
    pub instruments: Vec<Instrument>,
    pub sections: Vec<Section>,
    /// The chord of each bar.
    pub harmony: Vec<Chord>,
    pub notes: Vec<Note>,
    /// What the piece drew, in its module's words: the story, the dance,
    /// the theme, the form, the schemata, the players.
    pub summary: String,
    /// Seconds the room takes to fall 60 dB after a low tone stops.
    pub room: f32,
    /// The channel of the one player who tells the tune, where one does.
    pub lead: Option<u8>,
    /// The places a player of the piece may need, in order: every
    /// phrase's first bar and where the coda begins.
    pub marks: Vec<(Mark, u32)>,
    /// The story the piece tells, as its ladder names it; empty where it
    /// tells none.
    pub story: &'static str,
    /// Where the key rises: from each tick on, the semitones the key
    /// stands over the one the piece opens in, in order. Empty where the
    /// piece keeps its key.
    pub lifts: Vec<(u32, i8)>,
}

impl Score {
    pub fn new(key: Key, meter: Meter, eighth_bpm: f32, instruments: Vec<Instrument>, room: f32) -> Self {
        Score { key, meter, eighth_bpm, tempo: Vec::new(), instruments, sections: Vec::new(), harmony: Vec::new(), notes: Vec::new(), summary: String::new(), room, lead: None, marks: Vec::new(), story: "", lifts: Vec::new() }
    }

    pub fn chord_at(&self, tick: u32) -> Chord {
        self.harmony[((tick / self.bar()) as usize).min(self.harmony.len() - 1)]
    }

    /// The key `tick` is heard in: the piece's, risen as far as it has
    /// by then, under the bar's chord, so a borrowed chord's tones are the
    /// bar's own. The risen key alone where there is no harmony.
    pub fn key_at(&self, tick: u32) -> Key {
        let by = self.lifts.iter().take_while(|(from, _)| *from <= tick).last().map_or(0, |(_, by)| *by);
        let key = Key { tonic: (self.key.tonic as i32 + by as i32) as u8, ..self.key };
        if self.harmony.is_empty() {
            return key;
        }
        key.under(self.chord_at(tick))
    }

    /// Whether `tick` opens a group of the bar.
    pub fn strong(&self, tick: u32) -> bool {
        tick % TICKS_PER_EIGHTH == 0 && self.meter.strong((tick / TICKS_PER_EIGHTH) % self.meter.eighths())
    }

    /// Ticks in a bar.
    pub fn bar(&self) -> u32 {
        self.meter.eighths() * TICKS_PER_EIGHTH
    }

    /// Where `ticks` falls in time, through every tempo step before it.
    pub fn seconds(&self, ticks: u32) -> f64 {
        let span = |from: u32, to: u32, bpm: f32| (to - from) as f64 / TICKS_PER_EIGHTH as f64 * 60.0 / bpm as f64;
        let (mut at, mut bpm, mut s) = (0u32, self.eighth_bpm, 0.0);
        for (from, next) in self.tempo.iter().take_while(|(from, _)| *from < ticks) {
            s += span(at, *from, bpm);
            (at, bpm) = (*from, *next);
        }
        s + span(at, ticks, bpm)
    }

    /// The tick at `seconds`, the inverse of `seconds`.
    pub fn tick_at(&self, seconds: f64) -> u32 {
        let per_s = |bpm: f32| bpm as f64 / 60.0 * TICKS_PER_EIGHTH as f64;
        let (mut at, mut bpm, mut s) = (0u32, self.eighth_bpm, 0.0);
        for (from, next) in &self.tempo {
            let reach = s + (*from - at) as f64 / per_s(bpm);
            if reach > seconds {
                break;
            }
            (at, bpm, s) = (*from, *next, reach);
        }
        at + ((seconds - s) * per_s(bpm)) as u32
    }

    /// The tempo at `tick`, eighths a minute.
    pub fn bpm_at(&self, tick: u32) -> f32 {
        self.tempo.iter().take_while(|(from, _)| *from <= tick).last().map_or(self.eighth_bpm, |(_, bpm)| *bpm)
    }

    /// Moves the whole score later by the bars of `opening`, whose chords
    /// head the harmony: an opening written after its body, in front of
    /// it. Notes, sections, marks and tempo steps all move.
    pub fn delay(&mut self, opening: &[Chord]) {
        let by = opening.len() as u32 * self.bar();
        for n in &mut self.notes {
            n.start += by;
        }
        for s in &mut self.sections {
            s.start += by;
            s.end += by;
        }
        for (_, at) in &mut self.marks {
            *at += by;
        }
        for (at, _) in &mut self.tempo {
            *at += by;
        }
        for (at, _) in &mut self.lifts {
            *at += by;
        }
        self.harmony.splice(0..0, opening.iter().copied());
    }

    /// Slows the tempo from tick `from` to tick `to` a beat at a time, down
    /// to `slowest` of what it was there: little at first and more to the
    /// end, as a player slows into a last chord rather than evenly.
    pub fn ritardando(&mut self, from: u32, to: u32, slowest: f32) {
        let base = self.bpm_at(from);
        let beats: Vec<u32> = (from..to).filter(|t| self.strong(*t)).collect();
        let n = beats.len().max(1) as f32;
        for (k, at) in beats.into_iter().enumerate() {
            let x = (k + 1) as f32 / n;
            self.tempo.push((at, base * (1.0 - (1.0 - slowest) * x * x)));
        }
        self.tempo.sort_by_key(|(t, _)| *t);
    }

    /// Raises the key `by` semitones from tick `from` on: every pitched
    /// note struck there or after goes up with it, so a piece writes its
    /// music in the key it opens in and modulates once it is written. A
    /// held note sounding across `from` is struck again there, raised:
    /// left as it was, a chord tone held on would sound against the key.
    pub fn modulate(&mut self, from: u32, by: i8) {
        let pitched: Vec<u8> = self.instruments.iter().filter(|i| i.role != Role::Percussion).map(|i| i.channel).collect();
        let held: Vec<Note> = self.notes.iter().filter(|n| n.start < from && n.end() > from + TICKS_PER_EIGHTH / 4 && pitched.contains(&n.channel)).copied().collect();
        for n in self.notes.iter_mut().filter(|n| n.start < from && n.end() > from + TICKS_PER_EIGHTH / 4 && pitched.contains(&n.channel)) {
            n.len = from - n.start;
        }
        self.notes.extend(held.into_iter().map(|n| Note { start: from, len: n.end() - from, ..n }));
        for n in self.notes.iter_mut().filter(|n| n.start >= from && pitched.contains(&n.channel)) {
            n.pitch = (n.pitch as i32 + by as i32) as u8;
        }
        let was = self.lifts.last().map_or(0, |(_, l)| *l);
        self.lifts.push((from, was + by));
    }

    /// Plays bars `from..to` again after the last bar of the harmony, as
    /// they were played — every note sounding there clipped to them, a
    /// hair of legato from the bar before no note of them, a held note
    /// carried on where it already sounds — with their chords; returns
    /// the bar the repeat begins at.
    pub fn again(&mut self, from: u32, to: u32) -> u32 {
        let bar = self.bar();
        let at = self.harmony.len() as u32;
        let (a, z) = (from * bar, to * bar);
        let by = (at - from) * bar;
        let notes: Vec<Note> = self
            .notes
            .iter()
            .filter(|n| n.start < z && n.end() > a + TICKS_PER_EIGHTH / 4)
            .map(|n| {
                let start = n.start.max(a);
                Note { start: start + by, len: n.end().min(z) - start, ..*n }
            })
            .collect();
        for n in notes {
            if self.instrument(n.channel).role == Role::Sustain {
                self.hold(n);
            } else {
                self.add(n);
            }
        }
        let chords: Vec<Chord> = self.harmony[from as usize..to as usize].to_vec();
        self.harmony.extend(chords);
        at
    }

    /// Marks the first bar of every phrase from bar `from` up to bar `to`
    /// as a place the piece may be left at.
    pub fn mark_phrases(&mut self, from: u32, to: u32) {
        let bar = self.bar();
        self.marks.extend((from..to).step_by(crate::theory::phrase::BARS as usize).map(|b| (Mark::Phrase, b * bar)));
    }

    /// Marks bar `at` as where the coda begins.
    pub fn mark_coda(&mut self, at: u32) {
        self.marks.push((Mark::Coda, at * self.bar()));
    }

    /// The ticks a mark of `kind` stands at, in order.
    pub fn marked(&self, kind: Mark) -> impl Iterator<Item = u32> + '_ {
        self.marks.iter().filter(move |(k, _)| *k == kind).map(|(_, at)| *at)
    }

    /// Where the last section ends.
    pub fn end(&self) -> u32 {
        self.sections.iter().map(|s| s.end).max().unwrap_or(0)
    }

    pub fn instrument(&self, channel: u8) -> &Instrument {
        self.instruments.iter().find(|i| i.channel == channel).expect("a channel's instrument")
    }

    pub fn section_at(&self, tick: u32) -> Option<&Section> {
        self.sections.iter().find(|s| s.start <= tick && tick < s.end)
    }

    /// The pedal's trim at `tick`: the section's, moving to the next
    /// one's over the quarter bar before it begins, so a part's first
    /// stroke plays at the part's own level — a trim arriving after it
    /// lets the stroke that opens a fuller part through at the thinner
    /// part's gain, the piece's peak; falling away through a section that
    /// rings; full outside any section.
    pub fn trim_at(&self, tick: u32) -> f32 {
        let Some(i) = self.sections.iter().position(|s| s.start <= tick && tick < s.end) else {
            return 1.0;
        };
        let s = &self.sections[i];
        if s.rings {
            let x = (tick - s.start) as f32 / (s.end - s.start) as f32;
            return s.trim * 10f32.powf(-RING_DB * x * x / 20.0);
        }
        let Some(next) = self.sections.get(i + 1) else {
            return s.trim;
        };
        let over = (self.bar() / 4).min(s.end - s.start);
        if tick + over < s.end {
            return s.trim;
        }
        let t = (tick + over - s.end) as f32 / over as f32;
        s.trim + (next.trim - s.trim) * t
    }

    pub fn add(&mut self, note: Note) {
        self.notes.push(note);
    }

    /// Adds a held note, or extends the one it continues: the same pitch
    /// on the same channel ending where this begins, or within a hair of
    /// it, so a chord tone kept across a bar line sounds once.
    pub fn hold(&mut self, note: Note) {
        if let Some(prev) = self.notes.iter_mut().rev().find(|p| p.channel == note.channel && p.pitch == note.pitch && p.end() + TICKS_PER_EIGHTH / 4 >= note.start && p.start < note.start) {
            prev.len = note.end() - prev.start;
            return;
        }
        self.notes.push(note);
    }

    /// Notes sounding at `tick`.
    pub fn sounding_at(&self, tick: u32) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(move |n| n.start <= tick && tick < n.end())
    }

    /// Puts the notes in a fixed order, so two scores composed alike are
    /// equal note for note whatever order their voices were written in.
    pub fn finish(&mut self) {
        self.notes.sort_by_key(|n| (n.start, n.channel, n.pitch, n.len));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::Mode;

    fn score() -> Score {
        let key = Key::new("D", Mode::Aeolian);
        let meter = Meter::new(&[3, 2, 2]);
        let instruments = vec![
            Instrument { name: "drone", program: 42, channel: 0, role: Role::Drone, low: 24, high: 60, reverb: 40, pan: 0, level: 0.0 },
            Instrument { name: "pluck", program: 24, channel: 3, role: Role::Pluck, low: 45, high: 74, reverb: 40, pan: 0, level: 0.0 },
        ];
        let mut s = Score::new(key, meter, 160.0, instruments, 2.0);
        let bar = s.bar();
        s.sections.push(Section { name: "a", start: 0, end: 2 * bar, trim: 1.0, level: None, rings: false });
        s.sections.push(Section { name: "b", start: 2 * bar, end: 4 * bar, trim: 0.5, level: None, rings: false });
        s.harmony = vec![Chord::triad(0); 4];
        s.add(Note { start: 0, len: 4 * bar, pitch: 38, vel: 50, channel: 0 });
        s.add(Note { start: 0, len: 200, pitch: 50, vel: 80, channel: 3 });
        s.add(Note { start: 3 * bar, len: 200, pitch: 50, vel: 80, channel: 3 });
        s
    }

    /// Time runs through every tempo step: a bar at half the tempo lasts
    /// twice as long, and `tick_at` turns the seconds back into ticks.
    #[test]
    fn time_runs_through_the_tempo_steps() {
        let mut s = score();
        let bar = s.bar();
        let plain = s.seconds(bar);
        s.tempo = vec![(bar, s.eighth_bpm / 2.0)];
        assert!((s.seconds(bar) - plain).abs() < 1e-9);
        assert!((s.seconds(2 * bar) - 3.0 * plain).abs() < 1e-9);
        assert_eq!(s.bpm_at(bar - 1), s.eighth_bpm);
        assert_eq!(s.bpm_at(bar), s.eighth_bpm / 2.0);
        for tick in [0, bar / 3, bar, bar + bar / 2, 3 * bar] {
            assert!(s.tick_at(s.seconds(tick)).abs_diff(tick) <= 1, "{tick}");
        }
    }

    #[test]
    fn a_ringing_section_falls_away_from_its_trim() {
        let mut s = score();
        s.sections[1].rings = true;
        let bar = s.bar();
        assert_eq!(s.trim_at(2 * bar), 0.5);
        assert!(s.trim_at(3 * bar) < 0.5 && s.trim_at(4 * bar - 1) < s.trim_at(3 * bar));
        assert!((s.trim_at(4 * bar - 1) / 0.5 - 10f32.powf(-RING_DB / 20.0)).abs() < 0.01);
    }

    /// A modulation raises every pitched note from its tick and the key
    /// heard there with them, strikes again what is held across it, and
    /// leaves what came before.
    #[test]
    fn a_modulation_raises_the_key_and_the_notes_after_it() {
        let mut s = score();
        let bar = s.bar();
        s.modulate(2 * bar, 2);
        assert_eq!(s.notes[1].pitch, 50);
        assert_eq!(s.notes[2].pitch, 52);
        assert_eq!(s.notes[0].end(), 2 * bar, "the drone held across is cut there");
        assert!(s.notes.iter().any(|n| n.channel == 0 && n.start == 2 * bar && n.pitch == 40), "and struck again raised");
        assert_eq!(s.key_at(bar).tonic, s.key.tonic);
        assert_eq!(s.key_at(3 * bar).tonic % 12, (s.key.tonic + 2) % 12);
        assert_eq!(s.key_at(3 * bar).absolute_degree(52), s.key_at(bar).absolute_degree(50), "a risen key counts its degrees on");
        assert!(s.key_at(3 * bar).contains(54) && !s.key_at(bar).contains(54));
    }

    /// A stretch played again lands after the harmony with its chords,
    /// and a hair of legato into it is no note of it.
    #[test]
    fn a_stretch_played_again_follows_the_harmony() {
        let mut s = score();
        let bar = s.bar();
        s.add(Note { start: bar, len: bar + 10, pitch: 57, vel: 80, channel: 3 });
        s.harmony[3] = Chord::triad(4);
        let at = s.again(2, 4);
        assert_eq!(at, 4);
        assert_eq!(s.harmony.len(), 6);
        assert_eq!(s.harmony[5].root, 4);
        let copied: Vec<&Note> = s.notes.iter().filter(|n| n.start >= 4 * bar).collect();
        assert_eq!(copied.iter().filter(|n| n.channel == 3).count(), 1);
        assert_eq!(copied.iter().find(|n| n.channel == 3).unwrap().start, 5 * bar);
    }

    #[test]
    fn the_trim_settles_on_each_parts() {
        let s = score();
        let bar = s.bar();
        assert_eq!(s.trim_at(0), 1.0);
        assert_eq!(s.trim_at(2 * bar - bar / 8), 0.75);
        assert_eq!(s.trim_at(2 * bar), 0.5);
        assert_eq!(s.trim_at(3 * bar), 0.5);
        assert_eq!(s.trim_at(5 * bar), 1.0);
    }
}
