//! The numbers an NPC's decisions are shaped by, apart from the
//! considerations that read the fight: each continuous consideration's
//! bounds and floor, each channel's threshold and the movement channel's
//! momentum, set for every archetype or for one. The curve's shape is the
//! consideration's own and is not set here.
//!
//! The world holds one set as a resource, [`TUNED`] (`CombatPlugin`),
//! unless the balance arena gives a fight its own, so a search can tune
//! each archetype's fighter without a rebuild. What a search finds is
//! written back into [`TUNED`].

use std::collections::HashMap;

use common_bevy::archetype::EnemyArchetype;

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
/// momentum of the move under way, and what it sets of its considerations.
#[derive(Clone, Debug)]
pub struct Mind {
    pub wait: f32,
    pub hold: f32,
    pub momentum: f32,
    adjusts: HashMap<&'static str, Adjust>,
}

impl Default for Mind {
    fn default() -> Self {
        Self { wait: skills::WAIT, hold: moves::HOLD, momentum: moves::MOMENTUM, adjusts: HashMap::new() }
    }
}

impl Mind {
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
    adjusts: HashMap<&'static str, Adjust>,
}

/// The settings each archetype fights with, as the arena's best-response
/// search left them against the others: `<archetype>.<setting>` and value.
/// An archetype or setting not named keeps the consideration's own.
pub const TUNED: &[(&str, f32)] = &[
    ("ambusher.behind.floor", 0.703), ("ambusher.behind.to", 0.899), ("ambusher.detour.to", 0.497),
    ("ambusher.effect_added.from", 0.491), ("ambusher.hold", 0.198), ("ambusher.leash_left.to", 0.323),
    ("ambusher.momentum", 0.056), ("ambusher.wait", 0.076), ("ambusher.worth_answering.to", 0.557),
    ("berserker.behind.floor", 1.0), ("berserker.behind.to", 0.675), ("berserker.detour.to", 0.587),
    ("berserker.effect_added.from", 0.654), ("berserker.hold", 0.546), ("berserker.leash_left.to", 0.485),
    ("berserker.momentum", 0.128), ("berserker.wait", 0.27), ("berserker.worth_answering.to", 0.422),
    ("defender.behind.floor", 0.834), ("defender.behind.to", 0.888), ("defender.detour.to", 0.494),
    ("defender.effect_added.from", 0.614), ("defender.hold", 0.192), ("defender.leash_left.to", 0.217),
    ("defender.momentum", 0.159), ("defender.wait", 0.199), ("defender.worth_answering.to", 0.258),
    ("flanker.behind.floor", 0.05), ("flanker.behind.to", 1.0), ("flanker.detour.to", 1.901),
    ("flanker.effect_added.from", 0.867), ("flanker.hold", 0.108), ("flanker.leash_left.to", 0.285),
    ("flanker.momentum", 0.079), ("flanker.wait", 0.05), ("flanker.worth_answering.to", 0.27),
    ("juggernaut.behind.floor", 0.438), ("juggernaut.behind.to", 0.444), ("juggernaut.detour.to", 0.835),
    ("juggernaut.effect_added.from", 0.749), ("juggernaut.hold", 0.283), ("juggernaut.leash_left.to", 0.23),
    ("juggernaut.momentum", 0.041), ("juggernaut.wait", 0.214), ("juggernaut.worth_answering.to", 0.474),
    ("skirmisher.behind.floor", 0.36), ("skirmisher.behind.to", 0.488), ("skirmisher.detour.to", 0.856),
    ("skirmisher.effect_added.from", 0.725), ("skirmisher.hold", 0.277), ("skirmisher.leash_left.to", 0.448),
    ("skirmisher.momentum", 0.125), ("skirmisher.wait", 0.053), ("skirmisher.worth_answering.to", 0.447),
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
    /// `momentum`, or `<consideration>.<from|to|floor>`. Errs on anything
    /// unknown.
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
        minds.set("defender.behind.floor", "0.3").unwrap();
        let defender = minds.mind(Some(EnemyArchetype::Defender));
        let flanker = minds.mind(Some(EnemyArchetype::Flanker));
        assert_eq!((defender.wait, flanker.wait), (0.5, 0.5));
        assert_eq!(defender.adjusts["worth_answering"].to, Some(0.2));
        assert_eq!(flanker.adjusts["worth_answering"].to, Some(0.4));
        assert_eq!(defender.adjusts["behind"].floor, Some(0.3));
        assert!(!flanker.adjusts.contains_key("behind"));
    }

    #[test]
    fn every_tuned_setting_is_one_a_mind_knows() {
        let tuned = Minds::tuned();
        assert!(tuned.mind(Some(EnemyArchetype::Defender)).wait != Mind::default().wait);
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
