use std::collections::VecDeque;

use bevy::prelude::*;
use bimap::BiMap;

use crate::common::message::Event;

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct EntityMap(BiMap<Entity,Entity>);

#[derive(Debug, Default, Resource)]
pub struct Server {
    pub elapsed_offset: u128,
}

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct SpawnQueue(VecDeque<Event>);

#[derive(Debug, Default, Deref, DerefMut, Resource)]
pub struct MeshQueue(VecDeque<Event>);