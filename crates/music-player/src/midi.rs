//! MIDI out: the playing variation as MIDI — each player's program and
//! settings, then every note as the composer performs it — sent along the
//! playhead to a port another program plays, a DAW or a synthesizer.
//! While a port is open the player's own sound is silent, so what is
//! heard is the other program's. A MIDI clock runs beside the notes
//! through every tempo the piece steps through, started, stopped and
//! placed as the playhead is, so a program following it keeps the
//! piece's time.

use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use midir::{MidiOutput, MidiOutputConnection};
use music::perform::{self, Event};
use music::score::{Score, TICKS_PER_EIGHTH};

use crate::audio::Deck;

/// The name the player goes by among MIDI programs, and of the port it
/// makes for others to connect to where the system lets a program make
/// one: every system but Windows, where a loopback port such as loopMIDI
/// is made outside the player and picked from the list.
const NAME: &str = "Music Player";

/// How far the playhead may move between two looks and still be playing
/// on, seconds: further is a seek, and the notes sounding are stopped.
const JUMP_S: f64 = 0.25;

/// MIDI's real-time messages: a clock tick, of 24 to the quarter note;
/// start from the top; carry on from the song position; stop.
const CLOCK: u8 = 0xF8;
const START: u8 = 0xFA;
const CONTINUE: u8 = 0xFB;
const STOP: u8 = 0xFC;

/// Clock ticks to a song position's step, a sixteenth note.
const CLOCKS_PER_SIXTEENTH: usize = 6;

/// A variation's stream: its MIDI with the clock among it. Every message
/// keeps its place against those at its time, the settings at the start
/// ahead of all.
pub fn stream(score: &Score) -> Vec<Event> {
    let mut out = perform::midi(score);
    let step = TICKS_PER_EIGHTH * 2 / 24;
    out.extend((0..=score.end() / step).map(|k| Event { at: score.seconds(k * step), bytes: vec![CLOCK] }));
    out.sort_by(|a, b| a.at.max(0.0).partial_cmp(&b.at.max(0.0)).unwrap());
    out
}

/// Where MIDI goes.
#[derive(Clone, PartialEq, Eq)]
pub enum Port {
    Off,
    /// The player's own port.
    Made,
    /// A port the system lists, by name.
    Named(String),
}

impl Port {
    pub fn label(&self) -> String {
        match self {
            Port::Off => "Off".to_string(),
            Port::Made => format!("{NAME} (its own port)"),
            Port::Named(name) => name.clone(),
        }
    }
}

/// The ports a listener may pick: off, the player's own where it makes
/// one, then every port the system lists.
pub fn ports() -> Vec<Port> {
    let mut out = vec![Port::Off];
    if cfg!(unix) {
        out.push(Port::Made);
    }
    if let Ok(midi) = MidiOutput::new(NAME) {
        out.extend(midi.ports().iter().filter_map(|p| midi.port_name(p).ok()).map(Port::Named));
    }
    out
}

/// The port open, and why the last one asked for did not open.
pub struct Link {
    pub port: Port,
    pub error: Option<String>,
}

/// MIDI out's thread: told which port to open, and following the deck.
pub struct Out {
    tx: Sender<Port>,
    pub link: Arc<Mutex<Link>>,
}

impl Out {
    pub fn choose(&self, port: Port) {
        let _ = self.tx.send(port);
    }
}

pub fn spawn(deck: Arc<Mutex<Deck>>) -> Out {
    let (tx, rx) = mpsc::channel::<Port>();
    let link = Arc::new(Mutex::new(Link { port: Port::Off, error: None }));
    let shared = link.clone();
    std::thread::spawn(move || {
        let mut conn: Option<MidiOutputConnection> = None;
        let mut follow = Follow::default();
        loop {
            loop {
                match rx.try_recv() {
                    Ok(port) => {
                        if let Some(mut c) = conn.take() {
                            follow.halt(&mut |bytes| {
                                let _ = c.send(bytes);
                            });
                        }
                        let mut link = shared.lock().unwrap();
                        match open(&port) {
                            Ok(c) => {
                                conn = c;
                                *link = Link { port, error: None };
                            }
                            Err(e) => *link = Link { port: Port::Off, error: Some(e) },
                        }
                        follow = Follow::default();
                        deck.lock().unwrap().muted = conn.is_some();
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if let Some(c) = conn.as_mut() {
                follow.step(&mut |bytes| {
                    let _ = c.send(bytes);
                }, &deck);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    Out { tx, link }
}

fn open(port: &Port) -> Result<Option<MidiOutputConnection>, String> {
    let midi = || MidiOutput::new(NAME).map_err(|e| e.to_string());
    match port {
        Port::Off => Ok(None),
        #[cfg(unix)]
        Port::Made => {
            use midir::os::unix::VirtualOutput;
            midi()?.create_virtual(NAME).map(Some).map_err(|e| e.to_string())
        }
        #[cfg(not(unix))]
        Port::Made => Err("this system makes no port of a program's own".to_string()),
        Port::Named(name) => {
            let midi = midi()?;
            let found = midi.ports().into_iter().find(|p| midi.port_name(p).ok().as_deref() == Some(name.as_str()));
            let found = found.ok_or_else(|| format!("{name} is gone"))?;
            midi.connect(&found, NAME).map(Some).map_err(|e| e.to_string())
        }
    }
}

/// Every note on every channel stopped, and the sustain pedal lifted.
fn silence(send: &mut dyn FnMut(&[u8])) {
    for ch in 0..16u8 {
        send(&[0xB0 | ch, 64, 0]);
        send(&[0xB0 | ch, 123, 0]);
    }
}

fn starts_a_note(e: &Event) -> bool {
    e.bytes[0] & 0xF0 == 0x90 && e.bytes[2] > 0
}

/// Where the sending of a variation's stream stands: whose, the next
/// message, how far it has been sent, how many clock ticks of it, and
/// whether a note may sound and the clock runs.
#[derive(Default)]
struct Follow {
    events: Option<Arc<Vec<Event>>>,
    next: usize,
    at: f64,
    clocks: usize,
    sounding: bool,
    running: bool,
}

impl Follow {
    /// Sends what the playhead has passed since the last step. Another
    /// variation or a seek stops the clock and what sounds, and takes the
    /// stream up again from the playhead: every setting before it sent,
    /// no note and no tick. A pause stops them too; playing on starts the
    /// clock from the top, or from the song position it stands at.
    fn step(&mut self, send: &mut dyn FnMut(&[u8]), deck: &Mutex<Deck>) {
        let (events, at, playing) = {
            let d = deck.lock().unwrap();
            (d.midi.clone(), d.heard_s(), d.playing && !d.ended())
        };
        let same = match (&events, &self.events) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same || at < self.at || at > self.at + JUMP_S {
            self.halt(send);
            self.events = events;
            self.next = 0;
            self.clocks = 0;
            if let Some(events) = &self.events {
                while self.next < events.len() && events[self.next].at <= at {
                    let e = &events[self.next];
                    // A tick at the playhead is the one played from.
                    if e.bytes[0] == CLOCK && e.at >= at {
                        break;
                    }
                    if e.bytes[0] == CLOCK {
                        self.clocks += 1;
                    } else if !starts_a_note(e) {
                        send(&e.bytes);
                    }
                    self.next += 1;
                }
            }
        }
        if !playing {
            self.halt(send);
        } else if let Some(events) = &self.events {
            if !self.running {
                if self.clocks == 0 {
                    send(&[START]);
                } else {
                    let at = self.clocks / CLOCKS_PER_SIXTEENTH;
                    send(&[0xF2, (at & 0x7F) as u8, ((at >> 7) & 0x7F) as u8]);
                    send(&[CONTINUE]);
                }
                self.running = true;
            }
            while self.next < events.len() && events[self.next].at <= at {
                let e = &events[self.next];
                send(&e.bytes);
                if e.bytes[0] == CLOCK {
                    self.clocks += 1;
                } else if starts_a_note(e) {
                    self.sounding = true;
                }
                self.next += 1;
            }
        }
        self.at = at;
    }

    /// Stops the clock and every note sounding.
    fn halt(&mut self, send: &mut dyn FnMut(&[u8])) {
        if self.running {
            send(&[STOP]);
            self.running = false;
        }
        if self.sounding {
            silence(send);
            self.sounding = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music::pieces::{Params, PIECES};

    /// A deck at `at` seconds playing `events`, its render as long as the
    /// last of them.
    fn deck(events: &Arc<Vec<Event>>, at: f64, playing: bool) -> Mutex<Deck> {
        let len = (events.last().unwrap().at * music::render::SAMPLE_RATE as f64) as usize + 1;
        let audio = Some(Arc::new(vec![[0.0f32; 2]; len]));
        Mutex::new(Deck { audio, midi: Some(events.clone()), at: at * music::render::SAMPLE_RATE as f64, playing, ..Default::default() })
    }

    #[test]
    fn the_clock_starts_stops_and_takes_up_where_the_playhead_is() {
        let score = (PIECES[0].build)(&Params { seed: 0 });
        let events = Arc::new(stream(&score));
        let mut follow = Follow::default();
        let mut step = |at: f64, playing: bool| {
            let mut sent: Vec<Vec<u8>> = Vec::new();
            follow.step(&mut |b| sent.push(b.to_vec()), &deck(&events, at, playing));
            sent
        };

        let sent: Vec<Vec<u8>> = (0..=60).flat_map(|k| step(k as f64 * 0.1, true)).collect();
        assert_eq!(sent.iter().filter(|b| b[0] == START).count(), 1, "the top starts the clock, once");
        assert!(sent.iter().any(|b| b[0] == CLOCK) && sent.iter().any(|b| b[0] & 0xF0 == 0x90));

        let sent = step(6.0, false);
        assert_eq!(sent[0], vec![STOP], "a pause stops the clock");
        assert!(sent.contains(&vec![0xB0, 123, 0]), "and every note");

        let sent = step(30.0, true);
        let at = sent.iter().position(|b| b[0] == 0xF2).expect("a song position");
        assert_eq!(sent[at + 1], vec![CONTINUE], "a seek carries on from its position");
        assert!(sent[..at].iter().all(|b| b[0] != CLOCK && b[0] & 0xF0 != 0x90), "nothing passed is played on the way");
        let sixteenths = sent[at][1] as usize | (sent[at][2] as usize) << 7;
        let clocks = events.iter().filter(|e| e.bytes[0] == CLOCK && e.at <= 30.0).count();
        assert_eq!(sixteenths, clocks / CLOCKS_PER_SIXTEENTH);

        let sent = step(0.0, true);
        assert!(sent.contains(&vec![STOP]) && sent.contains(&vec![START]), "going back to the top starts the clock again");
    }

    #[test]
    fn a_stream_sets_up_first_and_ticks_through_every_tempo() {
        for piece in PIECES {
            let score = (piece.build)(&Params { seed: 0 });
            let events = stream(&score);
            let setup = 4 * score.instruments.len();
            assert!(events[..setup].iter().all(|e| e.bytes[0] != CLOCK && e.bytes[0] & 0xF0 != 0x90), "{}", piece.name);
            assert!(events.windows(2).all(|w| w[0].at.max(0.0) <= w[1].at.max(0.0)), "{}", piece.name);
            let ticks: Vec<f64> = events.iter().filter(|e| e.bytes[0] == CLOCK).map(|e| e.at).collect();
            assert_eq!(ticks.len() as u32, score.end() / (TICKS_PER_EIGHTH * 2 / 24) + 1, "{}", piece.name);
            // Every tick a twenty-fourth of the quarter note at the tempo
            // in force, one spanning a step between the two.
            let tick_s = |bpm: f32| 60.0 / (bpm as f64 / 2.0) / 24.0;
            let tempi = std::iter::once(score.eighth_bpm).chain(score.tempo.iter().map(|(_, bpm)| *bpm));
            let (short, long) = tempi.fold((f64::MAX, 0.0f64), |(s, l), bpm| (s.min(tick_s(bpm)), l.max(tick_s(bpm))));
            for w in ticks.windows(2) {
                let gap = w[1] - w[0];
                assert!(gap >= short - 1e-9 && gap <= long + 1e-9, "{}: a tick {gap} s apart", piece.name);
            }
        }
    }
}
