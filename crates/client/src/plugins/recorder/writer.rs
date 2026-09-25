//! Frames to a video file: a thread that pipes raw frames into ffmpeg,
//! in the order they were taken whatever order their readbacks land in.

use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread::JoinHandle,
};

use bevy::{prelude::*, render::render_resource::TextureFormat};

/// One captured frame: its number in the shot and its pixels.
pub struct Frame {
    pub index: u64,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: Vec<u8>,
}

/// Frames held between the game and the encoder. A full queue blocks the
/// capture, which holds the game: its clock is stepped by frame, so a
/// wait changes nothing in the picture.
const QUEUE: usize = 8;

pub struct Writer {
    sender: Option<SyncSender<Frame>>,
    thread: Option<JoinHandle<Result<u64, String>>>,
}

impl Writer {
    /// Starts a writer encoding to `path` at `fps`, scaled to `size`.
    pub fn start(path: PathBuf, fps: u32, size: [u32; 2]) -> Self {
        let (sender, receiver) = sync_channel(QUEUE);
        let thread = std::thread::spawn(move || encode(receiver, path, fps, size));
        Self { sender: Some(sender), thread: Some(thread) }
    }

    pub fn sender(&self) -> SyncSender<Frame> {
        self.sender.clone().expect("the writer is open")
    }

    /// Closes the stream and waits for ffmpeg to finish the file. Every
    /// sender handed out must have been dropped, or this waits forever.
    pub fn finish(mut self) -> Result<u64, String> {
        self.sender.take();
        self.thread.take().expect("finished once").join().map_err(|_| "the writer panicked".to_string())?
    }
}

/// The ffmpeg pixel format of a readback in `format`.
fn pix_fmt(format: TextureFormat) -> Result<&'static str, String> {
    match format {
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => Ok("bgra"),
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Ok("rgba"),
        other => Err(format!("no pixel format for {other:?}")),
    }
}

fn spawn_ffmpeg(first: &Frame, path: &PathBuf, fps: u32, size: [u32; 2]) -> Result<Child, String> {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo"])
        .args(["-pix_fmt", pix_fmt(first.format)?])
        .args(["-s", &format!("{}x{}", first.width, first.height)])
        .args(["-r", &fps.to_string(), "-i", "-"])
        .args(["-vf", &format!("scale={}:{}:flags=lanczos", size[0], size[1])])
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "12", "-pix_fmt", "yuv420p"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))
}

/// Writes frames in index order, holding any that arrive early. Returns
/// the frames written.
fn encode(receiver: Receiver<Frame>, path: PathBuf, fps: u32, size: [u32; 2]) -> Result<u64, String> {
    let mut child: Option<Child> = None;
    let mut early = BTreeMap::new();
    let mut next = 0u64;
    for frame in receiver {
        if child.is_none() {
            child = Some(spawn_ffmpeg(&frame, &path, fps, size)?);
        }
        early.insert(frame.index, frame);
        let stdin = child.as_mut().unwrap().stdin.as_mut().expect("piped");
        while let Some(frame) = early.remove(&next) {
            stdin.write_all(&frame.data).map_err(|e| format!("ffmpeg stdin: {e}"))?;
            next += 1;
        }
    }
    if !early.is_empty() {
        warn!("recorder: {} frames after a missing frame {next} were dropped", early.len());
    }
    let Some(mut child) = child else { return Ok(0) };
    drop(child.stdin.take());
    let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
    status.success().then_some(next).ok_or_else(|| format!("ffmpeg exited with {status}"))
}
