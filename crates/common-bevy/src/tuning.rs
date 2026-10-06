//! Every number combat is balanced by, in one set: the early kit's
//! skills, and how each attribute's stats scale in its three modes —
//! absolute (a potency that grows with level), commitment (a tier's effect)
//! and contest (a share won by a relative advantage).
//!
//! Each world holds one set as a resource, [`Tuning::DEFAULT`] on the live
//! client and server, so they agree on every number; the balance arena
//! gives each fight its own, to try values without a rebuild. Whatever
//! reads a number takes the set it plays by.

use bevy::prelude::Resource;

use crate::message::AbilityType;

#[derive(Clone, Copy, Debug, Resource)]
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
    /// Health every actor has before Physique and level
    pub base_health: f32,
    /// Endurance an actor holds for each point of its Endurance potency
    pub endurance_pool: f32,
    /// Endurance a skill costs for each point of its cost, as a share of
    /// the potency its kind reads
    pub endurance_cost: f32,
    /// Share of its endurance an actor regains each second, in combat or out
    pub endurance_regen: f32,
    /// Endurance an auto-attack struck past the forward faces costs for each
    /// point of the Force it strikes with; one struck within them is free
    pub off_arc_cost: f32,
    /// Share of that cost a swing pays in the first band past the
    /// forward faces, out to the first Grace tier's arc
    pub off_arc_share_min: f32,
    /// Share it pays in the last band, out to the third tier's arc; the
    /// band between pays evenly between
    pub off_arc_share_max: f32,
    /// Share longer an actor's recoveries run with its endurance spent
    pub fatigue_recovery: f32,
    /// Share shorter the windows of threats against an actor run with its
    /// endurance spent
    pub fatigue_window: f32,
    /// The power fatigue rises by as endurance is spent: above 1 it stays
    /// light while the pool holds and bites as it empties
    pub fatigue_bend: f32,
    /// Health each point of Physique adds before level
    pub health_per_physique: f32,
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
    /// Share of the time a skill fired early skipped that its chain owes,
    /// paid once the chain ends (`combos::recovery_after`). Authored, never
    /// searched: it sets how far Ferocity and Preparation reach
    pub early_owed: f32,
    /// Seconds behind the front threat a reaction reaches, by its user's
    /// Awareness; every actor has the least
    pub awareness_span_min: f32,
    pub awareness_span_max: f32,
    /// The half-angle either side of its heading an actor strikes within,
    /// in degrees, by its Grace
    pub grace_arc_min: f32,
    pub grace_arc_max: f32,
    /// Share harder a strike lands from past its target's forward faces, a
    /// flank, by its striker's Grace
    pub grace_flank_min: f32,
    pub grace_flank_max: f32,
    /// How much Intimidation's bank holds, filled each second an actor is
    /// engaged by its tier's index, twice that while it is ignored
    /// (`Intimidation`)
    pub intimidation_bank: f32,
    /// Share harder the skill a full bank releases into lands
    pub intimidation_share: f32,
    /// Share of its speed Intimidation's slow takes away, the aura's and
    /// the release's alike
    pub intimidation_slow: f32,
    /// Seconds a release slows its target
    pub intimidation_slow_secs: f32,
    /// Seconds a release roots a target already slowed
    pub intimidation_root_secs: f32,
    /// Seconds the aura's slow lingers on a foe once it stops ignoring the
    /// actor or leaves its reach
    pub intimidation_aura_secs: f32,
    /// Share faster an actor's recovery runs while it waits on a swing it
    /// could not strike, by its Patience
    pub patience_recovery_min: f32,
    pub patience_recovery_max: f32,

    // --- Contest: what a relative advantage wins ---
    /// Advantage in points that wins half of an effect's ceiling; every
    /// contest rises toward its ceiling and never reaches it
    pub contest_scale: f32,
    /// Contest points each level of gap is worth to the higher level
    pub contest_per_level: f32,
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

    // --- The early kit: each skill's cost, its seconds of recovery, and
    // what it does, every value flat. What sizes a skill is the potency its
    // kind reads, an action's Intuition and a reaction's Concentration; what
    // shapes it is the commitment it shows ---
    pub frenzy_cost: f32,
    pub frenzy_recovery: f32,
    /// Share of Intuition a bite strikes for
    pub frenzy_damage: f32,
    pub feint_cost: f32,
    pub feint_recovery: f32,
    pub overpower_cost: f32,
    pub overpower_recovery: f32,
    /// Share of Intuition an Overpower strikes for
    pub overpower_damage: f32,
    pub punish_cost: f32,
    pub punish_recovery: f32,
    /// Share of Intuition a Punish strikes for
    pub punish_damage: f32,
    /// Share harder a Punish lands on a target still in recovery
    pub punish_bonus: f32,
    /// Share of Intuition a Feint strikes for
    pub feint_damage: f32,
    pub parry_cost: f32,
    pub parry_recovery: f32,
    /// Endurance a reaction pays for each point of damage it clears, beside
    /// its flat cost, with no Resolve; less by its user's Concentration over
    /// base potency
    pub reaction_effort: f32,
    /// Endurance a reaction pays for each threat it clears, as a share of
    /// base potency, whatever the damage; less by Concentration as above
    pub reaction_per_threat: f32,
    pub counter_cost: f32,
    pub counter_recovery: f32,
    /// Share of each countered threat's damage sent back by a counterer
    /// with no Resolve; more by its Concentration over base potency
    pub counter_reflect: f32,
    pub leap_cost: f32,
    pub leap_recovery: f32,
    /// Tiles a Leap carries its user, clear of its target or toward it, at
    /// full commitment to its Instinct line
    pub leap_distance: usize,
    /// Share of base potency a Leap onto a target strikes for as it lands
    pub leap_strike: f32,
    pub stride_cost: f32,
    pub stride_recovery: f32,
    /// Seconds a Perfect Stride lasts
    pub stride_secs: f32,
    /// Share faster a Perfect Stride runs its user, at full commitment to
    /// its Agility line
    pub stride_speed: f32,

    // --- Lines: the share of a skill's own numbers it has with no points in
    // its line, rising evenly to all of them at full commitment
    // (`ActorAttributes::line_power`) ---
    pub frenzy_line: f32,
    pub overpower_line: f32,
    pub punish_line: f32,
    pub counter_line: f32,
    pub stride_line: f32,
    pub leap_line: f32,
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
        endurance_pool: 7.551,
        endurance_cost: 0.0,
        endurance_regen: 0.01,
        off_arc_cost: 0.0,
        off_arc_share_min: 1.0 / 3.0,
        off_arc_share_max: 1.0,
        fatigue_recovery: 3.545,
        fatigue_window: 0.319,
        fatigue_bend: 3.297,
        health_per_physique: 0.548,
        health_curve_k: 0.10,
        health_curve_p: 2.0,
        base_interval: 2.1,
        tempo_ceiling: 0.5,
        early_owed: 0.5,
        awareness_span_min: 0.25,
        awareness_span_max: 1.0,
        grace_arc_min: 60.0,
        grace_arc_max: 150.0,
        grace_flank_min: 0.0,
        grace_flank_max: 0.885,
        intimidation_bank: 36.0,
        intimidation_share: 0.146,
        intimidation_slow: 0.213,
        intimidation_slow_secs: 3.0,
        intimidation_root_secs: 1.0,
        intimidation_aura_secs: 1.0,
        patience_recovery_min: 0.0,
        patience_recovery_max: 0.832,
        contest_scale: 800.0,
        contest_per_level: 15.0,
        pushback_share: 0.5,
        composure_share: 0.231,
        combo_floor: 0.5,
        combo_share: 0.4,
        reaction_window: 3.0,
        window_bonus: 0.35,
        auto_damage: 1.029,
        force_auto: 1.0,
        stride_pace: 0.7,
        damage_spread: 0.05,
        crit_chance: 0.35,
        crit_power: 1.5,
        frenzy_cost: 40.0,
        frenzy_recovery: 8.053,
        frenzy_damage: 1.371,
        feint_cost: 15.0,
        feint_recovery: 6.827,
        feint_damage: 0.5,
        overpower_cost: 45.0,
        overpower_recovery: 6.0,
        overpower_damage: 1.773,
        punish_cost: 15.0,
        punish_recovery: 6.0,
        punish_damage: 1.515,
        punish_bonus: 0.603,
        parry_cost: 35.0,
        parry_recovery: 6.53,
        reaction_effort: 0.0,
        reaction_per_threat: 0.0,
        counter_cost: 75.0,
        counter_recovery: 8.167,
        counter_reflect: 0.871,
        leap_cost: 5.0,
        leap_recovery: 4.438,
        leap_distance: 12,
        leap_strike: 2.072,
        stride_cost: 45.0,
        stride_recovery: 5.647,
        frenzy_line: 0.2,
        overpower_line: 0.2,
        punish_line: 0.2,
        counter_line: 0.2,
        stride_line: 0.2,
        leap_line: 0.2,
        stride_secs: 7.129,
        stride_speed: 0.252,
    };

    /// What `ability` costs, its endurance reckoned from it
    /// (`ActorAttributes::skill_endurance`); an auto-attack is free.
    pub fn cost(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_cost,
            AbilityType::Feint => self.feint_cost,
            AbilityType::Overpower => self.overpower_cost,
            AbilityType::Punish => self.punish_cost,
            AbilityType::Parry => self.parry_cost,
            AbilityType::Counter => self.counter_cost,
            AbilityType::Leap => self.leap_cost,
            AbilityType::PerfectStride => self.stride_cost,
        }
    }

    /// The share of the potency it reads `ability` strikes for: none for
    /// one that strikes nothing of its own
    pub fn damage(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::Frenzy => self.frenzy_damage,
            AbilityType::Feint => self.feint_damage,
            AbilityType::Overpower => self.overpower_damage,
            AbilityType::Punish => self.punish_damage,
            AbilityType::Leap => self.leap_strike,
            AbilityType::AutoAttack | AbilityType::Parry | AbilityType::Counter | AbilityType::PerfectStride => 0.0,
        }
    }

    /// The share of its own numbers `ability` has with no points in its
    /// line: whole for a skill of no line
    pub fn line(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::Frenzy => self.frenzy_line,
            AbilityType::Overpower => self.overpower_line,
            AbilityType::Punish => self.punish_line,
            AbilityType::Counter => self.counter_line,
            AbilityType::PerfectStride => self.stride_line,
            AbilityType::Leap => self.leap_line,
            AbilityType::AutoAttack | AbilityType::Feint | AbilityType::Parry => 1.0,
        }
    }

    /// Seconds of recovery `ability` leaves its user in; an auto-attack
    /// runs on its own timer instead.
    pub fn recovery(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_recovery,
            AbilityType::Feint => self.feint_recovery,
            AbilityType::Overpower => self.overpower_recovery,
            AbilityType::Punish => self.punish_recovery,
            AbilityType::Parry => self.parry_recovery,
            AbilityType::Counter => self.counter_recovery,
            AbilityType::Leap => self.leap_recovery,
            AbilityType::PerfectStride => self.stride_recovery,
        }
    }

    /// Sets the knob `name` from text, as the arena's command line gives it.
    /// Errs on an unknown knob or a value that does not parse.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value}"))?;
        if name == "leap_distance" {
            self.leap_distance = number.round().max(1.0) as usize;
            return Ok(());
        }
        *self.knob(name)? = number;
        Ok(())
    }

    /// The knob `name`'s value, as `set` names it. Errs on an unknown knob.
    pub fn get(&self, name: &str) -> Result<f32, String> {
        if name == "leap_distance" {
            return Ok(self.leap_distance as f32);
        }
        let mut copy = *self;
        copy.knob(name).map(|knob| *knob)
    }

    /// The fractional knob `name`
    fn knob(&mut self, name: &str) -> Result<&mut f32, String> {
        Ok(match name {
            "base_interval" => &mut self.base_interval,
            "early_owed" => &mut self.early_owed,
            "awareness_span_min" => &mut self.awareness_span_min,
            "awareness_span_max" => &mut self.awareness_span_max,
            "grace_arc_min" => &mut self.grace_arc_min,
            "grace_arc_max" => &mut self.grace_arc_max,
            "intimidation_bank" => &mut self.intimidation_bank,
            "intimidation_share" => &mut self.intimidation_share,
            "intimidation_slow" => &mut self.intimidation_slow,
            "intimidation_slow_secs" => &mut self.intimidation_slow_secs,
            "intimidation_root_secs" => &mut self.intimidation_root_secs,
            "intimidation_aura_secs" => &mut self.intimidation_aura_secs,
            "grace_flank_min" => &mut self.grace_flank_min,
            "grace_flank_max" => &mut self.grace_flank_max,
            "patience_recovery_min" => &mut self.patience_recovery_min,
            "patience_recovery_max" => &mut self.patience_recovery_max,
            "tempo_ceiling" => &mut self.tempo_ceiling,
            "potency_base" => &mut self.potency_base,
            "potency_per_point" => &mut self.potency_per_point,
            "damage_curve_k" => &mut self.damage_curve_k,
            "damage_curve_p" => &mut self.damage_curve_p,
            "share_bend" => &mut self.share_bend,
            "base_health" => &mut self.base_health,
            "endurance_pool" => &mut self.endurance_pool,
            "endurance_cost" => &mut self.endurance_cost,
            "endurance_regen" => &mut self.endurance_regen,
            "off_arc_cost" => &mut self.off_arc_cost,
            "off_arc_share_min" => &mut self.off_arc_share_min,
            "off_arc_share_max" => &mut self.off_arc_share_max,
            "fatigue_recovery" => &mut self.fatigue_recovery,
            "fatigue_window" => &mut self.fatigue_window,
            "fatigue_bend" => &mut self.fatigue_bend,
            "health_per_physique" => &mut self.health_per_physique,
            "health_curve_k" => &mut self.health_curve_k,
            "health_curve_p" => &mut self.health_curve_p,
            "contest_scale" => &mut self.contest_scale,
            "contest_per_level" => &mut self.contest_per_level,
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
            "overpower_cost" => &mut self.overpower_cost,
            "overpower_recovery" => &mut self.overpower_recovery,
            "overpower_damage" => &mut self.overpower_damage,
            "punish_cost" => &mut self.punish_cost,
            "punish_recovery" => &mut self.punish_recovery,
            "punish_damage" => &mut self.punish_damage,
            "punish_bonus" => &mut self.punish_bonus,
            "parry_cost" => &mut self.parry_cost,
            "parry_recovery" => &mut self.parry_recovery,
            "reaction_effort" => &mut self.reaction_effort,
            "reaction_per_threat" => &mut self.reaction_per_threat,
            "counter_cost" => &mut self.counter_cost,
            "counter_recovery" => &mut self.counter_recovery,
            "counter_reflect" => &mut self.counter_reflect,
            "leap_cost" => &mut self.leap_cost,
            "leap_recovery" => &mut self.leap_recovery,
            "stride_cost" => &mut self.stride_cost,
            "stride_recovery" => &mut self.stride_recovery,
            "stride_secs" => &mut self.stride_secs,
            "stride_speed" => &mut self.stride_speed,
            "frenzy_line" => &mut self.frenzy_line,
            "overpower_line" => &mut self.overpower_line,
            "punish_line" => &mut self.punish_line,
            "counter_line" => &mut self.counter_line,
            "stride_line" => &mut self.stride_line,
            "leap_line" => &mut self.leap_line,
            "leap_strike" => &mut self.leap_strike,
            _ => return Err(format!("no tuning knob {name}")),
        })
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_reads_numbers_and_refuses_the_unknown() {
        let mut tuning = Tuning::default();
        tuning.set("frenzy_damage", "0.25").unwrap();
        tuning.set("leap_distance", "2").unwrap();
        tuning.set("intimidation_share", "0.25").unwrap();
        assert_eq!(tuning.frenzy_damage, 0.25);
        assert_eq!(tuning.leap_distance, 2);
        assert_eq!(tuning.intimidation_share, 0.25);
        assert_eq!(tuning.get("intimidation_share"), Ok(0.25));
        assert_eq!(tuning.get("leap_distance"), Ok(2.0));
        assert!(tuning.get("no_such_knob").is_err());
        assert!(tuning.set("frenzy_damage", "much").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
    }
}
