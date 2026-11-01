//! Every number combat is balanced by, in one set: the early kit's
//! skills, and how each attribute's stats scale in its three modes —
//! absolute (what a build holds, the same at every level), commitment (a
//! tier's effect) and contest (a share won by a relative advantage). Level
//! counts where a blow lands (`level_gap`) and in contests.
//!
//! Each world holds one set as a resource, [`Tuning::DEFAULT`] on the live
//! client and server, so they agree on every number; the balance arena
//! gives each fight its own, to try values without a rebuild. Whatever
//! reads a number takes the set it plays by.

use bevy::{prelude::Resource, reflect::{GetPath, Reflect}};

use crate::message::AbilityType;

#[derive(Clone, Copy, Debug, Reflect, Resource)]
pub struct Tuning {
    // --- Absolute: potency, health, and level where a blow lands ---
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
    /// Share of its endurance an actor regains each second, in combat or out
    pub endurance_regen: f32,
    /// Share longer an actor's recoveries run with its endurance spent
    pub fatigue_recovery: f32,
    /// Share shorter the windows of threats against an actor run with its
    /// endurance spent
    pub fatigue_window: f32,
    /// The power fatigue rises by as endurance is spent: above 1 it stays
    /// light while the pool holds and bites as it empties
    pub fatigue_bend: f32,

    // --- Commitment: each a ladder (`CommitmentTier::at`), a value for each
    // rung of its core (T1–T2), its facet (T3–T5) and its capstone (T6–T8) ---
    /// Share less of the time it skipped an early combo (Ferocity) or an
    /// early reaction (Preparation) owes its chain, by the commitment's facet;
    /// fired early with no facet, it owes all of it
    pub early_facet: [f32; 3],
    /// Share less the third early combo Ferocity's capstone allows owes
    pub ferocity_third: [f32; 3],
    /// Seconds wide the band a reaction takes from is with no Awareness
    pub awareness_band: f32,
    /// The band's seconds by Awareness's core
    pub awareness_core: [f32; 2],
    /// Share of a reaction's price each threat it takes past the first pays
    /// back, by Awareness's facet
    pub awareness_refund: [f32; 3],
    /// Seconds after a press its nearest incoming threat may land and draw
    /// the band to start at it, by Awareness's capstone
    pub awareness_snap: [f32; 3],
    /// Share harder a strike lands from past its target's forward faces, a
    /// flank, by Grace's core
    pub grace_flank: [f32; 2],
    /// The half-angle either side of its heading an actor strikes within, in
    /// degrees, with no Grace: the three forward faces
    pub grace_arc: f32,
    /// The arc's half-angle by Grace's facet
    pub grace_arc_facet: [f32; 3],
    /// Share of its speed a flank strike leaves its target for a swing, its
    /// stride broken, by Grace's capstone
    pub grace_stride: [f32; 3],
    /// Share of its speed a foe in an Intimidating actor's zone keeps, by
    /// Intimidation's core
    pub intimidation_pace: [f32; 2],
    /// Share more each skill costs a foe in the zone, by Intimidation's facet
    pub intimidation_toll: [f32; 3],
    /// Tiles past its reach the zone reaches, by Intimidation's capstone,
    /// which also pins a foe in it
    pub intimidation_zone: [i32; 3],
    /// Seconds Intimidation's slow, toll and pin linger on a foe that leaves
    /// the zone
    pub intimidation_aura_secs: f32,
    /// Tiles an early reaction carries its user, by Preparation's capstone
    pub preparation_slip: [usize; 3],
    /// Share likelier each stack of Overcommitted on a foe makes a skill of
    /// an actor with Patience crit it, by Patience's core
    pub patience_crit: [f32; 2],
    /// Share harder a patient actor's crit lands on an overcommitted foe,
    /// by Patience's facet
    pub patience_power: [f32; 3],
    /// Stacks of Overcommitted on a foe at which a patient actor's next skill
    /// on it crits for certain and spends them, by Patience's capstone
    pub patience_opening: [usize; 3],
    /// Seconds each stack of Overcommitted lasts, each on its own
    pub overcommit_secs: f32,

    // --- Contest: what a relative advantage wins ---
    /// Advantage in points that wins half of an effect's ceiling; every
    /// contest rises toward its ceiling and never reaches it
    pub contest_scale: f32,
    /// Contest points each level of gap is worth to the higher level
    pub contest_per_level: f32,
    /// Seconds between auto-attacks at the one pace every actor starts from
    pub base_interval: f32,
    /// Most faster a Tempo advantage brings auto-attacks
    pub tempo_ceiling: f32,
    /// Most of a recovery an Impact advantage pushes it back by
    pub pushback_share: f32,
    /// Most of a recovery a Fitness advantage takes off it; below 1, so
    /// no recovery ever runs out at once
    pub fitness_share: f32,
    /// Share of a recovery its combo unlocks through, at parity
    pub combo_floor: f32,
    /// Most more an Efficiency advantage unlocks it through
    pub combo_share: f32,
    /// Seconds every threat's window starts from
    pub reaction_window: f32,
    /// Most more a Reflex advantage widens it by
    pub window_bonus: f32,

    // --- Every swing and blow ---
    /// Share of base potency an auto-attack strikes for without Force
    pub auto_damage: f32,
    /// Share more an auto-attack strikes for from a Might and Agility pair
    /// holding the whole build, evenly less for less
    pub force_auto: f32,
    /// Share of its speed an actor keeps for a base interval after
    /// a strike across its line breaks its stride
    pub stride_pace: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
    /// Most often a blow crits, by its striker's Focus over its target's
    /// Fitness, approached and never reached
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
    /// Share of base potency a Punish strikes for on a target with no
    /// Overcommitted, whatever the build
    pub punish_base: f32,
    /// Share of base potency each stack of Overcommitted on its target adds
    /// to a Punish, at full commitment to its Instinct line
    pub punish_per_stack: f32,
    /// Share of Intuition a Feint strikes for
    pub feint_damage: f32,
    pub parry_cost: f32,
    pub parry_recovery: f32,
    pub counter_cost: f32,
    pub counter_recovery: f32,
    /// Share of each countered threat's damage sent back by a counterer
    /// whose Resolve line holds as much as its level allows; less by its
    /// line (`ActorAttributes::line_power`), `counter_line` of it with none
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
    pub clash_cost: f32,
    pub clash_recovery: f32,
    /// Seconds a Clash's return stuns each source, at full commitment to
    /// its Might line
    pub clash_stun: f32,
    pub sidestep_cost: f32,
    pub sidestep_recovery: f32,
    /// Tiles round its source a Sidestep steps, at full commitment to its
    /// Agility line
    pub sidestep_steps: f32,
    pub backstab_cost: f32,
    pub backstab_recovery: f32,
    /// Share of base potency a Backstab strikes for, at full commitment to
    /// its Agility line
    pub backstab_damage: f32,
    /// How much harder a Backstab's crit lands than another's: 0.5 lands it
    /// half again as hard
    pub backstab_crit: f32,
    pub pounce_cost: f32,
    pub pounce_recovery: f32,
    /// Tiles a Pounce lunges toward its source, at full commitment to its
    /// Instinct line
    pub pounce_tiles: f32,
    pub kick_cost: f32,
    pub kick_recovery: f32,
    /// Share of base potency a Kick strikes for, whatever the build
    pub kick_damage: f32,
    /// Tiles a Kick knocks its target back, at full commitment to its
    /// Discipline line
    pub kick_tiles: f32,
    pub defy_cost: f32,
    pub defy_recovery: f32,
    /// Share of base potency a Defy strikes for with nothing in its
    /// user's queue, whatever the build
    pub defy_damage: f32,
    /// Share more a Defy strikes for each threat waiting in its user's
    /// queue, at full commitment to its Resolve line
    pub defy_per_threat: f32,
    pub bloodlust_cost: f32,
    pub bloodlust_recovery: f32,
    /// Seconds a Bloodlust lasts
    pub bloodlust_secs: f32,
    /// Share of the damage its attacks land a Bloodlust returns as health,
    /// at full commitment to its Might line
    pub bloodlust_share: f32,
    pub brace_cost: f32,
    pub brace_recovery: f32,
    /// Seconds a Brace lasts, at full commitment to its Physique line
    pub brace_secs: f32,
    pub surge_cost: f32,
    pub surge_recovery: f32,
    /// Seconds a Surge lasts unspent
    pub surge_secs: f32,
    /// Share more a Surge adds to its user's next skill strike for each
    /// tile it moved, at full commitment to its Discipline line
    pub surge_per_tile: f32,
    pub vigil_cost: f32,
    pub vigil_recovery: f32,
    /// Seconds a Vigil lasts, at full commitment to its Resolve line
    pub vigil_secs: f32,
    pub lurk_cost: f32,
    pub lurk_recovery: f32,
    /// Seconds a Lurk lasts unbroken, at full commitment to its Instinct
    /// line
    pub lurk_secs: f32,
    pub yank_cost: f32,
    pub yank_recovery: f32,
    /// Tiles a Yank's return drags each source, at full commitment to its
    /// Physique line
    pub yank_tiles: f32,

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
        endurance_regen: 0.01,
        fatigue_recovery: 1.0,
        fatigue_window: 0.319,
        fatigue_bend: 2.0,
        base_interval: 2.1,
        tempo_ceiling: 0.5,
        early_facet: [0.3, 0.4, 0.5],
        ferocity_third: [0.6, 0.7, 0.8],
        awareness_band: 0.3,
        awareness_core: [0.5, 0.9],
        awareness_refund: [0.1, 0.2, 0.3],
        awareness_snap: [0.2, 0.4, 0.6],
        grace_flank: [0.15, 0.3],
        grace_arc: 60.0,
        grace_arc_facet: [90.0, 120.0, 150.0],
        grace_stride: [0.7, 0.6, 0.5],
        intimidation_pace: [0.85, 0.7],
        intimidation_toll: [0.1, 0.2, 0.3],
        intimidation_zone: [0, 1, 2],
        intimidation_aura_secs: 1.0,
        preparation_slip: [1, 2, 3],
        patience_crit: [0.05, 0.08],
        patience_power: [0.1, 0.2, 0.3],
        patience_opening: [10, 8, 6],
        overcommit_secs: 5.0,
        contest_scale: 400.0,
        contest_per_level: 7.5,
        pushback_share: 0.5,
        fitness_share: 0.231,
        combo_floor: 0.5,
        combo_share: 0.4,
        reaction_window: 3.0,
        window_bonus: 1.0,
        auto_damage: 1.029,
        force_auto: 0.42,
        stride_pace: 0.7,
        damage_spread: 0.05,
        crit_chance: 0.35,
        crit_power: 1.5,
        frenzy_cost: 4.0,
        frenzy_recovery: 3.0,
        frenzy_damage: 1.5,
        feint_cost: 2.0,
        feint_recovery: 1.0,
        feint_damage: 0.15,
        overpower_cost: 6.0,
        overpower_recovery: 2.0,
        overpower_damage: 1.757,
        punish_cost: 2.0,
        punish_recovery: 2.0,
        punish_base: 0.4,
        punish_per_stack: 0.21,
        parry_cost: 10.0,
        parry_recovery: 1.0,
        counter_cost: 15.0,
        counter_recovery: 2.0,
        counter_reflect: 0.5,
        leap_cost: 10.0,
        leap_recovery: 3.0,
        leap_distance: 12,
        leap_strike: 1.469,
        stride_cost: 10.0,
        stride_recovery: 1.0,
        frenzy_line: 0.2,
        overpower_line: 0.2,
        punish_line: 0.2,
        counter_line: 0.2,
        stride_line: 0.2,
        leap_line: 0.2,
        stride_secs: 5.849,
        stride_speed: 0.401,
        clash_cost: 15.0,
        clash_recovery: 2.0,
        clash_stun: 0.75,
        sidestep_cost: 12.0,
        sidestep_recovery: 1.5,
        sidestep_steps: 2.0,
        backstab_cost: 6.0,
        backstab_recovery: 2.0,
        backstab_damage: 1.5,
        backstab_crit: 0.5,
        pounce_cost: 8.0,
        pounce_recovery: 1.0,
        pounce_tiles: 3.0,
        kick_cost: 4.0,
        kick_recovery: 1.5,
        kick_damage: 0.8,
        kick_tiles: 3.0,
        defy_cost: 6.0,
        defy_recovery: 2.0,
        defy_damage: 0.8,
        defy_per_threat: 0.25,
        bloodlust_cost: 10.0,
        bloodlust_recovery: 1.0,
        bloodlust_secs: 6.0,
        bloodlust_share: 0.12,
        brace_cost: 10.0,
        brace_recovery: 1.0,
        brace_secs: 3.0,
        surge_cost: 6.0,
        surge_recovery: 1.0,
        surge_secs: 3.0,
        surge_per_tile: 0.15,
        vigil_cost: 10.0,
        vigil_recovery: 1.0,
        vigil_secs: 4.0,
        lurk_cost: 10.0,
        lurk_recovery: 1.0,
        lurk_secs: 6.0,
        yank_cost: 12.0,
        yank_recovery: 1.5,
        yank_tiles: 3.0,
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
    };

    /// The endurance `ability` costs, in points of the pool
    /// (`endurance_pool`); an auto-attack is free.
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
        }
    }

    /// The share of the potency it reads `ability` strikes for: none for
    /// one that strikes nothing of its own
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
            | AbilityType::Bloodlust | AbilityType::Brace | AbilityType::Surge | AbilityType::Vigil | AbilityType::Lurk => 0.0,
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
            AbilityType::AutoAttack | AbilityType::Feint | AbilityType::Parry => 1.0,
        }
    }

    /// How much harder a crit of `ability`'s lands than another's: a
    /// Backstab's `backstab_crit`, none for any other
    pub fn crit_severity(&self, ability: Option<AbilityType>) -> f32 {
        match ability {
            Some(AbilityType::Backstab) => self.backstab_crit,
            _ => 0.0,
        }
    }

    /// Seconds of recovery `ability` leaves its user in; an auto-attack
    /// runs on its own timer instead. Authored, never searched, and never
    /// longer than `reaction_window`: endurance is a fight's throttle.
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
            AbilityType::Clash => self.clash_recovery,
            AbilityType::Sidestep => self.sidestep_recovery,
            AbilityType::Yank => self.yank_recovery,
            AbilityType::Backstab => self.backstab_recovery,
            AbilityType::Pounce => self.pounce_recovery,
            AbilityType::Kick => self.kick_recovery,
            AbilityType::Defy => self.defy_recovery,
            AbilityType::Bloodlust => self.bloodlust_recovery,
            AbilityType::Brace => self.brace_recovery,
            AbilityType::Surge => self.surge_recovery,
            AbilityType::Vigil => self.vigil_recovery,
            AbilityType::Lurk => self.lurk_recovery,
        }
    }

    /// Sets the knob `name` from text, as the arena's command line gives it:
    /// any field by its own name, an element of a triple as `name[i]`; a
    /// count is rounded to at least one. Errs on an unknown knob or a value
    /// that does not parse.
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
        tuning.set("awareness_band", "0.25").unwrap();
        assert_eq!(tuning.frenzy_damage, 0.25);
        assert_eq!(tuning.leap_distance, 2);
        assert_eq!(tuning.awareness_band, 0.25);
        assert!(tuning.set("frenzy_damage", "much").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
        tuning.set("leap_distance", "0.2").unwrap();
        assert_eq!(tuning.leap_distance, 1, "a count is at least one");
        tuning.set("early_facet[1]", "0.5").unwrap();
        assert_eq!(tuning.early_facet[1], 0.5);
    }

    #[test]
    fn no_skill_recovers_longer_than_a_reaction_window() {
        let tuning = Tuning::DEFAULT;
        for ability in AbilityType::ALL.into_iter().filter(|&ability| ability != AbilityType::AutoAttack) {
            assert!(tuning.recovery(ability) <= tuning.reaction_window, "{ability:?}");
        }
    }

    #[test]
    fn every_ability_but_the_auto_attack_leaves_its_user_recovering() {
        let tuning = Tuning::DEFAULT;
        use AbilityType::*;
        assert_eq!(tuning.recovery(AutoAttack), 0.0, "an auto-attack runs on its own timer");
        for ability in AbilityType::ALL.into_iter().filter(|&ability| ability != AutoAttack) {
            assert!(tuning.recovery(ability) > 0.0, "{ability:?} leaves its user recovering");
        }
    }
}
