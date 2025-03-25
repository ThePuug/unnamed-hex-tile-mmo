pub mod map;

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::common::message::Event;

#[derive(Debug, Default, Resource)]
pub struct InputQueue {
    pub queue: VecDeque<Event>, 
    pub accumulator_out: u16,  
    pub accumulator_in: u16,
}
