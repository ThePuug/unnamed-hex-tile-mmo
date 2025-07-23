use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    Controlled,
    Wander,
}