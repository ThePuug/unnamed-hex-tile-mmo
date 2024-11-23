pub mod heading;
pub mod hx;
pub mod keybits;
pub mod offset;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::hx::*;

pub trait Calculate<T> {
    fn calculate(self) -> T;
}

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

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)] 
pub struct Actor;

