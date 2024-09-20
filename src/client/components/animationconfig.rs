use std::time::Duration;

use bevy::prelude::*;

pub struct AnimationDirection {
    pub start: usize,
    pub end: usize,
    pub flip: bool,
}

#[derive(Component)]
pub struct AnimationConfig {
    pub opts: [AnimationDirection; 4],
    pub fps: u8,
    pub frame_timer: Timer,
    pub selected: usize,
}

impl AnimationConfig {
    pub fn new(opts: [AnimationDirection; 4], fps: u8, selected: usize) -> Self {
        Self {
            opts,
            fps,
            frame_timer: Self::timer_from_fps(fps),
            selected,
        }
    }

    pub fn timer_from_fps(fps: u8) -> Timer {
        Timer::new(Duration::from_secs_f32(1.0 / fps as f32), TimerMode::Repeating)
    }
}
