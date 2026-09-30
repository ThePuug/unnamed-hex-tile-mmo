//! Every number combat is balanced by, in one set: the early kit's
//! skills, and how each attribute's stats scale in its three modes —
//! absolute (a potency that grows with level), commitment (a tier's effect)
//! and contest (a share won by a relative advantage).
//!
//! It is one process-wide set, read through [`tuning`]. The live client and
//! server never change it, so they agree on every number; only the balance
//! arena calls [`set_tuning`], between scenarios, to try values without a
//! rebuild. How long an NPC waits to use its signature is behaviour, not
//! balance, and is not here.

use std::sync::RwLock;

use crate::message::AbilityType;

#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    // --- Absolute: potency, level curves, health ---
    /// Potency every actor has before any attribute
    pub potency_base: f32,
    /// Potency each point of the attribute adds, the same for every attribute
    pub potency_per_point: f32,
    /// The damage level curve, `(1 + level × k)^p`, every potency scales by
    pub damage_curve_k: f32,
    pub damage_curve_p: f32,
    /// Points of an attribute that carry its absolute's passive effect half
    /// way to its ceiling; every share rises toward the ceiling and never
    /// reaches it
    pub share_bend: f32,
    /// Health every actor has before Vitality and level
    pub base_health: f32,
    /// Stamina every actor has
    pub stamina_base: f32,
    /// Endurance an actor holds for each point of its Endurance potency
    pub endurance_pool: f32,
    /// Endurance a skill costs for each point of the potency its kind reads
    pub endurance_cost: f32,
    /// Share of its endurance an actor regains each second its stamina is full
    pub endurance_regen: f32,
    /// Endurance an auto-attack struck past the forward faces costs for each
    /// point of the Force it strikes with; one struck within them is free
    pub off_arc_cost: f32,
    /// Stamina an auto-attack struck past the forward faces costs, as a
    /// skill would; one struck within them is free. Without it the swing
    /// waits
    pub off_arc_stamina: f32,
    /// Share longer an actor's recoveries run with its endurance spent
    pub fatigue_recovery: f32,
    /// Share shorter the windows of threats against an actor run with its
    /// endurance spent
    pub fatigue_window: f32,
    /// Health each point of Vitality adds before level
    pub health_per_vitality: f32,
    /// The health level curve, `(1 + level × k)^p`
    pub health_curve_k: f32,
    pub health_curve_p: f32,

    // --- Commitment: what each tier gives (where the tiers fall is fixed, `CommitmentTier::calculate`).
    // A tuned value is two knobs, `_min` at T0 and `_max` at T3, the tiers between evenly spaced
    // (`CommitmentTier::between`) ---
    /// Seconds between auto-attacks at the one pace every actor starts from
    pub base_interval: f32,
    /// Share faster auto-attacks come at the ceiling of Tempo's share
    pub tempo_ceiling: f32,
    /// Share of the recovery a combo fired early skipped that it is let off,
    /// by its user's Ferocity
    pub ferocity_relief_min: f32,
    pub ferocity_relief_max: f32,
    /// Seconds behind the front threat a reaction reaches, by its user's
    /// Awareness; every actor has the least
    pub awareness_span_min: f32,
    pub awareness_span_max: f32,
    /// Share of its own recovery a reaction used through a recovery is let
    /// off, by its user's Preparation
    pub preparation_relief_min: f32,
    pub preparation_relief_max: f32,
    /// The half-angle either side of its heading an actor strikes within,
    /// in degrees, by its Grace
    pub grace_arc_min: f32,
    pub grace_arc_max: f32,
    /// Share of each blow an actor lets land that it banks for its next
    /// skill, by its Grit
    pub grit_bank_min: f32,
    pub grit_bank_max: f32,

    // --- Contest: what a relative advantage wins ---
    /// Advantage in points that wins half of an effect's ceiling; every
    /// contest rises toward its ceiling and never reaches it
    pub contest_scale: f32,
    /// Contest points each level of gap is worth to the higher level
    pub contest_per_level: f32,
    /// Share longer and harder the effects an actor's skill inflicts hold at
    /// the ceiling of the share of the stat the skill reads
    pub effect_hold: f32,
    /// Most of a blow Toughness mitigates, approached and never reached
    pub mitigation_share: f32,
    /// Most of a recovery an Impact advantage pushes it back by
    pub pushback_share: f32,
    /// Most of a recovery a Composure advantage takes off it; below 1, so
    /// no recovery ever runs out at once
    pub composure_share: f32,
    /// Share of a recovery its combo unlocks through, at parity
    pub combo_floor: f32,
    /// Most more a Flow advantage unlocks it through
    pub combo_share: f32,
    /// Seconds every threat's window starts from
    pub reaction_window: f32,
    /// Most more a Reflex advantage widens it by
    pub window_bonus: f32,

    // --- Every swing and blow ---
    /// Share of base potency an auto-attack strikes for without Force
    pub auto_damage: f32,
    /// Share more an auto-attack strikes for at the ceiling of Force's share
    pub force_auto: f32,
    /// Share of its speed an actor keeps for a base interval after
    /// a strike across its line breaks its stride
    pub stride_pace: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
    /// Most often a blow crits, by its striker's Focus over its target's
    /// Toughness, approached and never reached
    pub crit_chance: f32,
    /// What a crit multiplies its blow by
    pub crit_power: f32,

    // --- The early kit: each skill's stamina, its seconds of recovery, and
    // what it does, every value flat. What sizes a skill is the potency its
    // kind reads, an action's Intuition and a reaction's Concentration; what
    // shapes it is the commitment it shows ---
    pub frenzy_cost: f32,
    pub frenzy_recovery: f32,
    /// Share of Intuition a bite strikes for
    pub frenzy_damage: f32,
    pub feint_cost: f32,
    pub feint_recovery: f32,
    /// Share of Intuition a Feint's two strikes deal together
    pub feint_damage: f32,
    /// Share of that the feint deals; the real strike deals the rest
    pub feint_share: f32,
    /// Seconds the real strike follows the feint by: longer than the span
    /// every actor has (`awareness_span_min`), so a reaction with no
    /// Awareness takes one and not the other
    pub feint_gap: f32,
    pub parry_cost: f32,
    pub parry_recovery: f32,
    /// Endurance a Parry pays for each point of damage it turns aside,
    /// with no Resolve; less by its user's Concentration over base potency
    pub parry_effort: f32,
    /// Endurance a Parry pays for each threat it turns aside, as a share of
    /// base potency, whatever the damage; less by Concentration as above
    pub parry_per_threat: f32,
    pub counter_cost: f32,
    pub counter_recovery: f32,
    /// Share of each countered threat's damage sent back by a counterer
    /// with no Resolve; more by its Concentration over base potency
    pub counter_reflect: f32,
    pub leap_cost: f32,
    pub leap_recovery: f32,
    /// Tiles a Leap carries its user, clear of its target or toward it
    pub leap_distance: usize,
    /// Share of Intuition a dive strikes for as it lands in reach
    pub leap_strike: f32,
    pub stride_cost: f32,
    pub stride_recovery: f32,
    /// Seconds a Perfect Stride lasts
    pub stride_secs: f32,
}

impl Tuning {
    /// The numbers the game plays by.
    pub const DEFAULT: Tuning = Tuning {
        potency_base: 19.208,
        potency_per_point: 0.2058,
        damage_curve_k: 0.15,
        damage_curve_p: 1.75,
        share_bend: 800.0,
        base_health: 588.0,
        stamina_base: 100.0,
        endurance_pool: 10.0,
        endurance_cost: 1.0,
        endurance_regen: 0.05,
        off_arc_cost: 0.25,
        off_arc_stamina: 15.0,
        fatigue_recovery: 0.5,
        fatigue_window: 0.3,
        health_per_vitality: 0.9604,
        health_curve_k: 0.10,
        health_curve_p: 2.0,
        base_interval: 2.1,
        tempo_ceiling: 0.5,
        ferocity_relief_min: 0.0,
        ferocity_relief_max: 0.5,
        awareness_span_min: 0.25,
        awareness_span_max: 1.0,
        preparation_relief_min: 0.0,
        preparation_relief_max: 0.5,
        grace_arc_min: 60.0,
        grace_arc_max: 150.0,
        grit_bank_min: 0.0,
        grit_bank_max: 0.3,
        contest_scale: 800.0,
        contest_per_level: 15.0,
        effect_hold: 1.0,
        mitigation_share: 0.525,
        pushback_share: 0.5,
        composure_share: 0.231,
        combo_floor: 0.1,
        combo_share: 0.66,
        reaction_window: 3.0,
        window_bonus: 0.35,
        auto_damage: 1.029,
        force_auto: 1.0,
        stride_pace: 0.7,
        damage_spread: 0.2,
        crit_chance: 0.35,
        crit_power: 1.5,
        frenzy_cost: 20.0,
        frenzy_recovery: 1.5,
        frenzy_damage: 1.5,
        feint_cost: 30.0,
        feint_recovery: 2.5,
        feint_damage: 2.5,
        feint_share: 0.25,
        feint_gap: 0.4,
        parry_cost: 25.0,
        parry_recovery: 1.5,
        parry_effort: 0.5,
        parry_per_threat: 0.25,
        counter_cost: 60.0,
        counter_recovery: 0.5,
        counter_reflect: 0.6,
        leap_cost: 30.0,
        leap_recovery: 1.0,
        leap_distance: 6,
        leap_strike: 1.0,
        stride_cost: 30.0,
        stride_recovery: 1.0,
        stride_secs: 4.0,
    };

    /// Stamina `ability` costs; an auto-attack is free.
    pub fn cost(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_cost,
            AbilityType::Feint => self.feint_cost,
            AbilityType::Parry => self.parry_cost,
            AbilityType::Counter => self.counter_cost,
            AbilityType::Leap => self.leap_cost,
            AbilityType::PerfectStride => self.stride_cost,
        }
    }

    /// Seconds of recovery `ability` leaves its user in; an auto-attack
    /// runs on its own timer instead.
    pub fn recovery(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_recovery,
            AbilityType::Feint => self.feint_recovery,
            AbilityType::Parry => self.parry_recovery,
            AbilityType::Counter => self.counter_recovery,
            AbilityType::Leap => self.leap_recovery,
            AbilityType::PerfectStride => self.stride_recovery,
        }
    }

    /// Share of the target's Toughness mitigation `ability`'s damage strikes
    /// past: a Counter returns the attacker's own blow whole. Every other
    /// threat meets mitigation in full.
    pub fn pierce(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::Counter => 1.0,
            _ => 0.0,
        }
    }

    /// Sets the knob `name` from text, as the arena's command line gives it.
    /// Errs on an unknown knob or a value that does not parse.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value}"))?;
        let knob = match name {
            "base_interval" => &mut self.base_interval,
            "ferocity_relief_min" => &mut self.ferocity_relief_min,
            "ferocity_relief_max" => &mut self.ferocity_relief_max,
            "awareness_span_min" => &mut self.awareness_span_min,
            "awareness_span_max" => &mut self.awareness_span_max,
            "preparation_relief_min" => &mut self.preparation_relief_min,
            "preparation_relief_max" => &mut self.preparation_relief_max,
            "grace_arc_min" => &mut self.grace_arc_min,
            "grace_arc_max" => &mut self.grace_arc_max,
            "grit_bank_min" => &mut self.grit_bank_min,
            "grit_bank_max" => &mut self.grit_bank_max,
            "tempo_ceiling" => &mut self.tempo_ceiling,
            "potency_base" => &mut self.potency_base,
            "potency_per_point" => &mut self.potency_per_point,
            "damage_curve_k" => &mut self.damage_curve_k,
            "damage_curve_p" => &mut self.damage_curve_p,
            "share_bend" => &mut self.share_bend,
            "base_health" => &mut self.base_health,
            "stamina_base" => &mut self.stamina_base,
            "endurance_pool" => &mut self.endurance_pool,
            "endurance_cost" => &mut self.endurance_cost,
            "endurance_regen" => &mut self.endurance_regen,
            "off_arc_cost" => &mut self.off_arc_cost,
            "off_arc_stamina" => &mut self.off_arc_stamina,
            "fatigue_recovery" => &mut self.fatigue_recovery,
            "fatigue_window" => &mut self.fatigue_window,
            "health_per_vitality" => &mut self.health_per_vitality,
            "health_curve_k" => &mut self.health_curve_k,
            "health_curve_p" => &mut self.health_curve_p,
            "contest_scale" => &mut self.contest_scale,
            "contest_per_level" => &mut self.contest_per_level,
            "effect_hold" => &mut self.effect_hold,
            "mitigation_share" => &mut self.mitigation_share,
            "pushback_share" => &mut self.pushback_share,
            "composure_share" => &mut self.composure_share,
            "combo_floor" => &mut self.combo_floor,
            "combo_share" => &mut self.combo_share,
            "reaction_window" => &mut self.reaction_window,
            "window_bonus" => &mut self.window_bonus,
            "auto_damage" => &mut self.auto_damage,
            "force_auto" => &mut self.force_auto,
            "stride_pace" => &mut self.stride_pace,
            "damage_spread" => &mut self.damage_spread,
            "crit_chance" => &mut self.crit_chance,
            "crit_power" => &mut self.crit_power,
            "frenzy_cost" => &mut self.frenzy_cost,
            "frenzy_recovery" => &mut self.frenzy_recovery,
            "frenzy_damage" => &mut self.frenzy_damage,
            "feint_cost" => &mut self.feint_cost,
            "feint_recovery" => &mut self.feint_recovery,
            "feint_damage" => &mut self.feint_damage,
            "feint_share" => &mut self.feint_share,
            "feint_gap" => &mut self.feint_gap,
            "parry_cost" => &mut self.parry_cost,
            "parry_recovery" => &mut self.parry_recovery,
            "parry_effort" => &mut self.parry_effort,
            "parry_per_threat" => &mut self.parry_per_threat,
            "counter_cost" => &mut self.counter_cost,
            "counter_recovery" => &mut self.counter_recovery,
            "counter_reflect" => &mut self.counter_reflect,
            "leap_cost" => &mut self.leap_cost,
            "leap_recovery" => &mut self.leap_recovery,
            "leap_strike" => &mut self.leap_strike,
            "stride_cost" => &mut self.stride_cost,
            "stride_recovery" => &mut self.stride_recovery,
            "stride_secs" => &mut self.stride_secs,
            "leap_distance" => {
                self.leap_distance = number.round().max(1.0) as usize;
                return Ok(());
            }
            _ => return Err(format!("no tuning knob {name}")),
        };
        *knob = number;
        Ok(())
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

static TUNING: RwLock<Tuning> = RwLock::new(Tuning::DEFAULT);

/// The numbers combat plays by, as a copy: hold it through a system, never
/// across a scenario the arena might change.
pub fn tuning() -> Tuning {
    *TUNING.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Replaces the numbers combat plays by. Only the balance arena calls it,
/// between scenarios, with no fight running.
pub fn set_tuning(tuning: Tuning) {
    *TUNING.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = tuning;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_reads_numbers_and_refuses_the_unknown() {
        let mut tuning = Tuning::default();
        tuning.set("frenzy_damage", "0.25").unwrap();
        tuning.set("leap_distance", "2").unwrap();
        tuning.set("grit_bank_max", "0.25").unwrap();
        assert_eq!(tuning.frenzy_damage, 0.25);
        assert_eq!(tuning.leap_distance, 2);
        assert_eq!(tuning.grit_bank_max, 0.25);
        assert!(tuning.set("frenzy_damage", "much").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
    }
}
