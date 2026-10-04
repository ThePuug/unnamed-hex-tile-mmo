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
    /// The player's level against the others', dB, sent as channel
    /// volume: at most about +6, where the volume reaches its top.
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

#[derive(Clone, Debug)]
pub struct Score {
    pub key: Key,
    pub meter: Meter,
    /// Eighth notes per minute.
    pub eighth_bpm: f32,
    pub instruments: Vec<Instrument>,
    pub sections: Vec<Section>,
    /// The chord of each bar.
    pub harmony: Vec<Chord>,
    pub notes: Vec<Note>,
    /// What the piece drew, in its module's words: the story, the dance,
    /// the theme, the form, the schemata, the players.
    pub summary: String,
    /// Whether the score is a loop: its last tick runs into its first,
    /// so nothing in it fades and a held tone that reaches the end is
    /// the same tone that opens it.
    pub loops: bool,
    /// Seconds the room takes to fall 60 dB after a low tone stops.
    pub room: f32,
    /// The channel of the one player who tells the tune, where one does.
    pub lead: Option<u8>,
}

impl Score {
    pub fn new(key: Key, meter: Meter, eighth_bpm: f32, instruments: Vec<Instrument>, room: f32) -> Self {
        Score { key, meter, eighth_bpm, instruments, sections: Vec::new(), harmony: Vec::new(), notes: Vec::new(), summary: String::new(), loops: false, room, lead: None }
    }

    pub fn chord_at(&self, tick: u32) -> Chord {
        self.harmony[((tick / self.bar()) as usize).min(self.harmony.len() - 1)]
    }

    /// Whether `tick` opens a group of the bar.
    pub fn strong(&self, tick: u32) -> bool {
        tick % TICKS_PER_EIGHTH == 0 && self.meter.strong((tick / TICKS_PER_EIGHTH) % self.meter.eighths())
    }

    /// Ticks in a bar.
    pub fn bar(&self) -> u32 {
        self.meter.eighths() * TICKS_PER_EIGHTH
    }

    pub fn seconds(&self, ticks: u32) -> f64 {
        ticks as f64 / TICKS_PER_EIGHTH as f64 * 60.0 / self.eighth_bpm as f64
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
    /// part's gain, the loop's peak; falling away through a section that
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

    /// The score played `times` in a row, as a loop is rendered: the
    /// sections, harmony and notes again at every pass, and a held voice
    /// that reaches a seam continued into the next pass rather than
    /// struck again, so the drone is one tone throughout.
    pub fn unrolled(&self, times: u32) -> Score {
        let period = self.end();
        let mut out = self.clone();
        for k in 1..times {
            let shift = k * period;
            for s in &self.sections {
                out.sections.push(Section { start: s.start + shift, end: s.end + shift, ..s.clone() });
            }
            out.harmony.extend(self.harmony.iter().copied());
            for n in &self.notes {
                let n = Note { start: n.start + shift, ..*n };
                match self.instrument(n.channel).role {
                    Role::Drone | Role::Sustain => out.hold(n),
                    _ => out.add(n),
                }
            }
        }
        out.finish();
        out
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

    #[test]
    fn unrolling_continues_the_drone_and_repeats_the_rest() {
        let s = score();
        let twice = s.unrolled(2);
        let period = s.end();
        assert_eq!(twice.end(), 2 * period);
        assert_eq!(twice.sections.len(), 4);
        assert_eq!(twice.harmony.len(), 8);
        let drones: Vec<&Note> = twice.notes.iter().filter(|n| n.channel == 0).collect();
        assert_eq!(drones.len(), 1, "the drone is struck once");
        assert_eq!(drones[0].len, 2 * period);
        assert_eq!(twice.notes.iter().filter(|n| n.channel == 3).count(), 4);
        assert!(twice.notes.iter().any(|n| n.channel == 3 && n.start == period));
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
