//! What the gate asks of a skill before it does anything, of the actor's
//! standing alone: the server refuses by it, an NPC's mind weighs nothing
//! it would refuse, and a client shows its answer on the skill bar. What
//! the gate asks of the world beside, a living hostile and room to land,
//! is the server's.

use crate::{
    components::ActorAttributes,
    message::{AbilityType, SkillKind},
    tuning::Tuning,
};

/// Why the gate refused an ability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityFailReason {
    NoTargets,
    /// The caster is stunned, or held while a drag slides it
    Held,
    /// The caster is activating a skill
    Activating,
    /// The skill is recharging
    Recharging,
    /// The skill costs more endurance than the caster holds
    NoEndurance,
    OutOfRange,
    /// The target stands outside the caster's arc
    NotFacing,
    /// A player's skill not on its bar
    NotHeld,
    /// A skill that is only a move, while its user is pinned
    Pinned,
    /// A rune whose tile ahead holds a rune already, or whose ground ahead
    /// takes no step; a move with nowhere to go
    NoRoom,
}

/// How an actor stands as the gate asks of it.
#[derive(Clone, Copy, Debug)]
pub struct Standing {
    pub held: bool,
    pub pinned: bool,
    pub activating: bool,
    /// The skill asked for is recharged
    pub recharged: bool,
    pub endurance: f32,
}

/// What the gate asks of `ability` before it does anything, in order: its
/// caster unheld, activating nothing unless the skill is a stance, the
/// skill recharged and its endurance held (`standing`); for one that is
/// only a move, no pin; for
/// one with a reach (`AbilityType::reach`), a target at `foe`'s distance and
/// inside its arc, None for no target. An NPC's skills channel asks the
/// same of what it perceives, so it never weighs a skill the gate refuses.
pub fn admits(tuning: &Tuning, ability: AbilityType, standing: &Standing, attrs: &ActorAttributes, reach: i32, foe: Option<(i32, bool)>) -> Result<(), AbilityFailReason> {
    if standing.held {
        return Err(AbilityFailReason::Held);
    }
    if standing.activating && ability.kind() != SkillKind::Stance {
        return Err(AbilityFailReason::Activating);
    }
    if !standing.recharged {
        return Err(AbilityFailReason::Recharging);
    }
    if standing.endurance < tuning.cost(ability) {
        return Err(AbilityFailReason::NoEndurance);
    }
    if standing.pinned && ability.moves() {
        return Err(AbilityFailReason::Pinned);
    }
    if let Some(within) = ability.reach(tuning, attrs, reach) {
        let (distance, in_arc) = foe.ok_or(AbilityFailReason::NoTargets)?;
        if !within.contains(&distance) {
            return Err(AbilityFailReason::OutOfRange);
        }
        if !in_arc {
            return Err(AbilityFailReason::NotFacing);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE: Standing = Standing { held: false, pinned: false, activating: false, recharged: true, endurance: f32::INFINITY };

    fn asked(ability: AbilityType, standing: Standing, foe: Option<(i32, bool)>) -> Result<(), AbilityFailReason> {
        admits(&Tuning::DEFAULT, ability, &standing, &ActorAttributes::default(), 2, foe)
    }

    #[test]
    fn a_stance_alone_is_admitted_mid_activation() {
        let busy = Standing { activating: true, ..FREE };
        assert_eq!(asked(AbilityType::Brace, busy, None), Ok(()));
        assert_eq!(asked(AbilityType::Parry, busy, None), Err(AbilityFailReason::Activating));
        assert_eq!(asked(AbilityType::Frenzy, busy, Some((1, true))), Err(AbilityFailReason::Activating));
    }

    #[test]
    fn it_asks_of_the_caster_before_the_skill_and_of_the_skill_before_its_target() {
        let near = Some((1, true));
        assert_eq!(asked(AbilityType::Frenzy, FREE, near), Ok(()));
        let all = Standing { held: true, pinned: true, activating: true, recharged: false, endurance: 0.0 };
        assert_eq!(asked(AbilityType::Frenzy, all, None), Err(AbilityFailReason::Held));
        assert_eq!(asked(AbilityType::Frenzy, Standing { held: false, ..all }, None), Err(AbilityFailReason::Activating));
        assert_eq!(asked(AbilityType::Frenzy, Standing { recharged: false, ..FREE }, None), Err(AbilityFailReason::Recharging));
        assert_eq!(asked(AbilityType::Frenzy, FREE, None), Err(AbilityFailReason::NoTargets));
        assert_eq!(asked(AbilityType::Frenzy, FREE, Some((9, true))), Err(AbilityFailReason::OutOfRange));
        assert_eq!(asked(AbilityType::Frenzy, FREE, Some((1, false))), Err(AbilityFailReason::NotFacing));
        let pinned = Standing { pinned: true, ..FREE };
        assert_eq!(asked(AbilityType::Leap, pinned, near), Err(AbilityFailReason::Pinned));
        assert_eq!(asked(AbilityType::Frenzy, pinned, near), Ok(()), "a pin refuses only a move");
    }
}
