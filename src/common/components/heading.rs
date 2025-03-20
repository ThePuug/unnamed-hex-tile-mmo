use std::f32::consts::PI;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::hx::*;

impl From<Heading> for Vec3 {
    fn from(val: Heading) -> Self {
        Vec3::from(val.0) * Vec3::new(0.25, 1., 0.25)
    }
}

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heading(pub Hx);

impl From<Heading> for Quat {
    fn from(value: Heading) -> Self {
        match (value.0.q, value.0.r) {
            (-1, 0) => Quat::from_rotation_y(PI*9./6.),
            (-1, 1) => Quat::from_rotation_y(PI*11./6.),
            (0, 1)  => Quat::from_rotation_y(PI*1./6.),
            (1, 0)  => Quat::from_rotation_y(PI*3./6.),
            (1, -1) => Quat::from_rotation_y(PI*5./6.),
            (0, -1) => Quat::from_rotation_y(PI*7./6.),
            _  => Quat::from_rotation_y(0.),
        }
    }
}
