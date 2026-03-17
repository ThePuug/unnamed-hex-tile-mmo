use bevy::prelude::*;

/// Prevents entity movement for a duration (e.g., after being kicked).
/// While staggered, a server system resets Position.offset to zero each tick,
/// freezing the entity in place regardless of AI behavior.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stagger {
    pub remaining: f32,
}

impl Stagger {
    pub fn new(duration: f32) -> Self {
        Self { remaining: duration }
    }
}
