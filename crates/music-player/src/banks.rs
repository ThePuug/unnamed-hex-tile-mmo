//! Installing the sampled banks from the window: a `music-banks` archive
//! dropped on it is unpacked into `music::banks::folder` on a thread of
//! its own, and the worker reloads the bank once it is in.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use music::banks::install;

pub enum Install {
    Unpacking { done: u64, total: u64 },
    Failed(String),
    Installed,
}

/// Unpacks `archive` into `folder` on its own thread, reporting into the
/// returned state.
pub fn spawn_install(archive: PathBuf, folder: PathBuf) -> Arc<Mutex<Install>> {
    let state = Arc::new(Mutex::new(Install::Unpacking { done: 0, total: 0 }));
    let report = state.clone();
    std::thread::spawn(move || {
        let end = match install(&archive, &folder, |done, total| *report.lock().unwrap() = Install::Unpacking { done, total }) {
            Ok(()) => Install::Installed,
            Err(e) => Install::Failed(e),
        };
        *report.lock().unwrap() = end;
    });
    state
}

