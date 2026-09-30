//! Every number combat is balanced by, in one set: the archetypes'
//! signatures, and how each attribute's stats scale in its three modes —
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
    /// Endurance a skill or reaction costs for each point of the potency
    /// it strikes with
    pub endurance_cost: f32,
    /// Share of its endurance an actor regains each second its stamina is full
    pub endurance_regen: f32,
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
    /// Seconds behind the front threat a reaction reaches, by its user's
    /// Awareness; every actor has the least
    pub awareness_span_min: f32,
    pub awareness_span_max: f32,
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
    /// Share longer and harder the effects an actor inflicts hold at the
    /// ceiling of Concentration's share
    pub concentration_hold: f32,
    /// Most of a blow Toughness mitigates, approached and never reached
    pub mitigation_share: f32,
    /// Most of a landed blow a Presence advantage spills onto each other
    /// hostile within the striker's reach
    pub spill_share: f32,
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

    // --- Abilities: what each costs, how long its recovery runs ---
    /// Stamina each player ability costs
    pub lunge_cost: f32,
    pub overpower_cost: f32,
    pub counter_cost: f32,
    pub kick_cost: f32,
    /// Seconds of recovery each ability leaves its user in
    pub lunge_recovery: f32,
    pub overpower_recovery: f32,
    pub counter_recovery: f32,
    pub kick_recovery: f32,
    pub rattle_recovery: f32,
    pub disengage_recovery: f32,
    pub volley_recovery: f32,
    pub flank_recovery: f32,

    // --- Signatures and their blows ---
    /// Share of base potency an auto-attack strikes for without Force
    pub auto_damage: f32,
    /// Share more an auto-attack strikes for at the ceiling of Force's share
    pub force_auto: f32,
    /// Share of its speed an actor keeps for a base interval after
    /// a strike across its line breaks its stride
    pub stride_pace: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
    /// Chance a blow crits at the ceiling of Intuition's share
    pub crit_chance: f32,
    /// What a crit multiplies its blow by without Agility
    pub crit_power: f32,
    /// How much more at the ceiling of Agility's share
    pub crit_severity: f32,
    /// Share of Force a Lunge's strike deals
    pub lunge_force: f32,
    /// Share of Force each tick of a Lunge's DoT deals
    pub lunge_dot: f32,
    /// Share of the target's Toughness mitigation a Lunge strikes past
    pub lunge_pierce: f32,
    /// Stamina a Rattle costs
    pub rattle_cost: f32,
    /// Share of the Juggernaut's own health a Rattle strikes for
    pub rattle_health: f32,
    /// Share more a Rattle strikes for with each daze stack already on its target
    pub rattle_growth: f32,
    /// Share of its pace each Rattle stack takes from the target
    pub rattle_daze: f32,
    /// Most stacks a target carries
    pub rattle_stacks: u8,
    /// Stamina a Disengage costs
    pub disengage_cost: f32,
    /// Tiles a Disengage leaps away from an attacker in contact at least; it
    /// leaps further where that falls short of breaking its own reach
    pub disengage_leap: usize,
    /// Tiles a Disengage leaps toward an attacker already out of contact
    pub disengage_close: usize,
    /// Share of Intuition a Disengage adds to its caster's next auto-attack
    pub disengage_intuition: f32,
    /// Stamina a Volley costs
    pub volley_cost: f32,
    /// Shots in a Volley
    pub volley_shots: u8,
    /// Share of Tempo each Volley shot strikes for
    pub volley_precision: f32,
    /// Share of its speed a Volley takes from its target
    pub volley_slow: f32,
    /// Seconds a Volley's slow lasts
    pub volley_slow_secs: f32,
    /// Tiles a Kiter leaps clear as its Volley's slow lands
    pub volley_leap: usize,
    /// Stamina a Flank costs
    pub flank_cost: f32,
    /// Seconds a Flank stuns its target
    pub flank_stun: f32,
    /// Share of Endurance a Flank strikes for
    pub flank_endurance: f32,
    /// Share of each countered threat's damage sent back, times the counterer's Concentration over base potency
    pub counter_reflect: f32,
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
        fatigue_recovery: 0.5,
        fatigue_window: 0.3,
        health_per_vitality: 0.9604,
        health_curve_k: 0.10,
        health_curve_p: 2.0,
        base_interval: 2.1,
        tempo_ceiling: 0.5,
        awareness_span_min: 0.25,
        awareness_span_max: 1.0,
        grace_arc_min: 60.0,
        grace_arc_max: 150.0,
        grit_bank_min: 0.0,
        grit_bank_max: 0.3,
        contest_scale: 800.0,
        contest_per_level: 15.0,
        concentration_hold: 1.0,
        mitigation_share: 0.525,
        spill_share: 0.5,
        pushback_share: 0.5,
        composure_share: 0.231,
        combo_floor: 0.1,
        combo_share: 0.66,
        reaction_window: 3.0,
        window_bonus: 0.35,
        lunge_cost: 5.0,
        overpower_cost: 40.0,
        counter_cost: 60.0,
        kick_cost: 40.0,
        lunge_recovery: 2.0,
        overpower_recovery: 3.0,
        counter_recovery: 0.5,
        kick_recovery: 4.0,
        rattle_recovery: 2.0,
        disengage_recovery: 0.98,
        volley_recovery: 1.4,
        flank_recovery: 2.0,
        auto_damage: 1.029,
        force_auto: 1.0,
        stride_pace: 0.7,
        damage_spread: 0.2,
        crit_chance: 0.35,
        crit_power: 1.5,
        crit_severity: 1.0,
        lunge_force: 0.7,
        lunge_dot: 0.147,
        lunge_pierce: 0.49,
        rattle_cost: 19.6,
        rattle_health: 0.035,
        rattle_growth: 0.6723,
        rattle_daze: 0.0686,
        rattle_stacks: 3,
        disengage_cost: 39.2,
        disengage_leap: 1,
        disengage_close: 8,
        disengage_intuition: 0.686,
        volley_cost: 20.0,
        volley_shots: 3,
        volley_precision: 0.33,
        volley_slow: 0.5,
        volley_slow_secs: 3.43,
        volley_leap: 10,
        flank_cost: 30.0,
        flank_stun: 1.0,
        flank_endurance: 1.0,
        counter_reflect: 0.2271,
    };

    /// Stamina `ability` costs; an auto-attack is free.
    pub fn cost(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Lunge => self.lunge_cost,
            AbilityType::Overpower => self.overpower_cost,
            AbilityType::Counter => self.counter_cost,
            AbilityType::Kick => self.kick_cost,
            AbilityType::Rattle => self.rattle_cost,
            AbilityType::Disengage => self.disengage_cost,
            AbilityType::Volley => self.volley_cost,
            AbilityType::Flank => self.flank_cost,
        }
    }

    /// Seconds of recovery `ability` leaves its user in; an auto-attack
    /// runs on its own timer instead.
    pub fn recovery(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Lunge => self.lunge_recovery,
            AbilityType::Overpower => self.overpower_recovery,
            AbilityType::Counter => self.counter_recovery,
            AbilityType::Kick => self.kick_recovery,
            AbilityType::Rattle => self.rattle_recovery,
            AbilityType::Disengage => self.disengage_recovery,
            AbilityType::Volley => self.volley_recovery,
            AbilityType::Flank => self.flank_recovery,
        }
    }

    /// Share of the target's Toughness mitigation `ability`'s damage strikes
    /// past: a Lunge carries the whole body behind it, and a Counter returns
    /// the attacker's own blow whole. Every other threat meets mitigation in full.
    pub fn pierce(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::Lunge => self.lunge_pierce,
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
            "awareness_span_min" => &mut self.awareness_span_min,
            "awareness_span_max" => &mut self.awareness_span_max,
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
            "fatigue_recovery" => &mut self.fatigue_recovery,
            "fatigue_window" => &mut self.fatigue_window,
            "health_per_vitality" => &mut self.health_per_vitality,
            "health_curve_k" => &mut self.health_curve_k,
            "health_curve_p" => &mut self.health_curve_p,
            "contest_scale" => &mut self.contest_scale,
            "contest_per_level" => &mut self.contest_per_level,
            "concentration_hold" => &mut self.concentration_hold,
            "mitigation_share" => &mut self.mitigation_share,
            "spill_share" => &mut self.spill_share,
            "pushback_share" => &mut self.pushback_share,
            "composure_share" => &mut self.composure_share,
            "combo_floor" => &mut self.combo_floor,
            "combo_share" => &mut self.combo_share,
            "reaction_window" => &mut self.reaction_window,
            "window_bonus" => &mut self.window_bonus,
            "lunge_cost" => &mut self.lunge_cost,
            "overpower_cost" => &mut self.overpower_cost,
            "counter_cost" => &mut self.counter_cost,
            "kick_cost" => &mut self.kick_cost,
            "lunge_recovery" => &mut self.lunge_recovery,
            "overpower_recovery" => &mut self.overpower_recovery,
            "counter_recovery" => &mut self.counter_recovery,
            "kick_recovery" => &mut self.kick_recovery,
            "rattle_recovery" => &mut self.rattle_recovery,
            "disengage_recovery" => &mut self.disengage_recovery,
            "volley_recovery" => &mut self.volley_recovery,
            "flank_recovery" => &mut self.flank_recovery,
            "auto_damage" => &mut self.auto_damage,
            "force_auto" => &mut self.force_auto,
            "stride_pace" => &mut self.stride_pace,
            "damage_spread" => &mut self.damage_spread,
            "crit_chance" => &mut self.crit_chance,
            "crit_power" => &mut self.crit_power,
            "crit_severity" => &mut self.crit_severity,
            "lunge_force" => &mut self.lunge_force,
            "lunge_dot" => &mut self.lunge_dot,
            "lunge_pierce" => &mut self.lunge_pierce,
            "rattle_cost" => &mut self.rattle_cost,
            "rattle_health" => &mut self.rattle_health,
            "rattle_growth" => &mut self.rattle_growth,
            "rattle_daze" => &mut self.rattle_daze,
            "disengage_cost" => &mut self.disengage_cost,
            "disengage_intuition" => &mut self.disengage_intuition,
            "volley_cost" => &mut self.volley_cost,
            "volley_precision" => &mut self.volley_precision,
            "volley_slow" => &mut self.volley_slow,
            "volley_slow_secs" => &mut self.volley_slow_secs,
            "flank_cost" => &mut self.flank_cost,
            "flank_stun" => &mut self.flank_stun,
            "flank_endurance" => &mut self.flank_endurance,
            "counter_reflect" => &mut self.counter_reflect,
            "volley_shots" => {
                self.volley_shots = number.round().max(1.0) as u8;
                return Ok(());
            }
            "rattle_stacks" => {
                self.rattle_stacks = number.round().max(1.0) as u8;
                return Ok(());
            }
            "disengage_leap" => {
                self.disengage_leap = number.round().max(1.0) as usize;
                return Ok(());
            }
            "disengage_close" => {
                self.disengage_close = number.round().max(1.0) as usize;
                return Ok(());
            }
            "volley_leap" => {
                self.volley_leap = number.round().max(1.0) as usize;
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
        tuning.set("lunge_pierce", "0.25").unwrap();
        tuning.set("volley_shots", "2").unwrap();
        tuning.set("grit_bank_max", "0.25").unwrap();
        assert_eq!(tuning.lunge_pierce, 0.25);
        assert_eq!(tuning.volley_shots, 2);
        assert_eq!(tuning.grit_bank_max, 0.25);
        assert!(tuning.set("lunge_pierce", "much").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
    }
}
