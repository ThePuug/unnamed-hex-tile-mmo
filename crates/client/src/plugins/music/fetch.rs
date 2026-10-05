//! The sampled banks, fetched on the first run that lacks them: the game
//! ships GeneralUser GS alone, and the banks archive — near a gigabyte, two
//! unpacked — comes from its release into the folder the music player
//! installs into too.

use std::{
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::Path,
};

use bevy::prelude::*;
use music::banks::{self, RELEASE, RELEASE_SHA256};
use sha2::{Digest, Sha256};

/// Downloads `RELEASE`'s archive beside `folder`, refuses it unless it
/// hashes to `RELEASE_SHA256`, unpacks it into `folder` in place of what
/// was there, and lets the archive go. A run cut short leaves a partial
/// file the next run starts over.
pub fn fetch(folder: &Path) -> Result<(), String> {
    let archive = folder.with_file_name(format!("{RELEASE}.7z"));
    let partial = folder.with_file_name(format!("{RELEASE}.7z.partial"));
    let io = |e: std::io::Error| format!("{}: {e}", partial.display());
    fs::create_dir_all(folder.parent().unwrap_or(folder)).map_err(io)?;

    let url = banks::release_url();
    let response = ureq::get(&url).call().map_err(|e| format!("{url}: {e}"))?;
    let total = response.body().content_length();
    let mut body = response.into_body().into_reader();
    let mut out = BufWriter::new(File::create(&partial).map_err(io)?);
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let (mut done, mut told) = (0u64, 0u64);
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(io)?;
        done += n as u64;
        if let Some(total) = total {
            let tenth = done * 10 / total.max(1);
            if tenth > told {
                told = tenth;
                info!("music: fetching {RELEASE}, {}% of {} MB", tenth * 10, total >> 20);
            }
        }
    }
    out.flush().map_err(io)?;
    drop(out);

    let digest = format!("{:x}", hash.finalize());
    if digest != RELEASE_SHA256 {
        let _ = fs::remove_file(&partial);
        return Err(format!("{url} hashes to {digest}, not {RELEASE_SHA256}"));
    }
    fs::rename(&partial, &archive).map_err(io)?;
    info!("music: unpacking {RELEASE} into {}", folder.display());
    let unpacked = banks::install(&archive, folder, |_, _| {});
    let _ = fs::remove_file(&archive);
    unpacked
}
