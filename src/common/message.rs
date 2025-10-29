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
    Despawn { ent: Entity },
    Discover { ent: Entity, qrz: Qrz },
    /// Server-side only: request to discover a chunk
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },
    /// Server → Client: chunk data containing up to 64 tiles (8x8)
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: ArrayVec<[(Qrz, EntityType); 64]>,
    },
    Gcd { ent: Entity, typ: GcdType },
    Init { ent: Entity, dt: u128 },
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    Incremental { ent: Entity, component: Component },
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz, attrs: Option<ActorAttributes> },
    /// Entity died (Try event - server-internal only)
    Death { ent: Entity },
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
