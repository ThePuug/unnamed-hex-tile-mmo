use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::keybits::KeyBits;

#[derive(Debug, Deserialize, Serialize)]
pub enum EntityType {
    Player,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Event {
    Spawn { ent: Entity, typ: EntityType },
    Despawn { ent: Entity },
    Input { ent: Entity, key_bits: KeyBits },
}

#[derive(Component, Debug, Deserialize, Serialize)]
pub enum Message {
    Do { event: Event },
    Try { event: Event },
}
