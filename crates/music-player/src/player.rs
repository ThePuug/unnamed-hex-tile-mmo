//! What plays: the history gone through, the queue the listener filled,
//! and the composer's next draw, played only when the queue is empty.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::Color32;
use music::band::Band;
use music::pieces::{Setting, Style, TRACKS};
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

/// A play: a track, the band playing it, the setting it is played in and
/// the play's seed, its render once it arrives.
pub struct Variation {
    pub piece: usize,
    pub band: &'static Band,
    pub setting: Setting,
    pub seed: u64,
    pub source: Source,
    pub take: Option<Arc<Take>>,
    pub failed: Option<String>,
}

impl Variation {
    pub fn new(piece: usize, band: &'static Band, setting: Setting, seed: u64, source: Source) -> Self {
        Variation { piece, band, setting, seed, source, take: None, failed: None }
    }

    pub fn job(&self) -> Job {
        (self.piece, self.band, self.setting, self.seed)
    }

}

pub struct Take {
    pub score: Score,
    pub audio: Arc<Vec<[f32; 2]>>,
    /// The score as MIDI, for MIDI out.
    pub midi: Arc<Vec<Event>>,
    pub sheet: Sheet,
}

pub type Job = (usize, &'static Band, Setting, u64);

/// Which popover is open, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Popover {
    Style,
    Track,
    Setting,
    Band,
    Midi,
    Credits,
}

pub struct Player {
    /// What plays: `band` plays `track`, of `style`, in `setting`, at
    /// the seed as typed, now or queued; and what the composer draws from
    /// when the queue runs out (`draw`), keeping what is locked, where
    /// `autoplay` goes on past the queue. None is chosen until the
    /// listener chooses it.
    pub band: Option<&'static Band>,
    pub style: Option<Style>,
    pub track: Option<usize>,
    pub setting: Setting,
    pub seed: String,
    pub locks: Locks,
    pub autoplay: bool,
    pub history: Vec<Variation>,
    /// The history entry playing; none before the listener first plays.
    pub at: Option<usize>,
    /// What the listener asked for next, in order.
    pub queue: Vec<Variation>,
    /// The composer's next draw, played when the queue is empty and
    /// `autoplay` is on.
    pub composed: Option<Variation>,
    pub popover: Option<Popover>,
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
        let picked = Picked { band: None, style: None, track: None, setting: Setting::None, seed: None };
        let composed = Some(draw(&mut rng, picked, Locks::default()));
        Player {
            band: None,
            style: None,
            track: None,
            setting: Setting::None,
            seed: "0".to_string(),
            locks: Locks::default(),
            autoplay: false,
            history: Vec::new(),
            at: None,
            queue: Vec::new(),
            composed,
            popover: None,
            playing: false,
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

    pub fn current(&self) -> Option<&Variation> {
        self.at.map(|i| &self.history[i])
    }

    /// Where what follows the current variation stands in the history.
    fn after(&self) -> usize {
        self.at.map_or(0, |i| i + 1)
    }

    /// Takes in what the worker finished, puts the current variation on
    /// the deck once it has arrived, and asks for the one needed next.
    pub fn poll(&mut self) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Bank(b) => self.bank = Some(b),
                Done::Take(job, taken) => {
                    let taken = taken.map(Arc::new);
                    let near = self.after().saturating_sub(2)..=self.after();
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
        if let Some(t) = current.and_then(|v| v.take.as_ref()) {
            if !deck.audio.as_ref().is_some_and(|a| Arc::ptr_eq(a, &t.audio)) {
                deck.audio = Some(t.audio.clone());
                deck.midi = Some(t.midi.clone());
                deck.at = 0.0;
            }
        }
        deck.playing = self.playing;
        let ended = deck.ended();
        drop(deck);

        if self.current().is_some_and(|v| v.failed.is_some()) && self.playing {
            self.next_or_stop();
        } else if ended && self.playing {
            let until = *self.rest_until.get_or_insert_with(|| Instant::now() + REST);
            if Instant::now() >= until {
                if self.repeat {
                    self.seek(0.0);
                } else {
                    self.next_or_stop();
                }
            }
        }

        let need = self.current().into_iter().chain(self.following()).find(|v| v.take.is_none() && v.failed.is_none()).map(Variation::job);
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
    /// with autoplay the composer's draw.
    pub fn following(&self) -> Option<&Variation> {
        self.history.get(self.after()).or(self.queue.first()).or(self.composed.as_ref().filter(|_| self.autoplay))
    }

    /// Moves on to what follows, where anything does.
    pub fn next(&mut self) {
        if self.following().is_some() {
            self.advance();
        }
    }

    /// Plays from where the player stands: with nothing played yet, what
    /// follows, else the composer's draw whether autoplay is on or not.
    pub fn play(&mut self) {
        if self.at.is_none() {
            self.advance();
        }
        self.playing = true;
        self.rest_until = None;
    }

    /// Moves on to the history's next entry, taking the queue's head, else
    /// the composer's draw, where the history runs out.
    fn advance(&mut self) {
        let i = self.after();
        if i == self.history.len() {
            let up = if self.queue.is_empty() {
                let fresh = self.draw();
                std::mem::replace(&mut self.composed, Some(fresh))
            } else {
                Some(self.queue.remove(0))
            };
            match up {
                Some(v) => self.history.push(v),
                None => return,
            }
        }
        self.go(i);
    }

    /// Moves on to what follows; where nothing does, stops at the start of
    /// what played.
    fn next_or_stop(&mut self) {
        if self.following().is_some() {
            self.next();
        } else {
            self.playing = false;
            self.seek(0.0);
        }
    }

    pub fn previous(&mut self) {
        match self.at {
            Some(i) if i > 0 && self.position() <= RESTART_S => self.go(i - 1),
            _ => self.seek(0.0),
        }
    }

    /// Plays `piece` at `seed` by `band` in `setting` now, after the
    /// current variation in the history, so "previous" comes back to what
    /// was playing.
    pub fn play_now(&mut self, piece: usize, band: &'static Band, setting: Setting, seed: u64) {
        let i = self.after();
        self.history.insert(i, Variation::new(piece, band, setting, seed, Source::Chosen));
        self.go(i);
        self.playing = true;
    }

    /// Adds `piece` at `seed` by `band` in `setting` to the queue's end.
    pub fn enqueue(&mut self, piece: usize, band: &'static Band, setting: Setting, seed: u64) {
        self.queue.push(Variation::new(piece, band, setting, seed, Source::Queued));
    }

    /// The selection changed, or the listener rolled the draw again: the
    /// composer's waiting draw is drawn afresh from the selection, as its
    /// locks keep it.
    pub fn selected(&mut self) {
        self.composed = Some(self.draw());
    }

    /// A fresh draw of the composer's from the selection.
    fn draw(&mut self) -> Variation {
        let picked = Picked { band: self.band, style: self.style, track: self.track, setting: self.chosen_setting(), seed: self.typed_seed() };
        draw(&mut self.rng, picked, self.locks)
    }

    /// `style` chosen: the track stays where it is of the style, else
    /// is chosen no more.
    pub fn choose_style(&mut self, style: Style) {
        self.style = Some(style);
        self.track = self.track.filter(|t| TRACKS[*t].style == style);
        self.selected();
    }

    /// Track `i` chosen, and with it its style.
    pub fn choose_track(&mut self, i: usize) {
        self.track = Some(i);
        self.style = Some(TRACKS[i].style);
        self.selected();
    }

    /// Makes history entry `i` current, and lets go of every render but
    /// its neighbours': a variation runs to tens of megabytes, and one
    /// gone back to is rendered again.
    pub fn go(&mut self, i: usize) {
        self.at = Some(i);
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
        let after = self.after();
        for v in self.history.iter_mut().skip(after).chain(self.queue.first_mut()).chain(self.composed.as_mut()) {
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

    /// The setting chosen, where the chosen track is made for it, else
    /// none.
    pub fn chosen_setting(&self) -> Setting {
        match self.track {
            Some(t) if !TRACKS[t].plays_in(self.setting) => Setting::None,
            _ => self.setting,
        }
    }

    /// The settings to choose from: the chosen track's, else those of any
    /// track of the chosen style, else of any track; and none.
    pub fn settings(&self) -> Vec<Setting> {
        let made = |t: usize| self.track.is_none_or(|c| c == t) && self.style.is_none_or(|s| s == TRACKS[t].style);
        Setting::ALL.into_iter().filter(|s| *s == Setting::None || (0..TRACKS.len()).any(|t| made(t) && TRACKS[t].plays_in(*s))).collect()
    }
}

/// Which of the selection the composer keeps when it draws.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Locks {
    pub band: bool,
    pub style: bool,
    pub track: bool,
    pub setting: bool,
    pub seed: bool,
}

/// The selection a draw starts from; what is not chosen is drawn, locked
/// or not.
#[derive(Clone, Copy)]
pub struct Picked {
    pub band: Option<&'static Band>,
    pub style: Option<Style>,
    pub track: Option<usize>,
    pub setting: Setting,
    pub seed: Option<u64>,
}

/// The composer's next play from `picked`: the band, the style, the
/// track, the setting and the seed each drawn afresh where not locked or
/// not chosen, a locked seed as typed. A track is of the style; a track drawn is one
/// made for the setting where the setting is kept; a setting drawn is one
/// the track is made for. The one rule the listener does not see: a band drawn is of
/// the style, and a style drawn is a locked band's, unless the listener
/// locked them apart.
pub fn draw(rng: &mut Rng, picked: Picked, locks: Locks) -> Variation {
    let (kept_band, kept_style, kept_track) = (picked.band.filter(|_| locks.band), picked.style.filter(|_| locks.style), picked.track.filter(|_| locks.track));
    let made_for = |i: &usize| !locks.setting || TRACKS[*i].plays_in(picked.setting);
    let style = if let Some(t) = kept_track {
        TRACKS[t].style
    } else if let Some(s) = kept_style {
        s
    } else if let Some(b) = kept_band {
        b.style
    } else {
        let styles: Vec<Style> = Style::ALL.iter().copied().filter(|s| (0..TRACKS.len()).any(|i| TRACKS[i].style == *s && made_for(&i))).collect();
        *rng.pick(if styles.is_empty() { &Style::ALL[..] } else { &styles })
    };
    let track = kept_track.unwrap_or_else(|| {
        let of_style: Vec<usize> = (0..TRACKS.len()).filter(|i| TRACKS[*i].style == style).collect();
        let made: Vec<usize> = of_style.iter().copied().filter(made_for).collect();
        *rng.pick(if made.is_empty() { &of_style } else { &made })
    });
    let setting = if !locks.setting {
        match TRACKS[track].settings {
            [] => Setting::None,
            made => *rng.pick(made),
        }
    } else if TRACKS[track].plays_in(picked.setting) {
        picked.setting
    } else {
        Setting::None
    };
    let band = kept_band.unwrap_or_else(|| {
        let roster: Vec<&'static Band> = music::band::of_style(style).collect();
        *rng.pick(&roster)
    });
    let seed = match (locks.seed, picked.seed) {
        (true, Some(seed)) => seed,
        _ => rng.below(SEEDS as usize) as u64,
    };
    Variation::new(track, band, setting, seed, Source::Composed)
}

/// The styles in their order, each with its tracks.
pub fn styles() -> Vec<(Style, Vec<usize>)> {
    Style::ALL.iter().map(|s| (*s, (0..TRACKS.len()).filter(|i| TRACKS[*i].style == *s).collect())).collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use music::band;

    fn track(name: &str) -> usize {
        TRACKS.iter().position(|t| t.name == name).unwrap()
    }

    /// A draw with nothing locked keeps a band and its track in one style
    /// and a track in a setting it is made for; what is locked stays, a
    /// band and a track locked apart among it, and a locked seed holds;
    /// what is locked but not chosen is drawn.
    #[test]
    fn a_draw_keeps_what_is_locked() {
        let mut rng = Rng::new(7);
        let blues = band::of_style(Style::Blues).next().unwrap();
        let picked = Picked { band: Some(blues), style: Some(Style::Metal), track: Some(track("speed-metal")), setting: Setting::Combat, seed: Some(42) };
        for _ in 0..200 {
            let v = draw(&mut rng, picked, Locks::default());
            assert_eq!(v.band.style, TRACKS[v.piece].style);
            assert!(TRACKS[v.piece].plays_in(v.setting));
            let apart = Locks { band: true, track: true, ..Locks::default() };
            let v = draw(&mut rng, picked, apart);
            assert_eq!((v.band.name, Some(v.piece)), (blues.name, picked.track));
            let v = draw(&mut rng, picked, Locks { setting: true, ..Locks::default() });
            assert!(TRACKS[v.piece].plays_in(Setting::Combat) && v.setting == Setting::Combat);
            let all = Locks { band: true, style: true, track: true, setting: true, seed: true };
            let v = draw(&mut rng, picked, all);
            assert_eq!((Some(v.piece), v.band.name, v.setting, v.seed), (picked.track, blues.name, Setting::Combat, 42));
            let unchosen = Picked { band: None, style: None, track: None, ..picked };
            let v = draw(&mut rng, unchosen, all);
            assert_eq!(v.band.style, TRACKS[v.piece].style);
        }
    }
}
