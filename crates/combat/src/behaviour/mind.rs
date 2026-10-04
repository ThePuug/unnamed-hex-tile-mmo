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
    Approach::Direct, Approach::Distant, Approach::Ambushing, Approach::Patient,
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
    ("ambusher.foe_just_acted.floor", 0.871), ("ambusher.foe_just_acted.from", 1.656),
    ("ambusher.hold", 0.46), ("ambusher.just_acted.binding", 2.295), ("ambusher.just_acted.direct", 0.445),
    ("ambusher.just_acted.distant", 2.131), ("ambusher.just_acted.evasive", 1.732), ("ambusher.just_acted.patient", 1.623),
    ("ambusher.momentum", 0.033), 
    ("ambusher.reactions_left.floor", 0.81), 
    ("ambusher.strike_worth.floor", 0.0), ("ambusher.strike_worth.to", 0.644), ("ambusher.wait", 0.06),
    ("ambusher.worth_answering.to", 0.338), ("berserker.foe_just_acted.floor", 0.833), ("berserker.foe_just_acted.from", 1.351), ("berserker.hold", 0.158),
    ("berserker.just_acted.ambushing", 1.914), ("berserker.just_acted.binding", 2.7), ("berserker.just_acted.distant", 1.291),
    ("berserker.just_acted.evasive", 1.241), ("berserker.just_acted.patient", 0.523), ("berserker.momentum", 0.231),
    ("berserker.strike_worth.floor", 0.761),
    ("berserker.strike_worth.to", 0.688), ("berserker.wait", 0.05), ("berserker.worth_answering.to", 0.329),
    ("defender.foe_just_acted.floor", 0.301), ("defender.foe_just_acted.from", 2.597),
    ("defender.hold", 0.428), ("defender.just_acted.ambushing", 2.565), ("defender.just_acted.binding", 1.756),
    ("defender.just_acted.direct", 1.547), ("defender.just_acted.distant", 0.662), ("defender.just_acted.evasive", 0.312),
    ("defender.momentum", 0.21), 
    ("defender.strike_worth.floor", 1.0), ("defender.strike_worth.to", 0.37), ("defender.wait", 0.14),
    ("defender.worth_answering.to", 0.315), ("juggernaut.foe_just_acted.floor", 0.456),
    ("juggernaut.foe_just_acted.from", 0.847), ("juggernaut.hold", 0.369),
    ("juggernaut.just_acted.ambushing", 1.787), ("juggernaut.just_acted.direct", 1.299), ("juggernaut.just_acted.distant", 2.459),
    ("juggernaut.just_acted.evasive", 1.753), ("juggernaut.just_acted.patient", 0.416), ("juggernaut.momentum", 0.1),
    ("juggernaut.strike_worth.floor", 0.991),
    ("juggernaut.strike_worth.to", 0.063), ("juggernaut.wait", 0.05), ("juggernaut.worth_answering.to", 0.05),
    ("kiter.foe_across.floor", 1.0), 
    ("kiter.foe_just_acted.floor", 0.596), ("kiter.foe_just_acted.from", 0.886), ("kiter.hold", 0.558),
    ("kiter.just_acted.ambushing", 2.495), ("kiter.just_acted.binding", 1.859), ("kiter.just_acted.direct", 2.187),
    ("kiter.just_acted.evasive", 1.274), ("kiter.just_acted.patient", 0.88), ("kiter.momentum", 0.087),
    ("kiter.strike_worth.floor", 0.478), ("kiter.strike_worth.to", 0.673),
    ("kiter.wait", 0.154), ("kiter.worth_answering.to", 0.8), 
    ("skirmisher.foe_just_acted.floor", 0.551), ("skirmisher.foe_just_acted.from", 0.524),
    ("skirmisher.hold", 0.449), ("skirmisher.just_acted.ambushing", 2.439),
    ("skirmisher.just_acted.binding", 2.515), ("skirmisher.just_acted.direct", 1.843), ("skirmisher.just_acted.distant", 3.0),
    ("skirmisher.just_acted.patient", 3.0), ("skirmisher.momentum", 0.247), 
    
    ("skirmisher.strike_worth.floor", 0.193), ("skirmisher.strike_worth.to", 0.495),
    ("skirmisher.wait", 0.394), ("skirmisher.worth_answering.to", 0.05),
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
        let kiter = minds.mind(Some(EnemyArchetype::Kiter));
        assert_eq!((defender.wait, kiter.wait), (0.5, 0.5));
        assert_eq!(defender.adjusts["worth_answering"].to, Some(0.2));
        assert_eq!(kiter.adjusts["worth_answering"].to, Some(0.4));
        assert_eq!(defender.adjusts["strike_worth"].floor, Some(0.3));
        assert!(!kiter.adjusts.contains_key("strike_worth"));
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
