use bevy::prelude::*;

use crate::common::components::*;

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Offset{
    pub state: Vec3,
    pub step: Vec3,
}

impl Calculate<Vec3> for (Hx, Vec3) {
    fn calculate(self) -> Vec3 {
        // apply z-offset before we "flatten"
        let v: Vec3 = Vec3{ z: self.0.z as f32 + self.1.z, ..self.0.into()};
        // we want higher r-values to display behind lower r-values
        // we want to allow for 100 units of map depth variation on screen
        // we want to allow for map size of -2^16 to 2^16
        // we want to allow for total map depth of -1000 to 1000
        let z = ((v.z - self.0.r as f32 * 100.) / 2_i32.pow(16) as f32) * 1000.;
        // apply xy-offset after we "flatten"
        v + Vec3 { x: 0., y: v.z * TILE_RISE, z } + self.1.xy().extend(0.)
    }
}
