use bevy::prelude::*;

use crate::common::components::*;

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Offset{
    pub state: Vec3,
    pub step: Vec3,
    pub prev_step: Vec3,
    /// Elapsed time since last position update (for NPC interpolation), in seconds
    #[serde(skip)]
    pub interp_elapsed: f32,
    /// Expected duration to reach step from prev_step (for NPC interpolation), in seconds
    #[serde(skip)]
    pub interp_duration: f32,
}
