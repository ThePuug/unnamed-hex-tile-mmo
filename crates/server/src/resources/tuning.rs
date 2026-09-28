//! The numbers archetype signatures and their pacing are balanced by. They
//! live in one resource so the balance arena can try values from its command
//! line without a rebuild; the live server runs the defaults.

use bevy::prelude::*;
use common_bevy::{message::AbilityType, spatial_difficulty::EnemyArchetype};

#[derive(Clone, Debug, Resource)]
pub struct ArchetypeTuning {
    /// Milliseconds each archetype waits before a signature use, drawn from
    /// `min..=max` once the ability is affordable
    pub berserker_delay: (u64, u64),
    pub juggernaut_delay: (u64, u64),
    pub kiter_delay: (u64, u64),
    pub defender_delay: (u64, u64),
    pub skirmisher_delay: (u64, u64),
    pub cutthroat_delay: (u64, u64),
    /// Share of the target's Toughness mitigation a Lunge strikes past
    pub lunge_pierce: f32,
    /// Share of an attack's damage its roll lands either side of it
    pub damage_spread: f32,
    /// Contest points each level of gap is worth to the higher level in every relative contest
    pub contest_per_level: f32,
    /// An NPC's health before Vitality, the game's `BASE_HEALTH` unless tried otherwise
    pub base_health: f32,
    /// An NPC's health per point of Vitality, the game's `HEALTH_PER_VITALITY` unless tried otherwise
    pub health_per_vitality: f32,
    /// Share of base potency an auto-attack strikes for, the same for every actor
    pub auto_damage: f32,
    /// Share of Force a Lunge's strike deals
    pub lunge_force: f32,
    /// Share of Force each tick of a Lunge's DoT deals
    pub lunge_dot: f32,
    /// Share of the Juggernaut's own health a Rattle strikes for
    pub rattle_health: f32,
    /// Share more a Rattle strikes for with each daze stack already on its target
    pub rattle_growth: f32,
    /// Share of its pace each Rattle stack takes from the target
    pub rattle_daze: f32,
    /// Most stacks a target carries
    pub rattle_stacks: u8,
    /// Tiles a Disengage leaps
    pub disengage_leap: usize,
    /// Share of Technique a Disengage adds to its caster's next auto-attack
    pub disengage_technique: f32,
    /// Share of Force each Volley shot strikes for
    pub volley_force: f32,
    /// Share of its speed a Volley takes from its target
    pub volley_slow: f32,
    /// Seconds a Volley's slow lasts
    pub volley_slow_secs: f32,
    /// Pace a Volley sets its Kiter running at, a share of its speed past whole
    pub volley_run: f32,
    /// Seconds a Volley's run lasts
    pub volley_run_secs: f32,
    /// Seconds a Flank stuns its target
    pub flank_stun: f32,
    /// Share of Intuition a Flank strikes for
    pub flank_intuition: f32,
    /// Share of each countered threat's damage sent back, times the counterer's Gravitas over base potency
    pub counter_reflect: f32,
}

impl Default for ArchetypeTuning {
    fn default() -> Self {
        Self {
            berserker_delay: (4000, 6000),
            juggernaut_delay: (5000, 7000),
            kiter_delay: (2000, 4000),
            defender_delay: (1000, 3000),
            skirmisher_delay: (5000, 7000),
            cutthroat_delay: (3000, 5000),
            lunge_pierce: 0.49,
            damage_spread: 0.2,
            contest_per_level: common_bevy::systems::combat::damage::CONTEST_PER_LEVEL,
            base_health: common_bevy::components::BASE_HEALTH,
            health_per_vitality: common_bevy::components::HEALTH_PER_VITALITY,
            auto_damage: 1.05,
            lunge_force: 0.7,
            lunge_dot: 0.21,
            rattle_health: 0.035,
            rattle_growth: 0.343,
            rattle_daze: 0.049,
            rattle_stacks: 3,
            disengage_leap: 3,
            disengage_technique: 0.49,
            volley_force: 0.84,
            volley_slow: 0.069,
            volley_slow_secs: 2.058,
            volley_run: 3.0,
            volley_run_secs: 2.5,
            flank_stun: 3.0,
            flank_intuition: 1.0,
            counter_reflect: 0.331,
        }
    }
}

impl ArchetypeTuning {
    /// The range an NPC of `archetype` draws its signature delay from, in milliseconds
    pub fn delay(&self, archetype: EnemyArchetype) -> (u64, u64) {
        match archetype {
            EnemyArchetype::Berserker => self.berserker_delay,
            EnemyArchetype::Juggernaut => self.juggernaut_delay,
            EnemyArchetype::Kiter => self.kiter_delay,
            EnemyArchetype::Defender => self.defender_delay,
            EnemyArchetype::Skirmisher => self.skirmisher_delay,
            EnemyArchetype::Cutthroat => self.cutthroat_delay,
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

    /// Sets the knob `name` from text, as the arena's command line gives it:
    /// a delay as `min-max`, anything else as a number. Errs on an unknown
    /// knob or a value that does not parse.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let number = || value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value}"));
        let range = || {
            let (min, max) = value.split_once('-').ok_or(format!("{name} takes min-max milliseconds, not {value}"))?;
            let parse = |part: &str| part.parse::<u64>().map_err(|_| format!("{name} takes min-max milliseconds, not {value}"));
            Ok::<_, String>((parse(min)?, parse(max)?))
        };
        match name {
            "b_delay" => self.berserker_delay = range()?,
            "j_delay" => self.juggernaut_delay = range()?,
            "k_delay" => self.kiter_delay = range()?,
            "d_delay" => self.defender_delay = range()?,
            "s_delay" => self.skirmisher_delay = range()?,
            "c_delay" => self.cutthroat_delay = range()?,
            "lunge_pierce" => self.lunge_pierce = number()?,
            "lunge_dot" => self.lunge_dot = number()?,
            "damage_spread" => self.damage_spread = number()?,
            "contest_per_level" => self.contest_per_level = number()?,
            "base_health" => self.base_health = number()?,
            "health_per_vitality" => self.health_per_vitality = number()?,
            "auto_damage" => self.auto_damage = number()?,
            "lunge_force" => self.lunge_force = number()?,
            "rattle_health" => self.rattle_health = number()?,
            "rattle_growth" => self.rattle_growth = number()?,
            "rattle_daze" => self.rattle_daze = number()?,
            "rattle_stacks" => self.rattle_stacks = number()? as u8,
            "disengage_leap" => self.disengage_leap = number()? as usize,
            "disengage_technique" => self.disengage_technique = number()?,
            "volley_force" => self.volley_force = number()?,
            "volley_slow" => self.volley_slow = number()?,
            "volley_slow_secs" => self.volley_slow_secs = number()?,
            "volley_run" => self.volley_run = number()?,
            "volley_run_secs" => self.volley_run_secs = number()?,
            "flank_stun" => self.flank_stun = number()?,
            "flank_intuition" => self.flank_intuition = number()?,
            "counter_reflect" => self.counter_reflect = number()?,
            _ => return Err(format!("no tuning knob {name}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_run_forwards() {
        let tuning = ArchetypeTuning::default();
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender, EnemyArchetype::Skirmisher, EnemyArchetype::Cutthroat] {
            let (min, max) = tuning.delay(archetype);
            assert!(min <= max, "{archetype:?} delay range runs backwards");
        }
    }

    #[test]
    fn set_reads_ranges_and_numbers_and_refuses_the_unknown() {
        let mut tuning = ArchetypeTuning::default();
        tuning.set("b_delay", "100-200").unwrap();
        tuning.set("lunge_pierce", "0.25").unwrap();
        assert_eq!(tuning.berserker_delay, (100, 200));
        assert_eq!(tuning.lunge_pierce, 0.25);
        assert!(tuning.set("b_delay", "100").is_err());
        assert!(tuning.set("no_such_knob", "1").is_err());
    }
}
