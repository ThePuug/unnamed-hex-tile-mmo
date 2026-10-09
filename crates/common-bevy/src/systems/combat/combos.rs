use crate::{
    components::{recovery::{Chain, Combo, GlobalRecovery}, ActorAttributes},
    message::AbilityType,
    systems::combat::damage as damage_calc,
};
use crate::tuning::Tuning;

/// When a skill may be used under a recovery (`timing`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// In its own time: out of recovery, or the combo it offers once that
    /// unlocks
    OnTime,
    /// Before the recovery would offer it, by a commitment
    Early(Early),
}

/// The commitment that fires a skill early
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Early {
    /// The combo the recovery offers, strike or reaction
    Ferocity,
    /// Any reaction, once a strike taken in its own time stands in the chain
    Preparation,
}

/// When `ability` may be used under `recovery` by an actor with `attrs`,
/// `reacting` where this use is a reaction (`AbilityType::reacts`): in its
/// own time, or early while its Ferocity has a step left in the chain (the
/// combo offered) or its Preparation has one (any reaction, once a strike
/// taken in its own time stands in the chain); None where it may not. An
/// offered reaction fired early spends Ferocity's step first.
pub fn timing(ability: AbilityType, reacting: bool, recovery: Option<&GlobalRecovery>, attrs: &ActorAttributes) -> Option<Timing> {
    let Some(recovery) = recovery.filter(|recovery| recovery.is_active()) else { return Some(Timing::OnTime) };
    let offered = recovery.combo.filter(|combo| combo.ability == ability);
    if offered.is_some_and(|combo| combo.is_unlocked(recovery.remaining)) {
        return Some(Timing::OnTime);
    }
    let chain = recovery.chain;
    if offered.is_some() && (chain.early_combos as usize) < attrs.ferocity().count() {
        Some(Timing::Early(Early::Ferocity))
    } else if reacting && chain.struck && (chain.early_reactions as usize) < attrs.preparation().count() {
        Some(Timing::Early(Early::Preparation))
    } else {
        None
    }
}

/// Whether `ability` may be used under `recovery` by an actor with `attrs`
/// (`timing`). Every ability but the auto-attack, which keeps its own
/// clock, asks here.
pub fn may_use(ability: AbilityType, reacting: bool, recovery: Option<&GlobalRecovery>, attrs: &ActorAttributes) -> bool {
    timing(ability, reacting, recovery, attrs).is_some()
}

/// The recovery `ability` leaves an actor with `attrs` in, used under
/// `prior`, the recovery it was used in, and contested by `against`, the
/// one it was used against: a strike's target, or the source of the threat
/// a reaction answers (`GlobalRecovery::against`). It runs the ability's own
/// seconds, longer by `Tuning::fatigue_recovery` of the actor's `fatigue`,
/// 0 to 1 (`Endurance::fatigue`).
///
/// Used out of recovery, it opens a chain; inside one, it continues
/// `prior`'s. Fired early (`timing`), it adds `Tuning::early_owed` of what it
/// skipped to what the chain owes, which `GlobalRecovery::tick` turns into
/// a recovery once the chain's last runs out: from now to when `prior`
/// would have offered it, its combo's unlock or its end. A strike taken in
/// its own time lets Preparation fire reactions early from then on.
///
/// It offers the ability's combo (`AbilityType::combo`), unlocking through
/// the ability's own seconds: earlier by a floor every actor has
/// (`Tuning::combo_floor`), and more by the user's Efficiency over the
/// Impact of a strike's target, the level gap weighing in; a reaction's is
/// contested by no one.
pub fn recovery_after(tuning: &Tuning, ability: AbilityType, reacting: bool, prior: Option<&GlobalRecovery>, attrs: &ActorAttributes, against: Option<&ActorAttributes>, fatigue: f32) -> GlobalRecovery {
    let prior = prior.filter(|prior| prior.is_active());
    let own = tuning.recovery(ability) * (1.0 + tuning.fatigue_recovery * fatigue);
    let mut recovery = GlobalRecovery::new(own).against(against);

    recovery.combo = ability.combo().and_then(|next| {
        let defender = against.filter(|_| !reacting).unwrap_or(attrs);
        let edge = damage_calc::level_edge(tuning, attrs.total_level(), defender.total_level());
        let contest = damage_calc::contest_factor(tuning, attrs.efficiency(), defender.impact(), edge);
        let reduction = tuning.combo_floor + tuning.combo_share * contest;
        (reduction >= f32::EPSILON).then(|| Combo { ability: next, unlock_at: (own * reduction).min(own) })
    });

    let strike = !reacting && (ability.reach(1).is_some() || ability == AbilityType::Leap);
    recovery.chain = match (prior, timing(ability, reacting, prior, attrs)) {
        (Some(prior), Some(Timing::Early(by))) => {
            let offered_at = prior.combo.filter(|combo| combo.ability == ability).map_or(0.0, |combo| combo.unlock_at);
            let mut chain = prior.chain;
            chain.owed += tuning.early_owed * (prior.remaining - offered_at).max(0.0);
            match by {
                Early::Ferocity => chain.early_combos += 1,
                Early::Preparation => chain.early_reactions += 1,
            }
            chain
        }
        (Some(prior), _) => Chain { struck: prior.chain.struck || strike, ..prior.chain },
        (None, _) => Chain { struck: strike, ..Chain::default() },
    };
    recovery
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offering(seconds: f32, ability: AbilityType, unlock_at: f32) -> GlobalRecovery {
        GlobalRecovery { combo: Some(Combo { ability, unlock_at }), ..GlobalRecovery::new(seconds) }
    }

    /// All of its levels in Might, Discipline or nothing
    fn fierce() -> ActorAttributes { ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0) }
    fn prepared() -> ActorAttributes { ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0) }
    fn plain() -> ActorAttributes { ActorAttributes::default() }

    #[test]
    fn only_the_offered_combo_passes_the_recovery_once_it_unlocks() {
        let recovery = offering(2.0, AbilityType::Frenzy, 1.5);
        let mut later = recovery;
        later.tick(1.0);
        assert!(may_use(AbilityType::Feint, AbilityType::Feint.is_reaction(), None, &plain()), "out of recovery, anything");
        assert!(!may_use(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&recovery), &plain()), "not before it unlocks");
        assert!(may_use(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&later), &plain()), "the combo, once unlocked");
        assert!(!may_use(AbilityType::Feint, AbilityType::Feint.is_reaction(), Some(&later), &plain()), "nothing else");
    }

    #[test]
    fn ferocity_fires_the_offered_combo_early_up_to_its_tier_in_a_chain() {
        let fierce = fierce();
        let steps = fierce.ferocity().count();
        assert!(steps > 0, "all of it in Might reaches a Ferocity tier");
        let recovery = offering(2.0, AbilityType::Frenzy, 1.5);
        assert_eq!(timing(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&recovery), &fierce), Some(Timing::Early(Early::Ferocity)));
        assert!(!may_use(AbilityType::Feint, AbilityType::Feint.is_reaction(), Some(&recovery), &fierce), "only the combo offered");
        assert!(!may_use(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&recovery), &plain()), "no Ferocity, no early combo");
        let spent = GlobalRecovery { chain: Chain { early_combos: steps as u8, ..Chain::default() }, ..recovery };
        assert!(!may_use(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&spent), &fierce), "no more than its tier in a chain");
        let reaction = offering(2.0, AbilityType::Parry, 1.5);
        assert_eq!(timing(AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&reaction), &fierce), Some(Timing::Early(Early::Ferocity)), "a reaction offered as the combo too");
    }

    #[test]
    fn preparation_fires_any_reaction_early_once_a_strike_stands_in_the_chain() {
        let prepared = prepared();
        let steps = prepared.preparation().count();
        assert!(steps > 0, "all of it in Discipline reaches a Preparation tier");
        let struck = GlobalRecovery { chain: Chain { struck: true, ..Chain::default() }, ..GlobalRecovery::new(2.0) };
        assert_eq!(timing(AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&struck), &prepared), Some(Timing::Early(Early::Preparation)));
        assert!(!may_use(AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), Some(&struck), &prepared), "reactions only");
        assert_eq!(timing(AbilityType::Leap, AbilityType::Leap.reacts(true), Some(&struck), &prepared), Some(Timing::Early(Early::Preparation)), "a Leap clear is one");
        assert!(!may_use(AbilityType::Leap, AbilityType::Leap.reacts(false), Some(&struck), &prepared), "a Leap onto its foe is not");
        assert!(!may_use(AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&struck), &plain()), "no Preparation, no early reaction");
        assert!(!may_use(AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&GlobalRecovery::new(2.0)), &prepared), "with no strike in the chain, none");
        let spent = GlobalRecovery { chain: Chain { struck: true, early_reactions: steps as u8, ..Chain::default() }, ..struck };
        assert!(!may_use(AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&spent), &prepared), "no more than its tier in a chain");
    }

    #[test]
    fn an_early_skill_owes_half_of_what_it_skipped_and_one_on_time_nothing() {
        // A 1s recovery offering its combo halfway, fired a quarter second
        // in: a quarter second skipped, an eighth owed
        let tuning = Tuning::DEFAULT;
        let mut prior = offering(1.0, AbilityType::Parry, 0.5);
        prior.tick(0.25);
        let early = recovery_after(&tuning, AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&prior), &fierce(), None, 0.0);
        assert!((early.chain.owed - 0.125).abs() < 1e-6, "owes {}", early.chain.owed);
        assert_eq!(early.remaining, tuning.recovery(AbilityType::Parry), "its own recovery, the skipped time owed apart");
        assert_eq!(early.chain.early_combos, 1);

        let mut waited = prior;
        waited.tick(0.3);
        let on_time = recovery_after(&tuning, AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&waited), &fierce(), None, 0.0);
        assert_eq!(on_time.chain.owed, 0.0, "taken once unlocked, it owes nothing");
        assert_eq!(on_time.chain.early_combos, 0);
    }

    #[test]
    fn a_parry_then_its_feint_then_a_parry_fired_early_is_one_chain() {
        let tuning = Tuning::DEFAULT;
        let prepared = prepared();
        let parry = recovery_after(&tuning, AbilityType::Parry, AbilityType::Parry.is_reaction(), None, &prepared, None, 0.0);
        assert!(!parry.chain.struck, "a reaction opens it, striking nothing");
        assert!(!may_use(AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&parry), &prepared), "so no reaction fires early yet");

        let mut unlocked = parry;
        unlocked.tick(parry.remaining - parry.combo.unwrap().unlock_at);
        assert_eq!(timing(AbilityType::Feint, AbilityType::Feint.is_reaction(), Some(&unlocked), &prepared), Some(Timing::OnTime));
        let feint = recovery_after(&tuning, AbilityType::Feint, AbilityType::Feint.is_reaction(), Some(&unlocked), &prepared, None, 0.0);
        assert!(feint.chain.struck, "its Feint, taken in its own time, strikes");

        assert_eq!(timing(AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&feint), &prepared), Some(Timing::Early(Early::Preparation)));
        let again = recovery_after(&tuning, AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&feint), &prepared, None, 0.0);
        let offered_at = feint.combo.filter(|combo| combo.ability == AbilityType::Parry).map_or(0.0, |combo| combo.unlock_at);
        assert!((again.chain.owed - tuning.early_owed * (feint.remaining - offered_at)).abs() < 1e-5, "half of what it skipped of the Feint's recovery");
        assert_eq!((again.chain.early_reactions, again.chain.struck), (1, true), "one chain throughout");
    }

    #[test]
    fn a_chain_sums_what_each_early_skill_owes() {
        let tuning = Tuning::DEFAULT;
        let prepared = prepared();
        let struck = GlobalRecovery { chain: Chain { struck: true, ..Chain::default() }, ..GlobalRecovery::new(2.0) };
        let first = recovery_after(&tuning, AbilityType::Counter, AbilityType::Counter.is_reaction(), Some(&struck), &prepared, None, 0.0);
        if prepared.preparation().count() > 1 {
            let second = recovery_after(&tuning, AbilityType::Parry, AbilityType::Parry.is_reaction(), Some(&first), &prepared, None, 0.0);
            let skipped = 2.0 + first.remaining;
            assert!((second.chain.owed - tuning.early_owed * skipped).abs() < 1e-5, "each pays half its own skip, summed");
        }
        assert!((first.chain.owed - tuning.early_owed * 2.0).abs() < 1e-5);
    }

    #[test]
    fn out_of_recovery_a_skill_opens_a_fresh_chain() {
        let tuning = Tuning::DEFAULT;
        let fresh = recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, &fierce(), None, 0.0);
        assert_eq!(fresh.chain, Chain { struck: true, ..Chain::default() }, "a strike in its own time, owing nothing");
        assert_eq!(fresh.remaining, tuning.recovery(AbilityType::Frenzy));
    }

    #[test]
    fn a_recovery_offers_its_combo_inside_its_own_seconds() {
        let tuning = Tuning::DEFAULT;
        let own = tuning.recovery(AbilityType::Frenzy);
        let fresh = recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, &plain(), None, 0.0);
        if let Some(combo) = fresh.combo {
            assert_eq!(Some(combo.ability), AbilityType::Frenzy.combo());
            assert!((0.0..=own).contains(&combo.unlock_at));
        }
        assert!(recovery_after(&tuning, AbilityType::Counter, AbilityType::Counter.is_reaction(), None, &plain(), None, 0.0).combo.is_none(), "a Counter leads on to nothing");
    }

    #[test]
    fn a_combo_unlocks_partway_through_its_recovery_and_efficiency_brings_it_sooner() {
        let tuning = Tuning::DEFAULT;
        let defender = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let plain = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let flowing = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let own = tuning.recovery(AbilityType::Frenzy);
        let left = |attrs| recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, attrs, Some(&defender), 0.0).combo.unwrap().unlock_at;
        assert!(left(&plain) > 0.0 && left(&plain) < own, "at parity, it unlocks partway through");
        assert!(left(&flowing) > left(&plain), "an Efficiency advantage unlocks it with more of the recovery left");
    }

    #[test]
    fn fatigue_lengthens_a_recovery() {
        let tuning = Tuning::DEFAULT;
        let fresh = recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, &plain(), None, 0.0);
        let tired = recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, &plain(), None, 0.5);
        let spent = recovery_after(&tuning, AbilityType::Frenzy, AbilityType::Frenzy.is_reaction(), None, &plain(), None, 1.0);
        assert!(tired.remaining > fresh.remaining && spent.remaining > tired.remaining, "the more spent, the longer");
    }

    #[test]
    fn a_bites_combo_is_another_bite_a_parrys_is_a_feint_and_no_other_skill_leads_on() {
        use AbilityType::*;
        assert_eq!(Frenzy.combo(), Some(Frenzy));
        assert_eq!(Parry.combo(), Some(Feint));
        for ability in [AutoAttack, Feint, Overpower, Punish, Counter, Leap, PerfectStride] {
            assert_eq!(ability.combo(), None, "{ability:?}");
        }
    }
}
