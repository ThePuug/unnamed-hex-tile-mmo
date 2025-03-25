use std::{
    f64::consts::SQRT_3, 
    ops::{Add, Sub}
};

use bevy::prelude::*;
use fixed::{types::extra::U0, FixedI16};
use serde::{Deserialize, Serialize};

pub const TILE_SIZE: f32 = 1.; // half-width furthest from center
pub const TILE_RISE: f32 = TILE_SIZE * 0.8;
// note orientation is negated to make +z move into the screen and +x move to the right
const ORIENTATION: ([f64; 4], [f64; 4]) = (
    [-SQRT_3, -SQRT_3/2., -0., -3./2.],
    [-SQRT_3/3., 1./3., -0., -2./3.],
);

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Hx {
    pub q: i16,
    pub r: i16,
    pub z: i16,
}

impl Hx {
    pub fn distance(&self, other: &Hx) -> i16 {
        *[
            (self.q - other.q).abs(),
            (self.r - other.r).abs(),
            (-self.q-self.r - (-other.q-other.r)).abs()
        ].iter().max().unwrap()
    }
}

impl Add<Hx> for Hx {
    type Output = Hx;
    fn add(self, rhs: Hx) -> Self::Output {
        Hx { q: self.q + rhs.q, r: self.r + rhs.r, z: self.z + rhs.z }
    }
}

impl Sub<Hx> for Hx {
    type Output = Hx;
    fn sub(self, rhs: Hx) -> Self::Output {
        Hx { q: self.q - rhs.q, r: self.r - rhs.r, z: self.z - rhs.z }
    }
}

impl From<Vec3> for Hx {
    fn from(px: Vec3) -> Hx {
        let q = (ORIENTATION.1[0] * px.x as f64 + ORIENTATION.1[1] * px.z as f64) / TILE_SIZE as f64;
        let r = (ORIENTATION.1[2] * px.x as f64 + ORIENTATION.1[3] * px.z as f64) / TILE_SIZE as f64;
        let z = px.y as f64 / TILE_RISE as f64;
        round(q, r, z)
    }
}

impl From<Hx> for Vec3 {
    fn from(hx: Hx) -> Vec3 {
        let x = (ORIENTATION.0[0] * hx.q as f64 + ORIENTATION.0[1] * hx.r as f64) * TILE_SIZE as f64;
        let z = (ORIENTATION.0[2] * hx.q as f64 + ORIENTATION.0[3] * hx.r as f64) * TILE_SIZE as f64;
        let y = hx.z as f64 * TILE_RISE as f64;
        Vec3 { x: x as f32, y: y as f32, z: z as f32 }
    }
}

impl From<Hx> for [FixedI16<U0>; 4] {
    fn from(hx: Hx) -> [FixedI16<U0>; 4] {
        [hx.q.into(), hx.r.into(), (-hx.q-hx.r).into(), hx.z.into()]
    }
}

fn round(q0: f64, r0: f64, z0: f64) -> Hx {
    let s0 = -q0-r0;
    let mut q = q0.round();
    let mut r = r0.round();
    let s = s0.round();

    let q_diff = (q - q0).abs();
    let r_diff = (r - r0).abs();
    let s_diff = (s - s0).abs();

    if q_diff > r_diff && q_diff > s_diff {
        q = -r-s;
    } else if r_diff > s_diff {
        r = -q-s;
    }

    Hx { q: q as i16, r: r as i16, z: z0.round() as i16 }
}
