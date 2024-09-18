use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::hx::*;

pub mod message;
pub mod keybits;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct DecoratorDescriptor {
    pub index: usize,
    pub is_solid: bool,
}

#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub enum EntityType {
    Actor,
    Decorator(DecoratorDescriptor),
}

#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Pos {
    pub hx: Hx,
    pub offset: Vec3,
}

pub trait IntoScreen {
    fn into_screen(self) -> Vec3;
}

impl IntoScreen for Pos {
    fn into_screen(self) -> Vec3 {
        let v: Vec3 = self.hx.into();
        Vec3 { z: (self.hx.z - self.hx.r) as f32, ..v } + self.offset
    }
}
