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
    /// Share of the target's Toughness mitigation a Lunge strikes past
    pub lunge_pierce: f32,
    /// Share of Force a Charge strikes for
    pub charge_force: f32,
    /// Seconds a Charge holds its target in place
    pub charge_stagger: f32,
    /// Tiles a Disengage leaps
    pub disengage_leap: usize,
    /// Share of Technique each countered threat strikes its source for
    pub counter_technique: f32,
    /// Share of each countered threat's damage sent back
    pub counter_reflect: f32,
}

impl Default for ArchetypeTuning {
    fn default() -> Self {
        Self {
            berserker_delay: (7000, 11000),
            juggernaut_delay: (3000, 5000),
            kiter_delay: (500, 2500),
            defender_delay: (2000, 4000),
            lunge_pierce: 0.25,
            charge_force: 0.6,
            charge_stagger: 1.0,
            disengage_leap: 3,
            counter_technique: 0.5,
            counter_reflect: 0.2,
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
            "lunge_pierce" => self.lunge_pierce = number()?,
            "charge_force" => self.charge_force = number()?,
            "charge_stagger" => self.charge_stagger = number()?,
            "disengage_leap" => self.disengage_leap = number()? as usize,
            "counter_technique" => self.counter_technique = number()?,
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
        for archetype in [EnemyArchetype::Berserker, EnemyArchetype::Juggernaut, EnemyArchetype::Kiter, EnemyArchetype::Defender] {
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
