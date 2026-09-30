use crate::{
    components::{recovery::{Burst, Combo, GlobalRecovery}, ActorAttributes},
    message::AbilityType,
    systems::combat::damage as damage_calc,
};

/// Whether `ability` may be used under `recovery`: anything out of
/// recovery; in it, the combo it offers once that unlocks, or before then
/// while its burst has a step left inside its opener's recovery. Every
/// ability that can be offered asks here.
pub fn may_use(ability: AbilityType, recovery: Option<&GlobalRecovery>) -> bool {
    match recovery {
        Some(recovery) if recovery.is_active() => recovery.combo.is_some_and(|combo| {
            combo.ability == ability
                && (combo.is_unlocked(recovery.remaining) || recovery.burst.is_some_and(|burst| burst.steps > 0 && burst.window > 0.0))
        }),
        _ => true,
    }
}

/// Whether `ability`, a reaction, may be used through `recovery`:
/// Discipline's Preparation lets an actor with `attrs` use up to its tier of
/// reactions in any one recovery, each adding its own onto the rest
/// (`recovery_after`).
pub fn reacts_through(ability: AbilityType, recovery: Option<&GlobalRecovery>, attrs: Option<&ActorAttributes>) -> bool {
    let (Some(recovery), Some(attrs)) = (recovery, attrs) else { return false };
    ability.is_reaction() && recovery.is_active() && (recovery.reactions as usize) < attrs.preparation().index()
}

/// The recovery `ability` leaves an actor with `attrs` in, used under
/// `prior`, the recovery it was used in, and contested by `against`, the
/// one it was used against: a strike's target, or the source of the threat
/// a reaction answers (`GlobalRecovery::against`). The ability's own
/// seconds run longer by `Tuning::fatigue_recovery` of the actor's
/// `fatigue`, 0 to 1 (`Endurance::fatigue`).
///
/// A combo taken before it unlocked carries what it skipped of `prior`,
/// less the share its Ferocity lets it off (`ActorAttributes::ferocity_relief`),
/// so a burst fired early costs less than it would have played out. Taken
/// once unlocked, it carries nothing. A reaction used through a recovery
/// (`reacts_through`) carries all of it, its own added on.
///
/// It offers the ability's combo (`AbilityType::combo`), unlocking through
/// the ability's own seconds and never through what was carried: earlier by
/// a floor every actor has (`Tuning::combo_floor`), and more by the user's
/// Flow over the Reflex of a strike's target, the level gap weighing in; a
/// reaction's is contested by no one.
///
/// A combo fired early spends a step of the burst `prior` was part of; any
/// other use of an ability that leads on opens one of the Ferocity tier's
/// steps, 0 to 3, inside its own seconds.
pub fn recovery_after(ability: AbilityType, prior: Option<&GlobalRecovery>, attrs: &ActorAttributes, against: Option<&ActorAttributes>, fatigue: f32) -> GlobalRecovery {
    let tuning = crate::tuning::tuning();
    let own = tuning.recovery(ability) * (1.0 + tuning.fatigue_recovery * fatigue);
    let mut recovery = GlobalRecovery::new(own).against(against);

    let prior = prior.filter(|prior| prior.is_active());
    let taken = prior.and_then(|prior| prior.combo).filter(|combo| combo.ability == ability);
    let carried = match (prior, taken) {
        (Some(prior), Some(combo)) => (prior.remaining - combo.unlock_at).max(0.0) * (1.0 - attrs.ferocity_relief()),
        (Some(prior), None) => {
            recovery.reactions = prior.reactions.saturating_add(1);
            prior.remaining
        }
        (None, _) => 0.0,
    };
    recovery.remaining += carried;
    recovery.duration += carried;
    recovery.carried = carried;

    recovery.combo = ability.combo().and_then(|next| {
        let defender = against.filter(|_| !ability.is_reaction()).unwrap_or(attrs);
        let edge = damage_calc::level_edge(attrs.total_level(), defender.total_level());
        let contest = damage_calc::contest_factor(attrs.flow(), defender.reflex(), edge);
        let reduction = tuning.combo_floor + tuning.combo_share * contest;
        (reduction >= f32::EPSILON).then(|| Combo { ability: next, unlock_at: (own * (1.0 - reduction)).max(0.0) })
    });

    let early = prior.zip(taken).is_some_and(|(prior, combo)| !combo.is_unlocked(prior.remaining));
    recovery.burst = if early {
        prior.and_then(|prior| prior.burst).map(|burst| Burst { steps: burst.steps.saturating_sub(1), ..burst })
    } else {
        let steps = attrs.ferocity().index() as u8;
        (steps > 0 && ability.combo().is_some()).then_some(Burst { window: own, steps })
    };
    recovery
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(seconds: f32, ability: AbilityType, unlock_at: f32) -> GlobalRecovery {
        GlobalRecovery { combo: Some(Combo { ability, unlock_at }), ..GlobalRecovery::new(seconds) }
    }

    #[test]
    fn only_the_offered_combo_passes_the_recovery_once_it_unlocks() {
        let recovery = offering(2.0, AbilityType::Lunge, 1.5);
        let mut later = recovery;
        later.tick(1.0);
        assert!(may_use(AbilityType::Overpower, None), "out of recovery, anything");
        assert!(!may_use(AbilityType::Lunge, Some(&recovery)), "not before it unlocks");
        assert!(may_use(AbilityType::Lunge, Some(&later)), "the combo, once unlocked");
        assert!(!may_use(AbilityType::Overpower, Some(&later)), "nothing else");
    }

    #[test]
    fn preparation_reacts_through_a_recovery_up_to_its_tier_and_pays_after() {
        let disciplined = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        let recovering = GlobalRecovery::new(2.0);
        assert!(reacts_through(AbilityType::Counter, Some(&recovering), Some(&disciplined)));
        assert!(!reacts_through(AbilityType::Counter, Some(&recovering), Some(&plain)), "no Preparation, no reaction in recovery");
        assert!(!reacts_through(AbilityType::Lunge, Some(&recovering), Some(&disciplined)), "reactions only");
        let through = recovery_after(AbilityType::Counter, Some(&recovering), &disciplined, None, 0.0);
        let own = crate::tuning::tuning().recovery(AbilityType::Counter);
        assert!((through.remaining - (own + 2.0)).abs() < 1e-5, "its own recovery added onto the rest");
        assert_eq!(through.reactions, 1);
        let full = GlobalRecovery { reactions: 3, ..through };
        assert!(!reacts_through(AbilityType::Counter, Some(&full), Some(&disciplined)), "no more than the tier");
    }

    #[test]
    fn a_burst_fires_the_combo_early_while_it_has_steps() {
        let burst = Burst { window: 1.0, steps: 1 };
        let with = |burst| GlobalRecovery { burst: Some(burst), ..offering(2.0, AbilityType::Lunge, 1.5) };
        assert!(may_use(AbilityType::Lunge, Some(&with(burst))), "at once, with a step");
        assert!(!may_use(AbilityType::Lunge, Some(&with(Burst { steps: 0, ..burst }))), "none left");
        assert!(!may_use(AbilityType::Lunge, Some(&with(Burst { window: 0.0, ..burst }))), "past the opener's recovery");
        assert!(!may_use(AbilityType::Overpower, Some(&with(burst))), "only the combo");
    }

    #[test]
    fn an_early_combo_carries_what_it_skipped_and_an_unlocked_one_nothing() {
        let plain = ActorAttributes::default();
        let prior = offering(2.0, AbilityType::Lunge, 1.5);
        let own = crate::tuning::tuning().recovery(AbilityType::Lunge);
        let early = recovery_after(AbilityType::Lunge, Some(&prior), &plain, None, 0.0);
        assert!((early.remaining - (own + 0.5)).abs() < 1e-5, "the half second it skipped comes after");
        let mut waited = prior;
        waited.tick(0.6);
        assert_eq!(recovery_after(AbilityType::Lunge, Some(&waited), &plain, None, 0.0).remaining, own, "on time, its own");
        assert_eq!(recovery_after(AbilityType::Lunge, None, &plain, None, 0.0).remaining, own, "fresh, its own");
    }

    #[test]
    fn a_recovery_offers_its_combo_inside_its_own_seconds() {
        let plain = ActorAttributes::default();
        let own = crate::tuning::tuning().recovery(AbilityType::Lunge);
        let fresh = recovery_after(AbilityType::Lunge, None, &plain, None, 0.0);
        if let Some(combo) = fresh.combo {
            assert_eq!(Some(combo.ability), AbilityType::Lunge.combo());
            assert!((0.0..=own).contains(&combo.unlock_at));
        }

        // What an early combo carried is never unlocked through
        let prior = offering(2.0, AbilityType::Lunge, 0.5);
        let early = recovery_after(AbilityType::Lunge, Some(&prior), &plain, None, 0.0);
        assert!(early.carried > 0.0);
        assert_eq!(early.combo.map(|combo| combo.unlock_at), fresh.combo.map(|combo| combo.unlock_at));
        assert!(recovery_after(AbilityType::Rattle, None, &plain, None, 0.0).combo.is_none(), "a signature leads on to nothing");
    }

    #[test]
    fn fatigue_lengthens_a_recovery() {
        let plain = ActorAttributes::default();
        let fresh = recovery_after(AbilityType::Lunge, None, &plain, None, 0.0);
        let tired = recovery_after(AbilityType::Lunge, None, &plain, None, 0.5);
        let spent = recovery_after(AbilityType::Lunge, None, &plain, None, 1.0);
        assert!(tired.remaining > fresh.remaining && spent.remaining > tired.remaining, "the more spent, the longer");
    }

    #[test]
    fn ferocity_opens_a_burst_and_each_early_combo_spends_a_step() {
        let fierce = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let steps = fierce.ferocity().index() as u8;
        assert!(steps > 0, "all of it in Might reaches a Ferocity tier");
        let opener = recovery_after(AbilityType::Lunge, None, &fierce, None, 0.0);
        assert_eq!(opener.burst.map(|burst| burst.steps), Some(steps));
        let combo = AbilityType::Lunge.combo().unwrap();
        assert!(may_use(combo, Some(&opener)), "the combo fires at once");

        let second = recovery_after(combo, Some(&opener), &fierce, None, 0.0);
        assert_eq!(second.burst.map(|burst| burst.steps), Some(steps - 1), "and spends a step");
        let skipped = opener.remaining - opener.combo.unwrap().unlock_at;
        assert!(second.carried > 0.0 && second.carried < skipped, "carrying what it skipped, less what its Ferocity lets it off");
        assert!(recovery_after(AbilityType::Lunge, None, &ActorAttributes::default(), None, 0.0).burst.is_none(), "no Ferocity, no burst");
    }

    #[test]
    fn the_player_combos_run_round_one_ring() {
        // Following each combo from Lunge reaches every player ability once and comes back
        let mut at = AbilityType::Lunge;
        let mut seen = vec![at];
        loop {
            at = at.combo().expect("every ring ability leads on");
            if at == AbilityType::Lunge { break; }
            assert!(!seen.contains(&at), "{at:?} comes round twice");
            seen.push(at);
        }
        assert_eq!(seen.len(), 4, "the ring is Lunge, Overpower, Counter and Kick: {seen:?}");
    }
}
