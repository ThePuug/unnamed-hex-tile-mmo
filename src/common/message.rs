use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ *,
    common::{
        components::{
            heading::*,
            hx::*,
            keybits::*,
            offset::*,
        },
        systems::gcd::*,
    },
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    Discover { ent: Entity, hx: Hx },
    Init { ent: Entity, dt: u128 },
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    Gcd { ent: Entity, typ: GcdType },
    Incremental { ent: Entity, attr: Attribute },
    Spawn { ent: Entity, typ: EntityType, hx: Hx },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Attribute {
    Hx { hx: Hx },
    Heading { heading: Heading },
    Offset { offset: Offset },
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event 
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try { 
    pub event: Event 
}
