//! The sampled banks a listener installs: the `RELEASE` archive, unpacked
//! into the banks folder, plays every voice `voices` names in place of
//! GeneralUser GS's program. The game and the music player share the one
//! folder, so a listener holding both holds the banks once.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};

use crate::render::Bank;
use crate::voices::VOICES;
use sevenz_rust2::{ArchiveReader, Password};

/// The release whose archive holds every file `voices` names, and the
/// SHA-256 of that archive. A voice added or a bank changed is a new
/// release, named here.
pub const RELEASE: &str = "music-banks-v5";
pub const RELEASE_SHA256: &str = "b39cb037c1202601ce4d793fc278e15c04c426e23c522c338dd54db0d0f822ba";

/// Where `RELEASE`'s archive is published.
pub fn release_url() -> String {
    format!("https://github.com/ThePuug/unnamed-hex-tile-mmo/releases/download/{RELEASE}/{RELEASE}.7z")
}

/// GeneralUser GS at `given`, else where `Bank::find` finds it, playing
/// every sampled bank beside it, and every one installed in `folder` in
/// its place.
pub fn open(given: Option<PathBuf>) -> Result<Bank, String> {
    let bank = Bank::find(given).ok_or_else(|| "no SoundFont found; set SOUNDFONT".to_string()).and_then(|p| Bank::load(&p))?;
    Ok(match folder() {
        Some(dir) => bank.with_banks(&dir),
        None => bank,
    })
}

/// Where installed banks live: the studio's `music-banks` folder, which
/// neither the game nor the music player owns.
pub fn folder() -> Option<PathBuf> {
    common::user_data::folder("music-banks")
}

/// Unpacks `archive` beside `folder` and puts it in the folder's place
/// once whole, so a failed or half-done unpack never replaces what plays.
/// An archive is taken only if it holds every file `voices` names.
pub fn install(archive: &Path, folder: &Path, progress: impl Fn(u64, u64)) -> Result<(), String> {
    let name = archive.file_name().map_or_else(|| archive.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut reader = ArchiveReader::open(archive, Password::empty()).map_err(|e| format!("{name}: {e}"))?;
    let entries = &reader.archive().files;
    let mut missing: Vec<&str> = VOICES.iter().map(|v| v.file).filter(|f| !entries.iter().any(|e| e.name == *f)).collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return Err(format!("{name} is not a banks archive: it lacks {}", missing.join(", ")));
    }
    let total: u64 = entries.iter().map(|e| e.size).sum();

    let partial = folder.with_extension("partial");
    let io = |e: std::io::Error| format!("{}: {e}", partial.display());
    if partial.exists() {
        fs::remove_dir_all(&partial).map_err(io)?;
    }
    fs::create_dir_all(&partial).map_err(io)?;
    let mut done = 0;
    let mut buf = vec![0u8; 1 << 20];
    reader
        .for_each_entries(|entry, data| {
            let Some(to) = inside(&partial, &entry.name) else {
                return Err(sevenz_rust2::Error::Other(format!("{} leaves the banks folder", entry.name).into()));
            };
            if entry.is_directory {
                fs::create_dir_all(&to)?;
                return Ok(true);
            }
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = BufWriter::new(File::create(&to)?);
            loop {
                let n = data.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                done += n as u64;
                progress(done, total);
            }
            out.flush()?;
            Ok(true)
        })
        .map_err(|e| format!("{name}: {e}"))?;

    let io = |e: std::io::Error| format!("{}: {e}", folder.display());
    if folder.exists() {
        fs::remove_dir_all(folder).map_err(io)?;
    }
    fs::rename(&partial, folder).map_err(io)
}

/// `name` from the archive as a path under `root`, where it stays there:
/// an archive names its files with either separator, and a name climbing
/// out or rooted elsewhere is refused.
fn inside(root: &Path, name: &str) -> Option<PathBuf> {
    let relative = PathBuf::from(name.replace('\\', "/"));
    relative.components().all(|c| matches!(c, Component::Normal(_))).then(|| root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder of its own under the system's temp folder.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("music-banks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An archive of `files` (name, contents), packed from a folder.
    fn archive(at: &Path, files: &[(&str, &str)]) -> PathBuf {
        let src = at.join("src");
        for (name, body) in files {
            let path = src.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        let out = at.join("banks.7z");
        sevenz_rust2::compress_to_path(&src, &out).unwrap();
        out
    }

    fn every_voice() -> Vec<(&'static str, &'static str)> {
        let mut files: Vec<_> = VOICES.iter().map(|v| (v.file, "bank")).collect();
        files.dedup();
        files
    }

    #[test]
    fn an_archive_of_every_voice_replaces_the_folder_whole() {
        let at = scratch("whole");
        let mut files = every_voice();
        files.push(("licenses/CC0-1.0.txt", "licence"));
        let archive = archive(&at, &files);
        let folder = at.join("banks");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("stale.sf2"), "old").unwrap();

        install(&archive, &folder, |_, _| {}).unwrap();

        for v in VOICES {
            assert!(folder.join(v.file).is_file(), "{} installed", v.file);
        }
        assert!(folder.join("licenses/CC0-1.0.txt").is_file());
        assert!(!folder.join("stale.sf2").exists(), "the old folder is replaced, not merged");
        assert!(!folder.with_extension("partial").exists());
        let _ = fs::remove_dir_all(&at);
    }

    #[test]
    fn an_archive_lacking_a_voice_leaves_the_folder_alone() {
        let at = scratch("lacking");
        let files: Vec<_> = every_voice().into_iter().skip(1).collect();
        let archive = archive(&at, &files);
        let folder = at.join("banks");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("kept.sf2"), "old").unwrap();

        let e = install(&archive, &folder, |_, _| {}).unwrap_err();

        assert!(e.contains(VOICES[0].file), "{e}");
        assert!(folder.join("kept.sf2").is_file());
        let _ = fs::remove_dir_all(&at);
    }

    #[test]
    fn a_name_leaving_the_folder_is_refused() {
        let root = Path::new("banks");
        assert_eq!(inside(root, r"licenses\a.txt"), Some(root.join("licenses/a.txt")));
        assert_eq!(inside(root, "../a.sf2"), None);
        assert_eq!(inside(root, "licenses/../../a.sf2"), None);
        assert_eq!(inside(root, "/a.sf2"), None);
    }
}
