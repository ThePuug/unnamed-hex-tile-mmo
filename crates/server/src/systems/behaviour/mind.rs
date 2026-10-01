//! The numbers an NPC's decisions are shaped by, apart from the
//! considerations that read the fight: each continuous consideration's
//! bounds and floor, and each channel's threshold and momentum, set for
//! every archetype or for one. The curve's shape is the consideration's
//! own and is not set here.
//!
//! It is one process-wide set, read through [`mind_of`], holding
//! [`TUNED`] unless the balance arena replaces it ([`set_minds`]) between
//! scenarios, so a search can tune each archetype's fighter without a
//! rebuild. What a search finds is written back into [`TUNED`].

use std::{collections::HashMap, sync::{LazyLock, RwLock}};

use common_bevy::spatial_difficulty::EnemyArchetype;

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
    ("berserker.wait", 0.387), ("berserker.hold", 0.212), ("berserker.momentum", 0.113),
    ("berserker.fatigue_after.floor", 0.0), ("berserker.foe_recovering.floor", 0.672), ("berserker.foe_recovering.to", 0.33),
    ("berserker.combo_offered.floor", 0.439), ("berserker.burst_carried.floor", 0.292),
    ("juggernaut.wait", 0.406), ("juggernaut.hold", 0.32), ("juggernaut.momentum", 0.166),
    ("juggernaut.fatigue_after.floor", 0.057), ("juggernaut.foe_recovering.floor", 0.534), ("juggernaut.foe_recovering.to", 0.488),
    ("juggernaut.grit_banked.floor", 0.306),
    ("kiter.wait", 0.402), ("kiter.hold", 0.341), ("kiter.momentum", 0.163),
    ("kiter.fatigue_after.floor", 0.09), ("kiter.foe_across.floor", 0.25), ("kiter.foe_closing.from", 1.118),
    ("kiter.foe_closing.to", 0.805), ("kiter.shot_cost.to", 0.731), ("kiter.stride_kept.floor", 0.615),
    ("defender.wait", 0.15), ("defender.hold", 0.15), ("defender.momentum", 0.312),
    ("defender.fatigue_after.floor", 0.317), ("defender.worth_answering.to", 0.454), ("defender.pressure_share.floor", 1.0),
    ("defender.span_closed.floor", 0.011),
    ("skirmisher.wait", 0.246), ("skirmisher.hold", 0.189), ("skirmisher.momentum", 0.261),
    ("skirmisher.fatigue_after.floor", 0.369), ("skirmisher.worth_answering.to", 0.541), ("skirmisher.room_to_dodge.floor", 0.183),
    ("skirmisher.room_to_land.to", 0.234), ("skirmisher.bank_full.floor", 0.107), ("skirmisher.foe_nearing.from", 0.747),
    ("skirmisher.close_bank_full.floor", 0.085), ("skirmisher.room_to_flee.to", 0.329),
    ("ambusher.wait", 0.347), ("ambusher.hold", 0.429), ("ambusher.momentum", 0.201),
    ("ambusher.fatigue_after.floor", 0.121), ("ambusher.worth_answering.to", 0.098), ("ambusher.pressure_share.floor", 0.521),
    ("ambusher.span_closed.floor", 0.4), ("ambusher.reactions_left.floor", 0.586),
];

/// Every mind setting: those for all archetypes, and each archetype's on
/// top. Its default sets nothing; [`Minds::tuned`] holds [`TUNED`].
#[derive(Clone, Debug, Default)]
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

static MINDS: LazyLock<RwLock<Minds>> = LazyLock::new(|| RwLock::new(Minds::tuned()));

/// The mind of an NPC of `archetype`, as the settings stand
pub fn mind_of(archetype: Option<EnemyArchetype>) -> Mind {
    MINDS.read().unwrap_or_else(|poisoned| poisoned.into_inner()).mind(archetype)
}

/// Replaces every mind setting. Only the balance arena calls it, between
/// scenarios, with no fight running.
pub fn set_minds(minds: Minds) {
    *MINDS.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = minds;
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
        minds.set("defender.span_closed.floor", "0.3").unwrap();
        let defender = minds.mind(Some(EnemyArchetype::Defender));
        let kiter = minds.mind(Some(EnemyArchetype::Kiter));
        assert_eq!((defender.wait, kiter.wait), (0.5, 0.5));
        assert_eq!(defender.adjusts["worth_answering"].to, Some(0.2));
        assert_eq!(kiter.adjusts["worth_answering"].to, Some(0.4));
        assert_eq!(defender.adjusts["span_closed"].floor, Some(0.3));
        assert!(!kiter.adjusts.contains_key("span_closed"));
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
        assert!(minds.set("all.open.floor", "0.5").is_err(), "a condition is not tuned");
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
