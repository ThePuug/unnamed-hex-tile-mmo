use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::hx::*;

impl Into<Vec3> for Heading {
    fn into(self) -> Vec3 {
        Vec2::default().lerp(Vec3::from(self.0).xy(), 0.25).extend(self.0.z as f32)
    }
}

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heading(pub Hx);
