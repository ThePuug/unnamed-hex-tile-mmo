pub mod terrain;

use std::collections::HashMap;

use bevy::prelude::*;
use renet::ClientId;
use bimap::BiMap;

use crate::common::resources::InputQueue;

#[derive(Default, Resource)]
pub struct Lobby(pub BiMap<ClientId, Entity>);

#[derive(Default, Resource)]
pub struct InputQueues(pub HashMap<Entity, InputQueue>);