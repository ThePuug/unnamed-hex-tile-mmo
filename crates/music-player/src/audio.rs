//! The output: the variation under the playhead, played by the device's
//! stream at whatever rate it runs.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use music::render::SAMPLE_RATE;

/// The variation under the playhead, shared with the audio callback.
#[derive(Default)]
pub struct Deck {
    pub audio: Option<Arc<Vec<[f32; 2]>>>,
    /// The playhead, in frames of the render at `SAMPLE_RATE`.
    pub at: f64,
    pub playing: bool,
}

impl Deck {
    pub fn ended(&self) -> bool {
        self.audio.as_ref().is_some_and(|a| self.at >= a.len() as f64)
    }

    /// The next output frame, `step` render frames on, linearly
    /// interpolated where the device runs at another rate.
    pub fn frame(&mut self, step: f64) -> [f32; 2] {
        let Some(audio) = &self.audio else { return [0.0; 2] };
        if !self.playing || self.at >= audio.len() as f64 {
            return [0.0; 2];
        }
        let i = self.at as usize;
        let f = (self.at - i as f64) as f32;
        let (a, b) = (audio[i], audio[(i + 1).min(audio.len() - 1)]);
        self.at += step;
        [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
    }
}

pub fn open_output(deck: Arc<Mutex<Deck>>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host().default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: cpal::StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => output::<f32>(&device, &config, deck),
        cpal::SampleFormat::I16 => output::<i16>(&device, &config, deck),
        cpal::SampleFormat::U16 => output::<u16>(&device, &config, deck),
        cpal::SampleFormat::I32 => output::<i32>(&device, &config, deck),
        f => return Err(format!("unsupported output sample format {f:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

pub fn output<T: SizedSample + FromSample<f32>>(device: &cpal::Device, config: &cpal::StreamConfig, deck: Arc<Mutex<Deck>>) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let step = SAMPLE_RATE as f64 / config.sample_rate as f64;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut deck = deck.lock().unwrap();
                for out in data.chunks_mut(channels) {
                    let [l, r] = deck.frame(step);
                    for (c, s) in out.iter_mut().enumerate() {
                        let v = match (channels, c) {
                            (1, _) => (l + r) * 0.5,
                            (_, 0) => l,
                            (_, 1) => r,
                            _ => 0.0,
                        };
                        *s = T::from_sample(v);
                    }
                }
            },
            |e| eprintln!("music-player: output: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

