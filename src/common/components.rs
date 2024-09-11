use bevy::prelude::*;
use renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::common::hxpx::Hx;

pub type InputKeys = u8;

pub enum KeyBit {
    UP = 1 << 0, 
    DOWN = 1 << 1, 
    LEFT = 1 << 2, 
    RIGHT = 1 << 3, 
    // JUMP = 1 << 4,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum EntityType {
    Player,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Event {
    Spawn { ent: Entity, typ: EntityType, pos: Hx },
    Despawn { ent: Entity },
    Input { ent: Entity, input: Input },
}

#[derive(Component, Debug, Deserialize, Serialize)]
pub struct Player {
    pub id: ClientId,
    pub pos: Hx,
}

#[derive(Component, Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Input {
    pub keys: InputKeys,
}

#[derive(Component, Debug, Deserialize, Serialize)]
pub enum Message {
    Do { event: Event },
    Try { event: Event },
}
