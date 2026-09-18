use bevy::prelude::*;
use qrz::Qrz;

use crate::components::{heading::Heading, position::Position};

/// What the last movement intent told other clients, and the position at the
/// previous tick so motion since then can be detected. Server only.
#[derive(Component, Debug, Default)]
pub struct MovementIntentState {
    pub last_tick: Position,
    pub sent_heading: Heading,
    pub sent_moving: bool,
    pub sent_tile: Qrz,
    pub sent_airborne: bool,
}
