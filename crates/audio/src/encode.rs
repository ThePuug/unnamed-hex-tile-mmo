//! Setting a level and writing the files: the WAV a person opens, and
//! the OGG the client plays, carrying what the client must know as
//! Vorbis comments, so the asset declares and the client reads.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::measure;
use crate::SAMPLE_RATE;

/// The audio with its integrated loudness moved to `lufs` by one gain.
pub fn set_loudness(audio: &mut [[f32; 2]], lufs: f32) {
    let now = measure::integrated(audio);
    scale(audio, 10f32.powf((lufs - now) / 20.0));
}

/// The audio with its loudest moment moved to `lufs` by one gain: a
/// one-shot placed at the level it plays at.
pub fn set_loudest_moment(audio: &mut [[f32; 2]], lufs: f32) {
    let now = measure::loudest_moment(audio);
    scale(audio, 10f32.powf((lufs - now) / 20.0));
}

/// The audio with its true peak moved to `dbtp` by one gain.
pub fn set_peak(audio: &mut [[f32; 2]], dbtp: f32) {
    let now = measure::true_peak(audio);
    scale(audio, 10f32.powf((dbtp - now) / 20.0));
}

fn scale(audio: &mut [[f32; 2]], gain: f32) {
    for s in audio.iter_mut() {
        s[0] *= gain;
        s[1] *= gain;
    }
}

fn pcm16(audio: &[[f32; 2]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(audio.len() * 4);
    for s in audio {
        for ch in s {
            let v = (ch.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes
}

/// 16-bit stereo PCM at `SAMPLE_RATE`.
pub fn wav(audio: &[[f32; 2]], path: &Path) -> Result<(), String> {
    let data = pcm16(audio);
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// Vorbis at quality 6 through ffmpeg, from a WAV already written, with
/// `tags` as Vorbis comments. Bit-exact: the Ogg muxer otherwise draws a
/// random stream serial each run, so the same audio encoded twice
/// differs and every build rewrites every file.
pub fn ogg(wav: &Path, tags: &[(&str, String)], path: &Path) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-i"]).arg(wav).args(["-c:a", "libvorbis", "-q:a", "6", "-fflags", "+bitexact", "-flags:a", "+bitexact"]);
    for (k, v) in tags {
        cmd.arg("-metadata").arg(format!("{k}={v}"));
    }
    let status = cmd.arg(path).stdin(Stdio::null()).status().map_err(|e| format!("ffmpeg: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg failed on {}", path.display()))
    }
}
