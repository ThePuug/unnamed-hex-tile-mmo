//! The numbers an NPC's decisions are shaped by, apart from the
//! considerations that read the fight: each continuous consideration's
//! bounds and floor, each channel's threshold, the movement channel's
//! momentum and the skills channel's combo bonus, set for every archetype
//! or for one. The curve's shape is the consideration's
//! own and is not set here.
//!
//! The world holds one set as a resource, [`TUNED`] (`CombatPlugin`),
//! unless the balance arena gives a fight its own, so a search can tune
//! each archetype's fighter without a rebuild. What a search finds is
//! written back into [`TUNED`].

use std::collections::HashMap;

use common_bevy::{components::entity_type::actor::Approach, archetype::EnemyArchetype};

/// Every Approach a foe may show, for naming one in a setting
const APPROACHES: [Approach; 7] = [
    Approach::Direct, Approach::Oblique, Approach::Ambushing, Approach::Patient,
    Approach::Binding, Approach::Evasive, Approach::Overwhelming,
];

use super::{moves, skills, utility::{Consideration, Curve}};

/// What a mind sets of one consideration; what it leaves unset stays the
/// consideration's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Adjust {
    pub from: Option<f32>,
    pub to: Option<f32>,
    pub floor: Option<f32>,
}

/// How one NPC's decisions are shaped: the threshold of each channel, the
/// momentum of the move under way, the bonus of the combo its recovery
/// offers, what it sets of its considerations, and how much each foe
/// Approach makes the foe's last skill count.
#[derive(Clone, Debug)]
pub struct Mind {
    pub wait: f32,
    pub hold: f32,
    pub momentum: f32,
    pub combo: f32,
    adjusts: HashMap<&'static str, Adjust>,
    just_acted: HashMap<Approach, f32>,
}

impl Default for Mind {
    fn default() -> Self {
        Self { wait: skills::WAIT, hold: moves::HOLD, momentum: moves::MOMENTUM, combo: skills::COMBO, adjusts: HashMap::new(), just_acted: HashMap::new() }
    }
}

impl Mind {
    /// How much a skill its foe just used counts, by the foe's Approach as
    /// its target frame shows it: the seconds since are divided by it, so
    /// above 1 the skill counts longer and below 1 it fades sooner. 1 for
    /// an Approach not set, or a foe that shows none.
    pub fn just_acted(&self, approach: Option<Approach>) -> f32 {
        approach.and_then(|approach| self.just_acted.get(&approach)).copied().unwrap_or(1.0).max(f32::EPSILON)
    }

    /// `consideration` as this mind shapes it
    pub fn shape<V>(&self, consideration: &Consideration<V>) -> Consideration<V> {
        let Some(adjust) = self.adjusts.get(consideration.name) else { return *consideration };
        let (from, to) = consideration.bounds;
        Consideration {
            bounds: (adjust.from.unwrap_or(from), adjust.to.unwrap_or(to)),
            curve: Curve { floor: adjust.floor.unwrap_or(consideration.curve.floor), ..consideration.curve },
            ..*consideration
        }
    }
}

/// What is set for every archetype, or for one.
#[derive(Clone, Debug, Default)]
struct Overrides {
    wait: Option<f32>,
    hold: Option<f32>,
    momentum: Option<f32>,
    combo: Option<f32>,
    adjusts: HashMap<&'static str, Adjust>,
    just_acted: HashMap<Approach, f32>,
}

/// The settings each archetype fights with, as the arena's best-response
/// search left them against the others: `<archetype>.<setting>` and value.
/// An archetype or setting not named keeps the consideration's own.
pub const TUNED: &[(&str, f32)] = &[
    ("ambusher.combo", 0.199), ("ambusher.detour.to", 1.188), ("ambusher.effect_added.from", 0.24),
    ("ambusher.foe_just_acted.floor", 0.824), ("ambusher.foe_just_acted.from", 2.46), ("ambusher.hold", 0.429),
    ("ambusher.just_acted.binding", 2.291), ("ambusher.just_acted.direct", 1.548), ("ambusher.just_acted.oblique", 1.97),
    ("ambusher.just_acted.evasive", 1.16), ("ambusher.just_acted.patient", 2.326), ("ambusher.leash_left.to", 0.158),
    ("ambusher.momentum", 0.077), ("ambusher.recovery_left.floor", 0.775), ("ambusher.recovery_left.to", 9.578),
    ("ambusher.strike_worth.floor", 0.336), ("ambusher.strike_worth.to", 0.652), ("ambusher.time_to_be_struck.to", 0.77),
    ("ambusher.wait", 0.069), ("ambusher.worth_answering.to", 0.351), ("berserker.combo", 0.147),
    ("berserker.detour.to", 0.673), ("berserker.effect_added.from", 0.435), ("berserker.foe_just_acted.floor", 0.758),
    ("berserker.foe_just_acted.from", 2.464), ("berserker.hold", 0.239), ("berserker.just_acted.ambushing", 1.671),
    ("berserker.just_acted.binding", 2.38), ("berserker.just_acted.oblique", 2.137), ("berserker.just_acted.evasive", 1.582),
    ("berserker.just_acted.patient", 1.037), ("berserker.leash_left.to", 0.259), ("berserker.momentum", 0.153),
    ("berserker.recovery_left.floor", 0.62), ("berserker.recovery_left.to", 5.799), ("berserker.strike_worth.floor", 0.495),
    ("berserker.strike_worth.to", 0.173), ("berserker.time_to_be_struck.to", 1.066), ("berserker.wait", 0.233),
    ("berserker.worth_answering.to", 0.05), ("defender.combo", 0.205), ("defender.detour.to", 0.938),
    ("defender.effect_added.from", 0.19), ("defender.foe_just_acted.floor", 0.703), ("defender.foe_just_acted.from", 3.049),
    ("defender.hold", 0.462), ("defender.just_acted.ambushing", 1.846), ("defender.just_acted.binding", 2.095),
    ("defender.just_acted.direct", 1.402), ("defender.just_acted.oblique", 1.262), ("defender.just_acted.evasive", 0.596),
    ("defender.leash_left.to", 0.129), ("defender.momentum", 0.194), ("defender.recovery_left.floor", 0.942),
    ("defender.recovery_left.to", 7.572), ("defender.strike_worth.floor", 0.56), ("defender.strike_worth.to", 0.305),
    ("defender.time_to_be_struck.to", 0.201), ("defender.wait", 0.121), ("defender.worth_answering.to", 0.221),
    ("juggernaut.combo", 0.153), ("juggernaut.detour.to", 1.335), ("juggernaut.effect_added.from", 0.673),
    ("juggernaut.foe_just_acted.floor", 0.615), ("juggernaut.foe_just_acted.from", 0.825), ("juggernaut.hold", 0.263),
    ("juggernaut.just_acted.ambushing", 2.553), ("juggernaut.just_acted.direct", 0.573), ("juggernaut.just_acted.oblique", 1.85),
    ("juggernaut.just_acted.evasive", 2.265), ("juggernaut.just_acted.patient", 0.707), ("juggernaut.leash_left.to", 0.238),
    ("juggernaut.momentum", 0.086), ("juggernaut.recovery_left.floor", 0.755), ("juggernaut.recovery_left.to", 2.71),
    ("juggernaut.strike_worth.floor", 0.394), ("juggernaut.strike_worth.to", 0.369), ("juggernaut.time_to_be_struck.to", 0.991),
    ("juggernaut.wait", 0.266), ("juggernaut.worth_answering.to", 0.431), 
    ("flanker.combo", 0.004), ("flanker.detour.to", 2.0),
    ("flanker.behind.floor", 0.376), ("flanker.behind.to", 0.927), ("flanker.effect_added.from", 0.372), ("flanker.foe_just_acted.floor", 0.602),
    ("flanker.foe_just_acted.from", 2.07), ("flanker.hold", 0.326), ("flanker.just_acted.ambushing", 1.935),
    ("flanker.just_acted.binding", 0.916), ("flanker.just_acted.direct", 2.132), ("flanker.just_acted.evasive", 2.347),
    ("flanker.just_acted.patient", 1.056), ("flanker.leash_left.to", 0.27), ("flanker.momentum", 0.069),
    ("flanker.recovery_left.floor", 0.685), ("flanker.recovery_left.to", 9.452), 
    ("flanker.strike_worth.floor", 0.671), ("flanker.strike_worth.to", 0.579),
    ("flanker.time_to_be_struck.to", 0.681), ("flanker.wait", 0.192), ("flanker.worth_answering.to", 0.05),
    ("skirmisher.combo", 0.14), ("skirmisher.detour.to", 0.833), ("skirmisher.effect_added.from", 0.33),
    ("skirmisher.foe_just_acted.floor", 0.666), ("skirmisher.foe_just_acted.from", 1.753), ("skirmisher.hold", 0.475),
    ("skirmisher.just_acted.ambushing", 1.571), ("skirmisher.just_acted.binding", 1.678), ("skirmisher.just_acted.direct", 0.654),
    ("skirmisher.just_acted.oblique", 2.284), ("skirmisher.just_acted.patient", 2.55), ("skirmisher.leash_left.to", 0.252),
    ("skirmisher.momentum", 0.258), ("skirmisher.recovery_left.floor", 0.811), ("skirmisher.recovery_left.to", 5.437),
    ("skirmisher.strike_worth.floor", 0.116), ("skirmisher.strike_worth.to", 0.613), ("skirmisher.time_to_be_struck.to", 0.66),
    ("skirmisher.wait", 0.61), ("skirmisher.worth_answering.to", 0.05),
];

/// Every mind setting: those for all archetypes, and each archetype's on
/// top. Its default sets nothing; [`Minds::tuned`] holds [`TUNED`].
#[derive(Clone, Debug, Default, bevy::prelude::Resource)]
pub struct Minds {
    all: Overrides,
    each: HashMap<EnemyArchetype, Overrides>,
}

impl Minds {
    /// The settings in [`TUNED`]
    pub fn tuned() -> Self {
        let mut minds = Self::default();
        for &(key, value) in TUNED {
            minds.set(key, &value.to_string()).unwrap_or_else(|error| panic!("TUNED: {error}"));
        }
        minds
    }

    /// The mind of an NPC of `archetype`: every archetype's settings, its
    /// own over them
    pub fn mind(&self, archetype: Option<EnemyArchetype>) -> Mind {
        let mut mind = Mind::default();
        let own = archetype.and_then(|archetype| self.each.get(&archetype));
        for overrides in std::iter::once(&self.all).chain(own) {
            mind.wait = overrides.wait.unwrap_or(mind.wait);
            mind.hold = overrides.hold.unwrap_or(mind.hold);
            mind.momentum = overrides.momentum.unwrap_or(mind.momentum);
            mind.combo = overrides.combo.unwrap_or(mind.combo);
            mind.just_acted.extend(overrides.just_acted.iter().map(|(&approach, &weight)| (approach, weight)));
            for (&name, adjust) in &overrides.adjusts {
                let set = mind.adjusts.entry(name).or_default();
                set.from = adjust.from.or(set.from);
                set.to = adjust.to.or(set.to);
                set.floor = adjust.floor.or(set.floor);
            }
        }
        mind
    }

    /// Sets `key` from text, as the arena gives it: `<who>.<setting>`,
    /// where who is `all` or an archetype and the setting `wait`, `hold`,
    /// `momentum`, `combo`, `just_acted.<approach>`, or
    /// `<consideration>.<from|to|floor>`. Errs on anything unknown.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let number = value.parse::<f32>().map_err(|_| format!("mind.{key} takes a number, not {value}"))?;
        let (who, setting) = key.split_once('.').ok_or_else(|| format!("mind.{key} names no archetype and setting"))?;
        let overrides = if who == "all" {
            &mut self.all
        } else {
            let archetype = EnemyArchetype::ALL.into_iter()
                .find(|archetype| format!("{archetype:?}").eq_ignore_ascii_case(who))
                .ok_or_else(|| format!("mind: no archetype {who}"))?;
            self.each.entry(archetype).or_default()
        };
        match setting {
            "wait" => overrides.wait = Some(number),
            "hold" => overrides.hold = Some(number),
            "momentum" => overrides.momentum = Some(number),
            "combo" => overrides.combo = Some(number),
            _ if setting.starts_with("just_acted.") => {
                let name = &setting["just_acted.".len()..];
                let approach = APPROACHES.into_iter()
                    .find(|approach| format!("{approach:?}").eq_ignore_ascii_case(name))
                    .ok_or_else(|| format!("mind: no Approach {name}"))?;
                overrides.just_acted.insert(approach, number);
            }
            _ => {
                let (name, part) = setting.rsplit_once('.').ok_or_else(|| format!("mind: no setting {setting}"))?;
                let name = skills::TUNABLE.iter().chain(moves::TUNABLE).find(|&&known| known == name)
                    .ok_or_else(|| format!("mind: no consideration {name} to tune"))?;
                let adjust = overrides.adjusts.entry(name).or_default();
                match part {
                    "from" => adjust.from = Some(number),
                    "to" => adjust.to = Some(number),
                    "floor" => adjust.floor = Some(number),
                    _ => return Err(format!("mind: a consideration sets from, to or floor, not {part}")),
                }
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archetypes_settings_lie_over_every_archetypes() {
        let mut minds = Minds::default();
        minds.set("all.wait", "0.5").unwrap();
        minds.set("all.worth_answering.to", "0.4").unwrap();
        minds.set("defender.worth_answering.to", "0.2").unwrap();
        minds.set("defender.strike_worth.floor", "0.3").unwrap();
        let defender = minds.mind(Some(EnemyArchetype::Defender));
        let flanker = minds.mind(Some(EnemyArchetype::Flanker));
        assert_eq!((defender.wait, flanker.wait), (0.5, 0.5));
        assert_eq!(defender.adjusts["worth_answering"].to, Some(0.2));
        assert_eq!(flanker.adjusts["worth_answering"].to, Some(0.4));
        assert_eq!(defender.adjusts["strike_worth"].floor, Some(0.3));
        assert!(!flanker.adjusts.contains_key("strike_worth"));
    }

    #[test]
    fn every_tuned_setting_is_one_a_mind_knows() {
        let tuned = Minds::tuned();
        assert!(tuned.mind(Some(EnemyArchetype::Defender)).wait != Mind::default().wait);
    }

    #[test]
    fn a_foes_approach_weighs_how_long_its_last_skill_counts() {
        let mut minds = Minds::default();
        minds.set("berserker.just_acted.ambushing", "0.4").unwrap();
        let berserker = minds.mind(Some(EnemyArchetype::Berserker));
        assert_eq!(berserker.just_acted(Some(Approach::Ambushing)), 0.4);
        assert_eq!(berserker.just_acted(Some(Approach::Patient)), 1.0, "an Approach not set counts as it is");
        assert_eq!(berserker.just_acted(None), 1.0);
        assert!(minds.set("berserker.just_acted.sneaky", "1").is_err());
    }

    #[test]
    fn it_refuses_what_it_does_not_know() {
        let mut minds = Minds::default();
        assert!(minds.set("all.no_such_thing.to", "1").is_err());
        assert!(minds.set("wizard.wait", "1").is_err());
        assert!(minds.set("all.worth_answering.slope", "1").is_err());
        assert!(minds.set("all.wait", "high").is_err());
        assert!(minds.set("all.usable.floor", "0.5").is_err(), "a condition is not tuned");
    }

    #[test]
    fn a_shaped_consideration_keeps_what_is_not_set() {
        let consideration: Consideration<f32> = Consideration { name: "worth_answering", read: |x| *x, bounds: (0.0, 0.3), curve: Curve::RISING.floored(0.2) };
        let mut minds = Minds::default();
        minds.set("all.worth_answering.to", "0.6").unwrap();
        let shaped = minds.mind(None).shape(&consideration);
        assert_eq!(shaped.bounds, (0.0, 0.6));
        assert_eq!(shaped.curve, Curve::RISING.floored(0.2));
    }
}
