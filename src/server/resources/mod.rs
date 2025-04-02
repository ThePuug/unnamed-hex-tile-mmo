pub mod terrain;

use std::collections::HashMap;

use bevy::prelude::*;
use renet::ClientId;
use bimap::BiMap;

use crate::common::resources::InputQueue;

#[derive(Default, Deref, DerefMut, Resource)]
pub struct Lobby(BiMap<ClientId, Entity>);

#[derive(Default, Deref, DerefMut, Resource)]
pub struct InputQueues(HashMap<Entity, InputQueue>);

#[derive(Default, Resource)]
pub struct RunTime {
    pub elapsed_offset: u128,
}
