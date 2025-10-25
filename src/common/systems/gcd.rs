
use bevy::ecs::event::Event;
use serde::{Deserialize, Serialize};

use crate::common::components::{entity_type::*, spawner::*};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Event, PartialEq, Serialize)]
pub enum GcdType {
    Attack,
    Spawn(EntityType),
    PlaceSpawner(Spawner),
}
