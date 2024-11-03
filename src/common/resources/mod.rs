pub mod map;

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::keybits::KeyBits;

#[derive(Debug, Default)]
pub struct InputAccumulator {
    pub key_bits: KeyBits,
    pub dt: u16,
}

#[derive(Debug, Default, Resource)]
pub struct InputQueue(pub VecDeque<InputAccumulator>);
