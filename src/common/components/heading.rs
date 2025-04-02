use std::f32::consts::PI;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::{
    hx::*,
    keybits::*,
};

pub const HERE: Vec3 = Vec3::new(0.33, 0., 0.33);

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heading(Hx);

impl Heading {
    pub fn new(hx: Hx) -> Self {
        Self(hx)
    }
}

impl From<Heading> for Quat {
    fn from(value: Heading) -> Self {
        match (value.q, value.r) {
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

impl From<KeyBits> for Heading {
    fn from(value: KeyBits) -> Self {
        Heading::new(if value.all_pressed([KB_HEADING_Q, KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 1, r: -1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q, KB_HEADING_R]) { Hx { q: -1, r: 1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q, KB_HEADING_NEG]) { Hx { q: -1, r: 0, z: 0 } }
            else if value.all_pressed([KB_HEADING_R, KB_HEADING_NEG]) { Hx { q: 0, r: -1, z: 0 } }
            else if value.all_pressed([KB_HEADING_Q]) { Hx { q: 1, r: 0, z: 0 } }
            else if value.all_pressed([KB_HEADING_R]) { Hx { q: 0, r: 1, z: 0 } }
            else { Hx::default() })
    }
}