use bevy::prelude::*;

use crate::common::components::*;

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Offset{
    pub state: Vec3,
    pub step: Vec3,
}

impl Calculate<Vec3> for (Hx, Vec3) {
    fn calculate(self) -> Vec3 {
        let v: Vec3 = Vec3{ z: self.0.z as f32 + self.1.z, ..self.0.into()};
        let z = ((v.z - self.0.r as f32 * 100.) / 2_i32.pow(16) as f32) * 1000.;
        v + Vec3 { x: 0., y: v.z * TILE_RISE, z } + self.1.xy().extend(0.)
    }
}
