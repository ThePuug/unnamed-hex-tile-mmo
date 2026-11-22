//! # Engagements
//!
//! The engagement — a group of NPCs spawned together — and what tracks it:
//! its members, who last watched it, and where each member stands round
//! its target. Builds one at a location and finds where a party stands to
//! engage. Where a den or party goes in the world is the installer's to say.

use bevy::prelude::*;
use qrz::Qrz;

use common_bevy::{
    components::{
        behaviour::Side,
        equipment::{Equipment, Item, Piece},
        entity_type::{
            actor::{ActorIdentity, ActorImpl, Origin},
            EntityType,
        },
        heading::Heading,
        position::Position,
        AirTime, Loc,
    },
    plugins::nntree::NearestNeighbor,
    archetype::{calculate_enemy_attributes, EnemyArchetype},
    moment::Moment,
    systems::combat::resources::Fighter,
};
use common_bevy::tuning::Tuning;

/// A group of NPCs spawned together, its members its [`Members`], and
/// any that join it after. It is cleaned up once every one is dead, or no
/// client has watched it for a while (`engagement_cleanup`).
#[derive(Component, Debug, Clone)]
pub struct Engagement {
    /// How many of its NPCs may be activating a skill at one target at
    /// once: high by default, lower to make an engagement easier.
    pub attack_capacity: u8,
}

/// An engagement's attack capacity unless set lower
pub const ATTACK_CAPACITY: u8 = 6;

impl Default for Engagement {
    fn default() -> Self {
        Self { attack_capacity: ATTACK_CAPACITY }
    }
}

/// The engagement an NPC belongs to. Its target must exist as it is
/// inserted, or the engine drops it.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = Members)]
pub struct EngagementMember(pub Entity);

/// An engagement's NPCs, kept by the engine from their [`EngagementMember`]:
/// a member despawned leaves it, and it goes with the last. Never
/// `linked_spawn`: an NPC's Despawn is routed to clients before it goes.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = EngagementMember)]
pub struct Members(Vec<Entity>);

impl Members {
    /// Every member not yet despawned.
    pub fn all(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}

/// Last time players were near this engagement (for abandonment tracking)
#[derive(Component, Debug, Clone, Copy)]
pub struct LastPlayerProximity {
    /// The moment a player was last within proximity range, on the
    /// server's clock
    pub last_seen: Moment,
}

impl LastPlayerProximity {
    pub fn new(current_time: Moment) -> Self {
        Self {
            last_seen: current_time,
        }
    }

    /// Update last seen time
    pub fn update(&mut self, current_time: Moment) {
        self.last_seen = current_time;
    }

    /// Check if abandoned (no players for given duration)
    pub fn is_abandoned(&self, current_time: Moment, abandonment_duration: std::time::Duration) -> bool {
        current_time.since(self.last_seen) >= abandonment_duration
    }
}

/// What an engagement's places were last assigned for: the target its NPCs
/// chase, the tile it stood on, and how many of them lived. The assignment
/// runs afresh when any of the three changes, and not otherwise, so a place
/// taken between assignments stands until then.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct HexAssignment {
    pub last_player_tile: Option<Qrz>,
    pub target_player: Option<Entity>,
    pub living: usize,
}

/// The hex an NPC is assigned to stand on, the one record of it: the
/// assignment and a Flank write it, chase walks to it.
#[derive(Clone, Component, Copy, Debug)]
pub struct AssignedHex(pub Qrz);

/// How far apart two parties staged to fight stand, as the balance arena
/// sets its teams apart: inside the range either acquires a target from.
pub const STAGE_GAP: i32 = 24;

/// The tile a party stands on to engage one at `from`, ahead along `dir`:
/// `STAGE_GAP` out, drawn in until it stands where the one there spots it,
/// by the measure of reach (`behaviour::spotted`). On flat ground it is the
/// arena's gap.
pub fn engaging_at(from: Qrz, dir: Qrz, elevation: impl Fn(i32, i32) -> i32) -> Qrz {
    let range = crate::behaviour::ACQUISITION_RANGE as i32;
    (1..=STAGE_GAP).rev()
        .map(|d| from + dir * d)
        .map(|at| Qrz { z: elevation(at.q, at.r) + 1, ..at })
        .find(|at| Loc::new(*at).distance(&Loc::new(from)) <= range)
        .unwrap_or(from + dir)
}

/// Spawn an engagement of `npc_count` NPCs of `archetype` at `level`, on
/// `side`, round `location`; each stands a tile above the ground
/// `elevation` gives at its column. Returns the engagement.
pub fn spawn_engagement(
    tuning: &Tuning,
    location: Qrz,
    archetype: EnemyArchetype,
    side: Side,
    level: u8,
    npc_count: u8,
    elevation: impl Fn(i32, i32) -> i32,
    commands: &mut Commands,
    time: &Time,
) -> Entity {
    let engagement_entity = commands
        .spawn((
            Engagement::default(),
            Loc::new(location),
            LastPlayerProximity::new(Moment::ZERO + time.elapsed()),
            HexAssignment::default(),
        ))
        .id();

    for i in 0..npc_count {
        spawn_member(tuning, engagement_entity, location, i as usize, archetype, side, level, &elevation, commands, time);
    }

    engagement_entity
}

/// Spawns one NPC of `archetype` at `level` into `engagement`, on `side`,
/// in the `index`th place round `location`, a tile above the ground
/// `elevation` gives there. The engagement must exist, or be spawned by
/// the same commands before this.
#[allow(clippy::too_many_arguments)]
pub fn spawn_member(
    tuning: &Tuning,
    engagement: Entity,
    location: Qrz,
    index: usize,
    archetype: EnemyArchetype,
    side: Side,
    level: u8,
    elevation: impl Fn(i32, i32) -> i32,
    commands: &mut Commands,
    time: &Time,
) -> Entity {
    let base = location + member_offset(index);
    let npc_location = Qrz { z: elevation(base.q, base.r) + 1, ..base };

    let actor_impl = ActorImpl {
        origin: Origin::Evolved,
        approach: archetype.profile().approach,
        resilience: archetype.profile().resilience,
        identity: ActorIdentity::Npc(archetype),
    };

    let npc_loc = Loc::new(npc_location);
    let npc_entity = commands
        .spawn((
            EntityType::Actor(actor_impl),
            npc_loc,
            Fighter::new(tuning, calculate_enemy_attributes(level, archetype), Moment::ZERO + time.elapsed()),
            side,
            EngagementMember(engagement),
        ))
        .id();
    // An NPC wears what the server gives it, with no bag and no asking.
    let worn = kit(archetype);
    if worn.items().next().is_some() {
        commands.entity(npc_entity).insert(worn);
    }

    let chase = crate::behaviour::chase::Chase {
        acquisition_range: crate::behaviour::ACQUISITION_RANGE,
        leash_distance: crate::behaviour::LEASH_DISTANCE,
    };
    commands.entity(npc_entity).insert((
        NearestNeighbor::new(npc_entity, npc_loc),
        chase,
        crate::behaviour::perception::Skill::default(),
        crate::behaviour::Bar::of(archetype),
        crate::behaviour::perception::Sight::default(),
        crate::behaviour::moves::Move::default(),
        common_bevy::components::AttackRange::default(),
        Heading::default(),
        common_bevy::components::Turn::default(),
        Position::at_tile(npc_location),
        AirTime::default(),
        common_bevy::components::movement_intent_state::MovementIntentState::default(),
    ));
    npc_entity
}

/// Where the `index`th member of a pack stands round its place: one of
/// the six neighbours, in a fixed order round again past the sixth.
pub fn member_offset(index: usize) -> Qrz {
    let directions = [
        Qrz { q: 1, r: 0, z: 0 },
        Qrz { q: -1, r: 0, z: 0 },
        Qrz { q: 0, r: 1, z: 0 },
        Qrz { q: 0, r: -1, z: 0 },
        Qrz { q: 1, r: -1, z: 0 },
        Qrz { q: -1, r: 1, z: 0 },
    ];
    directions[index % directions.len()]
}

/// What an NPC of `archetype` wears: the Defender its plate cuirass and
/// sword-breaker; the rest go bare.
fn kit(archetype: EnemyArchetype) -> Equipment {
    let mut worn = Equipment::default();
    if archetype == EnemyArchetype::Defender {
        worn.wear(Item { piece: Piece::named("sentinel-cuirass"), seed: 0 });
        worn.wear(Item { piece: Piece::named("sword-breaker"), seed: 0 });
    }
    worn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_the_flat_a_party_engages_at_the_stage_gap() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        assert_eq!(engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |_, _| 0), Qrz { q: STAGE_GAP, r: 0, z: 1 });
    }

    #[test]
    fn up_a_slope_a_party_draws_in_until_it_is_spotted() {
        let from = Qrz { q: 0, r: 0, z: 1 };
        let at = engaging_at(from, Qrz { q: 1, r: 0, z: 0 }, |q, _| q);
        assert!(Loc::new(at).distance(&Loc::new(from)) <= crate::behaviour::ACQUISITION_RANGE as i32, "{at:?}");
        assert!(at.q < STAGE_GAP, "drawn in from the gap: {at:?}");
    }
}
