use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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