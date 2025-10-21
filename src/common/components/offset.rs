use bevy::prelude::*;

use crate::common::components::*;

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Offset{
    pub state: Vec3,
    pub step: Vec3,
    pub prev_step: Vec3,
}
