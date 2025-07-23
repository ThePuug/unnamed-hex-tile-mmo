pub mod resources;
pub mod systems;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::behaviour::*;

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Tick {
    pub ent: Entity,
    pub behaviour: Behaviour,
}