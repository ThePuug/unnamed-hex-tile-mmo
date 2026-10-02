pub mod ally_target;
pub mod behaviour;
pub mod engagement;
pub mod entity_type;
pub mod equipment;
pub mod grit;
pub mod heading;
pub mod hex_assignment;
pub mod keybits;
pub mod loaded_by;
pub mod movement_intent_state;
pub mod displacing;
pub mod position;
pub mod reaction_queue;
pub mod recovery;
pub mod resources;
pub mod returning;
pub mod status;
pub mod target;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i32, r: i32, z: i32) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }

    /// Combat distance accounting for elevation.
    /// Single z-level differences are slopes (free), 2+ z-levels are cliffs
    /// that add their excess height to the distance.
    pub fn distance(&self, other: &Loc) -> i32 {
        let z_diff = (self.z - other.z).abs();
        self.flat_distance(other) + (z_diff - 1).max(0)
    }
}

#[cfg(test)]
mod loc_tests {
    use super::*;

    #[test]
    fn test_distance_flat() {
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 3, r: 0, z: 0 });
        assert_eq!(a.distance(&b), 3);
    }

    #[test]
    fn test_distance_slope_is_free() {
        // Single z-level = slope, no extra distance
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 5, r: 0, z: 1 });
        assert_eq!(a.distance(&b), 5, "slope adds no distance");
    }

    #[test]
    fn test_distance_cliff_adds_excess() {
        // z_diff=3, excess=2 above slope threshold
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 5, r: 0, z: 3 });
        assert_eq!(a.distance(&b), 7, "5 flat + (3-1) cliff = 7");
    }

    #[test]
    fn test_distance_adjacent_slope_is_melee() {
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 1, r: 0, z: 1 });
        assert_eq!(a.distance(&b), 1, "adjacent tile on slope is melee range");
    }

    #[test]
    fn test_distance_adjacent_cliff_blocks_melee() {
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 1, r: 0, z: 2 });
        assert_eq!(a.distance(&b), 2, "adjacent tile on cliff exceeds melee range");
    }

    #[test]
    fn test_distance_vertical_cliff() {
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 0, r: 0, z: 10 });
        assert_eq!(a.distance(&b), 9, "0 flat + (10-1) cliff = 9");
    }

    #[test]
    fn test_distance_ranged_blocked_by_steep_cliff() {
        let a = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let b = Loc::new(Qrz { q: 5, r: 0, z: 5 });
        assert_eq!(a.distance(&b), 9, "5 flat + (5-1) cliff = 9, exceeds range 6");
    }

    #[test]
    fn test_distance_symmetric() {
        let a = Loc::new(Qrz { q: 2, r: -1, z: 5 });
        let b = Loc::new(Qrz { q: -3, r: 4, z: 1 });
        assert_eq!(a.distance(&b), b.distance(&a));
    }

    #[test]
    fn test_distance_same_loc() {
        let a = Loc::new(Qrz { q: 3, r: 2, z: 7 });
        assert_eq!(a.distance(&a), 0);
    }

}

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

/// A player's turn state, confirmed with the position: the heading and the
/// milliseconds since it last stepped, saturating at the repeat interval.
/// Server authority, mirrored into `Heading` for everything that reads the
/// facing; the local player replays its open inputs from it into `Heading`.
#[derive(Clone, Component, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Turn {
    pub heading: heading::Heading,
    pub since_step_ms: u16,
}

impl Default for Turn {
    /// Rested: the first press steps at once.
    fn default() -> Self {
        Turn { heading: heading::Heading::default(), since_step_ms: crate::systems::movement::TURN_REPEAT_MS }
    }
}

#[derive(Clone, Component, Copy, Default)]
pub struct Actor;

/// Discrete commitment tier: T0 (<20%), T1 (≥20%), T2 (≥40%), T3 (≥60%).
///
/// The percentage is against the most a single attribute could reach
/// (`ActorAttributes::ceiling`), not against the summed budget. A summed
/// denominator inflates with spread, so a spectrum build would tier lower than
/// an axis build holding identical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommitmentTier {
    /// No commitment identity — baseline only
    T0,
    /// Identity unlocked — noticeable specialization (≥20%)
    T1,
    /// Identity deepened — significant commitment (≥40%)
    T2,
    /// Identity defining — dominant aspect of build (≥60%)
    T3,
}

impl CommitmentTier {
    /// Calculate commitment tier from a derived attribute value and total budget.

    /// This is a pure function — it does not know which attribute produced the value
    /// or how it was derived from A/S/S. It only cares about the percentage.
    pub fn calculate(derived_value: u16, total_budget: u32) -> Self {
        if total_budget == 0 {
            return Self::T0;
        }
        // Where the tiers fall is fixed; only what each gives is tuned
        let share = derived_value as f32 / total_budget as f32;
        if share >= 0.6 {
            Self::T3
        } else if share >= 0.4 {
            Self::T2
        } else if share >= 0.2 {
            Self::T1
        } else {
            Self::T0
        }
    }

    /// What a commitment gives at this tier, where it gives `min` at T0 and
    /// `max` at T3: the tiers between are evenly spaced, so every tuned
    /// value a commitment gives is those two numbers.
    pub fn between(self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.index() as f32 / 3.0
    }

    /// The tier's place, 0 to 3, as `Tuning`'s tier arrays list their effects
    pub fn index(self) -> usize {
        match self {
            Self::T0 => 0,
            Self::T1 => 1,
            Self::T2 => 2,
            Self::T3 => 3,
        }
    }
}

/// The six attributes, two to a pair. Each is read three ways, by one rule
/// apiece: its value, which contests weigh ([`ActorAttributes::value`]);
/// its potency, which grows with level ([`ActorAttributes::potency`]); and
/// its commitment tier ([`ActorAttributes::tier`]). The stat each reading
/// goes by has a method of its name there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attribute {
    Might,
    Agility,
    Vitality,
    Discipline,
    Instinct,
    Resolve,
}

/// One pair of opposed attributes, as the levels put into it. `axis`
/// commits to one of the two: negative the left, positive the right.
/// `spectrum` reaches both. `shift` leans the spectrum from the side the
/// axis committed to toward the other, as far as the spectrum goes; a pair
/// with no axis has no side to lean from, so it never shifts.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Pair {
    pub axis: i8,
    pub spectrum: i8,
    pub shift: i8,
}

/// Which attribute of a [`Pair`]: the one a negative axis commits to, or
/// the one a positive axis does
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum End {
    Left = -1,
    Right = 1,
}

impl Pair {
    /// What a level of axis gives the attribute it commits to
    const AXIS: i16 = 16;
    /// What a level of spectrum gives the committed attribute, and what a
    /// level of shift moves from it to the other
    const SPECTRUM: i16 = 12;
    /// What a level of spectrum gives each attribute of a pair with no axis
    const BALANCED: i16 = 6;

    pub fn new(axis: i8, spectrum: i8, shift: i8) -> Self {
        Self { axis, spectrum: spectrum.max(0), shift }
    }

    /// The levels put into the pair
    pub fn levels(self) -> u32 {
        self.axis.unsigned_abs() as u32 + self.spectrum.max(0) as u32
    }

    fn committed_to(self, side: End) -> bool {
        self.axis.signum() == side as i8
    }

    /// What the attribute on `side` is worth now: on the committed side the
    /// axis and the spectrum, less what is shifted away; on the other, what
    /// is shifted to it; with no axis, the spectrum at its balanced rate.
    fn value(self, side: End) -> u16 {
        let spectrum = self.spectrum.max(0) as i16;
        let shifted = self.shift as i16 * side as i16 * Self::SPECTRUM;
        if self.axis == 0 {
            (spectrum * Self::BALANCED) as u16
        } else if self.committed_to(side) {
            (self.axis.unsigned_abs() as i16 * Self::AXIS + spectrum * Self::SPECTRUM + shifted).max(0) as u16
        } else {
            shifted.max(0) as u16
        }
    }

    /// The most the attribute on `side` can be worth under any shift
    fn reach(self, side: End) -> u16 {
        let spectrum = self.spectrum.max(0) as u16;
        if self.axis == 0 {
            spectrum * Self::BALANCED as u16
        } else if self.committed_to(side) {
            self.axis.unsigned_abs() as u16 * Self::AXIS as u16 + spectrum * Self::SPECTRUM as u16
        } else {
            spectrum * Self::SPECTRUM as u16
        }
    }

    /// Shifts as far toward `shift` as the pair allows: away from the
    /// committed side only, no further than the spectrum, and not at all
    /// with no axis.
    pub fn set_shift(&mut self, shift: i8) {
        let most = self.spectrum.max(0);
        self.shift = match self.axis.signum() {
            0 => 0,
            1 => shift.clamp(-most, 0),
            _ => shift.clamp(0, most),
        };
    }
}

/// What an actor has put its levels into: three pairs of opposed
/// attributes, Might and Agility, Vitality and Discipline, Instinct and
/// Resolve. Every value an actor fights with is read from these through
/// the methods here.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
#[require(grit::Grit, Swing)]
pub struct ActorAttributes {
    /// Might ↔ Agility
    physique: Pair,
    /// Vitality ↔ Discipline
    conditioning: Pair,
    /// Instinct ↔ Resolve
    temperament: Pair,
    /// The levels the actor has, which a respec spends again and never
    /// changes: a draft that has not placed them all is still an actor of
    /// this level
    level: u32,
}

impl ActorAttributes {
    /// An actor's attributes from the levels in each pair: axis, spectrum
    /// and shift of Might ↔ Agility, of Vitality ↔ Discipline, then of
    /// Instinct ↔ Resolve. A shift is taken as given, unclamped. Its level
    /// is the levels these put in.
    pub fn new(
        might_agility_axis: i8,
        might_agility_spectrum: i8,
        might_agility_shift: i8,
        vitality_discipline_axis: i8,
        vitality_discipline_spectrum: i8,
        vitality_discipline_shift: i8,
        instinct_resolve_axis: i8,
        instinct_resolve_spectrum: i8,
        instinct_resolve_shift: i8,
    ) -> Self {
        let pairs = [
            Pair::new(might_agility_axis, might_agility_spectrum, might_agility_shift),
            Pair::new(vitality_discipline_axis, vitality_discipline_spectrum, vitality_discipline_shift),
            Pair::new(instinct_resolve_axis, instinct_resolve_spectrum, instinct_resolve_shift),
        ];
        let [physique, conditioning, temperament] = pairs;
        Self { physique, conditioning, temperament, level: Self::invested(&pairs) }
    }

    pub fn might_agility_axis(&self) -> i8 { self.physique.axis }
    pub fn might_agility_spectrum(&self) -> i8 { self.physique.spectrum }
    pub fn might_agility_shift(&self) -> i8 { self.physique.shift }

    pub fn vitality_discipline_axis(&self) -> i8 { self.conditioning.axis }
    pub fn vitality_discipline_spectrum(&self) -> i8 { self.conditioning.spectrum }
    pub fn vitality_discipline_shift(&self) -> i8 { self.conditioning.shift }

    pub fn instinct_resolve_axis(&self) -> i8 { self.temperament.axis }
    pub fn instinct_resolve_spectrum(&self) -> i8 { self.temperament.spectrum }
    pub fn instinct_resolve_shift(&self) -> i8 { self.temperament.shift }

    pub fn set_might_agility_shift(&mut self, shift: i8) { self.physique.set_shift(shift) }
    pub fn set_vitality_discipline_shift(&mut self, shift: i8) { self.conditioning.set_shift(shift) }
    pub fn set_instinct_resolve_shift(&mut self, shift: i8) { self.temperament.set_shift(shift) }

    /// The three pairs: Might ↔ Agility, Vitality ↔ Discipline, Instinct ↔ Resolve
    pub fn pairs(&self) -> [Pair; 3] {
        [self.physique, self.conditioning, self.temperament]
    }

    /// The levels `pairs` put in
    pub fn invested(pairs: &[Pair; 3]) -> u32 {
        pairs.iter().map(|pair| pair.levels()).sum()
    }

    /// Whether `pairs` is a draft an actor of `level` may hold: no more
    /// levels than it has, and no spectrum below nothing. A respec it takes
    /// has placed every one (`is_complete`).
    pub fn fits(pairs: &[Pair; 3], level: u32) -> bool {
        pairs.iter().all(|pair| pair.spectrum >= 0) && Self::invested(pairs) <= level
    }

    /// Whether `pairs` is a respec an actor of `level` may take: a draft
    /// that `fits` and places every level.
    pub fn is_complete(pairs: &[Pair; 3], level: u32) -> bool {
        Self::fits(pairs, level) && Self::invested(pairs) == level
    }

    /// Takes a whole respec, or lays a draft over a copy to show it: each
    /// pair's axis and spectrum as given, its shift as far as the pair
    /// allows, and the level as it was.
    pub fn apply_respec(&mut self, pairs: [Pair; 3]) {
        for (own, respec) in [&mut self.physique, &mut self.conditioning, &mut self.temperament].into_iter().zip(pairs) {
            *own = Pair::new(respec.axis, respec.spectrum, 0);
            own.set_shift(respec.shift);
        }
    }

    fn pair(&self, attribute: Attribute) -> (Pair, End) {
        match attribute {
            Attribute::Might => (self.physique, End::Left),
            Attribute::Agility => (self.physique, End::Right),
            Attribute::Vitality => (self.conditioning, End::Left),
            Attribute::Discipline => (self.conditioning, End::Right),
            Attribute::Instinct => (self.temperament, End::Left),
            Attribute::Resolve => (self.temperament, End::Right),
        }
    }

    /// What `attribute` is worth now. Every relative contest weighs one
    /// actor's value against another's (`damage::contest_factor`).
    pub fn value(&self, attribute: Attribute) -> u16 {
        let (pair, end) = self.pair(attribute);
        pair.value(end)
    }

    /// The most `attribute` can be worth under any shift
    pub fn reach(&self, attribute: Attribute) -> u16 {
        let (pair, end) = self.pair(attribute);
        pair.reach(end)
    }

    /// `attribute`'s absolute stat, a potency that grows with level:
    /// `Tuning::potency_base` and `potency_per_point` more for each point of
    /// the attribute, scaled by the damage level curve.
    pub fn potency(&self, attribute: Attribute) -> f32 {
        let tuning = crate::tuning::tuning();
        (tuning.potency_base + self.value(attribute) as f32 * tuning.potency_per_point) * self.damage_level_multiplier()
    }

    /// `attribute`'s commitment tier: its value as a share of the most any
    /// one attribute could reach at the actor's level (`ceiling`). The summed
    /// budget would tier a spectrum build below an axis build holding the
    /// same points.
    pub fn tier(&self, attribute: Attribute) -> CommitmentTier {
        CommitmentTier::calculate(self.value(attribute), self.ceiling())
    }

    /// The most any one attribute can be worth at the actor's level: every
    /// level in one axis.
    pub fn ceiling(&self) -> u32 {
        self.total_level() * Pair::AXIS as u32
    }

    // Each attribute by name, and the most any shift could make it

    pub fn might(&self) -> u16 { self.value(Attribute::Might) }
    pub fn agility(&self) -> u16 { self.value(Attribute::Agility) }
    pub fn vitality(&self) -> u16 { self.value(Attribute::Vitality) }
    pub fn discipline(&self) -> u16 { self.value(Attribute::Discipline) }
    pub fn instinct(&self) -> u16 { self.value(Attribute::Instinct) }
    pub fn resolve(&self) -> u16 { self.value(Attribute::Resolve) }

    pub fn might_reach(&self) -> u16 { self.reach(Attribute::Might) }
    pub fn agility_reach(&self) -> u16 { self.reach(Attribute::Agility) }
    pub fn vitality_reach(&self) -> u16 { self.reach(Attribute::Vitality) }
    pub fn discipline_reach(&self) -> u16 { self.reach(Attribute::Discipline) }
    pub fn instinct_reach(&self) -> u16 { self.reach(Attribute::Instinct) }
    pub fn resolve_reach(&self) -> u16 { self.reach(Attribute::Resolve) }

    // Absolute: an attribute's potency, by the name its stat goes by

    /// Force, Might's: what its share adds to an auto-attack (`auto_damage`)
    pub fn force(&self) -> f32 { self.potency(Attribute::Might) }
    /// Tempo, Agility's: how fast its auto-attacks come (`cadence_interval`)
    pub fn tempo(&self) -> f32 { self.potency(Attribute::Agility) }
    /// Endurance, Discipline's: how deep the endurance pool is (`max_endurance`)
    pub fn endurance(&self) -> f32 { self.potency(Attribute::Discipline) }
    /// Intuition, Instinct's: what an action is sized by (`skill_potency`)
    pub fn intuition(&self) -> f32 { self.potency(Attribute::Instinct) }
    /// Concentration, Resolve's: what a reaction is sized by (`skill_potency`)
    pub fn concentration(&self) -> f32 { self.potency(Attribute::Resolve) }

    /// Constitution, Vitality's, which is max health: the health every actor
    /// has (`Tuning::base_health`) and what each point of Vitality adds
    /// (`Tuning::health_per_vitality`), scaled by the health level curve.
    pub fn constitution(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        (tuning.base_health + self.vitality() as f32 * tuning.health_per_vitality) * self.hp_level_multiplier()
    }

    pub fn max_health(&self) -> f32 {
        self.constitution()
    }

    // Relative: the value a contest weighs, by the name it goes by there

    /// Impact, Might: pushes a target's recovery back, against its Composure
    pub fn impact(&self) -> u16 { self.value(Attribute::Might) }
    /// Flow, Agility: unlocks a combo sooner, against the target's Reflex
    pub fn flow(&self) -> u16 { self.value(Attribute::Agility) }
    /// Toughness, Vitality: mitigates a blow, against the attacker's Focus
    pub fn toughness(&self) -> u16 { self.value(Attribute::Vitality) }
    /// Composure, Discipline: shortens its own recovery, against the opponent's Impact
    pub fn composure(&self) -> u16 { self.value(Attribute::Discipline) }
    /// Reflex, Instinct: widens a threat's window, against the attacker's Flow
    pub fn reflex(&self) -> u16 { self.value(Attribute::Instinct) }
    /// Focus, Resolve: decides whether a blow crits and meets a defender's
    /// mitigation, against their Toughness (`damage::crit_chance`)
    pub fn focus(&self) -> u16 { self.value(Attribute::Resolve) }

    // Commitment: an attribute's tier, by the name it goes by

    /// Ferocity, Might: combos fire before they unlock, and pay less for it (`ferocity_relief`)
    pub fn ferocity(&self) -> CommitmentTier { self.tier(Attribute::Might) }
    /// Grace, Agility: the arc it strikes within (`arc`)
    pub fn grace(&self) -> CommitmentTier { self.tier(Attribute::Agility) }
    /// Grit, Vitality: its index is how many of the blows it lets land it banks
    pub fn grit(&self) -> CommitmentTier { self.tier(Attribute::Vitality) }
    /// Preparation, Discipline: its index is how many reactions the actor
    /// may use in any one recovery, each paying less of its own after
    /// (`preparation_relief`)
    pub fn preparation(&self) -> CommitmentTier { self.tier(Attribute::Discipline) }
    /// Patience, Instinct: stamina refilled faster waiting on a swing it could not strike (`patience_regen`)
    pub fn patience(&self) -> CommitmentTier { self.tier(Attribute::Instinct) }
    /// Awareness, Resolve: how far behind the front threat its reactions reach (`span`)
    pub fn awareness(&self) -> CommitmentTier { self.tier(Attribute::Resolve) }

    /// The actor's level: the levels it has, whether or not a draft laid
    /// over it has placed them all
    pub fn total_level(&self) -> u32 {
        self.level
    }

    /// A level curve, `(1 + level × k)^p`: 1 at level 0
    pub fn level_multiplier(level: u32, k: f32, p: f32) -> f32 {
        (1.0 + level as f32 * k).powf(p)
    }

    /// The health level curve at the actor's level
    pub fn hp_level_multiplier(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        Self::level_multiplier(self.total_level(), tuning.health_curve_k, tuning.health_curve_p)
    }

    /// The damage level curve at the actor's level, which every potency scales by
    pub fn damage_level_multiplier(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        Self::level_multiplier(self.total_level(), tuning.damage_curve_k, tuning.damage_curve_p)
    }

    /// Movement speed: the same for every actor, no attribute governing it
    pub fn movement_speed(&self) -> f32 {
        crate::systems::movement::MOVEMENT_SPEED
    }

    /// The potency every actor has before any attribute, scaled by level: what
    /// each absolute stat starts from.
    pub fn base_potency(&self) -> f32 {
        crate::tuning::tuning().potency_base * self.damage_level_multiplier()
    }

    /// How far its points have carried `attribute`'s absolute stat toward
    /// its ceiling: 0 with none, half at `Tuning::share_bend` points, rising
    /// toward 1 with diminishing returns, the same at every level. Each
    /// absolute's passive effect is its ceiling times this share.
    pub fn share(&self, attribute: Attribute) -> f32 {
        let points = self.value(attribute) as f32;
        points / (points + crate::tuning::tuning().share_bend)
    }

    /// The stamina pool: `Tuning::stamina_base`, the same for every actor
    pub fn max_stamina(&self) -> f32 {
        crate::tuning::tuning().stamina_base
    }

    /// The endurance pool: `Tuning::endurance_pool` for each point of
    /// Endurance, so it deepens with level and with Discipline
    pub fn max_endurance(&self) -> f32 {
        crate::tuning::tuning().endurance_pool * self.endurance()
    }

    /// The attribute `ability` reads: Resolve for a reaction, Instinct for
    /// any other skill, an action.
    fn read_by(ability: crate::message::AbilityType) -> Attribute {
        if ability.is_reaction() { Attribute::Resolve } else { Attribute::Instinct }
    }

    /// The potency `ability`, a skill, is sized by: Concentration for a
    /// reaction, Intuition for an action. With none of that attribute it is
    /// base potency. An auto-attack is sized by `auto_damage` instead.
    pub fn skill_potency(&self, ability: crate::message::AbilityType) -> f32 {
        self.potency(Self::read_by(ability))
    }

    /// The endurance `ability`, a skill, costs this actor:
    /// `Tuning::endurance_cost` of the potency it reads (`skill_potency`)
    /// for each point of stamina it costs, so a cheap skill is cheap in
    /// both. It grows with level as the pool does, so a pool with no
    /// Discipline in it holds the same count of a build's skills at any
    /// level. A reaction pays besides for what it clears
    /// (`reaction_effort`).
    pub fn skill_endurance(&self, ability: crate::message::AbilityType) -> f32 {
        let tuning = crate::tuning::tuning();
        tuning.endurance_cost * tuning.cost(ability) * self.skill_potency(ability)
    }

    /// The endurance it costs this actor's reaction to clear a threat of
    /// `damage`, beside the reaction's flat cost: `Tuning::reaction_per_threat`
    /// of base potency for the threat itself and `Tuning::reaction_effort`
    /// for each point of its damage, with no Resolve, all of it less by its
    /// Concentration over base potency. A threat costs something however
    /// light, so a stream of small ones is not cleared free.
    pub fn reaction_effort(&self, damage: f32) -> f32 {
        let tuning = crate::tuning::tuning();
        let base = self.base_potency();
        (tuning.reaction_per_threat * base + damage * tuning.reaction_effort) * base / self.concentration()
    }

    /// An auto-attack's damage: `Tuning::auto_damage` of base potency, more by
    /// `Tuning::force_auto` at the ceiling of Force's share
    pub fn auto_damage(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.base_potency() * tuning.auto_damage * (1.0 + tuning.force_auto * self.share(Attribute::Might))
    }

    /// How much longer and harder the stuns, slows and knockbacks this
    /// actor inflicts with `ability` hold: 1 with none of the attribute the
    /// ability reads, `Tuning::effect_hold` more at the ceiling of that
    /// attribute's share. A share, never the potency, so an effect keeps
    /// under its ceiling at any level.
    pub fn hold(&self, ability: crate::message::AbilityType) -> f32 {
        1.0 + crate::tuning::tuning().effect_hold * self.share(Self::read_by(ability))
    }

    /// The share of the recovery a combo fired early skipped that this actor
    /// is let off, by its Ferocity: `Tuning::ferocity_relief_min` to
    /// `ferocity_relief_max`.
    pub fn ferocity_relief(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.ferocity().between(tuning.ferocity_relief_min, tuning.ferocity_relief_max)
    }

    /// The share of its own recovery a reaction this actor uses through a
    /// recovery is let off, by its Preparation:
    /// `Tuning::preparation_relief_min` to `preparation_relief_max`.
    pub fn preparation_relief(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.preparation().between(tuning.preparation_relief_min, tuning.preparation_relief_max)
    }

    /// How much each blow this actor lets land fills its Grit's bank
    /// (`components::grit::Grit`): its tier's index, 0 to 3.
    pub fn grit_fill(&self) -> u8 {
        self.grit().index() as u8
    }

    /// The share faster this actor's stamina refills while it waits on a
    /// swing it could not strike (`Status::waiting`): `Tuning::patience_regen_min`
    /// at T0 to `patience_regen_max` at T3.
    pub fn patience_regen(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.patience().between(tuning.patience_regen_min, tuning.patience_regen_max)
    }

    /// How far behind the front threat this actor's reactions reach, from
    /// Awareness: `Tuning::awareness_span_min`, which every actor has, to
    /// `awareness_span_max`. A reaction takes the front threat and every
    /// threat landing within this long after it (`ReactionQueue::swept`).
    pub fn span(&self) -> std::time::Duration {
        let tuning = crate::tuning::tuning();
        std::time::Duration::from_secs_f32(self.awareness().between(tuning.awareness_span_min, tuning.awareness_span_max))
    }

    /// The half-angle either side of its heading this actor strikes within:
    /// `Tuning::grace_arc_min`, the three forward faces, with no Grace, and
    /// each tier wider to `grace_arc_max`, which leaves only what stands
    /// straight behind it out of reach. A strike past the forward faces
    /// breaks its stride (`targeting::across`).
    pub fn arc(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.grace().between(tuning.grace_arc_min, tuning.grace_arc_max)
    }

    /// Seconds between auto-attacks: `Tuning::base_interval`, the one pace
    /// every actor starts from, quickened by `Tuning::tempo_ceiling` at the
    /// ceiling of Tempo's share. No actor swings slower than the base.
    pub fn cadence_interval(&self) -> std::time::Duration {
        let tuning = crate::tuning::tuning();
        std::time::Duration::from_secs_f32(tuning.base_interval / (1.0 + tuning.tempo_ceiling * self.share(Attribute::Agility)))
    }
}

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();

/// When an actor's next auto-attack comes due, as the server counts it:
/// an interval after its last, or the moment its fight found it not yet
/// swinging. A due swing waits for a target it can strike, and while it
/// waits Patience refills the actor's stamina faster
/// (`ActorAttributes::patience_regen`). None disengaged: a swing is due,
/// and nothing waits until an engagement starts the clock.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Swing {
    pub due: Option<std::time::Duration>,
}

impl Swing {
    /// How long the due swing has waited at `now`; None while the next is
    /// still to come due. Out of combat it is due and has waited no time.
    pub fn waited(&self, now: std::time::Duration) -> Option<std::time::Duration> {
        match self.due {
            Some(due) => now.checked_sub(due),
            None => Some(std::time::Duration::ZERO),
        }
    }
}

/// Auto-attack range in hex tiles. Default is 2, melee reach, so a blow
/// lands on a target a step away as well as one beside it.
/// Eventually sourced from equipped weapon; for now set per-archetype at spawn.
#[derive(Clone, Component, Copy, Debug)]
pub struct AttackRange(pub i32);

impl Default for AttackRange {
    fn default() -> Self {
        Self(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== MOVEMENT SPEED TESTS =====
    // TODO: Re-enable when movement speed is allocated to a meta-attribute

    // ===== LEVEL MULTIPLIER TESTS =====
    // Property tests only — no specific formula values, survives balance tuning

    #[test]
    fn test_concentration_follows_resolve_not_might() {
        let resolve = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let might = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        assert!(resolve.concentration() > might.concentration());
        assert!(might.force() > resolve.force());
        assert_eq!(might.concentration(), resolve.force());
    }

    #[test]
    fn a_share_rises_with_investment_and_diminishes() {
        let share = |points: i8| {
            let attrs = ActorAttributes::new(-points, 0, 0, 0, 0, 0, 0, 0, 0);
            attrs.share(Attribute::Might)
        };
        assert_eq!(share(0), 0.0, "none invested, none of the ceiling");
        assert!(share(5) > 0.0 && share(10) > share(5), "more invested, more of it");
        assert!(share(10) - share(5) < share(5) - share(0), "each point gives less");
        assert!(share(100) < 1.0, "never the whole ceiling");
    }

    #[test]
    fn endurance_deepens_its_own_pool_and_every_actor_has_the_one_stamina() {
        let disciplined = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let mighty = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(disciplined.max_stamina(), plain.max_stamina());
        assert!(disciplined.max_endurance() > mighty.max_endurance(), "Discipline deepens it");
        assert!(mighty.max_endurance() > plain.max_endurance(), "and so does level");
        use crate::message::AbilityType::{Counter, Frenzy};
        assert_eq!(disciplined.skill_endurance(Frenzy), mighty.skill_endurance(Frenzy), "a skill costs the same at a level where neither reads its stat");
        let skills = |attrs: &ActorAttributes| attrs.max_endurance() / attrs.skill_endurance(Frenzy);
        assert!((skills(&mighty) - skills(&plain)).abs() < 1e-3, "with no Discipline a pool holds as many skills at any level");

        let (instinctive, resolute) = (ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0), ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0));
        assert!(instinctive.skill_endurance(Frenzy) > mighty.skill_endurance(Frenzy), "a skill costs by the potency it reads");
        assert_eq!(instinctive.skill_endurance(Counter), mighty.skill_endurance(Counter));
        assert!(resolute.skill_endurance(Counter) > mighty.skill_endurance(Counter));
        use crate::message::AbilityType::{Feint, Overpower};
        assert!(plain.skill_endurance(Feint) < plain.skill_endurance(Overpower), "a skill cheap in stamina is cheap in endurance");
    }

    #[test]
    fn an_action_reads_intuition_and_a_reaction_concentration() {
        use crate::message::AbilityType::*;
        let instinctive = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let resolute = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let mighty = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        for action in [Frenzy, Feint, Overpower, Punish, Leap, PerfectStride] {
            assert_eq!(instinctive.skill_potency(action), instinctive.intuition(), "{action:?}");
            assert_eq!(resolute.skill_potency(action), resolute.base_potency(), "{action:?} reads no Resolve");
            assert!(instinctive.hold(action) > 1.0 && resolute.hold(action) == 1.0, "its effects hold by Intuition's share");
        }
        for reaction in [Parry, Counter] {
            assert_eq!(resolute.skill_potency(reaction), resolute.concentration(), "{reaction:?}");
            assert_eq!(instinctive.skill_potency(reaction), instinctive.base_potency(), "{reaction:?} reads no Instinct");
            assert!(resolute.hold(reaction) > 1.0 && instinctive.hold(reaction) == 1.0, "its effects hold by Concentration's share");
        }
        assert_eq!(mighty.skill_potency(Frenzy), mighty.base_potency(), "with none of either, base potency");
        assert_eq!(mighty.reaction_effort(10.0), instinctive.reaction_effort(10.0), "a parry costs the same with no Resolve");
        assert!(resolute.reaction_effort(10.0) < mighty.reaction_effort(10.0), "and less by Concentration");
        assert!(mighty.reaction_effort(20.0) > mighty.reaction_effort(10.0), "more by the damage it turns aside");
        assert!(mighty.reaction_effort(0.0) > 0.0, "and something for a threat however light");
        assert!(instinctive.hold(Frenzy) < 1.0 + crate::tuning::tuning().effect_hold, "a share keeps an effect under its ceiling");
    }

    #[test]
    fn tempo_quickens_the_swing_and_no_one_is_slower_than_the_base() {
        let base = std::time::Duration::from_secs_f32(crate::tuning::tuning().base_interval);
        let quick = |points: i8| ActorAttributes::new(points, 0, 0, 0, 0, 0, 0, 0, 0).cadence_interval();
        assert_eq!(ActorAttributes::default().cadence_interval(), base);
        assert_eq!(ActorAttributes::new(-10, 0, 0, -10, 0, 0, 0, 0, 0).cadence_interval(), base, "no other attribute changes the pace");
        assert!(quick(5) < base && quick(10) < quick(5), "more Agility, a faster swing");
    }

    #[test]
    fn grace_widens_the_arc_to_all_but_straight_behind() {
        let graceful = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(plain.arc(), crate::systems::targeting::STRIDE_ARC, "no Grace, the forward faces");
        assert!(graceful.arc() > plain.arc() && graceful.arc() < 180.0, "full commitment still cannot strike straight behind");
    }

    #[test]
    fn awareness_lengthens_the_span_and_every_actor_has_the_least() {
        let aware = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let plain = ActorAttributes::default();
        assert!(plain.span() > std::time::Duration::ZERO, "no Awareness, still a span");
        assert!(aware.span() > plain.span());
    }

    #[test]
    fn a_tier_gives_its_value_evenly_between_the_two_ends() {
        use CommitmentTier::*;
        assert_eq!([T0, T1, T2, T3].map(|tier| tier.between(60.0, 150.0)), [60.0, 90.0, 120.0, 150.0]);
        assert_eq!(T2.between(1.0, 1.0), 1.0, "a value the same at both ends is the same at every tier");
    }

    #[test]
    fn patience_refills_stamina_faster_by_its_tier() {
        let patient = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let tuning = crate::tuning::tuning();
        assert_eq!(ActorAttributes::default().patience_regen(), tuning.patience_regen_min, "without Patience, no faster");
        assert_eq!(patient.patience_regen(), patient.patience().between(tuning.patience_regen_min, tuning.patience_regen_max));
        assert!(patient.patience_regen() > ActorAttributes::default().patience_regen());
    }

    #[test]
    fn force_strengthens_auto_attacks() {
        let might = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let vital = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        assert!(might.auto_damage() > vital.auto_damage());
        assert_eq!(vital.auto_damage(), vital.base_potency() * crate::tuning::tuning().auto_damage);
    }

    #[test]
    fn test_level_multiplier_identity_at_zero() {
        // Level 0 must always return 1.0 regardless of k/p
        assert_eq!(ActorAttributes::level_multiplier(0, 0.10, 1.5), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.15, 2.0), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.10, 1.2), 1.0);
        assert_eq!(ActorAttributes::level_multiplier(0, 0.99, 5.0), 1.0);
    }

    #[test]
    fn test_level_multiplier_monotonically_increasing() {
        // Higher level must always produce higher multiplier (same k/p)
        for level in 0..20u32 {
            let lower = ActorAttributes::level_multiplier(level, 0.10, 1.5);
            let higher = ActorAttributes::level_multiplier(level + 1, 0.10, 1.5);
            assert!(
                higher > lower,
                "Multiplier must increase with level: level {} ({}) >= level {} ({})",
                level + 1, higher, level, lower
            );
        }
    }

    #[test]
    fn test_level_multiplier_super_linear_growth() {
        // The gap between consecutive levels should increase (super-linear, not linear)
        // gap(level N→N+1) < gap(level N+1→N+2) for p > 1
        let gap_low = ActorAttributes::level_multiplier(2, 0.10, 1.5)
            - ActorAttributes::level_multiplier(1, 0.10, 1.5);
        let gap_high = ActorAttributes::level_multiplier(9, 0.10, 1.5)
            - ActorAttributes::level_multiplier(8, 0.10, 1.5);
        assert!(
            gap_high > gap_low,
            "Growth rate should accelerate: gap at high levels ({}) > gap at low levels ({})",
            gap_high, gap_low
        );
    }

    #[test]
    fn test_damage_multiplier_exceeds_hp_multiplier() {
        // Damage scales more aggressively than HP at all positive levels
        for level in 1..=20u32 {
            let attrs = ActorAttributes::new(
                -(level as i8).min(127), 0, 0,  // some might investment
                0, 0, 0,
                0, 0, 0,
            );
            assert!(
                attrs.damage_level_multiplier() >= attrs.hp_level_multiplier(),
                "Damage multiplier should >= HP multiplier at level {}",
                level
            );
        }
    }

    #[test]
    fn test_max_health_increases_with_level() {
        let level_0 = ActorAttributes::default();
        let level_5 = ActorAttributes::new(-3, -2, 0, 0, 0, 0, 0, 0, 0); // 5 points invested
        let level_10 = ActorAttributes::new(-5, -3, 0, -1, -1, 0, 0, 0, 0); // 10 points invested

        assert!(
            level_5.max_health() > level_0.max_health(),
            "Level 5 should have more HP than level 0"
        );
        assert!(
            level_10.max_health() > level_5.max_health(),
            "Level 10 should have more HP than level 5"
        );
    }

    #[test]
    fn test_default_attrs_max_health_is_base() {
        // Level 0, no investment: max_health = base HP * multiplier(0) = base * 1.0
        let attrs = ActorAttributes::default();
        assert_eq!(attrs.total_level(), 0);
        assert_eq!(attrs.max_health(), crate::tuning::tuning().base_health, "Level 0 with no vitality should have the base health");
    }

    // ===== COMMITMENT TIER TESTS (, Layer 2) =====

    #[test]
    fn test_commitment_tier_thresholds() {
        // T0: below 20%
        assert_eq!(CommitmentTier::calculate(19, 100), CommitmentTier::T0);
        assert_eq!(CommitmentTier::calculate(0, 100), CommitmentTier::T0);

        // T1: 20% and above
        assert_eq!(CommitmentTier::calculate(20, 100), CommitmentTier::T1);
        assert_eq!(CommitmentTier::calculate(39, 100), CommitmentTier::T1);

        // T2: 40% and above
        assert_eq!(CommitmentTier::calculate(40, 100), CommitmentTier::T2);
        assert_eq!(CommitmentTier::calculate(59, 100), CommitmentTier::T2);

        // T3: 60% and above
        assert_eq!(CommitmentTier::calculate(60, 100), CommitmentTier::T3);
        assert_eq!(CommitmentTier::calculate(100, 100), CommitmentTier::T3);
    }

    #[test]
    fn test_commitment_tier_zero_budget() {
        // Zero total budget always returns T0
        assert_eq!(CommitmentTier::calculate(0, 0), CommitmentTier::T0);
        assert_eq!(CommitmentTier::calculate(50, 0), CommitmentTier::T0);
    }

    #[test]
    fn test_commitment_tier_ordering() {
        // Tiers are ordered T0 < T1 < T2 < T3
        assert!(CommitmentTier::T0 < CommitmentTier::T1);
        assert!(CommitmentTier::T1 < CommitmentTier::T2);
        assert!(CommitmentTier::T2 < CommitmentTier::T3);
    }

    #[test]
    fn test_commitment_tier_non_round_budget() {
        // Verify with non-round total budget values
        // 30 out of 73 = 41.1% → T2 (≥40%)
        assert_eq!(CommitmentTier::calculate(30, 73), CommitmentTier::T2);
        // 14 out of 73 = 19.2% → T0 (<20%)
        assert_eq!(CommitmentTier::calculate(14, 73), CommitmentTier::T0);
        // 15 out of 73 = 20.5% → T1 (≥20%)
        assert_eq!(CommitmentTier::calculate(15, 73), CommitmentTier::T1);
        // 44 out of 73 = 60.3% → T3 (≥60%)
        assert_eq!(CommitmentTier::calculate(44, 73), CommitmentTier::T3);
    }

    // ===== COMMITMENT_TIER_FOR TESTS (Layer 2) =====

    #[test]
    fn test_tier_of_convenience() {
        // Specialist build: heavy investment in one attribute
        // axis=-5, spectrum=0 → might=80, agility=0, ceiling=80
        // might commitment: 80/80 = 100% → T3
        let attrs = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T3);
        assert_eq!(attrs.tier(Attribute::Agility), CommitmentTier::T0);
    }

    #[test]
    fn test_tier_of_balanced_build() {
        // A balanced spectrum pays 6 a level to each side, against a ceiling
        // of 16 a level: every level in one pair is 54/144 = 37.5% → T1 both
        let committed = ActorAttributes::new(0, 9, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(committed.tier(Attribute::Might), CommitmentTier::T1);
        assert_eq!(committed.tier(Attribute::Agility), CommitmentTier::T1);

        // Spread evenly over all three pairs: 18/144 = 12.5% → T0 everywhere
        let spread = ActorAttributes::new(0, 3, 0, 0, 3, 0, 0, 3, 0);
        assert_eq!(spread.tier(Attribute::Might), CommitmentTier::T0);
        assert_eq!(spread.tier(Attribute::Resolve), CommitmentTier::T0);
    }

    #[test]
    fn test_commitment_tier_budget_constraints() {
        // T3+T2 takes every level: 6+4 axis at level 10, ceiling 160
        // might: 96/160 = 60% → T3; vitality: 64/160 = 40% → T2
        let attrs = ActorAttributes::new(-6, 0, 0, -4, 0, 0, 0, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T3);
        assert_eq!(attrs.tier(Attribute::Vitality), CommitmentTier::T2);

        // 5+5 axis: 80/160 = 50% each → T2, so two T3s are out of reach
        let split = ActorAttributes::new(-5, 0, 0, -5, 0, 0, 0, 0, 0);
        assert_eq!(split.tier(Attribute::Might), CommitmentTier::T2);
        assert_eq!(split.tier(Attribute::Vitality), CommitmentTier::T2);
    }

    #[test]
    fn test_commitment_tier_dual_t2() {
        // 4+4+2 axis at level 10, ceiling 160
        // might, vitality: 64/160 = 40% → T2; instinct: 32/160 = 20% → T1
        let attrs = ActorAttributes::new(-4, 0, 0, -4, 0, 0, -2, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T2);
        assert_eq!(attrs.tier(Attribute::Vitality), CommitmentTier::T2);
        assert_eq!(attrs.tier(Attribute::Instinct), CommitmentTier::T1);
    }

    // ===== SHIFT CONSTRAINT TESTS =====
    // Shift is constrained by axis direction: can only shift toward the side WITHOUT axis

    #[test]
    fn test_shift_constrained_when_axis_on_right_might_agility() {
        // axis=5 (agility/right side), spectrum=5
        // shift can only go negative (toward might/left): [-5, 0]
        let mut attrs = ActorAttributes::new(5, 5, 0, 0, 0, 0, 0, 0, 0);

        // Try to set positive shift (should clamp to 0)
        attrs.set_might_agility_shift(3);
        assert_eq!(attrs.might_agility_shift(), 0, "Positive shift should clamp to 0 when axis on right");

        // Set negative shift (should work)
        attrs.set_might_agility_shift(-3);
        assert_eq!(attrs.might_agility_shift(), -3, "Negative shift should work when axis on right");

        // Try to exceed max negative shift (should clamp to -spectrum)
        attrs.set_might_agility_shift(-10);
        assert_eq!(attrs.might_agility_shift(), -5, "Shift should clamp to -spectrum");
    }

    #[test]
    fn test_shift_constrained_when_axis_on_left_might_agility() {
        // axis=-5 (might/left side), spectrum=5
        // shift can only go positive (toward agility/right): [0, +5]
        let mut attrs = ActorAttributes::new(-5, 5, 0, 0, 0, 0, 0, 0, 0);

        // Try to set negative shift (should clamp to 0)
        attrs.set_might_agility_shift(-3);
        assert_eq!(attrs.might_agility_shift(), 0, "Negative shift should clamp to 0 when axis on left");

        // Set positive shift (should work)
        attrs.set_might_agility_shift(3);
        assert_eq!(attrs.might_agility_shift(), 3, "Positive shift should work when axis on left");

        // Try to exceed max positive shift (should clamp to +spectrum)
        attrs.set_might_agility_shift(10);
        assert_eq!(attrs.might_agility_shift(), 5, "Shift should clamp to +spectrum");
    }

    #[test]
    fn test_shift_constrained_vitality_discipline() {
        // Test same constraints for vitality/discipline pair
        let mut attrs = ActorAttributes::new(0, 0, 0, 3, 4, 0, 0, 0, 0);

        // axis=3 (discipline/right), shift can only go negative
        attrs.set_vitality_discipline_shift(2);
        assert_eq!(attrs.vitality_discipline_shift(), 0, "Positive shift should clamp when axis on right");

        attrs.set_vitality_discipline_shift(-2);
        assert_eq!(attrs.vitality_discipline_shift(), -2, "Negative shift should work when axis on right");
    }

    #[test]
    fn test_shift_constrained_instinct_resolve() {
        // Test same constraints for instinct/resolve pair
        let mut attrs = ActorAttributes::new(0, 0, 0, 0, 0, 0, -4, 3, 0);

        // axis=-4 (instinct/left), shift can only go positive
        attrs.set_instinct_resolve_shift(-2);
        assert_eq!(attrs.instinct_resolve_shift(), 0, "Negative shift should clamp when axis on left");

        attrs.set_instinct_resolve_shift(2);
        assert_eq!(attrs.instinct_resolve_shift(), 2, "Positive shift should work when axis on left");
    }

    #[test]
    fn test_shift_zero_allowed_regardless_of_axis() {
        // Shift=0 should always be valid regardless of axis direction
        let mut attrs = ActorAttributes::new(5, 5, 0, -3, 4, 0, 0, 6, 0);

        attrs.set_might_agility_shift(0);
        assert_eq!(attrs.might_agility_shift(), 0);

        attrs.set_vitality_discipline_shift(0);
        assert_eq!(attrs.vitality_discipline_shift(), 0);

        attrs.set_instinct_resolve_shift(0);
        assert_eq!(attrs.instinct_resolve_shift(), 0);
    }

    #[test]
    fn a_respec_fits_within_the_level_and_leans_no_further_than_its_pair_allows() {
        let level = 10;
        let spent = [Pair::new(-6, 2, 0), Pair::new(0, 1, 0), Pair::new(1, 0, 0)];
        assert!(ActorAttributes::fits(&spent, level));
        assert!(!ActorAttributes::fits(&[Pair::new(-6, 2, 0), Pair::new(0, 2, 0), Pair::new(1, 0, 0)], level), "a level too many");
        assert!(!ActorAttributes::fits(&[Pair { axis: 0, spectrum: -1, shift: 0 }, Pair::default(), Pair::default()], level), "a spectrum below nothing");

        assert!(ActorAttributes::is_complete(&spent, level));
        assert!(!ActorAttributes::is_complete(&[Pair::new(-6, 1, 0), Pair::new(0, 1, 0), Pair::new(1, 0, 0)], level), "a level left unplaced");

        let mut attrs = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        attrs.apply_respec([Pair::new(-6, 2, 5), Pair::new(0, 1, 1), Pair::new(1, 0, -3)]);
        assert_eq!(attrs.pairs(), [Pair::new(-6, 2, 2), Pair::new(0, 1, 0), Pair::new(1, 0, 0)], "each shift as far as its pair allows");
        assert_eq!(attrs.total_level(), level);
    }

    #[test]
    fn a_draft_keeps_the_level_so_its_tiers_answer_only_to_their_own_pair() {
        let attrs = ActorAttributes::new(-3, 0, 0, 7, 0, 0, 0, 0, 0);
        let mut draft = attrs;
        draft.apply_respec([Pair::new(-3, 0, 0), Pair::new(2, 0, 0), Pair::default()]);
        assert_eq!(draft.total_level(), attrs.total_level(), "levels taken out of one pair are still the actor's");
        assert_eq!(draft.ferocity(), attrs.ferocity(), "Might untouched, its tier holds");
        assert_eq!(draft.damage_level_multiplier(), attrs.damage_level_multiplier());
    }
}
