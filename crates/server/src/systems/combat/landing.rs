//! What a blow does beyond its damage, applied as it lands: a stun, a daze,
//! a slow, and the Kiter's leap that rides the slow. Every such effect waits
//! in its target's queue with the threat that carries it, so it lands no
//! sooner than the damage, and a reaction that clears the threat clears the
//! effect with it. Only a wound's DoT lands ahead of its blow, ticking while
//! the wound stands.
//!
//! An ability's effects take one of two timings: here, with the threat, when
//! they are worth something only if the blow lands; or with the cast, in the
//! ability's own handler, when they stand on their own, as Disengage's leap
//! clear of a blow does. The caster's movement goes through `leap` either way.
//!
//! Every blow that lands, whatever struck it, also spills onto the striker's
//! other hostiles within its reach, by its Presence against their Toughness
//! ([`SpillReach`]).

use bevy::{ecs::system::SystemParam, prelude::*};
use common_bevy::{
    components::{
        behaviour::Side,
        recovery::GlobalRecovery,
        resources::RespawnTimer,
        status::{Daze, Status, Timed},
        stunned::Stunned,
        ActorAttributes, AttackRange, Loc,
    },
    message::{AbilityType, Component, Do, Try, Event as GameEvent},
    plugins::nntree::NNTree,
    resources::map::Map,
    systems::combat::damage,
};

use common_bevy::tuning::Tuning;

/// The least pace a daze leaves: a dazed actor still moves and swings.
const MIN_PACE: f32 = 0.1;

/// A Kiter's last Volley, cast `at` the time its shots were queued: the
/// first of them to land leaps the Kiter clear and marks it `leapt`, so the
/// rest of the burst, landing in the same tick, leap no further. The Volley
/// sets it as it is cast, long before a shot lands, so a landing finds it.
#[derive(Clone, Component, Copy, Debug)]
pub struct VolleyBurst {
    pub at: std::time::Duration,
    pub leapt: bool,
}

/// Lands the effect of a blow from `ability`, struck by `source` on
/// `target` in a threat queued `at`, as it resolves or is dismissed. The
/// source's `hold` (`ActorAttributes::hold`), from `source_attrs`,
/// lengthens and deepens it.
#[allow(clippy::too_many_arguments)]
pub fn land(
    ability: Option<AbilityType>,
    target: Entity,
    source: Entity,
    at: std::time::Duration,
    source_attrs: Option<&ActorAttributes>,
    tuning: &Tuning,
    statuses: &mut Query<&mut Status>,
    recoveries: &Query<&GlobalRecovery>,
    locs: &Query<&Loc>,
    bursts: &mut Query<&mut VolleyBurst>,
    map: &Map,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
) {
    let hold = source_attrs.map_or(1.0, ActorAttributes::hold);
    match ability {
        // A stun holds its target completely, a lockout as long with it
        Some(AbilityType::Flank) => {
            let stunned = Stunned { remaining: tuning.flank_stun * hold };
            let lockout = recoveries.get(target).map_or(0.0, |recovery| recovery.remaining).max(stunned.remaining);
            if let Ok(mut entity) = commands.get_entity(target) {
                entity.try_insert((stunned, GlobalRecovery::new(lockout, AbilityType::Flank).against(source_attrs)));
            }
            writer.write(Do { event: GameEvent::Incremental { ent: target, component: Component::Stunned(stunned) } });
        }
        Some(AbilityType::Rattle) => update(target, statuses, commands, writer, |status| {
            let stacks = Status::stacks_of(Some(status)).saturating_add(1).min(tuning.rattle_stacks);
            status.daze = Some(Daze { stacks, pace: (1.0 - tuning.rattle_daze * hold * stacks as f32).max(MIN_PACE) });
        }),
        // Each shot that lands slows its target afresh; the first of a burst
        // to land carries the Kiter clear, so a burst leaps once, as the
        // target can no longer follow
        Some(AbilityType::Volley) => {
            update(target, statuses, commands, writer, |status| {
                status.slow = Some(Timed { pace: (1.0 - tuning.volley_slow * hold).max(MIN_PACE), remaining: tuning.volley_slow_secs * hold });
            });
            let first_of_burst = bursts.get_mut(source).is_ok_and(|mut burst| {
                let first = burst.at == at && !burst.leapt;
                burst.leapt |= first;
                first
            });
            if first_of_burst {
                if let (Ok(at), Ok(from)) = (locs.get(source), locs.get(target)) {
                    if let Some(landing) = super::leap::away(map, **at, **from, tuning.volley_leap) {
                        super::leap::leap(source, landing, commands, writer);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Who a landed blow spills onto: the living actors near its striker, found
/// by the tree, with their sides, attributes and reach.
#[derive(SystemParam)]
pub struct SpillReach<'w, 's> {
    nntree: Res<'w, NNTree>,
    actors: Query<'w, 's, (&'static Loc, &'static Side, &'static ActorAttributes, Option<&'static AttackRange>), Without<RespawnTimer>>,
}

impl SpillReach<'_, '_> {
    /// Spills a blow of `damage` that `source` landed on `target` onto every
    /// other actor hostile to `source` within its reach, each its share
    /// ([`damage::spill_share`]), the level edge the striker's.
    pub fn spill(&self, source: Entity, target: Entity, damage: f32, commands: &mut Commands) {
        let Ok((loc, side, attrs, range)) = self.actors.get(source) else { return };
        let reach = range.copied().unwrap_or_default().0.max(0) as i64;
        for near in self.nntree.locate_within_distance(*loc, reach * reach) {
            if near.ent == source || near.ent == target {
                continue;
            }
            let Ok((_, other_side, other, _)) = self.actors.get(near.ent) else { continue };
            if !side.is_hostile_to(*other_side) {
                continue;
            }
            let share = damage::spill_share(attrs.presence(), other.toughness(), damage::level_edge(attrs.total_level(), other.total_level()));
            if share > 0.0 {
                commands.trigger(Try { event: GameEvent::Spill { ent: near.ent, source, damage: damage * share } });
            }
        }
    }
}

/// A strike across its striker's line breaks its stride: `Tuning::stride_pace`
/// of its speed for one auto-attack interval.
pub fn stumble(
    trigger: On<Try>,
    mut statuses: Query<&mut Status>,
    mut commands: Commands,
    mut writer: MessageWriter<Do>,
) {
    let Try { event: GameEvent::Stumble { ent } } = trigger.event() else { return };
    let tuning = common_bevy::tuning::tuning();
    update(*ent, &mut statuses, &mut commands, &mut writer, |status| {
        status.stride = Some(Timed { pace: tuning.stride_pace, remaining: tuning.auto_interval });
    });
}

/// Changes `ent`'s status by `change`, giving it one if it has none, and
/// sends the whole of it.
fn update(
    ent: Entity,
    statuses: &mut Query<&mut Status>,
    commands: &mut Commands,
    writer: &mut MessageWriter<Do>,
    change: impl FnOnce(&mut Status),
) {
    let status = match statuses.get_mut(ent) {
        Ok(mut status) => {
            change(&mut status);
            *status
        }
        Err(_) => {
            let Ok(mut entity) = commands.get_entity(ent) else { return };
            let mut status = Status::default();
            change(&mut status);
            entity.try_insert(status);
            status
        }
    };
    writer.write(Do { event: GameEvent::Incremental { ent, component: Component::Status(status) } });
}
