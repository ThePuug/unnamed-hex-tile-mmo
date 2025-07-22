
use bevy::ecs::event::Event;
use serde::{Deserialize, Serialize};

use crate::common::components::EntityType;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Event, PartialEq, Serialize)]
pub enum GcdType {
    Attack,
    Spawn(EntityType),
}
