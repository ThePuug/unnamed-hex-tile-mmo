pub mod ally_target;
pub mod behaviour;
pub mod entity_type;
pub mod equipment;
pub mod heading;
pub mod keybits;
pub mod loaded_by;
pub mod movement_intent_state;
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
use crate::tuning::Tuning;

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
    fn test_distance_symmetric() {
        let a = Loc::new(Qrz { q: 2, r: -1, z: 5 });
        let b = Loc::new(Qrz { q: -3, r: 4, z: 1 });
        assert_eq!(a.distance(&b), b.distance(&a));
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

/// A commitment tier, T0 to T8: a tier for every 12% of the whole build
/// an attribute holds (`ActorAttributes::ceiling`). 12% is the least share
/// both an axis step (4%) and a spectrum step (3%) land on, so neither kind
/// of build overshoots a tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitmentTier(u8);

impl CommitmentTier {
    pub const T0: Self = Self(0);
    pub const T1: Self = Self(1);
    pub const T2: Self = Self(2);
    pub const T3: Self = Self(3);
    pub const T4: Self = Self(4);
    pub const T5: Self = Self(5);
    pub const T6: Self = Self(6);
    pub const T7: Self = Self(7);
    pub const T8: Self = Self(8);
    /// The highest tier: 96% of the whole build
    pub const TOP: u8 = 8;

    /// The tier `value` holds out of `whole`: one for every 12%, none with
    /// no whole. Where the tiers fall is fixed; only what each gives is tuned.
    pub fn calculate(value: u16, whole: u32) -> Self {
        if whole == 0 {
            return Self::T0;
        }
        Self(((value as u32 * 100) / (whole * 12)).min(Self::TOP as u32) as u8)
    }

    /// How deep this tier holds `unlock`: None below the tier it opens at,
    /// 0 there, one more for each tier after, no deeper than its last rung
    pub fn rung(self, unlock: Unlock) -> Option<usize> {
        self.0.checked_sub(unlock.opens()).map(|past| past.min(unlock.rungs() - 1) as usize)
    }

    /// `values`, one for each rung of `unlock`, at this tier: None below it
    pub fn at<T: Copy, const N: usize>(self, unlock: Unlock, values: [T; N]) -> Option<T> {
        self.rung(unlock).map(|rung| values[rung.min(N - 1)])
    }

    /// The tier's place, 0 to 8
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The three unlocks of a commitment's ladder, each a mechanic of its own
/// and each deepened by the tiers after it until the next opens: the core
/// at T1 (T2 its depth), the facet at T3 (T4-T5) and the capstone at T6
/// (T7-T8). A capstone costs 18 of a build's 25 steps, so no build holds two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlock {
    Core,
    Facet,
    Capstone,
}

impl Unlock {
    /// The tier it opens at
    const fn opens(self) -> u8 {
        match self {
            Self::Core => 1,
            Self::Facet => 3,
            Self::Capstone => 6,
        }
    }

    /// How many tiers it holds before the next opens or the ladder ends
    const fn rungs(self) -> u8 {
        match self {
            Self::Core => 2,
            Self::Facet | Self::Capstone => 3,
        }
    }
}

/// The six attributes, two to a pair. Each is read three ways, by one rule
/// apiece: its value, which contests weigh ([`ActorAttributes::value`]);
/// its pair's share, which the pair's absolute reads ([`ActorAttributes::share`]); and
/// its commitment tier ([`ActorAttributes::tier`]). The stat each reading
/// goes by has a method of its name there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attribute {
    Might,
    Agility,
    Physique,
    Discipline,
    Instinct,
    Resolve,
}

/// One pair of opposed attributes, as the steps of the build put into it.
/// `axis` commits to one of the two: negative the left, positive the right.
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
    /// What a step of axis gives the attribute it commits to, in halves of
    /// a percent of the whole build: 4%
    const AXIS: i16 = 8;
    /// What a step of spectrum gives the committed attribute, and what a
    /// step of shift moves from it to the other: 3%
    const SPECTRUM: i16 = 6;
    /// What a step of spectrum gives each attribute of a pair with no axis: 1.5%
    const BALANCED: i16 = 3;

    pub fn new(axis: i8, spectrum: i8, shift: i8) -> Self {
        Self { axis, spectrum: spectrum.max(0), shift }
    }

    /// The steps put into the pair; a shift costs none
    pub fn steps(self) -> u32 {
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

/// What an actor has put its build's steps into: three pairs of opposed
/// attributes, Might and Agility, Physique and Discipline, Instinct and
/// Resolve. Every value an actor fights with is read from these through
/// the methods here, each a share of the whole build ([`Self::ceiling`]),
/// never of the level.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Serialize)]
#[require(Swing)]
pub struct ActorAttributes {
    might_agility: Pair,
    physique_discipline: Pair,
    instinct_resolve: Pair,
    /// The actor's level, which a respec never changes: it holds a step of
    /// the build for each, up to [`Self::STEPS`]
    level: u32,
}

impl ActorAttributes {
    /// The steps a whole build holds: a character gains one a level up to
    /// it, and from then on a level leaves every share as it was
    pub const STEPS: u32 = 25;

    /// An actor's attributes from the steps in each pair: axis, spectrum
    /// and shift of Might ↔ Agility, of Physique ↔ Discipline, then of
    /// Instinct ↔ Resolve. A shift is taken as given, unclamped. Its level
    /// is the steps these put in ([`Self::at_level`] for a higher one).
    pub fn new(
        might_agility_axis: i8,
        might_agility_spectrum: i8,
        might_agility_shift: i8,
        physique_discipline_axis: i8,
        physique_discipline_spectrum: i8,
        physique_discipline_shift: i8,
        instinct_resolve_axis: i8,
        instinct_resolve_spectrum: i8,
        instinct_resolve_shift: i8,
    ) -> Self {
        let pairs = [
            Pair::new(might_agility_axis, might_agility_spectrum, might_agility_shift),
            Pair::new(physique_discipline_axis, physique_discipline_spectrum, physique_discipline_shift),
            Pair::new(instinct_resolve_axis, instinct_resolve_spectrum, instinct_resolve_shift),
        ];
        let [might_agility, physique_discipline, instinct_resolve] = pairs;
        Self { might_agility, physique_discipline, instinct_resolve, level: Self::invested(&pairs) }
    }

    /// The same build at `level`, never below the steps it places
    pub fn at_level(self, level: u32) -> Self {
        Self { level: level.max(Self::invested(&self.pairs())), ..self }
    }

    /// The steps an actor of `level` holds: one a level, up to [`Self::STEPS`]
    pub fn held(level: u32) -> u32 {
        level.min(Self::STEPS)
    }

    pub fn might_agility_axis(&self) -> i8 { self.might_agility.axis }
    pub fn might_agility_spectrum(&self) -> i8 { self.might_agility.spectrum }
    pub fn might_agility_shift(&self) -> i8 { self.might_agility.shift }

    pub fn physique_discipline_axis(&self) -> i8 { self.physique_discipline.axis }
    pub fn physique_discipline_spectrum(&self) -> i8 { self.physique_discipline.spectrum }
    pub fn physique_discipline_shift(&self) -> i8 { self.physique_discipline.shift }

    pub fn instinct_resolve_axis(&self) -> i8 { self.instinct_resolve.axis }
    pub fn instinct_resolve_spectrum(&self) -> i8 { self.instinct_resolve.spectrum }
    pub fn instinct_resolve_shift(&self) -> i8 { self.instinct_resolve.shift }

    pub fn set_might_agility_shift(&mut self, shift: i8) { self.might_agility.set_shift(shift) }
    pub fn set_physique_discipline_shift(&mut self, shift: i8) { self.physique_discipline.set_shift(shift) }
    pub fn set_instinct_resolve_shift(&mut self, shift: i8) { self.instinct_resolve.set_shift(shift) }

    /// The three pairs: Might ↔ Agility, Physique ↔ Discipline, Instinct ↔ Resolve
    pub fn pairs(&self) -> [Pair; 3] {
        [self.might_agility, self.physique_discipline, self.instinct_resolve]
    }

    /// The steps `pairs` put in
    pub fn invested(pairs: &[Pair; 3]) -> u32 {
        pairs.iter().map(|pair| pair.steps()).sum()
    }

    /// Whether `pairs` is a draft an actor of `level` may hold: no more
    /// steps than it holds ([`Self::held`]), and no spectrum below nothing.
    /// A respec it takes has placed every one (`is_complete`).
    pub fn fits(pairs: &[Pair; 3], level: u32) -> bool {
        pairs.iter().all(|pair| pair.spectrum >= 0) && Self::invested(pairs) <= Self::held(level)
    }

    /// Whether `pairs` is a respec an actor of `level` may take: a draft
    /// that `fits` and places every step it holds.
    pub fn is_complete(pairs: &[Pair; 3], level: u32) -> bool {
        Self::fits(pairs, level) && Self::invested(pairs) == Self::held(level)
    }

    /// Takes a whole respec, or lays a draft over a copy to show it: each
    /// pair's axis and spectrum as given, its shift as far as the pair
    /// allows, and the level as it was.
    pub fn apply_respec(&mut self, pairs: [Pair; 3]) {
        for (own, respec) in [&mut self.might_agility, &mut self.physique_discipline, &mut self.instinct_resolve].into_iter().zip(pairs) {
            *own = Pair::new(respec.axis, respec.spectrum, 0);
            own.set_shift(respec.shift);
        }
    }

    fn pair(&self, attribute: Attribute) -> (Pair, End) {
        match attribute {
            Attribute::Might => (self.might_agility, End::Left),
            Attribute::Agility => (self.might_agility, End::Right),
            Attribute::Physique => (self.physique_discipline, End::Left),
            Attribute::Discipline => (self.physique_discipline, End::Right),
            Attribute::Instinct => (self.instinct_resolve, End::Left),
            Attribute::Resolve => (self.instinct_resolve, End::Right),
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

    /// The points `attribute`'s pair holds, its own and its partner's
    /// together: what the pair's absolute reads, wherever the build leans
    /// within it
    pub fn pair_points(&self, attribute: Attribute) -> u16 {
        let (pair, _) = self.pair(attribute);
        pair.value(End::Left) + pair.value(End::Right)
    }

    /// `attribute`'s commitment tier: its value as a share of the whole
    /// build (`ceiling`)
    pub fn tier(&self, attribute: Attribute) -> CommitmentTier {
        CommitmentTier::calculate(self.value(attribute), self.ceiling())
    }

    /// The whole build, every step in one axis: what every share is out of,
    /// the same at every level
    pub fn ceiling(&self) -> u32 {
        Self::STEPS * Pair::AXIS as u32
    }

    // Each attribute by name, and the most any shift could make it

    pub fn might(&self) -> u16 { self.value(Attribute::Might) }
    pub fn agility(&self) -> u16 { self.value(Attribute::Agility) }
    pub fn physique(&self) -> u16 { self.value(Attribute::Physique) }
    pub fn discipline(&self) -> u16 { self.value(Attribute::Discipline) }
    pub fn instinct(&self) -> u16 { self.value(Attribute::Instinct) }
    pub fn resolve(&self) -> u16 { self.value(Attribute::Resolve) }

    pub fn might_reach(&self) -> u16 { self.reach(Attribute::Might) }
    pub fn agility_reach(&self) -> u16 { self.reach(Attribute::Agility) }
    pub fn physique_reach(&self) -> u16 { self.reach(Attribute::Physique) }
    pub fn discipline_reach(&self) -> u16 { self.reach(Attribute::Discipline) }
    pub fn instinct_reach(&self) -> u16 { self.reach(Attribute::Instinct) }
    pub fn resolve_reach(&self) -> u16 { self.reach(Attribute::Resolve) }

    // Absolute, by the name its stat goes by; Force is Might and Agility's
    // share (`auto_damage`)

    /// Endurance, Instinct and Resolve's: how many times deeper than an
    /// uninvested one its pool runs (`max_endurance`), up to
    /// `Tuning::endurance_depth` more for a pair holding the whole build
    /// (`ceiling`)
    pub fn endurance(&self, tuning: &Tuning) -> f32 {
        1.0 + tuning.endurance_depth * self.pair_share(Attribute::Instinct)
    }

    /// Constitution, Physique and Discipline's, which is max health:
    /// `Tuning::base_health`, up to `Tuning::health_depth` more for a pair
    /// holding the whole build, the same at every level
    pub fn constitution(&self, tuning: &Tuning) -> f32 {
        tuning.base_health * (1.0 + tuning.health_depth * self.pair_share(Attribute::Physique))
    }

    pub fn max_health(&self, tuning: &Tuning) -> f32 {
        self.constitution(tuning)
    }

    // Relative: the value a contest weighs, by the name it goes by there

    /// Impact, Might: pushes a target's recovery back, against its Efficiency
    pub fn impact(&self) -> u16 { self.value(Attribute::Might) }
    /// Efficiency, Discipline: unlocks its combos sooner, against the
    /// target's Impact (`combos::recovery_after`)
    pub fn efficiency(&self) -> u16 { self.value(Attribute::Discipline) }
    /// Tempo, Agility: brings its auto-attacks sooner, against the target's
    /// Reflex (`cadence_interval`)
    pub fn tempo(&self) -> u16 { self.value(Attribute::Agility) }
    /// Reflex, Instinct: widens a threat's window, against the attacker's Tempo
    pub fn reflex(&self) -> u16 { self.value(Attribute::Instinct) }
    /// Fitness, Physique: runs its own recovery faster, against the
    /// opponent's Focus
    pub fn fitness(&self) -> u16 { self.value(Attribute::Physique) }
    /// Focus, Resolve: decides whether a blow crits, against the defender's
    /// Fitness (`damage::crit_chance`)
    pub fn focus(&self) -> u16 { self.value(Attribute::Resolve) }

    // Commitment: an attribute's tier, by the name it goes by

    /// Ferocity, Might: combos fired early (`early_combos`)
    pub fn ferocity(&self) -> CommitmentTier { self.tier(Attribute::Might) }
    /// Grace, Agility: the flank, the arc and the stride (`flank`, `arc`, `flank_stride`)
    pub fn grace(&self) -> CommitmentTier { self.tier(Attribute::Agility) }
    /// Intimidation, Physique: its zone (`intimidation_pace`, `intimidation_toll`, `intimidation_zone`)
    pub fn intimidation(&self) -> CommitmentTier { self.tier(Attribute::Physique) }
    /// Preparation, Discipline: reactions fired early (`early_reactions`, `slip`)
    pub fn preparation(&self) -> CommitmentTier { self.tier(Attribute::Discipline) }
    /// Patience, Instinct: each attack at it overcommits its attacker
    /// (`patience_crit`, `patience_power`, `patience_opening`)
    pub fn patience(&self) -> CommitmentTier { self.tier(Attribute::Instinct) }
    /// Awareness, Resolve: its band (`span`, `awareness_refund`, `awareness_snap`)
    pub fn awareness(&self) -> CommitmentTier { self.tier(Attribute::Resolve) }

    /// The actor's level, whether or not a draft laid over it has placed
    /// every step it holds
    pub fn total_level(&self) -> u32 {
        self.level
    }

    /// Movement speed: the same for every actor, no attribute governing it
    pub fn movement_speed(&self) -> f32 {
        crate::systems::movement::MOVEMENT_SPEED
    }

    /// The potency every actor has, the same at every level: what a skill's
    /// and an auto-attack's damage are shares of. Level counts where a blow
    /// lands (`damage::level_factor`).
    pub fn base_potency(&self, tuning: &Tuning) -> f32 {
        tuning.potency_base
    }

    /// `attribute`'s pair's share of the whole build (`ceiling`), both its
    /// attributes together: what each absolute reads, evenly, so every step
    /// into a pair adds as much as the one before
    pub fn pair_share(&self, attribute: Attribute) -> f32 {
        self.pair_points(attribute) as f32 / self.ceiling() as f32
    }

    /// The endurance pool: `Tuning::endurance_pool`, deeper by Endurance
    pub fn max_endurance(&self, tuning: &Tuning) -> f32 {
        tuning.endurance_pool * self.endurance(tuning)
    }

    /// The attribute line `ability` belongs to, whose points raise it: none
    /// for the auto-attack and the skills every fighter holds alike, Feint
    /// and Parry.
    pub fn line(ability: crate::message::AbilityType) -> Option<Attribute> {
        use crate::message::AbilityType::*;
        match ability {
            Frenzy => Some(Attribute::Might),
            Overpower => Some(Attribute::Physique),
            PerfectStride => Some(Attribute::Agility),
            Punish => Some(Attribute::Instinct),
            Leap => Some(Attribute::Discipline),
            Counter => Some(Attribute::Resolve),
            AutoAttack | Feint | Parry => None,
        }
    }

    /// The share of its own numbers `ability` has for this actor: whole for
    /// a skill of no line; for one of a line, `Tuning::line` with no share
    /// in its attribute, rising evenly to whole as the attribute's share of
    /// the whole build (`ceiling`) nears whole, so a skill is weak in a
    /// build that has not invested in it. What it scales is the
    /// skill's own: a strike's damage, Counter's reflection, Perfect
    /// Stride's speed and a Leap's distance.
    pub fn line_power(&self, tuning: &Tuning, ability: crate::message::AbilityType) -> f32 {
        let Some(attribute) = Self::line(ability) else { return 1.0 };
        let invested = if self.ceiling() == 0 { 0.0 } else { (self.value(attribute) as f32 / self.ceiling() as f32).min(1.0) };
        let floor = tuning.line(ability);
        floor + (1.0 - floor) * invested
    }

    /// Tiles this actor's Leap carries it: `Tuning::leap_distance` as its
    /// Discipline line has it, never less than one
    pub fn leap_tiles(&self, tuning: &Tuning) -> usize {
        let tiles = tuning.leap_distance as f32 * self.line_power(tuning, crate::message::AbilityType::Leap);
        tiles.round().max(1.0) as usize
    }

    /// The endurance `ability`, a skill, costs this actor: its flat price
    /// (`Tuning::cost`), whatever the build and level. A reaction pays it
    /// whatever it clears.
    pub fn skill_endurance(&self, tuning: &Tuning, ability: crate::message::AbilityType) -> f32 {
        tuning.cost(ability)
    }

    /// An auto-attack's damage: `Tuning::auto_damage` of base potency, more by
    /// `Tuning::force_auto` of Force, Might and Agility's share
    pub fn auto_damage(&self, tuning: &Tuning) -> f32 {
        self.base_potency(tuning) * tuning.auto_damage * (1.0 + tuning.force_auto * self.pair_share(Attribute::Might))
    }

    /// Combos it may fire early in a chain (`combos::timing`): one at
    /// Ferocity's core, two at its depth, and a third at its capstone
    pub fn early_combos(&self) -> usize {
        let ferocity = self.ferocity();
        ferocity.at(Unlock::Core, [1, 2]).unwrap_or(0) + ferocity.at(Unlock::Capstone, [1]).unwrap_or(0)
    }

    /// Reactions it may fire early in a chain once a strike taken in its own
    /// time stands in it (`combos::timing`): one at Preparation's core, two
    /// at its depth
    pub fn early_reactions(&self) -> usize {
        self.preparation().at(Unlock::Core, [1, 2]).unwrap_or(0)
    }

    /// Share less of the time it skipped the `nth` skill fired early in a
    /// chain owes (from 0), fired early by the commitment of `by`: its facet,
    /// or for Ferocity's third combo its capstone; none without them
    pub fn early_discount(&self, tuning: &Tuning, by: Attribute, nth: usize) -> f32 {
        let tier = self.tier(by);
        let third = if by == Attribute::Might && nth == 2 { tier.at(Unlock::Capstone, tuning.ferocity_third) } else { None };
        third.or_else(|| tier.at(Unlock::Facet, tuning.early_facet)).unwrap_or(0.0)
    }

    /// Tiles an early reaction carries it: Preparation's capstone; None
    /// below it
    pub fn slip(&self, tuning: &Tuning) -> Option<usize> {
        self.preparation().at(Unlock::Capstone, tuning.preparation_slip)
    }

    /// Share of its speed a foe in its zone keeps: Intimidation's core;
    /// None with no Intimidation, which has no zone
    pub fn intimidation_pace(&self, tuning: &Tuning) -> Option<f32> {
        self.intimidation().at(Unlock::Core, tuning.intimidation_pace)
    }

    /// Share more each skill costs a foe in its zone: Intimidation's facet
    pub fn intimidation_toll(&self, tuning: &Tuning) -> f32 {
        self.intimidation().at(Unlock::Facet, tuning.intimidation_toll).unwrap_or(0.0)
    }

    /// Tiles past its reach its zone reaches, a zone that pins a foe in it:
    /// Intimidation's capstone; None below it, where the zone is its reach
    /// and pins nothing
    pub fn intimidation_zone(&self, tuning: &Tuning) -> Option<i32> {
        self.intimidation().at(Unlock::Capstone, tuning.intimidation_zone)
    }

    /// The share harder this actor's strikes land from past their target's
    /// forward faces, a flank: Grace's core
    pub fn flank(&self, tuning: &Tuning) -> f32 {
        self.grace().at(Unlock::Core, tuning.grace_flank).unwrap_or(0.0)
    }

    /// Share of its speed a flank strike of this actor's leaves its target
    /// for a swing, its stride broken: Grace's capstone; None below it
    pub fn flank_stride(&self, tuning: &Tuning) -> Option<f32> {
        self.grace().at(Unlock::Capstone, tuning.grace_stride)
    }

    /// The share likelier this actor's skills crit a foe for each stack of
    /// Overcommitted on it (`Status::overcommitted`): Patience's core
    pub fn patience_crit(&self, tuning: &Tuning) -> f32 {
        self.patience().at(Unlock::Core, tuning.patience_crit).unwrap_or(0.0)
    }

    /// The share harder this actor's crits land on an overcommitted foe:
    /// Patience's facet
    pub fn patience_power(&self, tuning: &Tuning) -> f32 {
        self.patience().at(Unlock::Facet, tuning.patience_power).unwrap_or(0.0)
    }

    /// The stacks of Overcommitted on a foe at which this actor's next skill
    /// on it crits for certain and spends them: Patience's capstone; None
    /// below it
    pub fn patience_opening(&self, tuning: &Tuning) -> Option<usize> {
        self.patience().at(Unlock::Capstone, tuning.patience_opening)
    }

    /// How wide this actor's band is: `Tuning::awareness_band` for every
    /// actor, wider by Awareness's core. A reaction takes every threat
    /// landing within this long after it is pressed (`QueuedThreat::in_band`),
    /// so a wider band makes a reaction easier to time.
    pub fn span(&self, tuning: &Tuning) -> std::time::Duration {
        std::time::Duration::from_secs_f32(self.awareness().at(Unlock::Core, tuning.awareness_core).unwrap_or(tuning.awareness_band))
    }

    /// Share of a reaction's price each threat it takes past the first pays
    /// back: Awareness's facet
    pub fn awareness_refund(&self, tuning: &Tuning) -> f32 {
        self.awareness().at(Unlock::Facet, tuning.awareness_refund).unwrap_or(0.0)
    }

    /// How soon after a press its nearest incoming threat may land and draw
    /// the band to start at it (`ReactionQueue::band`), so the band reaches
    /// that much deeper: Awareness's capstone; None below it, where the band
    /// starts at the press
    pub fn awareness_snap(&self, tuning: &Tuning) -> Option<std::time::Duration> {
        self.awareness().at(Unlock::Capstone, tuning.awareness_snap).map(std::time::Duration::from_secs_f32)
    }

    /// The half-angle either side of its heading this actor strikes within:
    /// `Tuning::grace_arc`, the three forward faces, with no Grace, wider by
    /// Grace's facet. A strike past the forward faces breaks its stride
    /// (`targeting::across`).
    pub fn arc(&self, tuning: &Tuning) -> f32 {
        self.grace().at(Unlock::Facet, tuning.grace_arc_facet).unwrap_or(tuning.grace_arc)
    }

    /// Seconds between auto-attacks at `against`: `Tuning::base_interval`,
    /// the one pace every actor starts from, quickened toward
    /// `Tuning::tempo_ceiling` by its Tempo over its target's Reflex, the
    /// level gap weighing in; with no target, over none. No actor swings
    /// slower than the base.
    pub fn cadence_interval(&self, tuning: &Tuning, against: Option<&ActorAttributes>) -> std::time::Duration {
        use crate::systems::combat::damage::{contest_factor, level_edge};
        let (reflex, edge) = against.map_or((0, 0.0), |foe| (foe.reflex(), level_edge(tuning, self.total_level(), foe.total_level())));
        let contest = contest_factor(tuning, self.tempo(), reflex, edge);
        std::time::Duration::from_secs_f32(tuning.base_interval / (1.0 + tuning.tempo_ceiling * contest))
    }
}

/// When an actor's next auto-attack comes due, as the server counts it:
/// an interval after its last, or the moment its fight found it not yet
/// swinging. A due swing waits for a target it can strike. None
/// disengaged: a swing is due, and nothing waits until an engagement
/// starts the clock.
#[derive(Clone, Component, Copy, Debug, Default)]
pub struct Swing {
    pub due: Option<crate::moment::Moment>,
}

impl Swing {
    /// How long the due swing has waited at `now`; None while the next is
    /// still to come due. Out of combat it is due and has waited no time.
    pub fn waited(&self, now: crate::moment::Moment) -> Option<std::time::Duration> {
        match self.due {
            Some(due) => (now >= due).then(|| now.since(due)),
            None => Some(std::time::Duration::ZERO),
        }
    }
}

/// Auto-attack range in hex tiles: 2, melee reach, so a blow lands on a
/// target a step away as well as one beside it. Every actor's is the
/// default; nothing sets another.
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

    #[test]
    fn a_pair_share_rises_evenly_to_whole() {
        let share = |steps: i8| ActorAttributes::new(-steps, 0, 0, 0, 0, 0, 0, 0, 0).pair_share(Attribute::Might);
        assert_eq!(share(0), 0.0, "none invested, none of the build");
        assert_eq!(share(10) - share(5), share(5) - share(0), "each step adds as much as the last");
        assert_eq!(share(ActorAttributes::STEPS as i8), 1.0, "the whole build in the pair is whole");
    }

    #[test]
    fn endurance_deepens_its_own_pool_and_a_skill_costs_by_its_own_cost() {
        let tuning = Tuning::DEFAULT;
        let disciplined = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let mighty = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert!(disciplined.max_endurance(&tuning) > mighty.max_endurance(&tuning), "Instinct and Resolve deepen it");
        assert_eq!(disciplined.max_endurance(&tuning), ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0).max_endurance(&tuning), "either of the pair, alike");
        assert_eq!(mighty.max_endurance(&tuning), plain.max_endurance(&tuning), "and level does not");
        assert_eq!(plain.max_endurance(&tuning), tuning.endurance_pool);
        use crate::message::AbilityType::{Counter, Frenzy};
        assert_eq!(disciplined.skill_endurance(&tuning, Frenzy), mighty.skill_endurance(&tuning, Frenzy), "a skill costs the same in any build");
        assert_eq!(mighty.skill_endurance(&tuning, Frenzy), plain.skill_endurance(&tuning, Frenzy), "and at any level");

        let (instinctive, resolute) = (ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0), ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0));
        assert_eq!(instinctive.skill_endurance(&tuning, Frenzy), mighty.skill_endurance(&tuning, Frenzy), "a skill costs what its cost does, whatever the build");
        assert_eq!(resolute.skill_endurance(&tuning, Counter), mighty.skill_endurance(&tuning, Counter));
        use crate::message::AbilityType::{Feint, Overpower};
        assert!(plain.skill_endurance(&tuning, Feint) < plain.skill_endurance(&tuning, Overpower), "a cheap skill is cheap in endurance");
    }

    #[test]
    fn a_skill_is_raised_by_its_line_and_the_shared_ones_by_none() {
        let tuning = Tuning::DEFAULT;
        use crate::message::AbilityType::*;
        let mighty = ActorAttributes::new(-25, 0, 0, 0, 0, 0, 0, 0, 0);
        let instinctive = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(mighty.line_power(&tuning, Frenzy), 1.0, "a whole build in Might bites whole");
        assert_eq!(instinctive.line_power(&tuning, Frenzy), tuning.line(Frenzy), "with none, its floor");
        assert!(instinctive.line_power(&tuning, Frenzy) < 1.0, "a skill out of its line is weak");
        assert!(ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0).leap_tiles(&tuning) > mighty.leap_tiles(&tuning), "Discipline leaps further");
        for shared in [Feint, Parry, AutoAttack] {
            assert_eq!(mighty.line_power(&tuning, shared), 1.0, "{shared:?} belongs to no line");
            assert_eq!(instinctive.line_power(&tuning, shared), 1.0, "{shared:?} belongs to no line");
        }
        let half = ActorAttributes::new(-5, 0, 0, -5, 0, 0, 0, 0, 0);
        assert!(half.line_power(&tuning, Frenzy) > instinctive.line_power(&tuning, Frenzy) && half.line_power(&tuning, Frenzy) < 1.0, "between, by how much it has invested");
        assert_eq!(plain.line_power(&tuning, Frenzy), tuning.line(Frenzy), "a build of no level has invested nothing");
    }

    #[test]
    fn tempo_quickens_the_swing_and_no_one_is_slower_than_the_base() {
        let tuning = Tuning::DEFAULT;
        let base = std::time::Duration::from_secs_f32(tuning.base_interval);
        let quick = |points: i8| ActorAttributes::new(points, 0, 0, 0, 0, 0, 0, 0, 0).cadence_interval(&tuning, None);
        assert_eq!(ActorAttributes::default().cadence_interval(&tuning, None), base);
        assert_eq!(ActorAttributes::new(-10, 0, 0, -10, 0, 0, 0, 0, 0).cadence_interval(&tuning, None), base, "no other attribute changes the pace");
        assert!(quick(5) < base && quick(10) < quick(5), "more Agility, a faster swing");
        let reading = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        assert_eq!(ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0).cadence_interval(&tuning, Some(&reading)), base, "a foe's Reflex that matches it nullifies it");
    }

    #[test]
    fn grace_widens_the_arc_to_all_but_straight_behind() {
        let tuning = Tuning::DEFAULT;
        let graceful = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(plain.arc(&tuning), crate::systems::targeting::STRIDE_ARC, "no Grace, the forward faces");
        assert!(graceful.arc(&tuning) > plain.arc(&tuning) && graceful.arc(&tuning) < 180.0, "full commitment still cannot strike straight behind");
    }

    #[test]
    fn awareness_lengthens_the_span_and_every_actor_has_the_least() {
        let tuning = Tuning::DEFAULT;
        let aware = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let plain = ActorAttributes::default();
        assert!(plain.span(&tuning) > std::time::Duration::ZERO, "no Awareness, still a span");
        assert!(aware.span(&tuning) > plain.span(&tuning));
    }

    #[test]
    fn patience_crits_an_overcommitted_foe_likelier_then_harder_then_for_certain() {
        let tuning = Tuning::DEFAULT;
        let patient = |steps: i8| ActorAttributes::new(0, 0, 0, 0, 0, 0, -steps, 0, 0);
        assert_eq!(ActorAttributes::default().patience_crit(&tuning), 0.0, "without Patience, nothing");
        assert!(patient(3).patience_crit(&tuning) > 0.0 && patient(3).patience_power(&tuning) == 0.0, "the core first");
        assert!(patient(9).patience_power(&tuning) > 0.0 && patient(9).patience_opening(&tuning).is_none(), "then the facet");
        assert!(patient(18).patience_opening(&tuning).is_some(), "then the capstone");
        assert!(patient(24).patience_opening(&tuning) < patient(18).patience_opening(&tuning), "its depth opens sooner");
    }

    #[test]
    fn force_strengthens_auto_attacks() {
        let tuning = Tuning::DEFAULT;
        let might = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let vital = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        assert!(might.auto_damage(&tuning) > vital.auto_damage(&tuning));
        assert_eq!(vital.auto_damage(&tuning), vital.base_potency(&tuning) * tuning.auto_damage);
        assert_eq!(ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0).auto_damage(&tuning), might.auto_damage(&tuning), "Agility's points count as Might's");
    }

    #[test]
    fn health_grows_with_physique_and_discipline_never_with_level() {
        let tuning = Tuning::DEFAULT;
        let plain = ActorAttributes::default();
        assert_eq!(plain.max_health(&tuning), tuning.base_health, "nothing invested, the base");
        let mighty = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(mighty.max_health(&tuning), plain.max_health(&tuning), "level alone adds none");
        let vital = ActorAttributes::new(0, 0, 0, -10, 0, 0, 0, 0, 0);
        assert!(vital.max_health(&tuning) > plain.max_health(&tuning));
        assert_eq!(vital.max_health(&tuning), ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0).max_health(&tuning), "either of the pair, alike");
    }

    // ===== COMMITMENT TIER TESTS =====

    #[test]
    fn a_tier_comes_every_twelve_percent_of_the_whole_build() {
        let whole = 200;
        assert_eq!(CommitmentTier::calculate(0, whole), CommitmentTier::T0);
        assert_eq!(CommitmentTier::calculate(23, whole), CommitmentTier::T0, "just short of 12%");
        assert_eq!(CommitmentTier::calculate(24, whole), CommitmentTier::T1);
        assert_eq!(CommitmentTier::calculate(120, whole), CommitmentTier::T5, "60%");
        assert_eq!(CommitmentTier::calculate(192, whole), CommitmentTier::T8, "96%");
        assert_eq!(CommitmentTier::calculate(200, whole), CommitmentTier::T8, "and no higher");
        assert_eq!(CommitmentTier::calculate(50, 0), CommitmentTier::T0, "no whole, no tier");
    }

    #[test]
    fn an_axis_and_a_spectrum_both_land_on_every_tier() {
        // An axis reaches a tier in 3 steps, a spectrum with its axis set in 4
        for tier in 1..=6u8 {
            let axis = ActorAttributes::new(-(3 * tier as i8), 0, 0, 0, 0, 0, 0, 0, 0);
            assert_eq!(axis.tier(Attribute::Might).index(), tier as usize, "axis to T{tier}");
            let spectrum = ActorAttributes::new(-1, 4 * tier as i8, 0, 0, 0, 0, 0, 0, 0);
            let without = ActorAttributes::new(-1, 0, 0, 0, 0, 0, 0, 0, 0);
            assert_eq!(spectrum.might() - without.might(), 24 * tier as u16, "spectrum lands T{tier}'s share exactly");
        }
    }

    #[test]
    fn a_ladder_opens_its_core_facet_and_capstone_at_t1_t3_and_t6() {
        let tier = |t: u8| CommitmentTier::calculate(24 * t as u16, 200);
        let rungs = |unlock| (0..=CommitmentTier::TOP).map(|t| tier(t).rung(unlock)).collect::<Vec<_>>();
        assert_eq!(rungs(Unlock::Core), [None, Some(0), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1)]);
        assert_eq!(rungs(Unlock::Facet), [None, None, None, Some(0), Some(1), Some(2), Some(2), Some(2), Some(2)]);
        assert_eq!(rungs(Unlock::Capstone), [None, None, None, None, None, None, Some(0), Some(1), Some(2)]);
        assert_eq!(tier(4).at(Unlock::Facet, [1, 2, 3]), Some(2), "a value for each rung");
    }

    #[test]
    fn no_build_holds_two_capstones() {
        // A capstone takes 18 axis steps; a second would take 36 of 25
        let attributes = [Attribute::Might, Attribute::Agility, Attribute::Physique, Attribute::Discipline, Attribute::Instinct, Attribute::Resolve];
        let steps = ActorAttributes::STEPS as i8;
        for split in 0..=steps {
            let attrs = ActorAttributes::new(-split, 0, 0, -(steps - split), 0, 0, 0, 0, 0);
            let capstones = attributes.iter().filter(|&&a| attrs.tier(a).rung(Unlock::Capstone).is_some()).count();
            assert!(capstones <= 1, "{split}");
        }
    }

    #[test]
    fn each_commitment_climbs_its_ladder() {
        let tuning = Tuning::DEFAULT;
        let might = |steps: i8| ActorAttributes::new(-steps, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!([0, 3, 6, 18].map(|s| might(s).early_combos()), [0, 1, 2, 3], "Ferocity: a combo, a second, a third");
        assert!(might(9).early_discount(&tuning, Attribute::Might, 0) > 0.0 && might(6).early_discount(&tuning, Attribute::Might, 0) == 0.0);
        assert!(might(18).early_discount(&tuning, Attribute::Might, 2) > might(18).early_discount(&tuning, Attribute::Might, 0), "the third owes least");
        let agile = |steps: i8| ActorAttributes::new(steps, 0, 0, 0, 0, 0, 0, 0, 0);
        assert!(agile(3).flank(&tuning) > 0.0 && agile(3).arc(&tuning) == tuning.grace_arc);
        assert!(agile(9).arc(&tuning) > tuning.grace_arc && agile(9).flank_stride(&tuning).is_none());
        assert!(agile(18).flank_stride(&tuning).is_some());
        let physique = |steps: i8| ActorAttributes::new(0, 0, 0, -steps, 0, 0, 0, 0, 0);
        assert!(physique(0).intimidation_pace(&tuning).is_none() && physique(3).intimidation_pace(&tuning).is_some());
        assert!(physique(9).intimidation_toll(&tuning) > 0.0 && physique(9).intimidation_zone(&tuning).is_none());
        assert!(physique(18).intimidation_zone(&tuning).is_some());
        let discipline = |steps: i8| ActorAttributes::new(0, 0, 0, steps, 0, 0, 0, 0, 0);
        assert_eq!([0, 3, 6, 24].map(|s| discipline(s).early_reactions()), [0, 1, 2, 2], "Preparation: two early reactions at most");
        assert!(discipline(18).slip(&tuning).is_some() && discipline(15).slip(&tuning).is_none());
        let resolve = |steps: i8| ActorAttributes::new(0, 0, 0, 0, 0, 0, steps, 0, 0);
        assert!(resolve(6).span(&tuning) > resolve(3).span(&tuning) && resolve(3).span(&tuning) > resolve(0).span(&tuning));
        assert_eq!(resolve(24).span(&tuning), resolve(6).span(&tuning), "the band grows at the core only");
        assert!(resolve(9).awareness_refund(&tuning) > 0.0 && resolve(18).awareness_snap(&tuning).is_some());
    }

    #[test]
    fn a_build_holds_eight_tiers_through_axes_and_seven_spread_over_all_six() {
        let tiers = |attrs: ActorAttributes| [Attribute::Might, Attribute::Agility, Attribute::Physique, Attribute::Discipline, Attribute::Instinct, Attribute::Resolve]
            .map(|attribute| attrs.tier(attribute).index()).iter().sum::<usize>();
        let pure = ActorAttributes::new(-24, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(pure.tier(Attribute::Might), CommitmentTier::T8);
        assert_eq!(tiers(ActorAttributes::new(-12, 0, 0, -12, 0, 0, 0, 0, 0)), 8, "T4 + T4");
        // Every attribute: T2 + T1 in one pair, T1 + T1 in the others, 24 steps
        let spread = ActorAttributes::new(-6, 4, 4, -3, 4, 4, -3, 4, 4);
        assert_eq!(ActorAttributes::invested(&spread.pairs()), 24);
        assert_eq!(tiers(spread), 7);
        for attribute in [Attribute::Agility, Attribute::Discipline, Attribute::Resolve] {
            assert_eq!(spread.tier(attribute), CommitmentTier::T1, "{attribute:?}");
        }
    }

    #[test]
    fn a_character_holds_a_step_a_level_up_to_a_whole_build() {
        assert_eq!(ActorAttributes::held(10), 10);
        assert_eq!(ActorAttributes::held(30), ActorAttributes::STEPS);
        let pure = |level: u32| crate::archetype::calculate_enemy_attributes(level as u8, crate::archetype::EnemyArchetype::Berserker);
        assert_eq!(pure(10).tier(Attribute::Might), CommitmentTier::T3, "a level-10 archetype holds 40%");
        assert_eq!(pure(24).tier(Attribute::Might), CommitmentTier::T8);
        assert_eq!(pure(30).might(), pure(25).might(), "past 25 a level leaves every share as it was");
        assert_eq!(pure(30).total_level(), 30);
        assert!(ActorAttributes::is_complete(&pure(30).pairs(), 30), "a whole build is all a level-30 places");
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
    }
}
