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
    /// Potency each point of the attribute adds, for Force, Concentration, Precision and Intuition alike
    pub potency_per_point: f32,
    /// The damage level curve, `(1 + level × k)^p`, every potency scales by
    pub damage_curve_k: f32,
    pub damage_curve_p: f32,
    /// Health every actor has before Vitality and level
    pub base_health: f32,
    /// Health each point of Vitality adds before level
    pub health_per_vitality: f32,
    /// The health level curve, `(1 + level × k)^p`
    pub health_curve_k: f32,
    pub health_curve_p: f32,

    // --- Commitment: what each tier gives (where the tiers fall is fixed, `CommitmentTier::calculate`,
    // and so is the window each Awareness tier sees, `ActorAttributes::window_size`) ---
    /// Seconds between auto-attacks at each Intensity tier, T0 to T3
    pub cadence: [f32; 4],
    /// Shots in a Volley at each Intensity tier, T0 to T3: three at full
    /// commitment, so a Disengage, which takes the front blow, takes a third.
    /// None is a Volley without damage, one shot that only slows
    pub volley_shots: [u8; 4],

    // --- Contest: what a relative advantage wins ---
    /// Advantage in points that wins an effect's base share
    pub contest_scale: f32,
    /// Contest points each level of gap is worth to the higher level
    pub contest_per_level: f32,
    /// Share of a blow Toughness mitigates at the base advantage
    pub mitigation_share: f32,
    /// Share of a recovery an Impact advantage pushes it back by
    pub pushback_share: f32,
    /// Share of a recovery a Composure advantage takes off it
    pub composure_share: f32,
    /// Share of a recovery a synergy unlocks its follow-up through, at parity
    pub synergy_floor: f32,
    /// Share more a Flow advantage unlocks it through, at the base advantage
    pub synergy_share: f32,
    /// Seconds every threat's window starts from
    pub reaction_window: f32,
    /// Share more a Reflex advantage widens it by, at the base advantage
    pub window_bonus: f32,

    // --- Abilities: what each costs, how long it locks its user out ---
    /// Stamina each player ability costs
    pub lunge_cost: f32,
    pub overpower_cost: f32,
    pub counter_cost: f32,
    pub kick_cost: f32,
    pub deflect_cost: f32,
    /// Seconds each ability locks its user out of every other
    pub lunge_recovery: f32,
    pub overpower_recovery: f32,
    pub counter_recovery: f32,
    pub kick_recovery: f32,
    pub deflect_recovery: f32,
    pub rattle_recovery: f32,
    pub disengage_recovery: f32,
    pub volley_recovery: f32,
    pub flank_recovery: f32,

    // --- Signatures and their blows ---
    /// Share of base potency an auto-attack strikes for, the same for every actor
    pub auto_damage: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
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
    /// Tiles a Disengage leaps
    pub disengage_leap: usize,
    /// Share of Precision a Disengage adds to its caster's next auto-attack
    pub disengage_precision: f32,
    /// Stamina a Volley costs
    pub volley_cost: f32,
    /// Share of Force each Volley shot strikes for
    pub volley_force: f32,
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
    /// Share of Intuition a Flank strikes for
    pub flank_intuition: f32,
    /// Share of each countered threat's damage sent back, times the counterer's Concentration over base potency
    pub counter_reflect: f32,
}

impl Tuning {
    /// The numbers the game plays by.
    pub const DEFAULT: Tuning = Tuning {
        potency_base: 9.8,
        potency_per_point: 0.2058,
        damage_curve_k: 0.15,
        damage_curve_p: 1.75,
        base_health: 420.0,
        health_per_vitality: 1.96,
        health_curve_k: 0.10,
        health_curve_p: 2.0,
        cadence: [2.1, 1.89, 1.68, 1.47],
        volley_shots: [0, 1, 2, 3],
        contest_scale: 420.0,
        contest_per_level: 15.0,
        mitigation_share: 0.525,
        pushback_share: 0.5,
        composure_share: 0.33,
        synergy_floor: 0.1,
        synergy_share: 0.66,
        reaction_window: 3.0,
        window_bonus: 0.5,
        lunge_cost: 5.0,
        overpower_cost: 40.0,
        counter_cost: 60.0,
        kick_cost: 40.0,
        deflect_cost: 50.0,
        lunge_recovery: 2.0,
        overpower_recovery: 3.0,
        counter_recovery: 2.0,
        kick_recovery: 4.0,
        deflect_recovery: 1.0,
        rattle_recovery: 2.0,
        disengage_recovery: 2.8,
        volley_recovery: 2.0,
        flank_recovery: 2.0,
        auto_damage: 1.05,
        damage_spread: 0.2,
        lunge_force: 0.7,
        lunge_dot: 0.21,
        lunge_pierce: 0.49,
        rattle_cost: 28.0,
        rattle_health: 0.035,
        rattle_growth: 0.343,
        rattle_daze: 0.0686,
        rattle_stacks: 3,
        disengage_cost: 28.0,
        disengage_leap: 2,
        disengage_precision: 0.49,
        volley_cost: 20.0,
        volley_force: 1.12,
        volley_slow: 0.5,
        volley_slow_secs: 2.45,
        volley_leap: 10,
        flank_cost: 30.0,
        flank_stun: 3.0,
        flank_intuition: 1.0,
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
            AbilityType::Deflect => self.deflect_cost,
            AbilityType::Rattle => self.rattle_cost,
            AbilityType::Disengage => self.disengage_cost,
            AbilityType::Volley => self.volley_cost,
            AbilityType::Flank => self.flank_cost,
        }
    }

    /// Seconds `ability` locks its user out of every other; an auto-attack
    /// runs on its own timer instead.
    pub fn recovery(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Lunge => self.lunge_recovery,
            AbilityType::Overpower => self.overpower_recovery,
            AbilityType::Counter => self.counter_recovery,
            AbilityType::Kick => self.kick_recovery,
            AbilityType::Deflect => self.deflect_recovery,
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
    /// A tier's four values are `name_0` to `name_3` (`cadence_2=1.8`).
    /// Errs on an unknown knob or a value that does not parse.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value}"))?;
        let tier = |base: &str| name.strip_prefix(base).and_then(|rest| rest.strip_prefix('_')).and_then(|i| i.parse::<usize>().ok()).filter(|i| *i < 4);
        if let Some(i) = tier("volley_shots") {
            self.volley_shots[i] = number.round().max(0.0) as u8;
            return Ok(());
        }
        if let Some(i) = tier("cadence") {
            self.cadence[i] = number;
            return Ok(());
        }
        let knob = match name {
            "potency_base" => &mut self.potency_base,
            "potency_per_point" => &mut self.potency_per_point,
            "damage_curve_k" => &mut self.damage_curve_k,
            "damage_curve_p" => &mut self.damage_curve_p,
            "base_health" => &mut self.base_health,
            "health_per_vitality" => &mut self.health_per_vitality,
            "health_curve_k" => &mut self.health_curve_k,
            "health_curve_p" => &mut self.health_curve_p,
            "contest_scale" => &mut self.contest_scale,
            "contest_per_level" => &mut self.contest_per_level,
            "mitigation_share" => &mut self.mitigation_share,
            "pushback_share" => &mut self.pushback_share,
            "composure_share" => &mut self.composure_share,
            "synergy_floor" => &mut self.synergy_floor,
            "synergy_share" => &mut self.synergy_share,
            "reaction_window" => &mut self.reaction_window,
            "window_bonus" => &mut self.window_bonus,
            "lunge_cost" => &mut self.lunge_cost,
            "overpower_cost" => &mut self.overpower_cost,
            "counter_cost" => &mut self.counter_cost,
            "kick_cost" => &mut self.kick_cost,
            "deflect_cost" => &mut self.deflect_cost,
            "lunge_recovery" => &mut self.lunge_recovery,
            "overpower_recovery" => &mut self.overpower_recovery,
            "counter_recovery" => &mut self.counter_recovery,
            "kick_recovery" => &mut self.kick_recovery,
            "deflect_recovery" => &mut self.deflect_recovery,
            "rattle_recovery" => &mut self.rattle_recovery,
            "disengage_recovery" => &mut self.disengage_recovery,
            "volley_recovery" => &mut self.volley_recovery,
            "flank_recovery" => &mut self.flank_recovery,
            "auto_damage" => &mut self.auto_damage,
            "damage_spread" => &mut self.damage_spread,
            "lunge_force" => &mut self.lunge_force,
            "lunge_dot" => &mut self.lunge_dot,
            "lunge_pierce" => &mut self.lunge_pierce,
            "rattle_cost" => &mut self.rattle_cost,
            "rattle_health" => &mut self.rattle_health,
            "rattle_growth" => &mut self.rattle_growth,
            "rattle_daze" => &mut self.rattle_daze,
            "disengage_cost" => &mut self.disengage_cost,
            "disengage_precision" => &mut self.disengage_precision,
            "volley_cost" => &mut self.volley_cost,
            "volley_force" => &mut self.volley_force,
            "volley_slow" => &mut self.volley_slow,
            "volley_slow_secs" => &mut self.volley_slow_secs,
            "flank_cost" => &mut self.flank_cost,
            "flank_stun" => &mut self.flank_stun,
            "flank_intuition" => &mut self.flank_intuition,
            "counter_reflect" => &mut self.counter_reflect,
            "rattle_stacks" => {
                self.rattle_stacks = number.round().max(1.0) as u8;
                return Ok(());
            }
            "disengage_leap" => {
                self.disengage_leap = number.round().max(1.0) as usize;
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
    fn set_reads_numbers_and_tiers_and_refuses_the_unknown() {
        let mut tuning = Tuning::default();
        tuning.set("lunge_pierce", "0.25").unwrap();
        tuning.set("cadence_2", "1.8").unwrap();
        assert_eq!(tuning.lunge_pierce, 0.25);
        assert_eq!(tuning.cadence[2], 1.8);
        assert!(tuning.set("lunge_pierce", "much").is_err());
        assert!(tuning.set("cadence_4", "1").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
    }

    #[test]
    fn the_tiers_and_their_effects_run_in_order() {
        let tuning = Tuning::default();
        // A higher tier never gives less, and the top gives more than none
        for t in 0..3 {
            assert!(tuning.cadence[t] >= tuning.cadence[t + 1]);
            assert!(tuning.volley_shots[t] <= tuning.volley_shots[t + 1]);
        }
        assert!(tuning.cadence[3] < tuning.cadence[0]);
        assert!(tuning.volley_shots[3] > tuning.volley_shots[0]);
    }
}
