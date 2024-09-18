use std::ops::Add;

use bevy::math::Vec3;
use serde::{Deserialize, Serialize};

const SQRT_3: f32 = 1.7320508;
pub const TILE_SIZE: f32 = 24.;
pub const ISO_SCALE: f32 = 3. / 4.;
pub const TILE_SIZE_H: f32 = TILE_SIZE * 2. * ISO_SCALE;
pub const TILE_SIZE_W: f32 = SQRT_3 * TILE_SIZE;
const ORIENTATION: ([f32; 4], [f32; 4], f32) = (
    [SQRT_3, SQRT_3/2., 0., 3./2.],
    [SQRT_3/3., -1./3., 0., 2./3.],
    0.5
);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Hx {
    pub q: i16,
    pub r: i16,
    pub z: i16
}

impl Add<Hx> for Hx {
    type Output = Hx;
    fn add(self, rhs: Hx) -> Self::Output {
        Hx { q: self.q + rhs.q, r: self.r + rhs.r, z: self.z + rhs.z }
    }
}

impl From<Vec3> for Hx {
    fn from(px: Vec3) -> Hx {
        let px = Vec3 { x: px.x / TILE_SIZE, y: px.y / (ISO_SCALE * TILE_SIZE), z: px.z };
        let q = ORIENTATION.1[0] * px.x + ORIENTATION.1[1] * px.y;
        let r = ORIENTATION.1[2] * px.x + ORIENTATION.1[3] * px.y;
        Hx { q: q.round() as i16, r: r.round() as i16, z: px.z.round() as i16 }
    }
}

impl From<Hx> for Vec3 {
    fn from(hx: Hx) -> Vec3 {
        let x = (ORIENTATION.0[0] * hx.q as f32 + ORIENTATION.0[1] * hx.r as f32) * TILE_SIZE;
        let y = (ORIENTATION.0[2] * hx.q as f32 + ORIENTATION.0[3] * hx.r as f32) * ISO_SCALE * TILE_SIZE;
        let z = hx.z as f32;
        Vec3 { x, y, z }
    }
}
