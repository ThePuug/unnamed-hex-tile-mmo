pub mod terrain;

use bevy::{prelude::*, tasks::Task};
use renet::ClientId;
use bimap::BiMap;

use crate::common::message::Do;

#[derive(Default, Deref, DerefMut, Resource)]
pub struct Lobby(BiMap<ClientId, Entity>);

#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
}

#[derive(Default, Resource)]
pub struct AsyncTasks {
    pub task_behaviour_pathfind: Option<Task<Vec<Do>>>,
}