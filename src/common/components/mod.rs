pub mod message;
pub mod keybits;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::hx::*;

pub const KEYBIT_UP: u8 = 1 << 0;
pub const KEYBIT_DOWN: u8 = 1 << 1; 
pub const KEYBIT_LEFT: u8 = 1 << 2; 
pub const KEYBIT_RIGHT: u8 = 1 << 3; 

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

#[derive(Clone, Component, Copy, Default)]
pub struct Heading(pub Hx);

#[derive(Clone, Component, Copy, Default)] 
pub struct Actor;