//! The work done off the window's thread: rendering one variation at a
//! time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};

use music::pieces::PIECES;
use music::banks;
use music::render::{self, Bank};

use crate::midi;
use crate::player::{Job, Take};
use crate::sheet::Sheet;

/// The one variation the player needs rendered next; the worker takes
/// it when it is free, so a request made while it renders replaces any
/// still waiting.
#[derive(Default)]
pub struct Wanted {
    pub job: Mutex<Option<Job>>,
    pub posted: Condvar,
    /// Set when banks were installed: the worker loads the bank again
    /// before its next render.
    pub banks_changed: AtomicBool,
}

pub enum Done {
    /// The bank loaded, and how many sampled banks it plays from.
    Bank(Result<usize, String>),
    Take(Job, Result<Take, String>),
}

pub fn spawn_worker(wanted: Arc<Wanted>, busy: Arc<Mutex<Option<Job>>>) -> Receiver<Done> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Some(mut bank) = load(&tx) else { return };
        loop {
            let job = {
                let mut slot = wanted.job.lock().unwrap();
                while slot.is_none() {
                    slot = wanted.posted.wait(slot).unwrap();
                }
                let job = slot.take().unwrap();
                *busy.lock().unwrap() = Some(job);
                job
            };
            if wanted.banks_changed.swap(false, Ordering::Relaxed) {
                match load(&tx) {
                    Some(b) => bank = b,
                    None => return,
                }
            }
            let (piece, seed) = job;
            let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&PIECES[piece], seed, &bank)))
                .map(|(score, audio)| Take { sheet: Sheet::of(&score), midi: Arc::new(midi::stream(&score)), score, audio: Arc::new(audio) })
                .map_err(|_| format!("{} seed {seed} panicked as it composed", PIECES[piece].name));
            *busy.lock().unwrap() = None;
            if tx.send(Done::Take(job, taken)).is_err() {
                return;
            }
        }
    });
    rx
}

/// GeneralUser GS, and every sampled bank beside it or installed, told to
/// the player either way.
fn load(tx: &mpsc::Sender<Done>) -> Option<Bank> {
    let loaded = banks::open(None);
    let _ = tx.send(Done::Bank(loaded.as_ref().map(Bank::sampled).map_err(Clone::clone)));
    loaded.ok()
}
