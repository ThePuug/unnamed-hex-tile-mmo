use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::common::{
    chunk::ChunkId,
    components::{ behaviour::*, entity_type::*, heading::*, keybits::*, offset::*, resources::*, * },
    systems::gcd::*,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    /// Combat state change (Do event - broadcast to clients)
    CombatState { ent: Entity, in_combat: bool },
    /// Server → Client: chunk data containing up to 64 tiles (8x8)
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: ArrayVec<[(Qrz, EntityType); 64]>,
    },
    /// Entity died (Try event - server-internal only)
    Death { ent: Entity },
    Despawn { ent: Entity },
    Discover { ent: Entity, qrz: Qrz },
    /// Server-side only: request to discover a chunk
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },
    Gcd { ent: Entity, typ: GcdType },
    /// Update health (Do event - broadcast to clients)
    Health { ent: Entity, current: f32, max: f32 },
    Init { ent: Entity, dt: u128 },
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    Incremental { ent: Entity, component: Component },
    /// Update mana (Do event - broadcast to clients)
    Mana { ent: Entity, current: f32, max: f32, regen_rate: f32 },
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz, attrs: Option<ActorAttributes> },
    /// Update stamina (Do event - broadcast to clients)
    Stamina { ent: Entity, current: f32, max: f32, regen_rate: f32 },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Component {
    Behaviour(Behaviour),
    CombatState(CombatState),
    Health(Health),
    Heading(Heading),
    KeyBits(KeyBits),
    Loc(Loc),
    Mana(Mana),
    Offset(Offset),
    Stamina(Stamina),
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try {
    pub event: Event
}
