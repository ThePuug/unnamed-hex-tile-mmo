//! The work done off the window's thread: rendering one variation at a
//! time, or one variation again with parts muted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};

use music::pieces::TRACKS;
use music::banks;
use music::pieces::Params;
use music::render::{self, Bank};

use crate::midi;
use crate::player::{Job, Mix, Take};
use crate::sheet::Sheet;

/// What the player needs rendered: a variation, or a variation it has
/// rendered again without the parts named.
pub enum Want {
    Take(Job),
    Mix(Job, Arc<Take>, Vec<&'static str>),
}

impl Want {
    pub fn job(&self) -> Job {
        match self {
            Want::Take(job) | Want::Mix(job, _, _) => *job,
        }
    }

    /// Whether `other` asks for the same render.
    pub fn same(&self, other: &Want) -> bool {
        match (self, other) {
            (Want::Take(a), Want::Take(b)) => a == b,
            (Want::Mix(a, _, m), Want::Mix(b, _, n)) => a == b && m == n,
            _ => false,
        }
    }
}

/// The one render the player needs next; the worker takes it when it is
/// free, so a request made while it renders replaces any still waiting.
#[derive(Default)]
pub struct Wanted {
    pub want: Mutex<Option<Want>>,
    pub posted: Condvar,
    /// Set when banks were installed: the worker loads the bank again
    /// before its next render.
    pub banks_changed: AtomicBool,
}

pub enum Done {
    /// The bank loaded, and how many sampled banks it plays from.
    Bank(Result<usize, String>),
    Take(Job, Result<Take, String>),
    Mix(Job, Mix),
}

pub fn spawn_worker(wanted: Arc<Wanted>, busy: Arc<Mutex<Option<Job>>>) -> Receiver<Done> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Some(mut bank) = load(&tx) else { return };
        loop {
            let want = {
                let mut slot = wanted.want.lock().unwrap();
                while slot.is_none() {
                    slot = wanted.posted.wait(slot).unwrap();
                }
                let want = slot.take().unwrap();
                *busy.lock().unwrap() = Some(want.job());
                want
            };
            if wanted.banks_changed.swap(false, Ordering::Relaxed) {
                match load(&tx) {
                    Some(b) => bank = b,
                    None => return,
                }
            }
            let done = match want {
                Want::Take(job) => {
                    let (piece, band, setting, seed) = job;
                    let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&TRACKS[piece], &Params { seed, band, setting }, &bank)))
                        .map(|(score, audio)| Take { sheet: Sheet::of(&score), midi: Arc::new(midi::stream(&score)), score, audio: Arc::new(audio) })
                        .map_err(|_| format!("{} seed {seed} panicked as it composed", TRACKS[piece].name));
                    Done::Take(job, taken)
                }
                Want::Mix(job, take, muted) => Done::Mix(job, mixed(&take, muted, &bank)),
            };
            *busy.lock().unwrap() = None;
            if tx.send(done).is_err() {
                return;
            }
        }
    });
    rx
}

/// `take` rendered again without the parts named in `muted`, as long as
/// the take so the playhead carries over, its MIDI and its sheet without
/// them too. Where the render panics the mix is the take itself, so the
/// player stops asking.
fn mixed(take: &Take, muted: Vec<&'static str>, bank: &Bank) -> Mix {
    let channels: Vec<u8> = take.score.instruments.iter().filter(|i| muted.contains(&i.name)).map(|i| i.channel).collect();
    let score = take.score.without(&channels);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::mix(&score, bank))) {
        Ok(mut audio) => {
            audio.resize(take.audio.len(), [0.0; 2]);
            Mix { muted, audio: Arc::new(audio), midi: Arc::new(midi::stream(&score)), sheet: Arc::new(Sheet::of(&score)) }
        }
        Err(_) => {
            eprintln!("music-player: the mix without {} panicked as it rendered", muted.join(", "));
            Mix { muted, audio: take.audio.clone(), midi: take.midi.clone(), sheet: Arc::new(Sheet::of(&take.score)) }
        }
    }
}

/// GeneralUser GS, and every sampled bank beside it or installed, told to
/// the player either way.
fn load(tx: &mpsc::Sender<Done>) -> Option<Bank> {
    let loaded = banks::open(None);
    let _ = tx.send(Done::Bank(loaded.as_ref().map(Bank::sampled).map_err(Clone::clone)));
    loaded.ok()
}
