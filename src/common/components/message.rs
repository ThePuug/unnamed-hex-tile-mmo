use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::components::{ *,
    keybits::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub enum Event {
    Spawn { ent: Entity, typ: EntityType, hx: Hx },
    Despawn { ent: Entity },
    Input { ent: Entity, key_bits: KeyBits, dt: u8 },
    Move { ent: Entity, pos: Pos, heading: Heading },
}

#[derive(Component, Debug, Deserialize, Serialize)]
pub enum Message {
    Do { event: Event },
    Try { event: Event },
}
