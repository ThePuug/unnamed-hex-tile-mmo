use tinyvec::ArrayVec;
use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    #[default] Unset,
    Controlled,
}

#[derive(Clone, Component, Debug, Default, Deserialize, Serialize)]
pub struct PathTo {
    pub dest: Qrz,
    pub path: ArrayVec<[Qrz; 20]>,
}