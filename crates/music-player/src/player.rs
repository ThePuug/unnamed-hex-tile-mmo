//! What plays: the history gone through, the queue the listener filled,
//! and the composer's next draw, played only when the queue is empty.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::Color32;
use music::pieces::PIECES;
use music::render::SAMPLE_RATE;
use music::rng::Rng;
use music::perform::Event;
use music::score::Score;
use music::SEEDS;

use crate::audio::{open_output, Deck};
use crate::banks::{self, Install};
use music::banks::folder;
use crate::midi::{self, Port};
use crate::sheet::Sheet;
use crate::theme::{ALERT, DOT, LAMP, READY};
use crate::worker::{spawn_worker, Done, Wanted};

/// The silence between one variation's end and the next one's start.
pub const REST: Duration = Duration::from_secs(4);

/// Back past this far into a variation, "previous" starts it again
/// instead of going back one.
pub const RESTART_S: f64 = 3.0;

/// Where a variation came from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Played now from the play-a-seed panel.
    Chosen,
    Queued,
    Composed,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Chosen => "CHOSEN",
            Source::Queued => "QUEUED",
            Source::Composed => "COMPOSED",
        }
    }
}

/// A variation: a piece and a seed, its render once it arrives.
pub struct Variation {
    pub piece: usize,
    pub seed: u64,
    pub source: Source,
    pub take: Option<Arc<Take>>,
    pub failed: Option<String>,
}

impl Variation {
    pub fn new(piece: usize, seed: u64, source: Source) -> Self {
        Variation { piece, seed, source, take: None, failed: None }
    }

    pub fn job(&self) -> Job {
        (self.piece, self.seed)
    }

}

pub struct Take {
    pub score: Score,
    pub audio: Arc<Vec<[f32; 2]>>,
    /// The score as MIDI, for MIDI out.
    pub midi: Arc<Vec<Event>>,
    pub sheet: Sheet,
}

pub type Job = (usize, u64);

/// Which popover is open, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Popover {
    Pieces,
    Track,
    Midi,
}

pub struct Player {
    /// The pieces the composer draws from.
    pub chosen: Vec<bool>,
    pub history: Vec<Variation>,
    pub at: usize,
    /// What the listener asked for next, in order.
    pub queue: Vec<Variation>,
    /// The composer's next draw, played when the queue is empty; none
    /// while no piece is chosen.
    pub composed: Option<Variation>,
    /// The play-a-seed panel: its piece and the seed as typed.
    pub track: usize,
    pub seed: String,
    pub popover: Option<Popover>,
    pub filter: String,
    /// Whether the listener wants sound: the deck plays when this is set
    /// and the current variation has arrived.
    pub playing: bool,
    /// Whether the current variation plays again after its rest, in place
    /// of what follows.
    pub repeat: bool,
    /// When the rest after the current variation ends.
    pub rest_until: Option<Instant>,
    pub deck: Arc<Mutex<Deck>>,
    pub _stream: Option<cpal::Stream>,
    pub wanted: Arc<Wanted>,
    pub busy: Arc<Mutex<Option<Job>>>,
    pub done: Receiver<Done>,
    /// The bank as loaded: how many sampled banks it plays from.
    pub bank: Option<Result<usize, String>>,
    /// A banks archive being installed, or why the last install failed.
    pub install: Option<Arc<Mutex<Install>>>,
    pub output_error: Option<String>,
    pub midi: midi::Out,
    /// The MIDI ports as listed when the MIDI popover last opened.
    pub ports: Vec<Port>,
    pub rng: Rng,
}

impl Player {
    pub fn new() -> Self {
        let deck = Arc::new(Mutex::new(Deck::default()));
        let (stream, output_error) = match open_output(deck.clone()) {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e)),
        };
        let wanted = Arc::new(Wanted::default());
        let busy = Arc::new(Mutex::new(None));
        let done = spawn_worker(wanted.clone(), busy.clone());
        let midi = midi::spawn(deck.clone());
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        let mut rng = Rng::new(nanos);
        let chosen = vec![true; PIECES.len()];
        let first = draw(&mut rng, &chosen).expect("a piece is chosen");
        let composed = draw(&mut rng, &chosen);
        Player {
            chosen,
            history: vec![first],
            at: 0,
            queue: Vec::new(),
            composed,
            track: 0,
            seed: "0".to_string(),
            popover: None,
            filter: String::new(),
            playing: true,
            repeat: false,
            rest_until: None,
            deck,
            _stream: stream,
            wanted,
            busy,
            done,
            bank: None,
            install: None,
            output_error,
            midi,
            ports: Vec::new(),
            rng,
        }
    }

    pub fn current(&self) -> &Variation {
        &self.history[self.at]
    }

    /// Takes in what the worker finished, puts the current variation on
    /// the deck once it has arrived, and asks for the one needed next.
    pub fn poll(&mut self) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Bank(b) => self.bank = Some(b),
                Done::Take(job, taken) => {
                    let taken = taken.map(Arc::new);
                    let near = self.at.saturating_sub(1)..=self.at + 1;
                    let in_history = self.history.iter_mut().enumerate().filter(|(i, _)| near.contains(i)).map(|(_, v)| v);
                    for v in in_history.chain(self.queue.first_mut()).chain(self.composed.as_mut()).filter(|v| v.job() == job) {
                        match &taken {
                            Ok(t) => v.take = Some(t.clone()),
                            Err(e) => v.failed = Some(e.clone()),
                        }
                    }
                }
            }
        }
        if self.install.as_ref().is_some_and(|i| matches!(*i.lock().unwrap(), Install::Installed)) {
            self.install = None;
            self.installed();
        }
        // Only the queue's head is rendered ahead: a variation runs to tens
        // of megabytes, and one moved back behind it renders again.
        for v in self.queue.iter_mut().skip(1) {
            v.take = None;
        }

        let current = self.current();
        let mut deck = self.deck.lock().unwrap();
        if let Some(t) = &current.take {
            if !deck.audio.as_ref().is_some_and(|a| Arc::ptr_eq(a, &t.audio)) {
                deck.audio = Some(t.audio.clone());
                deck.midi = Some(t.midi.clone());
                deck.at = 0.0;
            }
        }
        deck.playing = self.playing;
        let ended = deck.ended();
        drop(deck);

        if self.current().failed.is_some() && self.playing {
            self.next();
        } else if ended && self.playing {
            let until = *self.rest_until.get_or_insert_with(|| Instant::now() + REST);
            if Instant::now() >= until {
                if self.repeat {
                    self.seek(0.0);
                } else {
                    self.next();
                }
            }
        }

        let need = std::iter::once(self.current()).chain(self.following()).find(|v| v.take.is_none() && v.failed.is_none()).map(Variation::job);
        if let Some(job) = need {
            if *self.busy.lock().unwrap() != Some(job) {
                let mut slot = self.wanted.job.lock().unwrap();
                if *slot != Some(job) {
                    *slot = Some(job);
                    self.wanted.posted.notify_one();
                }
            }
        }
    }

    /// What plays after the current variation: the one after it in the
    /// history where the listener went back, else the queue's head, else
    /// the composer's draw.
    pub fn following(&self) -> Option<&Variation> {
        self.history.get(self.at + 1).or(self.queue.first()).or(self.composed.as_ref())
    }

    /// Moves on to what follows; with nothing queued, nothing chosen and
    /// no history ahead, the current variation stays.
    pub fn next(&mut self) {
        if self.at + 1 == self.history.len() {
            let up = if self.queue.is_empty() {
                let fresh = draw(&mut self.rng, &self.chosen);
                std::mem::replace(&mut self.composed, fresh)
            } else {
                Some(self.queue.remove(0))
            };
            match up {
                Some(v) => self.history.push(v),
                None => return,
            }
        }
        self.go(self.at + 1);
    }

    pub fn previous(&mut self) {
        if self.at == 0 || self.position() > RESTART_S {
            self.seek(0.0);
        } else {
            self.go(self.at - 1);
        }
    }

    /// Plays `piece` at `seed` now, after the current variation in the
    /// history, so "previous" comes back to what was playing.
    pub fn play_now(&mut self, piece: usize, seed: u64) {
        self.history.insert(self.at + 1, Variation::new(piece, seed, Source::Chosen));
        self.go(self.at + 1);
        self.playing = true;
    }

    /// Adds `piece` at `seed` to the queue's end.
    pub fn enqueue(&mut self, piece: usize, seed: u64) {
        self.queue.push(Variation::new(piece, seed, Source::Queued));
    }

    /// Makes history entry `i` current, and lets go of every render but
    /// its neighbours': a variation runs to tens of megabytes, and one
    /// gone back to is rendered again.
    pub fn go(&mut self, i: usize) {
        self.at = i;
        self.rest_until = None;
        for (k, v) in self.history.iter_mut().enumerate() {
            if k + 1 < i || k > i + 1 {
                v.take = None;
            }
        }
        let mut deck = self.deck.lock().unwrap();
        deck.audio = None;
        deck.midi = None;
        deck.at = 0.0;
    }

    pub fn seek(&mut self, seconds: f64) {
        self.rest_until = None;
        self.deck.lock().unwrap().at = (seconds * SAMPLE_RATE as f64).max(0.0);
    }

    pub fn position(&self) -> f64 {
        self.deck.lock().unwrap().at / SAMPLE_RATE as f64
    }

    /// A change of the chosen pieces redraws the composer's draw where
    /// its piece is no longer among them, or draws one where none was.
    pub fn choose(&mut self, pieces: &[usize], on: bool) {
        for i in pieces {
            self.chosen[*i] = on;
        }
        if !self.composed.as_ref().is_some_and(|v| self.chosen[v.piece]) {
            self.composed = draw(&mut self.rng, &self.chosen);
        }
    }

    /// Installs the banks archive at `archive`, unless an install runs.
    pub fn install(&mut self, archive: PathBuf) {
        if self.install.as_ref().is_some_and(|i| matches!(*i.lock().unwrap(), Install::Unpacking { .. })) {
            return;
        }
        self.install = Some(match folder() {
            Some(folder) => banks::spawn_install(archive, folder),
            None => Arc::new(Mutex::new(Install::Failed("no data folder to install banks into".to_string()))),
        });
    }

    /// Banks were installed: the worker loads them before its next
    /// render, and what was rendered ahead on the old ones renders again.
    fn installed(&mut self) {
        self.wanted.banks_changed.store(true, Ordering::Relaxed);
        self.bank = None;
        for v in self.history.iter_mut().skip(self.at + 1).chain(self.queue.first_mut()).chain(self.composed.as_mut()) {
            v.take = None;
            v.failed = None;
        }
    }

    /// Where `v`'s render stands, as a dot's colour and a word: composing
    /// only while the worker renders it — it renders one at a time, and
    /// finishes one skipped before taking the next — else waiting.
    pub fn state(&self, v: &Variation) -> (Color32, &'static str) {
        match (&v.take, &v.failed) {
            (_, Some(_)) => (ALERT, "failed"),
            (Some(_), _) => (READY, "ready"),
            (None, None) if *self.busy.lock().unwrap() == Some(v.job()) => (LAMP, "composing…"),
            (None, None) => (DOT, "waiting"),
        }
    }

    /// The seed as typed, where there is one.
    pub fn typed_seed(&self) -> Option<u64> {
        self.seed.parse().ok()
    }
}

/// A seed of one of the chosen pieces, where any is.
pub fn draw(rng: &mut Rng, chosen: &[bool]) -> Option<Variation> {
    let pieces: Vec<usize> = (0..PIECES.len()).filter(|i| chosen[*i]).collect();
    if pieces.is_empty() {
        return None;
    }
    Some(Variation::new(*rng.pick(&pieces), rng.below(SEEDS as usize) as u64, Source::Composed))
}

/// The pools in the order the pieces are listed, each with its pieces.
pub fn pools() -> Vec<(&'static str, Vec<usize>)> {
    let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
    for (i, p) in PIECES.iter().enumerate() {
        match out.iter_mut().find(|(pool, _)| *pool == p.pool) {
            Some((_, members)) => members.push(i),
            None => out.push((p.pool, vec![i])),
        }
    }
    out
}

