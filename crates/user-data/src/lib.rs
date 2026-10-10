//! Where the studio's programs keep what they hold on a user's machine: a
//! folder per program under the studio's own, in the user's local data
//! folder. Never beside the program — a Mac app that changes its bundle
//! breaks its signature, and a program under Program Files cannot write
//! there.

use std::path::PathBuf;

/// `program`'s folder, or `None` where the platform names no data folder.
/// Nothing creates it: a writer makes it before its first write.
pub fn folder(program: &str) -> Option<PathBuf> {
    let var = |name| std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let studio = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Yasno Games")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Yasno Games")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))?.join("yasno-games")
    };
    Some(studio.join(program))
}
