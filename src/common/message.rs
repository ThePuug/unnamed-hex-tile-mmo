use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

use crate::common::{
    components::{ behaviour::*, entity_type::*, heading::*, keybits::*, offset::*, * },
    systems::gcd::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    Discover { ent: Entity, qrz: Qrz },
    Gcd { ent: Entity, typ: GcdType },
    Init { ent: Entity, dt: u128 },
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    Incremental { ent: Entity, component: Component },
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Component {
    Loc(Loc),
    Heading(Heading),
    KeyBits(KeyBits),
    Offset(Offset),
    Behaviour(Behaviour),
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event 
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try { 
    pub event: Event 
}
