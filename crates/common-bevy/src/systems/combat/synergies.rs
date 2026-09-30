use crate::{
    components::{recovery::{Combo, GlobalRecovery, Offer}, ActorAttributes},
    message::AbilityType,
    systems::combat::damage as damage_calc,
};

/// Whether `ability` may be used under `recovery`, the lockout: anything
/// out of lockout; in it, the follow-up it offers once that unlocks, or
/// before then while its combo has a step left inside its opener's lockout.
/// Every ability that can be offered asks here.
pub fn may_use(ability: AbilityType, recovery: Option<&GlobalRecovery>) -> bool {
    match recovery {
        Some(recovery) if recovery.is_active() => recovery.offer.is_some_and(|offer| {
            offer.ability == ability
                && (offer.is_unlocked(recovery.remaining) || recovery.combo.is_some_and(|combo| combo.steps > 0 && combo.window > 0.0))
        }),
        _ => true,
    }
}

/// Whether `ability`, a reaction, may be used through `recovery`'s lockout:
/// Discipline's Preparation lets an actor with `attrs` use up to its tier of
/// reactions in any one lockout, each adding its own onto the rest (`lockout`).
pub fn reacts_through(ability: AbilityType, recovery: Option<&GlobalRecovery>, attrs: Option<&ActorAttributes>) -> bool {
    let (Some(recovery), Some(attrs)) = (recovery, attrs) else { return false };
    ability.is_reaction() && recovery.is_active() && (recovery.reactions as usize) < attrs.preparation().index()
}

/// The lockout `ability` starts on an actor with `attrs`, used under
/// `prior`, the lockout it was used in, and contested by `against`, the one
/// it was used against: a strike's target, or the source of the threat a
/// reaction answers (`GlobalRecovery::against`).
///
/// A follow-up taken before its offer unlocked carries what it skipped of
/// `prior`, so a combo burst early costs what it would have played out:
/// Ferocity moves the lockout, never shortens it. Taken once unlocked, it
/// carries nothing. A reaction used through a lockout (`reacts_through`)
/// carries all of it, its own added on.
///
/// It offers the ability's follow-up (`AbilityType::follow_up`), unlocking
/// through the ability's own seconds and never through what was carried:
/// earlier by a floor every actor has (`Tuning::synergy_floor`), and more by
/// the user's Flow over the Reflex of a strike's target, the level gap
/// weighing in; a reaction's is contested by no one.
///
/// A follow-up fired early spends a step of the combo `prior` was part of;
/// any other use of an ability that leads on opens one of the Ferocity
/// tier's steps, 0 to 3, inside its own seconds.
pub fn lockout(ability: AbilityType, prior: Option<&GlobalRecovery>, attrs: &ActorAttributes, against: Option<&ActorAttributes>) -> GlobalRecovery {
    let tuning = crate::tuning::tuning();
    let own = tuning.recovery(ability);
    let mut recovery = GlobalRecovery::new(own).against(against);

    let prior = prior.filter(|prior| prior.is_active());
    let taken = prior.and_then(|prior| prior.offer).filter(|offer| offer.ability == ability);
    let carried = match (prior, taken) {
        (Some(prior), Some(offer)) => (prior.remaining - offer.unlock_at).max(0.0),
        (Some(prior), None) => {
            recovery.reactions = prior.reactions.saturating_add(1);
            prior.remaining
        }
        (None, _) => 0.0,
    };
    recovery.remaining += carried;
    recovery.duration += carried;
    recovery.carried = carried;

    recovery.offer = ability.follow_up().and_then(|follow_up| {
        let defender = against.filter(|_| !ability.is_reaction()).unwrap_or(attrs);
        let edge = damage_calc::level_edge(attrs.total_level(), defender.total_level());
        let contest = damage_calc::contest_factor(attrs.flow(), defender.reflex(), edge);
        let reduction = tuning.synergy_floor + tuning.synergy_share * contest;
        (reduction >= f32::EPSILON).then(|| Offer { ability: follow_up, unlock_at: (own * (1.0 - reduction)).max(0.0) })
    });

    let early = prior.zip(taken).is_some_and(|(prior, offer)| !offer.is_unlocked(prior.remaining));
    recovery.combo = if early {
        prior.and_then(|prior| prior.combo).map(|combo| Combo { steps: combo.steps.saturating_sub(1), ..combo })
    } else {
        let steps = attrs.ferocity().index() as u8;
        (steps > 0 && ability.follow_up().is_some()).then_some(Combo { window: own, steps })
    };
    recovery
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(seconds: f32, ability: AbilityType, unlock_at: f32) -> GlobalRecovery {
        GlobalRecovery { offer: Some(Offer { ability, unlock_at }), ..GlobalRecovery::new(seconds) }
    }

    #[test]
    fn only_the_offered_follow_up_passes_the_lockout_once_it_unlocks() {
        let recovery = offering(2.0, AbilityType::Lunge, 1.5);
        let mut later = recovery;
        later.tick(1.0);
        assert!(may_use(AbilityType::Overpower, None), "out of lockout, anything");
        assert!(!may_use(AbilityType::Lunge, Some(&recovery)), "not before it unlocks");
        assert!(may_use(AbilityType::Lunge, Some(&later)), "the offer, once unlocked");
        assert!(!may_use(AbilityType::Overpower, Some(&later)), "nothing else");
    }

    #[test]
    fn preparation_reacts_through_a_lockout_up_to_its_tier_and_pays_after() {
        let disciplined = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        let lockout_now = GlobalRecovery::new(2.0);
        assert!(reacts_through(AbilityType::Counter, Some(&lockout_now), Some(&disciplined)));
        assert!(!reacts_through(AbilityType::Counter, Some(&lockout_now), Some(&plain)), "no Preparation, no reaction in lockout");
        assert!(!reacts_through(AbilityType::Lunge, Some(&lockout_now), Some(&disciplined)), "reactions only");
        let through = lockout(AbilityType::Counter, Some(&lockout_now), &disciplined, None);
        let own = crate::tuning::tuning().recovery(AbilityType::Counter);
        assert!((through.remaining - (own + 2.0)).abs() < 1e-5, "its own lockout added onto the rest");
        assert_eq!(through.reactions, 1);
        let full = GlobalRecovery { reactions: 3, ..through };
        assert!(!reacts_through(AbilityType::Counter, Some(&full), Some(&disciplined)), "no more than the tier");
    }

    #[test]
    fn a_combo_fires_the_offer_early_while_it_has_steps() {
        let combo = Combo { window: 1.0, steps: 1 };
        let with = |combo| GlobalRecovery { combo: Some(combo), ..offering(2.0, AbilityType::Lunge, 1.5) };
        assert!(may_use(AbilityType::Lunge, Some(&with(combo))), "at once, with a step");
        assert!(!may_use(AbilityType::Lunge, Some(&with(Combo { steps: 0, ..combo }))), "none left");
        assert!(!may_use(AbilityType::Lunge, Some(&with(Combo { window: 0.0, ..combo }))), "past the opener's lockout");
        assert!(!may_use(AbilityType::Overpower, Some(&with(combo))), "only the offer");
    }

    #[test]
    fn an_early_follow_up_carries_what_it_skipped_and_an_unlocked_one_nothing() {
        let plain = ActorAttributes::default();
        let prior = offering(2.0, AbilityType::Lunge, 1.5);
        let own = crate::tuning::tuning().recovery(AbilityType::Lunge);
        let early = lockout(AbilityType::Lunge, Some(&prior), &plain, None);
        assert!((early.remaining - (own + 0.5)).abs() < 1e-5, "the half second it skipped comes after");
        let mut waited = prior;
        waited.tick(0.6);
        assert_eq!(lockout(AbilityType::Lunge, Some(&waited), &plain, None).remaining, own, "on time, its own");
        assert_eq!(lockout(AbilityType::Lunge, None, &plain, None).remaining, own, "fresh, its own");
    }

    #[test]
    fn a_lockout_offers_its_follow_up_inside_its_own_seconds() {
        let plain = ActorAttributes::default();
        let own = crate::tuning::tuning().recovery(AbilityType::Lunge);
        let fresh = lockout(AbilityType::Lunge, None, &plain, None);
        if let Some(offer) = fresh.offer {
            assert_eq!(Some(offer.ability), AbilityType::Lunge.follow_up());
            assert!((0.0..=own).contains(&offer.unlock_at));
        }

        // What an early follow-up carried is never unlocked through
        let prior = offering(2.0, AbilityType::Lunge, 0.5);
        let early = lockout(AbilityType::Lunge, Some(&prior), &plain, None);
        assert!(early.carried > 0.0);
        assert_eq!(early.offer.map(|offer| offer.unlock_at), fresh.offer.map(|offer| offer.unlock_at));
        assert!(lockout(AbilityType::Rattle, None, &plain, None).offer.is_none(), "a signature leads on to nothing");
    }

    #[test]
    fn ferocity_opens_a_combo_and_each_early_follow_up_spends_a_step() {
        let fierce = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let steps = fierce.ferocity().index() as u8;
        assert!(steps > 0, "all of it in Might reaches a Ferocity tier");
        let opener = lockout(AbilityType::Lunge, None, &fierce, None);
        assert_eq!(opener.combo.map(|combo| combo.steps), Some(steps));
        let follow_up = AbilityType::Lunge.follow_up().unwrap();
        assert!(may_use(follow_up, Some(&opener)), "the offer fires at once");

        let second = lockout(follow_up, Some(&opener), &fierce, None);
        assert_eq!(second.combo.map(|combo| combo.steps), Some(steps - 1), "and spends a step");
        assert!(second.carried > 0.0, "carrying what it skipped");
        assert!(lockout(AbilityType::Lunge, None, &ActorAttributes::default(), None).combo.is_none(), "no Ferocity, no combo");
    }

    #[test]
    fn the_player_synergies_run_round_one_ring() {
        // Following each follow-up from Lunge reaches every player ability once and comes back
        let mut at = AbilityType::Lunge;
        let mut seen = vec![at];
        loop {
            at = at.follow_up().expect("every ring ability leads on");
            if at == AbilityType::Lunge { break; }
            assert!(!seen.contains(&at), "{at:?} comes round twice");
            seen.push(at);
        }
        assert_eq!(seen.len(), 4, "the ring is Lunge, Overpower, Counter and Kick: {seen:?}");
    }
}
