
use bevy::ecs::event::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Event, Hash, PartialEq, Serialize)]
pub enum GcdType {
    Attack,
}
