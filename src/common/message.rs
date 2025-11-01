use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::common::{
    chunk::ChunkId,
    components::{ behaviour::*, entity_type::*, heading::*, keybits::*, offset::*, reaction_queue::*, resources::*, * },
    systems::combat::gcd::*,
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
    /// Server-internal: Deal damage (Try event)
    /// Triggers damage calculation and queue insertion
    DealDamage {
        source: Entity,
        target: Entity,
        base_damage: f32,
        damage_type: DamageType,
    },
    /// Server → Client: Insert threat into reaction queue
    InsertThreat { ent: Entity, threat: QueuedThreat },
    /// Server → Client: Apply damage to entity (threat resolved)
    ApplyDamage { ent: Entity, damage: f32, source: Entity },
    /// Server-internal: Resolve a threat (apply damage with modifiers)
    ResolveThreat { ent: Entity, threat: QueuedThreat },
    /// Client → Server: Use an ability (Try event)
    UseAbility { ent: Entity, ability: AbilityType },
    /// Server → Client: Ability usage failed
    AbilityFailed { ent: Entity, reason: AbilityFailReason },
    /// Server → Client: Clear threats from queue
    ClearQueue { ent: Entity, clear_type: ClearType },
    /// Client → Server: Measure network latency (client timestamp)
    Ping { client_time: u128 },
    /// Server → Client: Response to ping (echoes client timestamp)
    Pong { client_time: u128 },
}

/// Types of abilities that can be used
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AbilityType {
    BasicAttack,
    Dodge,
}

/// Reasons why an ability usage might fail
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AbilityFailReason {
    InsufficientStamina,
    InsufficientMana,
    NoTargets,
    OnCooldown,
    InvalidTarget,
}

/// Types of queue clears for reaction abilities
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClearType {
    /// Clear all threats (Dodge)
    All,
    /// Clear first N threats (Counter, Parry - future)
    First(usize),
    /// Clear threats by damage type (Ward - future)
    ByType(DamageType),
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
