//! A piece as its render left it, played from memory: interleaved stereo
//! at the renderer's rate, never encoded.

use std::{num::NonZero, sync::Arc, time::Duration};

use bevy::{
    audio::{ChannelCount, Decodable, SampleRate, Source},
    prelude::*,
};
use music::render::SAMPLE_RATE;

#[derive(Asset, TypePath)]
pub struct Take(Arc<Vec<[f32; 2]>>);

impl Take {
    pub fn new(audio: Vec<[f32; 2]>) -> Self {
        Take(Arc::new(audio))
    }
}

impl Decodable for Take {
    type Decoder = Playing;

    fn decoder(&self) -> Playing {
        Playing { audio: self.0.clone(), at: 0 }
    }
}

/// A take from its head: `at` counts samples, both channels.
pub struct Playing {
    audio: Arc<Vec<[f32; 2]>>,
    at: usize,
}

impl Iterator for Playing {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.audio.get(self.at / 2)?[self.at % 2];
        self.at += 1;
        Some(sample)
    }
}

impl Source for Playing {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.audio.len() * 2 - self.at)
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(SAMPLE_RATE).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.audio.len() as f64 / SAMPLE_RATE as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_plays_left_then_right_to_its_end() {
        let take = Take::new(vec![[1.0, 2.0], [3.0, 4.0]]);
        let mut playing = take.decoder();
        assert_eq!(playing.current_span_len(), Some(4));
        assert_eq!(playing.by_ref().collect::<Vec<_>>(), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(playing.current_span_len(), Some(0));
    }
}
