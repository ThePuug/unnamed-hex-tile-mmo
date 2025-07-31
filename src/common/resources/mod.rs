pub mod map;

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use crate::common::message::Event;

#[derive(Clone, Default, Deref, DerefMut, Resource)]
pub struct InputQueues(HashMap<Entity, InputQueue>);

#[derive(Clone, Debug, Default, Resource)]
pub struct InputQueue {
    pub queue: VecDeque<Event>, 
}
