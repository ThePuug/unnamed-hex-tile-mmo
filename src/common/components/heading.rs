use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::hx::*;

impl From<Heading> for Vec3 {
    fn from(val: Heading) -> Self {
        Vec2::default().lerp(Vec3::from(val.0).xy(), 0.25).extend(val.0.z as f32)
    }
}

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heading(pub Hx);
