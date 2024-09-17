use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ *,
    keybits::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub enum Event {
    Spawn { ent: Entity, typ: EntityType, translation: Vec3 },
    Despawn { ent: Entity },
    Input { ent: Entity, key_bits: KeyBits },
}

#[derive(Component, Debug, Deserialize, Serialize)]
pub enum Message {
    Do { event: Event },
    Try { event: Event },
}
