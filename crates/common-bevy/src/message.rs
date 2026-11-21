use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::{
    chunk::ChunkId,
    components::{ behaviour::*, entity_type::*, equipment::{Equipment, Inventory, Item}, heading::*, keybits::*, activation::Activation, position::Position, resources::*, * },
    moment::Moment,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Event {
    /// Server → Client: the actor leaves the client, fallen where `died`,
    /// else gone from its sight. On the server's own bus, the entity goes.
    Despawn { ent: Entity, died: bool },
    /// Server → Client: a dead actor stands again at `qrz`, its pools
    /// following. A client that kept its body moves it there unseen,
    /// stands it, and shows it once it is alive; one that did not asks for
    /// it as for any actor it has not spawned.
    Respawn { ent: Entity, qrz: qrz::Qrz },
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
    /// Server → Client: the client's actor, the game clock `dt` as it was
    /// sent, the world's calendar, which the date, the sky and the weather
    /// are read from (`systems::Calendar`), and where the haven stands.
    Init { ent: Entity, dt: Moment, calendar: crate::systems::Calendar, haven: qrz::Qrz },
    /// Client → Server: change the world's calendar for everyone. An admin
    /// request, as ungated on the wire as `Teleport` (issue #21).
    ChangeCalendar { change: crate::systems::CalendarChange },
    /// Server → Client: the world's calendar as it stands, sent to every
    /// client when an admin changes it.
    Calendar { calendar: crate::systems::Calendar },
    /// Client → Server: `dt` milliseconds of input `seq` on the client's own
    /// clock. A new `seq` opens an input; the same `seq` again extends it.
    Input { ent: Entity, key_bits: KeyBits, dt: u16, seq: u8 },
    /// Server → Client: input `seq` closed with the entity here. The client
    /// adopts the position and replays only the inputs still open.
    Confirm { ent: Entity, seq: u8, position: Position, airtime: Option<i16>, turn: Turn },
    Incremental { ent: Entity, component: Component },
    Spawn { ent: Entity, typ: EntityType, qrz: Qrz, attrs: Option<ActorAttributes> },
    /// Client → Server: `ent` asks to use `ability` on `target`, a player's
    /// choice the gate checks and never substitutes.
    UseAbility { ent: Entity, ability: AbilityType, target: Option<Entity> },
    /// Server → Client: `ent` began using a skill, sent once, whole: what,
    /// on whom, the moment it began on the game clock and how long until
    /// it lands, no time for a skill that takes hold at once. A control
    /// rune's pulse is its own, one for each foe then in its reach.
    Activate { ent: Entity, activation: Activation },
    /// Server → Client: how `ent`'s use of `ability` on `target` ended, for
    /// `damage` where it landed. One for every attack as it resolves, an
    /// activation's, a swing's or a block's return; and for an activation
    /// that ends with nothing done. `dot` marks a wound's tick, shown
    /// apart from a blow.
    Resolved { ent: Entity, ability: AbilityType, target: Option<Entity>, damage: f32, outcome: Outcome, dot: bool },
    /// Server → Client: `ent`'s `ability` recharges from `began`, on the
    /// game clock, for `length`, sent once as its activation ends.
    Recharge { ent: Entity, ability: AbilityType, began: Moment, length: std::time::Duration },
    /// Client → Server: ask for the server's game world time
    Ping,
    /// Server → Client: the answer to a Ping, the server's game world time
    /// as it answered
    Pong { dt: Moment },
    /// Server → Client: the state a remote entity is simulated from. Sent
    /// when any of it changes and at every tile crossing while moving.
    MovementIntent { ent: Entity, position: Position, heading: Heading, moving: bool, back: bool, airtime: Option<i16>, burdened: bool },
    /// Server → Client: the entity slides to a standing-height tile under an
    /// ability (lunge, knockback), arriving after `duration_ms`; round the
    /// tile `around` on its ring when given (a flank), else the straight way.
    Displace { ent: Entity, destination: Qrz, duration_ms: u16, around: Option<Qrz> },
    /// Client → Server (Try): put the entity on the ground at `qrz`'s
    /// column, an admin request; its z is unread, the server decides the
    /// height. Server → Client (Do): the entity now stands at `qrz`,
    /// wherever it was, and is drawn there at once.
    Teleport { ent: Entity, qrz: Qrz },
    /// Client → Server: the client's camera looks `steps` heading steps
    /// clockwise of the actor's heading, nothing but the far view streamed
    /// around that bearing; sent as it changes, 0 once the glance ends.
    Look { ent: Entity, steps: u8 },
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
    /// Server → Client: summaries for the visual frontier, computed
    /// server-side beyond the fixed streaming radius, and the regions the
    /// client drops, heights and mesh. The server decides both (INV-005).
    SummaryBatch {
        ent: Entity,
        additions: Vec<SummaryData>,
        removals: Vec<crate::summary_mesh::MeshRegionKey>,
    },
    /// Client → Server (Try): wear an item from the bag, or take it off
    Wear { ent: Entity, item: Item, on: bool },
    /// Client → Server (Try): get on the player's horse, or off it; the
    /// server answers with the actor's `Status`
    Mount { ent: Entity, on: bool },
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
    /// Client → Server (Try): put `loadout` on `ent`'s bar
    /// Server → Client (Do): the bar was taken
    SetLoadout { ent: Entity, loadout: crate::components::loadout::Loadout },
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
    /// an event names (an ability's target, an activation's, a blow's) are
    /// its handler's to exchange.
    pub fn ent_mut(&mut self) -> Option<&mut Entity> {
        match self {
            Event::Despawn { ent, .. }
            | Event::Respawn { ent, .. }
            | Event::DiscoverChunk { ent, .. }
            | Event::ChunkData { ent, .. }
            | Event::Init { ent, .. }
            | Event::Input { ent, .. }
            | Event::Confirm { ent, .. }
            | Event::Incremental { ent, .. }
            | Event::Spawn { ent, .. }
            | Event::UseAbility { ent, .. }
            | Event::Activate { ent, .. }
            | Event::Resolved { ent, .. }
            | Event::Recharge { ent, .. }
            | Event::MovementIntent { ent, .. }
            | Event::Displace { ent, .. }
            | Event::Teleport { ent, .. }
            | Event::Look { ent, .. }
            | Event::SpawnParty { ent, .. }
            | Event::EvictChunks { ent, .. }
            | Event::SummaryBatch { ent, .. }
            | Event::Wear { ent, .. }
            | Event::Mount { ent, .. }
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
            | Event::SetLoadout { ent, .. }
            | Event::View { ent } => Some(ent),
            Event::Ping { .. }
            | Event::Pong { .. }
            | Event::Play
            | Event::Leave
            | Event::ChangeCalendar { .. }
            | Event::Calendar { .. } => None,
        }
    }
}

/// What an actor uses in a fight: the auto-attack, and the kit, a strike, a
/// defence and a stance to each attribute's line, with a strike and a block
/// of no line.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum AbilityType {
    /// The swing every actor makes on its own clock, free
    AutoAttack,
    /// Might's strike, quick to return
    Frenzy,
    /// A light, quick strike of no line that interrupts
    Feint,
    /// Physique's strike: one heavy blow, slow to land
    Overpower,
    /// Instinct's strike, harder on a target mid-activation
    Punish,
    /// A block of no line: stops the next attack
    Parry,
    /// Resolve's defence: blocks the next attack and strikes its striker
    /// for a share of it
    Counter,
    /// Discipline's defence: clear of a target in its reach, or onto one
    /// out of it, striking as it arrives
    Leap,
    /// Agility's stance: for a while it runs faster
    PerfectStride,
    /// Might's defence: blocks the next attack and stuns its striker
    Clash,
    /// Agility's defence: steps round its target toward that one's back
    Sidestep,
    /// Physique's defence: drags its target to its user
    Yank,
    /// Agility's strike: a heavy blow that lands only from past its
    /// target's forward faces
    Backstab,
    /// Instinct's defence: lunges toward its target
    Pounce,
    /// Discipline's strike: a blow that knocks its target back as it lands
    Kick,
    /// Resolve's strike: a blow the harder the more of its user's health
    /// is lost
    Defy,
    /// Might's stance: for a while a share of the damage its attacks land
    /// comes back to it as health
    Bloodlust,
    /// Physique's stance: for a while nothing a foe lays on it holds or
    /// moves it
    Brace,
    /// Discipline's stance: for a moment each tile it moves adds to its
    /// next skill strike
    Surge,
    /// Resolve's stance: for a while every attack from outside its arc is
    /// blocked
    Vigil,
    /// Instinct's stance: for a while no foe beyond its reach sees it,
    /// until it attacks, and a skill struck out of it lands harder
    Lurk,
    /// Physique's rune, cut ahead: drags each foe in its reach straight
    /// toward it (`rune::Rune::Drag`)
    Drag,
}

/// The kinds of skill ([`AbilityType::kind`]): a strike, a blow on a
/// target in reach; a defence, which stops or escapes what is coming; a
/// stance, a while of something on its user; a rune, a while of something
/// on a place
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkillKind {
    Strike,
    Defence,
    Stance,
    Rune,
}

impl AbilityType {
    /// Every ability, the auto-attack first
    pub const ALL: [AbilityType; 22] = [
        AbilityType::AutoAttack, AbilityType::Frenzy, AbilityType::Feint, AbilityType::Overpower, AbilityType::Punish,
        AbilityType::Parry, AbilityType::Counter, AbilityType::Leap, AbilityType::PerfectStride,
        AbilityType::Clash, AbilityType::Sidestep, AbilityType::Yank, AbilityType::Backstab,
        AbilityType::Pounce, AbilityType::Kick, AbilityType::Defy, AbilityType::Bloodlust, AbilityType::Brace,
        AbilityType::Surge, AbilityType::Vigil, AbilityType::Lurk, AbilityType::Drag,
    ];

    /// The kind of skill it is, the row it stands in
    pub fn kind(self) -> SkillKind {
        match self {
            AbilityType::PerfectStride | AbilityType::Bloodlust | AbilityType::Brace | AbilityType::Surge | AbilityType::Vigil | AbilityType::Lurk => SkillKind::Stance,
            AbilityType::Parry | AbilityType::Counter | AbilityType::Clash | AbilityType::Sidestep | AbilityType::Yank
            | AbilityType::Pounce | AbilityType::Leap => SkillKind::Defence,
            AbilityType::Drag => SkillKind::Rune,
            AbilityType::AutoAttack | AbilityType::Frenzy | AbilityType::Feint | AbilityType::Overpower | AbilityType::Punish
            | AbilityType::Backstab | AbilityType::Kick | AbilityType::Defy => SkillKind::Strike,
        }
    }

    /// Whether it puts up a block (`Status::block`)
    pub fn blocks(self) -> bool {
        matches!(self, AbilityType::Parry | AbilityType::Counter | AbilityType::Clash)
    }

    /// Whether it is a strike that ends the activation of the foe it
    /// lands on
    pub fn interrupts(self) -> bool {
        self == AbilityType::Feint
    }

    /// Whether it lands only from past its target's forward faces
    pub fn flanks(self) -> bool {
        self == AbilityType::Backstab
    }

    /// Whether it is only a move of its user: a skill a pin refuses
    /// (`Status::is_pinned`)
    pub fn moves(self) -> bool {
        matches!(self, AbilityType::Leap | AbilityType::Sidestep | AbilityType::Pounce)
    }

    /// How near and how far, in tiles, it takes a target from an actor with
    /// `attrs` whose own reach is `own`: a strike as far as the actor
    /// reaches, a Yank as far past that as it drags. None for one the gate
    /// checks no reach for.
    pub fn reach(self, tuning: &crate::tuning::Tuning, attrs: &ActorAttributes, own: i32) -> Option<std::ops::RangeInclusive<i32>> {
        match self {
            AbilityType::Yank => Some(0..=own + attrs.line_tiles(tuning, AbilityType::Yank, tuning.yank_tiles) as i32),
            ability => (ability.kind() == SkillKind::Strike).then_some(0..=own),
        }
    }

    /// The rune it cuts, for a rune's skill
    pub fn rune(self) -> Option<crate::components::rune::Rune> {
        match self {
            AbilityType::Drag => Some(crate::components::rune::Rune::Drag),
            _ => None,
        }
    }
}

/// How a use of an ability ended (`Event::Resolved`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Outcome {
    /// It did what it does: a strike landed its blow
    Landed,
    /// Its target's block stopped it whole, with all it carried
    Blocked,
    /// Its target was dead, or out of its reach or its arc, as it resolved
    Missed,
    /// A foe ended its activation with nothing done
    Interrupted,
    /// Its user ended its activation by stepping or turning
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Component {
    CombatState(CombatState),
    Endurance(Endurance),
    Equipment(Equipment),
    Health(Health),
    Heading(Heading),
    Loc(Loc),
    PlayerControlled(PlayerControlled),
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
            Component::PlayerControlled(v) => { entity.insert(v); }
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
