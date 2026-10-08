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
    Approach::Direct, Approach::Oblique, Approach::Opportunistic, Approach::Vigilant,
    Approach::Binding, Approach::Fluid, Approach::Overwhelming,
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
    ("ambusher.behind.floor", 0.834), ("ambusher.behind.to", 0.872), ("ambusher.combo", 0.188),
    ("ambusher.detour.to", 0.739), ("ambusher.effect_added.from", 0.29), ("ambusher.exposure.floor", 0.522),
    ("ambusher.exposure.to", 0.88), ("ambusher.foe_just_acted.floor", 0.831), ("ambusher.foe_just_acted.from", 3.171),
    ("ambusher.hold", 0.403), ("ambusher.just_acted.binding", 2.074), ("ambusher.just_acted.direct", 0.848),
    ("ambusher.just_acted.fluid", 1.49), ("ambusher.just_acted.oblique", 1.625), ("ambusher.just_acted.vigilant", 1.469),
    ("ambusher.leash_left.to", 0.458), ("ambusher.momentum", 0.041), ("ambusher.recovery_left.floor", 0.894),
    ("ambusher.recovery_left.to", 7.319),
    ("ambusher.wait", 0.414), ("ambusher.worth_answering.to", 0.709), ("berserker.behind.floor", 0.781),
    ("berserker.behind.to", 0.403), ("berserker.combo", 0.061), ("berserker.detour.to", 0.312),
    ("berserker.effect_added.from", 0.702), ("berserker.exposure.floor", 0.636), ("berserker.exposure.to", 0.691),
    ("berserker.foe_just_acted.floor", 0.248), ("berserker.foe_just_acted.from", 3.047), ("berserker.hold", 0.333),
    ("berserker.just_acted.binding", 1.671), ("berserker.just_acted.fluid", 1.882), ("berserker.just_acted.oblique", 1.872),
    ("berserker.just_acted.opportunistic", 2.507), ("berserker.just_acted.vigilant", 1.675), ("berserker.leash_left.to", 0.165),
    ("berserker.momentum", 0.039), ("berserker.recovery_left.floor", 0.897), ("berserker.recovery_left.to", 10.191),
    ("berserker.wait", 0.351),
    ("berserker.worth_answering.to", 0.724), ("defender.behind.floor", 0.882), ("defender.behind.to", 0.282),
    ("defender.combo", 0.046), ("defender.detour.to", 0.53), ("defender.effect_added.from", 0.592),
    ("defender.exposure.floor", 0.391), ("defender.exposure.to", 0.478), ("defender.foe_just_acted.floor", 0.421),
    ("defender.foe_just_acted.from", 1.484), ("defender.hold", 0.268), ("defender.just_acted.binding", 2.146),
    ("defender.just_acted.direct", 1.281), ("defender.just_acted.fluid", 1.229), ("defender.just_acted.oblique", 1.537),
    ("defender.just_acted.opportunistic", 0.96), ("defender.leash_left.to", 0.386), ("defender.momentum", 0.233),
    ("defender.recovery_left.floor", 0.919), ("defender.recovery_left.to", 5.324),
    ("defender.wait", 0.323), ("defender.worth_answering.to", 0.231),
    ("flanker.behind.floor", 0.052), ("flanker.behind.to", 0.98), ("flanker.combo", 0.228),
    ("flanker.detour.to", 1.914), ("flanker.effect_added.from", 0.779), ("flanker.exposure.floor", 0.718),
    ("flanker.exposure.to", 0.833), ("flanker.foe_just_acted.floor", 0.804), ("flanker.foe_just_acted.from", 3.303),
    ("flanker.hold", 0.116), ("flanker.just_acted.binding", 0.89), ("flanker.just_acted.direct", 1.897),
    ("flanker.just_acted.fluid", 0.711), ("flanker.just_acted.opportunistic", 1.131), ("flanker.just_acted.vigilant", 1.173),
    ("flanker.leash_left.to", 0.153), ("flanker.momentum", 0.1), ("flanker.recovery_left.floor", 0.594),
    ("flanker.recovery_left.to", 11.409),
    ("flanker.wait", 0.63), ("flanker.worth_answering.to", 0.05), ("juggernaut.behind.floor", 0.815),
    ("juggernaut.behind.to", 0.601), ("juggernaut.combo", 0.133), ("juggernaut.detour.to", 0.423),
    ("juggernaut.effect_added.from", 0.253), ("juggernaut.exposure.floor", 0.782), ("juggernaut.exposure.to", 0.326),
    ("juggernaut.foe_just_acted.floor", 0.244), ("juggernaut.foe_just_acted.from", 3.257), ("juggernaut.hold", 0.141),
    ("juggernaut.just_acted.direct", 1.126), ("juggernaut.just_acted.fluid", 2.865), ("juggernaut.just_acted.oblique", 2.21),
    ("juggernaut.just_acted.opportunistic", 1.583), ("juggernaut.just_acted.vigilant", 1.18), ("juggernaut.leash_left.to", 0.152),
    ("juggernaut.momentum", 0.149), ("juggernaut.recovery_left.floor", 0.873), ("juggernaut.recovery_left.to", 4.362),
    ("juggernaut.wait", 0.359),
    ("juggernaut.worth_answering.to", 0.792), ("skirmisher.behind.floor", 0.75), ("skirmisher.behind.to", 0.509),
    ("skirmisher.combo", 0.208), ("skirmisher.detour.to", 0.582), ("skirmisher.effect_added.from", 0.464),
    ("skirmisher.exposure.floor", 0.747), ("skirmisher.exposure.to", 0.627), ("skirmisher.foe_just_acted.floor", 0.89),
    ("skirmisher.foe_just_acted.from", 2.402), ("skirmisher.hold", 0.175), ("skirmisher.just_acted.binding", 1.89),
    ("skirmisher.just_acted.direct", 1.458), ("skirmisher.just_acted.oblique", 1.985), ("skirmisher.just_acted.opportunistic", 0.83),
    ("skirmisher.just_acted.vigilant", 2.079), ("skirmisher.leash_left.to", 0.289), ("skirmisher.momentum", 0.246),
    ("skirmisher.recovery_left.floor", 0.685), ("skirmisher.recovery_left.to", 10.019),
    ("skirmisher.wait", 0.288), ("skirmisher.worth_answering.to", 0.158),
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
        minds.set("defender.exposure.floor", "0.3").unwrap();
        let defender = minds.mind(Some(EnemyArchetype::Defender));
        let flanker = minds.mind(Some(EnemyArchetype::Flanker));
        assert_eq!((defender.wait, flanker.wait), (0.5, 0.5));
        assert_eq!(defender.adjusts["worth_answering"].to, Some(0.2));
        assert_eq!(flanker.adjusts["worth_answering"].to, Some(0.4));
        assert_eq!(defender.adjusts["exposure"].floor, Some(0.3));
        assert!(!flanker.adjusts.contains_key("exposure"));
    }

    #[test]
    fn every_tuned_setting_is_one_a_mind_knows() {
        let tuned = Minds::tuned();
        assert!(tuned.mind(Some(EnemyArchetype::Defender)).wait != Mind::default().wait);
    }

    #[test]
    fn a_foes_approach_weighs_how_long_its_last_skill_counts() {
        let mut minds = Minds::default();
        minds.set("berserker.just_acted.opportunistic", "0.4").unwrap();
        let berserker = minds.mind(Some(EnemyArchetype::Berserker));
        assert_eq!(berserker.just_acted(Some(Approach::Opportunistic)), 0.4);
        assert_eq!(berserker.just_acted(Some(Approach::Vigilant)), 1.0, "an Approach not set counts as it is");
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
        assert!(minds.set("all.strike_worth.to", "0.5").is_err(), "nor what a skill is worth");
        assert!(minds.set("all.endurance_left.floor", "0.5").is_err(), "nor what it costs");
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
