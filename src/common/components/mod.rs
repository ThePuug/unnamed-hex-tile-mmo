pub mod heading;
pub mod keybits;
pub mod offset;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecoratorDescriptor {
    pub index: usize,
    pub is_solid: bool,
}

#[derive(Clone, Component, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EntityType {
    Actor,
    Decorator(DecoratorDescriptor),
}

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)] 
pub struct Actor;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();