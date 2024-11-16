use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ *,
    common::components::{
        hx::*,
        keybits::*,
    },
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    Discover { ent: Entity, hx: Hx },
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    Move { ent: Entity, hx: Hx, heading: Heading },
    Spawn { ent: Entity, typ: EntityType, hx: Hx },
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event 
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try { 
    pub event: Event 
}

