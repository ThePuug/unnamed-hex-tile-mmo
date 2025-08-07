use tinyvec::ArrayVec;
use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Behaviour {
    Controlled,
    Pathfind(Pathfind),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Pathfind {
    pub dest: Qrz,
    pub path: ArrayVec<[Qrz; 20]>,
}