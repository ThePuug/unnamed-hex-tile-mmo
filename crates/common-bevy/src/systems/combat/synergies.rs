use bevy::prelude::*;

use crate::{
    components::{recovery::{Combo, GlobalRecovery, SynergyUnlock, get_ability_recovery_duration}, ActorAttributes},
    message::AbilityType,
    systems::combat::damage as damage_calc,
};

/// Synergy trigger types for ability categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynergyTrigger {
    GapCloser,   // Lunge
    HeavyStrike, // Overpower
    Push,        // Knockback
    Mitigate,    // Counter
    Kick,        // Kick
}

/// Synergy rule definition (what ability unlocks what)
#[derive(Debug, Clone)]
pub struct SynergyRule {
    pub trigger: SynergyTrigger,
    pub target: AbilityType,
    pub unlock_reduction: f32, // How much earlier to unlock (in seconds)
}

/// MVP Synergy Rules (hardcoded for Phase 2, data-driven later in Phase 4)
pub const MVP_SYNERGIES: &[SynergyRule] = &[
    // Gap Closer → Heavy Strike: Overpower unlocks 0.5s early during Lunge recovery
    SynergyRule {
        trigger: SynergyTrigger::GapCloser,
        target: AbilityType::Overpower,
        unlock_reduction: 0.5, // Overpower available at 0.5s instead of 1.0s
    },
    // Heavy Strike → Mitigate: Counter unlocks 1.0s early during Overpower recovery
    SynergyRule {
        trigger: SynergyTrigger::HeavyStrike,
        target: AbilityType::Counter,
        unlock_reduction: 1.0, // Counter available at 1.0s instead of 2.2s (0.2s window)
    },
    // Kick → Lunge: what a kick knocks back, a Lunge closes on again
    SynergyRule {
        trigger: SynergyTrigger::Kick,
        target: AbilityType::Lunge,
        unlock_reduction: 1.0,
    },
    // Counter → Kick: what a counter answered, a kick drives off, closing
    // the ring Lunge → Overpower → Counter → Kick → Lunge
    SynergyRule {
        trigger: SynergyTrigger::Mitigate,
        target: AbilityType::Kick,
        unlock_reduction: 1.0,
    },
];

/// Get the synergy trigger type for an ability
pub fn get_synergy_trigger(ability: AbilityType) -> Option<SynergyTrigger> {
    match ability {
        AbilityType::Lunge => Some(SynergyTrigger::GapCloser),
        AbilityType::Overpower => Some(SynergyTrigger::HeavyStrike),
        AbilityType::Counter => Some(SynergyTrigger::Mitigate),  // Mitigate type
        AbilityType::Kick => Some(SynergyTrigger::Kick),
        AbilityType::AutoAttack | AbilityType::Rattle | AbilityType::Disengage | AbilityType::Volley | AbilityType::Flank => None, // No synergies
    }
}

/// Whether `ability` may be used under `recovery`, the lockout, `synergy`,
/// the follow-up the last ability offered, and `combo`, a Ferocity combo
/// under way: anything out of lockout; in it, the offered follow-up once it
/// unlocks, or before then while the combo has a step left inside its
/// opener's lockout. Every ability that can be offered asks here.
pub fn may_use(ability: AbilityType, recovery: Option<&GlobalRecovery>, synergy: Option<&SynergyUnlock>, combo: Option<&Combo>) -> bool {
    match recovery {
        Some(recovery) if recovery.is_active() => synergy.is_some_and(|synergy| {
            synergy.ability == ability
                && (synergy.is_unlocked(recovery.remaining) || combo.is_some_and(|combo| combo.steps > 0 && combo.window > 0.0))
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

/// The lockout `ability` starts, used under `prior`, the lockout it was
/// used in, and `synergy`, the offer it took. A follow-up taken before its
/// offer unlocked carries what it skipped of `prior`, so a combo burst early
/// costs what it would have played out: Ferocity moves the lockout, never
/// shortens it. Taken once unlocked, it carries nothing. A reaction used
/// through a lockout (`reacts_through`) carries all of it, its own added
/// on. It runs contested against `against`, the one it was used against:
/// a strike's target, or the source of the threat a reaction answers
/// (`GlobalRecovery::against`). Server and client both start lockouts
/// here, so they agree.
pub fn lockout(ability: AbilityType, prior: Option<&GlobalRecovery>, synergy: Option<&SynergyUnlock>, against: Option<&ActorAttributes>) -> GlobalRecovery {
    let mut recovery = GlobalRecovery::new(get_ability_recovery_duration(ability), ability).against(against);
    let carried = match (prior.filter(|prior| prior.is_active()), synergy.filter(|synergy| synergy.ability == ability)) {
        (Some(prior), Some(synergy)) => (prior.remaining - synergy.unlock_at).max(0.0),
        (Some(prior), None) => {
            recovery.reactions = prior.reactions.saturating_add(1);
            prior.remaining
        }
        (None, _) => 0.0,
    };
    recovery.remaining += carried;
    recovery.duration += carried;
    recovery.carried = carried;
    recovery
}

/// Settles `ability`'s place in a Ferocity combo after `ent` uses it with
/// `attrs`: a follow-up fired early spends a step; any other use of an
/// ability that offers a follow-up opens a combo of the Ferocity tier's
/// steps, 0 to 3, inside its own lockout of `opener` seconds.
pub fn settle_combo(ent: Entity, ability: AbilityType, early: bool, opener: f32, attrs: &ActorAttributes, combo: Option<&Combo>, commands: &mut Commands) {
    let Ok(mut entity) = commands.get_entity(ent) else { return };
    if early {
        if let Some(combo) = combo {
            entity.insert(Combo { steps: combo.steps.saturating_sub(1), ..*combo });
        }
        return;
    }
    let steps = attrs.ferocity().index() as u8;
    if steps > 0 && get_synergy_trigger(ability).is_some() {
        entity.insert(Combo { window: opener, steps });
    } else {
        entity.remove::<Combo>();
    }
}

/// Whether using `ability` now fires it early: in lockout, before the offer
/// it takes unlocks
pub fn is_early(ability: AbilityType, recovery: Option<&GlobalRecovery>, synergy: Option<&SynergyUnlock>) -> bool {
    recovery.is_some_and(|recovery| {
        recovery.is_active() && synergy.is_some_and(|synergy| synergy.ability == ability && !synergy.is_unlocked(recovery.remaining))
    })
}

/// Counts every combo's opener window down, ending the combo when it runs out
pub fn tick_combo(mut commands: Commands, mut query: Query<(Entity, &mut Combo)>, time: Res<Time>) {
    let dt = time.delta_secs();
    for (ent, mut combo) in &mut query {
        combo.window -= dt;
        if combo.window <= 0.0 {
            commands.entity(ent).remove::<Combo>();
        }
    }
}

/// Apply synergies when an ability is used.

/// Pattern 1 (Nullifying): 66% × contest_factor, the level gap weighing in
/// - Calculates percentage reduction of effective_recovery_base
/// - Creates early unlock window for synergized abilities
/// - Stacks multiplicatively with composure reduction

/// This should be called immediately after creating GlobalRecovery.
/// Both server and client run this function locally (no network broadcast needed).
pub fn apply_synergies(
    entity: Entity,
    used_ability: AbilityType,
    recovery: &GlobalRecovery,
    attacker_attrs: &ActorAttributes,
    defender_attrs: &ActorAttributes,
    commands: &mut Commands,
) {
    // Get the trigger type for the used ability
    let Some(trigger_type) = get_synergy_trigger(used_ability) else {
        return; // No synergies for this ability
    };

    // A floor every actor has, and a share more its Flow wins over the
    // defender's Reflex, the level gap weighing in
    let tuning = crate::tuning::tuning();
    let edge = damage_calc::level_edge(attacker_attrs.total_level(), defender_attrs.total_level());
    let contest = damage_calc::contest_factor(attacker_attrs.flow(), defender_attrs.reflex(), edge);

    let synergy_reduction = tuning.synergy_floor + tuning.synergy_share * contest;

    // No early unlock at all when the floor is none and the contest is lost
    if synergy_reduction < f32::EPSILON {
        return;
    }

    // Find and apply matching synergy rules
    for _rule in MVP_SYNERGIES {
        if _rule.trigger == trigger_type {
            // Apply percentage reduction to effective_recovery_base
            // recovery.remaining is already adjusted by composure, so this stacks multiplicatively
            // What the lockout carried from before never unlocks early
            let unlock_at = ((recovery.remaining - recovery.carried) * (1.0 - synergy_reduction)).max(0.0);

            // Insert synergy unlock component (both server and client do this locally)
            // Only insert if entity exists (may have been evicted client-side)
            let synergy = SynergyUnlock::new(_rule.target, unlock_at, used_ability);
            if let Ok(mut entity_cmd) = commands.get_entity(entity) {
                entity_cmd.insert(synergy);
            }
        }
    }
}

/// System to clean up expired synergies when recovery expires
pub fn synergy_cleanup_system(
    mut commands: Commands,
    recovery_query: Query<Entity, With<GlobalRecovery>>,
    synergy_query: Query<(Entity, &SynergyUnlock)>,
) {
    // Collect entities with synergies but no recovery
    let entities_with_recovery: std::collections::HashSet<Entity> =
        recovery_query.iter().collect();

    for (entity, _synergy) in synergy_query.iter() {
        if !entities_with_recovery.contains(&entity) {
            // Recovery expired, remove synergy
            commands.entity(entity).remove::<SynergyUnlock>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_offered_follow_up_passes_the_lockout_once_it_unlocks() {
        let recovery = GlobalRecovery::new(2.0, AbilityType::Kick);
        let offer = SynergyUnlock::new(AbilityType::Lunge, 1.5, AbilityType::Kick);
        let mut later = recovery;
        later.tick(1.0);
        assert!(may_use(AbilityType::Overpower, None, None, None), "out of lockout, anything");
        assert!(!may_use(AbilityType::Lunge, Some(&recovery), Some(&offer), None), "not before it unlocks");
        assert!(may_use(AbilityType::Lunge, Some(&later), Some(&offer), None), "the offer, once unlocked");
        assert!(!may_use(AbilityType::Overpower, Some(&later), Some(&offer), None), "nothing else");
    }

    #[test]
    fn preparation_reacts_through_a_lockout_up_to_its_tier_and_pays_after() {
        let disciplined = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        let lockout_now = GlobalRecovery::new(2.0, AbilityType::Overpower);
        assert!(reacts_through(AbilityType::Counter, Some(&lockout_now), Some(&disciplined)));
        assert!(!reacts_through(AbilityType::Counter, Some(&lockout_now), Some(&plain)), "no Preparation, no reaction in lockout");
        assert!(!reacts_through(AbilityType::Lunge, Some(&lockout_now), Some(&disciplined)), "reactions only");
        let through = lockout(AbilityType::Counter, Some(&lockout_now), None, None);
        let own = get_ability_recovery_duration(AbilityType::Counter);
        assert!((through.remaining - (own + 2.0)).abs() < 1e-5, "its own lockout added onto the rest");
        assert_eq!(through.reactions, 1);
        let full = GlobalRecovery { reactions: 3, ..through };
        assert!(!reacts_through(AbilityType::Counter, Some(&full), Some(&disciplined)), "no more than the tier");
    }

    #[test]
    fn a_combo_fires_the_offer_early_while_it_has_steps() {
        let recovery = GlobalRecovery::new(2.0, AbilityType::Kick);
        let offer = SynergyUnlock::new(AbilityType::Lunge, 1.5, AbilityType::Kick);
        let combo = Combo { window: 1.0, steps: 1 };
        assert!(may_use(AbilityType::Lunge, Some(&recovery), Some(&offer), Some(&combo)), "at once, with a step");
        assert!(!may_use(AbilityType::Lunge, Some(&recovery), Some(&offer), Some(&Combo { steps: 0, ..combo })), "none left");
        assert!(!may_use(AbilityType::Lunge, Some(&recovery), Some(&offer), Some(&Combo { window: 0.0, ..combo })), "past the opener's lockout");
        assert!(!may_use(AbilityType::Overpower, Some(&recovery), Some(&offer), Some(&combo)), "only the offer");
    }

    #[test]
    fn an_early_follow_up_carries_what_it_skipped_and_an_unlocked_one_nothing() {
        let prior = GlobalRecovery::new(2.0, AbilityType::Kick);
        let offer = SynergyUnlock::new(AbilityType::Lunge, 1.5, AbilityType::Kick);
        let own = get_ability_recovery_duration(AbilityType::Lunge);
        let early = lockout(AbilityType::Lunge, Some(&prior), Some(&offer), None);
        assert!((early.remaining - (own + 0.5)).abs() < 1e-5, "the half second it skipped comes after");
        let mut waited = prior;
        waited.tick(0.6);
        assert_eq!(lockout(AbilityType::Lunge, Some(&waited), Some(&offer), None).remaining, own, "on time, its own");
        assert_eq!(lockout(AbilityType::Lunge, None, None, None).remaining, own, "fresh, its own");
    }

    #[test]
    fn test_get_synergy_trigger() {
        assert_eq!(
            get_synergy_trigger(AbilityType::Lunge),
            Some(SynergyTrigger::GapCloser)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Overpower),
            Some(SynergyTrigger::HeavyStrike)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Counter),
            Some(SynergyTrigger::Mitigate)
        );
        assert_eq!(
            get_synergy_trigger(AbilityType::Kick),
            Some(SynergyTrigger::Kick)
        );
        assert_eq!(get_synergy_trigger(AbilityType::AutoAttack), None);
    }

    #[test]
    fn test_mvp_synergies_rules() {
        assert_eq!(MVP_SYNERGIES.len(), 4, "one rule for each step of the ring");

        // Lunge → Overpower
        let lunge_synergy = &MVP_SYNERGIES[0];
        assert_eq!(lunge_synergy.trigger, SynergyTrigger::GapCloser);
        assert_eq!(lunge_synergy.target, AbilityType::Overpower);
        assert_eq!(lunge_synergy.unlock_reduction, 0.5);

        // Overpower → Counter (replaces Knockback)
        let overpower_synergy = &MVP_SYNERGIES[1];
        assert_eq!(overpower_synergy.trigger, SynergyTrigger::HeavyStrike);
        assert_eq!(overpower_synergy.target, AbilityType::Counter);
        assert_eq!(overpower_synergy.unlock_reduction, 1.0);

        // Kick → Lunge
        let kick_synergy = &MVP_SYNERGIES[2];
        assert_eq!(kick_synergy.trigger, SynergyTrigger::Kick);
        assert_eq!(kick_synergy.target, AbilityType::Lunge);

        // Counter → Kick
        let counter_synergy = &MVP_SYNERGIES[3];
        assert_eq!(counter_synergy.trigger, SynergyTrigger::Mitigate);
        assert_eq!(counter_synergy.target, AbilityType::Kick);
    }

    #[test]
    fn the_player_synergies_run_round_one_ring() {
        // Following each synergy from Lunge reaches every player ability once and comes back
        let mut at = AbilityType::Lunge;
        let mut seen = vec![at];
        loop {
            let trigger = get_synergy_trigger(at).expect("every ring ability triggers one");
            at = MVP_SYNERGIES.iter().find(|rule| rule.trigger == trigger).expect("and it leads on").target;
            if at == AbilityType::Lunge { break; }
            assert!(!seen.contains(&at), "{at:?} comes round twice");
            seen.push(at);
        }
        assert_eq!(seen.len(), 4, "the ring is Lunge, Overpower, Counter and Kick: {seen:?}");
    }

    // Note: Following DEVELOPER role guidance to write durable unit tests.
    // The can_use_ability function is designed to be called from systems with ECS queries,
    // so we test the logic components (GlobalRecovery, SynergyUnlock) directly instead.

    #[test]
    fn test_ability_locked_by_recovery() {
        // Test that recovery locks abilities
        let recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        assert!(recovery.is_active(), "Recovery should be active");
    }

    #[test]
    fn test_synergy_unlock_logic() {
        // Test synergy unlock logic directly
        let synergy = SynergyUnlock::new(AbilityType::Overpower, 0.5, AbilityType::Lunge);

        // At 1.0s remaining (not unlocked yet)
        assert!(
            !synergy.is_unlocked(1.0),
            "Should not be unlocked at 1.0s remaining"
        );

        // At 0.5s remaining (unlocked)
        assert!(
            synergy.is_unlocked(0.5),
            "Should be unlocked at 0.5s remaining"
        );

        // At 0.3s remaining (unlocked)
        assert!(
            synergy.is_unlocked(0.3),
            "Should be unlocked at 0.3s remaining"
        );
    }

    #[test]
    fn test_lunge_synergy_timing() {
        // Test Lunge → Overpower synergy timing
        let recovery = GlobalRecovery::new(1.0, AbilityType::Lunge);
        let synergy = SynergyUnlock::new(AbilityType::Overpower, 0.5, AbilityType::Lunge);

        // At start (1.0s remaining): locked
        assert!(recovery.is_active());
        assert!(!synergy.is_unlocked(recovery.remaining));

        // After 0.5s (0.5s remaining): synergy unlocks
        let mut recovery_mid = recovery.clone();
        recovery_mid.tick(0.5);
        assert!(recovery_mid.is_active());
        assert!(
            synergy.is_unlocked(recovery_mid.remaining),
            "Overpower should unlock at 0.5s remaining"
        );
    }

    #[test]
    fn test_overpower_synergy_timing() {
        // Test Overpower → Counter synergy timing (replaces Knockback)
        let recovery = GlobalRecovery::new(2.0, AbilityType::Overpower);
        let synergy = SynergyUnlock::new(AbilityType::Counter, 1.0, AbilityType::Overpower);

        // At start (2.0s remaining): locked
        assert!(recovery.is_active());
        assert!(!synergy.is_unlocked(recovery.remaining));

        // After 0.5s (1.5s remaining): still locked
        let mut recovery_early = recovery.clone();
        recovery_early.tick(0.5);
        assert!(!synergy.is_unlocked(recovery_early.remaining));

        // After 1.0s (1.0s remaining): synergy unlocks
        let mut recovery_mid = recovery.clone();
        recovery_mid.tick(1.0);
        assert!(
            synergy.is_unlocked(recovery_mid.remaining),
            "Counter should unlock at 1.0s remaining"
        );
    }

    #[test]
    fn test_zero_finesse_produces_no_synergy_reduction() {
        // With 0 flow, contest_factor returns 0, so synergy_reduction = 0
        // apply_synergies should NOT insert a SynergyUnlock component
        let flow = 0u16;
        let reflex = 0u16;
        let contest = damage_calc::contest_factor(flow, reflex, 0.0);
        assert_eq!(contest, 0.0, "contest_factor(0, 0) should be 0");

        let synergy_reduction = 0.66_f32 * contest;
        assert!(synergy_reduction < f32::EPSILON,
            "No synergy reduction without flow investment");
    }

}
