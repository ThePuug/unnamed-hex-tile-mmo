use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::common::{
    chunk::ChunkId,
    components::{ behaviour::*, entity_type::*, heading::*, keybits::*, offset::*, reaction_queue::*, resources::*, * },
    systems::{combat::gcd::*, targeting::RangeTier},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    Discover { ent: Entity, qrz: Qrz },
    /// Server-side only: request to discover a chunk
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },
    /// Server → Client: chunk data containing up to 256 tiles (16x16)
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: ArrayVec<[(Qrz, EntityType); 256]>,
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
        ability: Option<AbilityType>,
    },
    /// Server → Client: Insert threat into reaction queue
    InsertThreat { ent: Entity, threat: QueuedThreat },
    /// Server → Client: Apply damage to entity (threat resolved)
    ApplyDamage { ent: Entity, damage: f32, source: Entity },
    /// Server-internal: Resolve a threat (apply damage with modifiers)
    ResolveThreat { ent: Entity, threat: QueuedThreat },
    /// Client → Server: Use an ability (Try event)
    /// target_loc: Optional target hex location (for validation and targeting)
    UseAbility { ent: Entity, ability: AbilityType, target_loc: Option<Qrz> },
    /// Server → Client: Ability usage failed
    AbilityFailed { ent: Entity, reason: AbilityFailReason },
    /// Server → Client: Clear threats from queue
    ClearQueue { ent: Entity, clear_type: ClearType },
    /// Client → Server: Measure network latency (client timestamp)
    Ping { client_time: u128 },
    /// Server → Client: Response to ping (echoes client timestamp)
    Pong { client_time: u128 },
    /// Client → Server: Set tier lock for targeting (ADR-010 Phase 1)
    SetTierLock { ent: Entity, tier: RangeTier },
    /// Server → Client: Entity intends to move to destination (ADR-011)
    /// Sent when movement starts (before completion) to enable client-side prediction
    MovementIntent {
        ent: Entity,
        destination: Qrz,   // Target tile
        duration_ms: u16,   // Expected travel time (for speed scaling)
    },
}

/// Types of abilities that can be used (ADR-009 MVP ability set)
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AbilityType {
    /// Q: Gap closer - teleport adjacent to target (4 hex range, 20 stam, 40 dmg)
    Lunge,
    /// W: Heavy strike - high damage melee attack (1 hex, 40 stam, 80 dmg, 2s CD)
    Overpower,
    /// E: Positioning - push target 1 hex away (2 hex range, 30 stam, 1.5s CD)
    Knockback,
    /// R: Emergency defense - clear all queued threats (50 stam, 0.5s GCD)
    Deflect,
    /// Passive: Auto-attack when adjacent to hostile (20 dmg every 1.5s, free)
    AutoAttack,
    /// NPC: Ranged attack with telegraph (20 dmg, 3s CD, 5-8 hex range)
    Volley,
}

/// Reasons why an ability usage might fail
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AbilityFailReason {
    InsufficientStamina,
    InsufficientMana,
    NoTargets,
    OnCooldown,
    InvalidTarget,
    OutOfRange,
}

/// Types of queue clears for reaction abilities
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClearType {
    /// Clear all threats (Dodge)
    All,
    /// Clear first N threats (Counter, Parry - future)
    First(usize),
    /// Clear last N threats (Knockback - removes most recent threat)
    Last(usize),
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
    PlayerControlled(PlayerControlled),
    Stamina(Stamina),
    TierLock(crate::common::components::tier_lock::TierLock),
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Do {
    pub event: Event
}

#[derive(Clone, Copy, Debug, Deserialize, Event, Serialize)]
pub struct Try {
    pub event: Event
}
