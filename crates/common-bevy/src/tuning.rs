//! Every number combat is balanced by, in one set: what every build
//! holds, the same at every level, and the kit's skills, each its
//! endurance cost, its seconds of activation and of recharge, and what it
//! does. Level counts where a blow lands (`level_gap`).
//!
//! Each world holds one set as a resource, [`Tuning::DEFAULT`] on the live
//! client and server, so they agree on every number; the balance arena
//! gives each fight its own, to try values without a rebuild. Whatever
//! reads a number takes the set it plays by.

use bevy::{prelude::Resource, reflect::{GetPath, Reflect}};

use crate::message::AbilityType;

#[derive(Clone, Copy, Debug, Reflect, Resource)]
pub struct Tuning {
    // --- Absolute: potency, the pools, and level where a blow lands ---
    /// Potency every actor has, the same at every level: what a skill's and
    /// an auto-attack's damage are shares of
    pub potency_base: f32,
    /// What a blow is multiplied by for each level its striker stands above
    /// its target, and divided by for each below (`damage::level_factor`)
    pub level_gap: f32,
    /// Health of a build with nothing of Physique and Discipline, the same
    /// at every level
    pub base_health: f32,
    /// Share more health a Physique and Discipline pair holding the whole
    /// build gives
    pub health_depth: f32,
    /// The endurance pool of a build with nothing of Instinct and Resolve:
    /// the points every skill's price is counted in, the same at every level
    pub endurance_pool: f32,
    /// Share deeper the pool runs for an Instinct and Resolve pair holding
    /// as much as one attribute can
    pub endurance_depth: f32,
    /// Share of the base pool (`endurance_pool`) every actor regains each
    /// second, in combat or out, however deep its own pool runs
    pub endurance_regen: f32,

    // --- Every swing and blow ---
    /// Seconds between auto-attacks, the one pace every actor swings at
    pub base_interval: f32,
    /// Share of base potency an auto-attack strikes for without Force
    pub auto_damage: f32,
    /// Share more an auto-attack strikes for from a Might and Agility pair
    /// holding the whole build, evenly less for less
    pub force_auto: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
    /// Tiles a second a drag slides its target, toward its source or away,
    /// held in place until it arrives
    pub drag_pace: f32,
    /// Seconds a blocking skill's block stands unless an attack spends it
    pub block_secs: f32,

    // --- The kit: each skill's endurance cost, its seconds of activation
    // and of recharge, and what it does, every value flat. A skill's own
    // numbers are scaled by its line (`ActorAttributes::line_power`) ---
    pub frenzy_cost: f32,
    pub frenzy_activation: f32,
    pub frenzy_recharge: f32,
    /// Share of base potency a Frenzy strikes for, at full commitment to
    /// its Might line
    pub frenzy_damage: f32,
    pub feint_cost: f32,
    pub feint_activation: f32,
    pub feint_recharge: f32,
    /// Share of base potency a Feint strikes for, whatever the build
    pub feint_damage: f32,
    pub overpower_cost: f32,
    pub overpower_activation: f32,
    pub overpower_recharge: f32,
    /// Share of base potency an Overpower strikes for, at full commitment
    /// to its Physique line
    pub overpower_damage: f32,
    pub punish_cost: f32,
    pub punish_activation: f32,
    pub punish_recharge: f32,
    /// Share of base potency a Punish strikes for, whatever the build
    pub punish_base: f32,
    /// Share of base potency more a Punish strikes for on a target
    /// mid-activation, at full commitment to its Instinct line
    pub punish_caught: f32,
    pub parry_cost: f32,
    pub parry_activation: f32,
    pub parry_recharge: f32,
    pub counter_cost: f32,
    pub counter_activation: f32,
    pub counter_recharge: f32,
    /// Share of the blow it blocked a Counter strikes its striker for, at
    /// full commitment to its Resolve line
    pub counter_reflect: f32,
    pub leap_cost: f32,
    pub leap_activation: f32,
    pub leap_recharge: f32,
    /// Tiles a Leap carries its user, clear of its target or toward it, at
    /// full commitment to its Discipline line
    pub leap_distance: usize,
    /// Share of base potency a Leap onto a target strikes for as it lands
    pub leap_strike: f32,
    pub stride_cost: f32,
    pub stride_activation: f32,
    pub stride_recharge: f32,
    /// Seconds a Perfect Stride lasts
    pub stride_secs: f32,
    /// Share faster a Perfect Stride runs its user, at full commitment to
    /// its Agility line
    pub stride_speed: f32,
    pub clash_cost: f32,
    pub clash_activation: f32,
    pub clash_recharge: f32,
    /// Seconds a Clash stuns the striker of the attack it blocks, at full
    /// commitment to its Might line
    pub clash_stun: f32,
    pub sidestep_cost: f32,
    pub sidestep_activation: f32,
    pub sidestep_recharge: f32,
    /// Tiles round its target a Sidestep steps, at full commitment to its
    /// Agility line
    pub sidestep_steps: f32,
    pub backstab_cost: f32,
    pub backstab_activation: f32,
    pub backstab_recharge: f32,
    /// Share of base potency a Backstab strikes for, at full commitment to
    /// its Agility line
    pub backstab_damage: f32,
    pub pounce_cost: f32,
    pub pounce_activation: f32,
    pub pounce_recharge: f32,
    /// Tiles a Pounce lunges toward its target, at full commitment to its
    /// Instinct line
    pub pounce_tiles: f32,
    pub kick_cost: f32,
    pub kick_activation: f32,
    pub kick_recharge: f32,
    /// Share of base potency a Kick strikes for, whatever the build
    pub kick_damage: f32,
    /// Tiles a Kick knocks its target back, at full commitment to its
    /// Discipline line
    pub kick_tiles: f32,
    pub defy_cost: f32,
    pub defy_activation: f32,
    pub defy_recharge: f32,
    /// Share of base potency a Defy strikes for at full health, whatever
    /// the build
    pub defy_damage: f32,
    /// Share of base potency more a Defy strikes for with none of its
    /// user's health left, evenly less for less lost, at full commitment to
    /// its Resolve line
    pub defy_low: f32,
    pub bloodlust_cost: f32,
    pub bloodlust_activation: f32,
    pub bloodlust_recharge: f32,
    /// Seconds a Bloodlust lasts
    pub bloodlust_secs: f32,
    /// Share of the damage its attacks land a Bloodlust returns as health,
    /// at full commitment to its Might line
    pub bloodlust_share: f32,
    pub brace_cost: f32,
    pub brace_activation: f32,
    pub brace_recharge: f32,
    /// Seconds a Brace lasts, at full commitment to its Physique line
    pub brace_secs: f32,
    pub surge_cost: f32,
    pub surge_activation: f32,
    pub surge_recharge: f32,
    /// Seconds a Surge lasts unspent
    pub surge_secs: f32,
    /// Share more a Surge adds to its user's next skill strike for each
    /// tile it moved, at full commitment to its Discipline line
    pub surge_per_tile: f32,
    pub vigil_cost: f32,
    pub vigil_activation: f32,
    pub vigil_recharge: f32,
    /// Seconds a Vigil lasts, at full commitment to its Resolve line
    pub vigil_secs: f32,
    pub lurk_cost: f32,
    pub lurk_activation: f32,
    pub lurk_recharge: f32,
    /// Seconds a Lurk lasts unbroken, at full commitment to its Instinct
    /// line
    pub lurk_secs: f32,
    /// Share harder a skill struck out of a Lurk lands, at full commitment
    /// to its Instinct line
    pub lurk_ambush: f32,
    pub yank_cost: f32,
    pub yank_activation: f32,
    pub yank_recharge: f32,
    /// Tiles a Yank drags its target toward its user, and past its own
    /// reach how far off it takes one, at full commitment to its Physique
    /// line
    pub yank_tiles: f32,
    pub drag_cost: f32,
    pub drag_activation: f32,
    pub drag_recharge: f32,
    /// Tiles ahead along its caster's heading a Drag rune is cut: from 4.2
    /// on, every step of a turn moves the tile, wherever in its hex the
    /// caster stands
    pub drag_distance: f32,
    /// Hexes from its tile a Drag rune reaches, at full commitment to its
    /// caster's Physique line
    pub drag_reach: f32,
    /// Seconds a Drag rune stands unless struck down or lifted
    pub drag_secs: f32,
    /// Seconds from one of a control rune's pulses beginning to the next
    pub rune_pulse: f32,
    /// Seconds a control rune's pulse winds up, shown on every foe then in
    /// its reach, before it works on those still inside
    pub rune_windup: f32,

    // --- Lines: the share of a skill's own numbers it has with no points in
    // its line, rising evenly to all of them at full commitment
    // (`ActorAttributes::line_power`) ---
    pub frenzy_line: f32,
    pub overpower_line: f32,
    pub punish_line: f32,
    pub counter_line: f32,
    pub stride_line: f32,
    pub leap_line: f32,
    pub clash_line: f32,
    pub sidestep_line: f32,
    pub yank_line: f32,
    pub backstab_line: f32,
    pub pounce_line: f32,
    pub kick_line: f32,
    pub defy_line: f32,
    pub bloodlust_line: f32,
    pub brace_line: f32,
    pub surge_line: f32,
    pub vigil_line: f32,
    pub lurk_line: f32,
    pub drag_line: f32,
}

impl Tuning {
    /// The numbers the game plays by.
    pub const DEFAULT: Tuning = Tuning {
        potency_base: 40.0,
        level_gap: 1.2,
        base_health: 1000.0,
        health_depth: 0.5,
        endurance_pool: 100.0,
        endurance_depth: 0.5,
        endurance_regen: 0.03,
        base_interval: 2.1,
        auto_damage: 0.6,
        force_auto: 0.42,
        damage_spread: 0.05,
        drag_pace: 8.0,
        block_secs: 2.0,
        frenzy_cost: 5.0,
        frenzy_activation: 0.5,
        frenzy_recharge: 3.0,
        frenzy_damage: 1.3,
        feint_cost: 5.0,
        feint_activation: 0.25,
        feint_recharge: 8.0,
        feint_damage: 0.4,
        overpower_cost: 10.0,
        overpower_activation: 1.5,
        overpower_recharge: 8.0,
        overpower_damage: 4.5,
        punish_cost: 5.0,
        punish_activation: 0.25,
        punish_recharge: 5.0,
        punish_base: 1.9,
        punish_caught: 3.0,
        parry_cost: 5.0,
        parry_activation: 0.0,
        parry_recharge: 8.0,
        counter_cost: 10.0,
        counter_activation: 0.0,
        counter_recharge: 10.0,
        counter_reflect: 0.8,
        leap_cost: 10.0,
        leap_activation: 0.0,
        leap_recharge: 12.0,
        leap_distance: 12,
        leap_strike: 1.8,
        stride_cost: 10.0,
        stride_activation: 0.0,
        stride_recharge: 20.0,
        stride_secs: 5.849,
        stride_speed: 0.401,
        clash_cost: 10.0,
        clash_activation: 0.0,
        clash_recharge: 12.0,
        clash_stun: 0.75,
        sidestep_cost: 10.0,
        sidestep_activation: 0.0,
        sidestep_recharge: 10.0,
        sidestep_steps: 2.0,
        backstab_cost: 10.0,
        backstab_activation: 0.25,
        backstab_recharge: 6.0,
        backstab_damage: 5.0,
        pounce_cost: 5.0,
        pounce_activation: 0.0,
        pounce_recharge: 10.0,
        pounce_tiles: 3.0,
        kick_cost: 5.0,
        kick_activation: 0.5,
        kick_recharge: 8.0,
        kick_damage: 1.1,
        kick_tiles: 3.0,
        defy_cost: 5.0,
        defy_activation: 0.75,
        defy_recharge: 6.0,
        defy_damage: 1.5,
        defy_low: 2.5,
        bloodlust_cost: 10.0,
        bloodlust_activation: 0.0,
        bloodlust_recharge: 20.0,
        bloodlust_secs: 6.0,
        bloodlust_share: 0.12,
        brace_cost: 10.0,
        brace_activation: 0.0,
        brace_recharge: 20.0,
        brace_secs: 3.0,
        surge_cost: 5.0,
        surge_activation: 0.0,
        surge_recharge: 15.0,
        surge_secs: 3.0,
        surge_per_tile: 0.15,
        vigil_cost: 10.0,
        vigil_activation: 0.0,
        vigil_recharge: 20.0,
        vigil_secs: 4.0,
        lurk_cost: 10.0,
        lurk_activation: 0.0,
        lurk_recharge: 20.0,
        lurk_secs: 6.0,
        lurk_ambush: 1.0,
        yank_cost: 10.0,
        yank_activation: 0.5,
        yank_recharge: 12.0,
        yank_tiles: 3.0,
        drag_cost: 10.0,
        drag_activation: 0.5,
        drag_recharge: 15.0,
        drag_distance: 5.0,
        drag_reach: 7.0,
        drag_secs: 12.0,
        rune_pulse: 2.0,
        rune_windup: 1.0,
        frenzy_line: 0.2,
        overpower_line: 0.2,
        punish_line: 0.2,
        counter_line: 0.2,
        stride_line: 0.2,
        leap_line: 0.2,
        clash_line: 0.2,
        sidestep_line: 0.2,
        yank_line: 0.2,
        backstab_line: 0.2,
        pounce_line: 0.2,
        kick_line: 0.2,
        defy_line: 0.2,
        bloodlust_line: 0.2,
        brace_line: 0.2,
        surge_line: 0.2,
        vigil_line: 0.2,
        lurk_line: 0.2,
        drag_line: 2.0 / 7.0,
    };

    /// The endurance `ability` costs, in points of the pool
    /// (`endurance_pool`), whatever the build and level; an auto-attack is
    /// free. Authored, never searched: a balance search raises prices until
    /// skills stop mattering.
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
            AbilityType::Clash => self.clash_cost,
            AbilityType::Sidestep => self.sidestep_cost,
            AbilityType::Yank => self.yank_cost,
            AbilityType::Backstab => self.backstab_cost,
            AbilityType::Pounce => self.pounce_cost,
            AbilityType::Kick => self.kick_cost,
            AbilityType::Defy => self.defy_cost,
            AbilityType::Bloodlust => self.bloodlust_cost,
            AbilityType::Brace => self.brace_cost,
            AbilityType::Surge => self.surge_cost,
            AbilityType::Vigil => self.vigil_cost,
            AbilityType::Lurk => self.lurk_cost,
            AbilityType::Drag => self.drag_cost,
        }
    }

    /// Seconds from `ability`'s use until it lands; none for one that takes
    /// hold at once, and for an auto-attack, which lands as it swings
    pub fn activation(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_activation,
            AbilityType::Feint => self.feint_activation,
            AbilityType::Overpower => self.overpower_activation,
            AbilityType::Punish => self.punish_activation,
            AbilityType::Parry => self.parry_activation,
            AbilityType::Counter => self.counter_activation,
            AbilityType::Leap => self.leap_activation,
            AbilityType::PerfectStride => self.stride_activation,
            AbilityType::Clash => self.clash_activation,
            AbilityType::Sidestep => self.sidestep_activation,
            AbilityType::Yank => self.yank_activation,
            AbilityType::Backstab => self.backstab_activation,
            AbilityType::Pounce => self.pounce_activation,
            AbilityType::Kick => self.kick_activation,
            AbilityType::Defy => self.defy_activation,
            AbilityType::Bloodlust => self.bloodlust_activation,
            AbilityType::Brace => self.brace_activation,
            AbilityType::Surge => self.surge_activation,
            AbilityType::Vigil => self.vigil_activation,
            AbilityType::Lurk => self.lurk_activation,
            AbilityType::Drag => self.drag_activation,
        }
    }

    /// Seconds `ability` recharges for from the end of its activation,
    /// however that ended; an auto-attack runs on its own clock instead
    /// (`base_interval`)
    pub fn recharge(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::AutoAttack => 0.0,
            AbilityType::Frenzy => self.frenzy_recharge,
            AbilityType::Feint => self.feint_recharge,
            AbilityType::Overpower => self.overpower_recharge,
            AbilityType::Punish => self.punish_recharge,
            AbilityType::Parry => self.parry_recharge,
            AbilityType::Counter => self.counter_recharge,
            AbilityType::Leap => self.leap_recharge,
            AbilityType::PerfectStride => self.stride_recharge,
            AbilityType::Clash => self.clash_recharge,
            AbilityType::Sidestep => self.sidestep_recharge,
            AbilityType::Yank => self.yank_recharge,
            AbilityType::Backstab => self.backstab_recharge,
            AbilityType::Pounce => self.pounce_recharge,
            AbilityType::Kick => self.kick_recharge,
            AbilityType::Defy => self.defy_recharge,
            AbilityType::Bloodlust => self.bloodlust_recharge,
            AbilityType::Brace => self.brace_recharge,
            AbilityType::Surge => self.surge_recharge,
            AbilityType::Vigil => self.vigil_recharge,
            AbilityType::Lurk => self.lurk_recharge,
            AbilityType::Drag => self.drag_recharge,
        }
    }

    /// The share of base potency `ability` strikes for before anything of
    /// its own is added: none for one that strikes nothing of its own
    pub fn damage(&self, ability: AbilityType) -> f32 {
        match ability {
            AbilityType::Frenzy => self.frenzy_damage,
            AbilityType::Feint => self.feint_damage,
            AbilityType::Overpower => self.overpower_damage,
            AbilityType::Punish => self.punish_base,
            AbilityType::Leap => self.leap_strike,
            AbilityType::Backstab => self.backstab_damage,
            AbilityType::Kick => self.kick_damage,
            AbilityType::Defy => self.defy_damage,
            AbilityType::AutoAttack | AbilityType::Parry | AbilityType::Counter | AbilityType::PerfectStride
            | AbilityType::Clash | AbilityType::Sidestep | AbilityType::Yank | AbilityType::Pounce
            | AbilityType::Bloodlust | AbilityType::Brace | AbilityType::Surge | AbilityType::Vigil | AbilityType::Lurk
            | AbilityType::Drag => 0.0,
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
            AbilityType::Clash => self.clash_line,
            AbilityType::Sidestep => self.sidestep_line,
            AbilityType::Yank => self.yank_line,
            AbilityType::Backstab => self.backstab_line,
            AbilityType::Pounce => self.pounce_line,
            AbilityType::Kick => self.kick_line,
            AbilityType::Defy => self.defy_line,
            AbilityType::Bloodlust => self.bloodlust_line,
            AbilityType::Brace => self.brace_line,
            AbilityType::Surge => self.surge_line,
            AbilityType::Vigil => self.vigil_line,
            AbilityType::Lurk => self.lurk_line,
            AbilityType::Drag => self.drag_line,
            AbilityType::AutoAttack | AbilityType::Feint | AbilityType::Parry => 1.0,
        }
    }

    /// Sets the knob `name` from text, as the arena's command line gives it:
    /// any field by its own name; a count is rounded to at least one. Errs
    /// on an unknown knob or a value that does not parse.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value}"))?;
        if let Ok(knob) = self.path_mut::<f32>(name) {
            *knob = number;
        } else if let Ok(count) = self.path_mut::<usize>(name) {
            *count = number.round().max(1.0) as usize;
        } else {
            return Err(format!("no tuning knob {name}"));
        }
        Ok(())
    }

    /// The knob `name`'s value, as `set` names it. Errs on an unknown knob.
    pub fn get(&self, name: &str) -> Result<f32, String> {
        if let Ok(knob) = self.path::<f32>(name) {
            Ok(*knob)
        } else if let Ok(count) = self.path::<usize>(name) {
            Ok(*count as f32)
        } else {
            Err(format!("no tuning knob {name}"))
        }
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
        assert_eq!(tuning.frenzy_damage, 0.25);
        assert_eq!(tuning.leap_distance, 2);
        assert_eq!(tuning.get("frenzy_damage"), Ok(0.25));
        assert_eq!(tuning.get("leap_distance"), Ok(2.0));
        assert!(tuning.get("no_such_knob").is_err());
        assert!(tuning.set("frenzy_damage", "much").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
        tuning.set("leap_distance", "0.2").unwrap();
        assert_eq!(tuning.leap_distance, 1, "a count is at least one");
    }

    #[test]
    fn every_skill_recharges_and_costs_and_the_auto_attack_does_neither() {
        let tuning = Tuning::DEFAULT;
        use AbilityType::*;
        assert_eq!((tuning.recharge(AutoAttack), tuning.cost(AutoAttack), tuning.activation(AutoAttack)), (0.0, 0.0, 0.0), "an auto-attack runs on its own clock, free");
        for ability in AbilityType::ALL.into_iter().filter(|&ability| ability != AutoAttack) {
            assert!(tuning.recharge(ability) > 0.0, "{ability:?} recharges");
            assert!(tuning.cost(ability) > 0.0, "{ability:?} costs endurance");
            assert!(tuning.activation(ability) >= 0.0);
        }
    }

    #[test]
    fn a_stance_takes_hold_at_once() {
        let tuning = Tuning::DEFAULT;
        for ability in AbilityType::ALL.into_iter().filter(|ability| ability.kind() == crate::message::SkillKind::Stance) {
            assert_eq!(tuning.activation(ability), 0.0, "{ability:?}");
        }
    }
}
