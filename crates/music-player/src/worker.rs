//! The work done off the window's thread: rendering one variation at a
//! time.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};

use music::pieces::PIECES;
use music::render::{self, Bank};

use crate::player::{Job, Take};
use crate::sheet::Sheet;

/// The one variation the player needs rendered next; the worker takes
/// it when it is free, so a request made while it renders replaces any
/// still waiting.
#[derive(Default)]
pub struct Wanted {
    pub job: Mutex<Option<Job>>,
    pub posted: Condvar,
}

pub enum Done {
    Bank(Result<(), String>),
    Take(Job, Result<Take, String>),
}

pub fn spawn_worker(wanted: Arc<Wanted>, busy: Arc<Mutex<Option<Job>>>) -> Receiver<Done> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let bank = match Bank::find(None).ok_or_else(|| "no SoundFont found; set SOUNDFONT".to_string()).and_then(|p| Bank::load(&p)) {
            Ok(b) => {
                let _ = tx.send(Done::Bank(Ok(())));
                b
            }
            Err(e) => {
                let _ = tx.send(Done::Bank(Err(e)));
                return;
            }
        };
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
            let (piece, seed) = job;
            let taken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::take(&PIECES[piece], seed, &bank)))
                .map(|(score, audio)| Take { sheet: Sheet::of(&score), score, audio: Arc::new(audio) })
                .map_err(|_| format!("{} seed {seed} panicked as it composed", PIECES[piece].name));
            *busy.lock().unwrap() = None;
            if tx.send(Done::Take(job, taken)).is_err() {
                return;
            }
        }
    });
    rx
}
