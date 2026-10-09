use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::{
    chunk::ChunkId,
    components::{ behaviour::*, entity_type::*, equipment::{Equipment, Inventory, Item}, heading::*, keybits::*, position::Position, reaction_queue::*, resources::*, * },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Event {
    Despawn { ent: Entity },
    /// Server-side only: request to discover a chunk and send ChunkData to client
    DiscoverChunk { ent: Entity, chunk_id: ChunkId },
    /// Server → Client: chunk data (hex chunk, radius 9, up to 271 tiles).
    /// Tiles are ordered by `chunk_tiles(chunk_id)` iteration order — receiver
    /// reconstructs (q, r) by zipping with the same iterator. Per tile: z,
    /// type, and the surface water stands at, or None where dry.
    ChunkData {
        ent: Entity,
        chunk_id: ChunkId,
        tiles: ArrayVec<[(i32, EntityType, Option<i32>); 272]>,
    },
    Init { ent: Entity, dt: u128 },
    /// Client → Server: `dt` milliseconds of input `seq` on the client.s own
    /// clock. A new `seq` opens an input; the same `seq` again extends it.
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    /// Server → Client: input `seq` closed with the entity here. The client
    /// adopts the position and replays only the inputs still open.
    Confirm { ent: Entity, seq: u8, position: Position, airtime: Option<i16>, turn: Turn },
    Incremental { ent: Entity, component: Component },
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz, attrs: Option<ActorAttributes> },
    /// Server-internal: Deal damage (Try event)
    /// Triggers damage calculation and queue insertion
    DealDamage {
        source: Entity,
        target: Entity,
        base_damage: f32,
        ability: Option<AbilityType>,
        /// Damage each DoT tick deals while the threat stands: a wound's, zero for a blow
        dot: f32,
        /// Share of its target's speed the threat slows away as it lands
        /// (`QueuedThreat::bind`), zero for most
        bind: f32,
        /// How long after now the strike is made: its threat's window starts then
        delay: std::time::Duration,
    },
    /// Server-internal: a wound's DoT tick lands outside the queue
    DotTick { ent: Entity, source: Entity, damage: f32, ability: Option<AbilityType> },
    /// Server-internal: `ent` struck across its own line and broke its stride
    Stumble { ent: Entity },
    /// Server → Client: Insert threat into reaction queue
    InsertThreat { ent: Entity, threat: QueuedThreat },
    /// Server → Client: Apply damage to entity (threat resolved). `dot` marks
    /// a wound's damage, shown apart from a blow's.
    ApplyDamage { ent: Entity, damage: f32, source: Entity, dot: bool },
    /// Server-internal: Resolve a threat (apply damage with modifiers)
    ResolveThreat { ent: Entity, threat: QueuedThreat },
    /// Client → Server (Try): Request to use an ability
    /// Server → Client (Do): Ability was used successfully
    /// target: the actor it is used on, a player's choice the server checks
    /// at: the moment it was used on the game clock, a player's key press
    /// as its client stamped it
    /// arrived: in a Do, the server's clock as the press reached it, which
    /// tells the client how far ahead its press came; a Try's is unread.
    /// The server holds a press until the later of the two and judges a
    /// reaction's band there (`abilities::Press`)
    UseAbility { ent: Entity, ability: AbilityType, target: Option<Entity>, at: std::time::Duration, arrived: std::time::Duration },
    /// Server → Client: Clear threats from queue
    ClearQueue { ent: Entity, clear_type: ClearType },
    /// Client → Server: ask for the server's game world time
    Ping,
    /// Server → Client: the answer to a Ping, the server's game world time
    /// as it answered
    Pong { dt: u128 },
    /// Server → Client: the state a remote entity is simulated from. Sent
    /// when any of it changes and at every tile crossing while moving.
    MovementIntent { ent: Entity, position: Position, heading: Heading, moving: bool, back: bool, airtime: Option<i16>, burdened: bool },
    /// Server → Client: the entity slides to a standing-height tile under an
    /// ability (lunge, knockback), arriving after `duration_ms`; round the
    /// tile `around` on its ring when given (a flank), else the straight way.
    Displace { ent: Entity, destination: Qrz, duration_ms: u16, around: Option<Qrz> },
    /// Client → Server: put the entity on the ground at this tile. An admin
    /// request; the server decides the height and answers with the tile
    /// update every client already treats as a teleport.
    Teleport { ent: Entity, q: i32, r: i32 },
    /// Client → Server: place a party of `size` `archetype`s at `level`
    /// ahead of the actor `ent`: on the wild side, beyond the range its
    /// pack acquires a target from, a den; or with `engage`, on a side of
    /// its own where it acquires them, as the balance arena sets its teams
    /// apart. Staging a fight is a party, a view of it, then its
    /// opposition engaged on the viewed fighter. An admin request, as
    /// ungated on the wire as `Teleport`.
    SpawnParty {
        ent: Entity,
        archetype: crate::archetype::EnemyArchetype,
        level: u8,
        size: u8,
        engage: bool,
    },
    /// Server → Client: evict these chunks (tiles + meshes). Server-authoritative
    /// to prevent client/server sync drift on which chunks are loaded.
    EvictChunks { ent: Entity, chunks: ArrayVec<[ChunkId; 64]> },
    /// Server → Client: batch of summary hex updates for the visual frontier.
    /// Summaries beyond the fixed streaming radius are computed server-side.
    SummaryBatch {
        ent: Entity,
        additions: Vec<SummaryData>,
        removals: Vec<SummaryKey>,
    },
    /// Client → Server (Try): wear an item from the bag, or take it off
    Wear { ent: Entity, item: Item, on: bool },
    /// Server → Client: the bag, sent to its owner only
    Inventory { ent: Entity, bag: Inventory },
    /// Client → Server (Try): gather what stands in `slot` of tile `(q, r)`
    Gather { ent: Entity, q: i32, r: i32, slot: u8 },
    /// Server → Client: tile `(q, r)` now holds `cover`, sent to `ent`, a
    /// player holding the tile's chunk. The whole cover, so applying it
    /// twice or after the chunk's own copy leaves the same tile.
    CoverChanged { ent: Entity, q: i32, r: i32, cover: common::Cover },
    /// Server → Client: the den whose pack stands on tile `at`, as drawn,
    /// or None where none stands there any more. Sent to `ent`, a player
    /// within sight of it; the whole den, so applying it twice leaves the
    /// same.
    Den { ent: Entity, at: Qrz, den: Option<crate::den::DenLook> },
    /// Server → Client: the loot window `ent` has open, a stack to an
    /// entry, or None where it has none open. Sent to its owner only.
    Loot { ent: Entity, entries: Option<Vec<common::Stack>> },
    /// Client → Server (Try): take entry `entry` of the open loot window,
    /// or every entry where None
    Take { ent: Entity, entry: Option<u8> },
    /// Client → Server (Try): close the open loot window
    CloseLoot { ent: Entity },
    /// Client → Server (Try): drop `count` of `kind` from the bag onto the
    /// tile the player stands on
    Drop { ent: Entity, kind: common::Stackable, count: u32 },
    /// Server → Client: what `ent` is seen doing at a gather, or nothing
    /// where None. Sent to every client that sees it, so each shows it.
    Activity { ent: Entity, activity: Option<common::gathering::Activity> },
    /// Client → Server (Try): put `ent`'s levels into `pairs` instead
    /// Server → Client (Do): the respec was taken
    RespecAttributes { ent: Entity, pairs: [Pair; 3] },
    /// Client → Server: put this connection's character in the world. The
    /// server answers with `Init`; a connection already playing is ignored.
    Play,
    /// Client → Server: take this connection's character out of the world,
    /// keeping the connection open to play again.
    Leave,
    /// Client → Server: see the world as the actor `ent`, without control:
    /// the connection's character leaves the world while it views, and
    /// `Leave` then `Play` bring a fresh one back. An admin request, as
    /// ungated on the wire as `Teleport`.
    /// Server → Client: this connection now sees the world as `ent`.
    View { ent: Entity },
}

impl Event {
    /// The entity this event is about, where it is about one: the id the
    /// wire exchanges, a client's for the server's and back. Other entities
    /// an event names (a blow's source, an ability's target) are its
    /// handler's to exchange.
    pub fn ent_mut(&mut self) -> Option<&mut Entity> {
        match self {
            Event::Despawn { ent }
            | Event::DiscoverChunk { ent, .. }
            | Event::ChunkData { ent, .. }
            | Event::Init { ent, .. }
            | Event::Input { ent, .. }
            | Event::Confirm { ent, .. }
            | Event::Incremental { ent, .. }
            | Event::Spawn { ent, .. }
            | Event::DotTick { ent, .. }
            | Event::Stumble { ent }
            | Event::InsertThreat { ent, .. }
            | Event::ApplyDamage { ent, .. }
            | Event::ResolveThreat { ent, .. }
            | Event::UseAbility { ent, .. }
            | Event::ClearQueue { ent, .. }
            | Event::MovementIntent { ent, .. }
            | Event::Displace { ent, .. }
            | Event::Teleport { ent, .. }
            | Event::SpawnParty { ent, .. }
            | Event::EvictChunks { ent, .. }
            | Event::SummaryBatch { ent, .. }
            | Event::Wear { ent, .. }
            | Event::Inventory { ent, .. }
            | Event::Gather { ent, .. }
            | Event::CoverChanged { ent, .. }
            | Event::Den { ent, .. }
            | Event::Loot { ent, .. }
            | Event::Take { ent, .. }
            | Event::CloseLoot { ent }
            | Event::Drop { ent, .. }
            | Event::Activity { ent, .. }
            | Event::RespecAttributes { ent, .. }
            | Event::View { ent } => Some(ent),
            Event::DealDamage { .. } | Event::Ping { .. } | Event::Pong { .. } | Event::Play | Event::Leave => None,
        }
    }
}

/// What an actor uses in a fight: the auto-attack, and the early kit, one
/// skill to an archetype, each showing what its attribute's commitment buys.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum AbilityType {
    /// The swing every actor makes on its own clock, free
    AutoAttack,
    /// The Berserker's bite. Its combo is itself, so Ferocity fires bites in a burst
    Frenzy,
    /// A light strike, cheap and quickly recovered from, on every bar. Its
    /// combo is a Parry
    Feint,
    /// The Juggernaut's heavy blow
    Overpower,
    /// The Ambusher's strike, harder on a target still in recovery
    Punish,
    /// A reaction clearing the threats in its span, on every bar;
    /// Preparation chains it through a recovery. Its combo is a Feint
    Parry,
    /// The Defender's reaction, clearing every threat in its span and
    /// sending a share of each back; Awareness lengthens the span
    Counter,
    /// The Skirmisher's leap: clear of a target in its reach, or onto one
    /// out of it
    Leap,
    /// The Flanker's stride: for a while its strikes past the forward faces
    /// break no stride, so Grace strikes on the run
    PerfectStride,
}

impl AbilityType {
    /// Whether it is a reaction, an answer to what is queued, where every
    /// other skill is an action. Discipline's Preparation fires a reaction
    /// early (`combos::may_use`).
    pub fn is_reaction(self) -> bool {
        matches!(self, AbilityType::Parry | AbilityType::Counter)
    }

    /// Whether using it now is a reaction: one always, and a Leap with its
    /// target `in_reach`, which leaps clear and answers its span as a
    /// reaction does
    pub fn reacts(self, in_reach: bool) -> bool {
        self.is_reaction() || (self == AbilityType::Leap && in_reach)
    }

    /// How near and how far, in tiles, `ability` strikes a target from an
    /// actor whose own reach is `own`: as far as the actor reaches. None
    /// for one the gate checks no target for: a reaction, a Leap, which
    /// checks its own, or the stride.
    pub fn reach(self, own: i32) -> Option<std::ops::RangeInclusive<i32>> {
        match self {
            AbilityType::AutoAttack | AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower | AbilityType::Punish => Some(0..=own),
            AbilityType::Parry | AbilityType::Counter | AbilityType::Leap | AbilityType::PerfectStride => None,
        }
    }

    /// The ability this one offers as its combo, the one that unlocks
    /// through its recovery ahead of the rest (`combos::recovery_after`): a
    /// bite's is another bite, and a Parry and a Feint offer each other.
    /// None for an ability that offers nothing.
    pub fn combo(self) -> Option<AbilityType> {
        match self {
            AbilityType::Frenzy => Some(AbilityType::Frenzy),
            AbilityType::Parry => Some(AbilityType::Feint),
            _ => None,
        }
    }
}

/// Types of queue clears for reaction abilities
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClearType {
    /// Clear every threat landing at `at` or within `span` after it
    /// (`ReactionQueue::swept`): what a reaction takes
    Span { at: std::time::Duration, span: std::time::Duration },
    /// Clear the one threat `source` inserted at `inserted_at`, wherever it
    /// stands: an expiry, since threats from different sources expire out of
    /// queue order.
    Threat { source: Entity, inserted_at: std::time::Duration },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Component {
    CombatState(CombatState),
    Endurance(Endurance),
    Equipment(Equipment),
    Health(Health),
    Heading(Heading),
    Loc(Loc),
    Mana(Mana),
    PlayerControlled(PlayerControlled),
    Recovery(crate::components::recovery::GlobalRecovery),
    Returning(crate::components::returning::Returning),
    Side(crate::components::behaviour::Side),
    Status(crate::components::status::Status),
}

impl Component {
    /// Insert this component into an entity via commands.
    /// Panics on Loc/Heading — those require special handling in do_incremental.
    pub fn insert_into(self, entity: &mut EntityCommands) {
        match self {
            Component::CombatState(v) => { entity.insert(v); }
            Component::Endurance(v) => { entity.insert(v); }
            Component::Equipment(v) => { entity.insert(v); }
            Component::Health(v) => { entity.insert(v); }
            Component::Mana(v) => { entity.insert(v); }
            Component::PlayerControlled(v) => { entity.insert(v); }
            Component::Recovery(v) => { entity.insert(v); }
            Component::Returning(v) => { entity.insert(v); }
            Component::Side(v) => { entity.insert(v); }
            Component::Status(v) => { entity.insert(v); }
            _ => unreachable!("Loc/Heading require special handling"),
        }
    }
}

/// Server-sent summary hex: one flat hex at summary-lattice coords (sq, sr)
/// and what it carries.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SummaryData {
    pub r: u32,
    pub sq: i32,
    pub sr: i32,
    pub cell: common::summary::SummaryCell,
}

/// Key identifying a summary hex in a specific band (summary-lattice coords).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SummaryKey {
    pub r: u32,
    pub sq: i32,
    pub sr: i32,
}

#[derive(Clone, Debug, Deserialize, Event, Message, Serialize)]
pub struct Do {
    pub event: Event
}

#[derive(Clone, Debug, Deserialize, Event, Message, Serialize)]
pub struct Try {
    pub event: Event
}
