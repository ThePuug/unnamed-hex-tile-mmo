use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    Controlled,
    Wander(Wander),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Wander {
    pub qrz: Qrz,
}