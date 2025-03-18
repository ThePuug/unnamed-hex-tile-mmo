pub mod map;

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::common::message::Event;

#[derive(Debug, Default, Resource)]
pub struct InputQueue(pub VecDeque<Event>);
