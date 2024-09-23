use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ *,
    common::components::hx::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    Discover { hx: Hx },
    Input { ent: Entity, key_bits: KeyBits },
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

