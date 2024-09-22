use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ *,
    common::components::hx::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Spawn { ent: Entity, typ: EntityType, hx: Hx },
    Despawn { ent: Entity },
    Move { ent: Entity, hx: Hx, heading: Heading },
    Discover { hx: Hx },
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event 
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try { 
    pub event: Event 
}

