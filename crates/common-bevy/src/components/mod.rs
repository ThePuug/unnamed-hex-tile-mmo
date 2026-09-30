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
pub mod npc_recovery;
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
/// The percentage is against `total_level × 10` — the most a single attribute
/// could reach — not against the summed budget. A summed denominator inflates
/// with spread, so a spectrum build would tier lower than an axis build holding
/// identical points.
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
}

impl ActorAttributes {
    /// An actor's attributes from the levels in each pair: axis, spectrum
    /// and shift of Might ↔ Agility, of Vitality ↔ Discipline, then of
    /// Instinct ↔ Resolve. A shift is taken as given, unclamped.
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
        Self {
            physique: Pair::new(might_agility_axis, might_agility_spectrum, might_agility_shift),
            conditioning: Pair::new(vitality_discipline_axis, vitality_discipline_spectrum, vitality_discipline_shift),
            temperament: Pair::new(instinct_resolve_axis, instinct_resolve_spectrum, instinct_resolve_shift),
        }
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

    /// Whether `pairs` is a respec an actor of `level` may take: no more
    /// levels than it has, and no spectrum below nothing.
    pub fn fits(pairs: &[Pair; 3], level: u32) -> bool {
        pairs.iter().all(|pair| pair.spectrum >= 0) && pairs.iter().map(|pair| pair.levels()).sum::<u32>() <= level
    }

    /// Takes a whole respec, already checked (`fits`): each pair's axis and
    /// spectrum as given, its shift as far as the pair allows.
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
    /// one attribute could reach at the actor's level, `total_level × 10`.
    /// The summed budget would tier a spectrum build below an axis build
    /// holding the same points.
    pub fn tier(&self, attribute: Attribute) -> CommitmentTier {
        CommitmentTier::calculate(self.value(attribute), self.total_level() * 10)
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

    /// Force, Might's: what a Lunge and an Overpower strike for, and what
    /// its share adds to an auto-attack
    pub fn force(&self) -> f32 { self.potency(Attribute::Might) }
    /// Precision, Agility's: what a Volley's shot and a Kick strike for, and
    /// how hard a crit lands (`crit_multiplier`)
    pub fn precision(&self) -> f32 { self.potency(Attribute::Agility) }
    /// Endurance, Discipline's: what a Flank strikes for, and how deep the
    /// endurance pool is (`max_endurance`)
    pub fn endurance(&self) -> f32 { self.potency(Attribute::Discipline) }
    /// Intuition, Instinct's: what a Disengage adds to the next swing, and
    /// how often a blow crits (`crit_chance`)
    pub fn intuition(&self) -> f32 { self.potency(Attribute::Instinct) }
    /// Concentration, Resolve's: the weight of what a Counter returns, and
    /// how long the effects an actor inflicts hold (`hold`)
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
    /// Toughness, Vitality: mitigates a blow, against the attacker's Presence
    pub fn toughness(&self) -> u16 { self.value(Attribute::Vitality) }
    /// Composure, Discipline: shortens its own recovery, against the opponent's Impact
    pub fn composure(&self) -> u16 { self.value(Attribute::Discipline) }
    /// Reflex, Instinct: widens a threat's window, against the attacker's Flow
    pub fn reflex(&self) -> u16 { self.value(Attribute::Instinct) }
    /// Presence, Resolve: spills a blow onto other hostiles and meets a
    /// defender's mitigation, against their Toughness
    pub fn presence(&self) -> u16 { self.value(Attribute::Resolve) }

    // Commitment: an attribute's tier, by the name it goes by

    /// Ferocity, Might: combos fire before they unlock
    pub fn ferocity(&self) -> CommitmentTier { self.tier(Attribute::Might) }
    /// Grace, Agility: the arc it strikes within (`arc`)
    pub fn grace(&self) -> CommitmentTier { self.tier(Attribute::Agility) }
    /// Grit, Vitality: the most of its health lost in any second (`grit_cap`)
    pub fn grit(&self) -> CommitmentTier { self.tier(Attribute::Vitality) }
    /// Preparation, Discipline: its index is how many reactions the actor
    /// may use in any one recovery
    pub fn preparation(&self) -> CommitmentTier { self.tier(Attribute::Discipline) }
    /// Patience, Instinct: the swings banked while it could not strike (`banked`)
    pub fn patience(&self) -> CommitmentTier { self.tier(Attribute::Instinct) }
    /// Awareness, Resolve: how much of the queue it sees (`window_size`)
    pub fn awareness(&self) -> CommitmentTier { self.tier(Attribute::Resolve) }

    /// The actor's level: every level it has put into an axis or a spectrum
    pub fn total_level(&self) -> u32 {
        self.physique.levels() + self.conditioning.levels() + self.temperament.levels()
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

    /// The endurance a skill or a reaction costs this actor:
    /// `Tuning::endurance_cost` for each point of base potency, which every
    /// skill strikes with. It grows with level as the pool does, so a pool
    /// with no Discipline in it holds the same count of skills at any level.
    pub fn skill_endurance(&self) -> f32 {
        crate::tuning::tuning().endurance_cost * self.base_potency()
    }

    /// The chance a blow this actor strikes crits: `Tuning::crit_chance` at
    /// the ceiling of Intuition's share
    pub fn crit_chance(&self) -> f32 {
        crate::tuning::tuning().crit_chance * self.share(Attribute::Instinct)
    }

    /// What a crit this actor strikes multiplies its blow by:
    /// `Tuning::crit_power`, `Tuning::crit_severity` more at the ceiling of
    /// Precision's share
    pub fn crit_multiplier(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        tuning.crit_power + tuning.crit_severity * self.share(Attribute::Agility)
    }

    /// An auto-attack's damage: `Tuning::auto_damage` of base potency, more by
    /// `Tuning::force_auto` at the ceiling of Force's share
    pub fn auto_damage(&self) -> f32 {
        let tuning = crate::tuning::tuning();
        self.base_potency() * tuning.auto_damage * (1.0 + tuning.force_auto * self.share(Attribute::Might))
    }

    /// How much longer and harder the stuns, dazes, slows and knockbacks this
    /// actor inflicts hold: 1 with no Concentration, `Tuning::concentration_hold`
    /// more at the ceiling of its share
    pub fn hold(&self) -> f32 {
        1.0 + crate::tuning::tuning().concentration_hold * self.share(Attribute::Resolve)
    }

    /// The most of its health this actor loses in any second
    /// (`components::grit::Grit`): `Tuning::grit_cap` by its Grit tier, all of
    /// it at T0. What would pass it lands in the seconds after.
    pub fn grit_cap(&self) -> f32 {
        crate::tuning::tuning().grit_cap[self.grit().index()]
    }

    /// The swings banked behind one that has `waited` since it came due,
    /// at an auto-attack `interval`: one for each interval waited, up to the
    /// Patience tier, 0 to 3, to land with it.
    pub fn banked(&self, waited: std::time::Duration, interval: std::time::Duration) -> u32 {
        let came_due = (waited.as_secs_f32() / interval.as_secs_f32()).floor() as u32;
        came_due.min(self.patience().index() as u32)
    }

    /// The reaction queue's window, from Awareness: one threat seen at T0
    /// and one more each tier, to four at T3. Fixed, not tuned; a Counter
    /// answers the whole window, so it grows with Awareness.
    pub fn window_size(&self) -> usize {
        self.awareness().index() + 1
    }

    /// The half-angle either side of its heading this actor strikes within:
    /// the three forward faces at T0, and each Grace tier wider, 90°, 120°,
    /// then every way at T3. Fixed, not tuned; a strike past the forward
    /// faces breaks its stride (`targeting::across`).
    pub fn arc(&self) -> f32 {
        [60.0, 90.0, 120.0, 180.0][self.grace().index()]
    }

    /// Seconds between auto-attacks: `Tuning::auto_interval`, the same for
    /// every actor
    pub fn cadence_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(crate::tuning::tuning().auto_interval)
    }
}

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();

/// When an actor's next auto-attack comes due, as the server counts it:
/// an interval after its last, or the moment its fight found it not yet
/// swinging. A due swing waits for a target it can strike, and the swings
/// that come due behind it while it waits, up to its Patience, land with
/// it (`ActorAttributes::banked`). None out of combat: a swing is due, and
/// nothing banks until a fight starts the clock.
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
    fn intuition_crits_more_often_and_precision_harder() {
        let instinct = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let agility = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(plain.crit_chance(), 0.0, "no Intuition, no crits");
        assert!(instinct.crit_chance() > 0.0 && instinct.crit_chance() < 1.0);
        assert!(agility.crit_multiplier() > plain.crit_multiplier(), "Precision lands a crit harder");
        assert!(plain.crit_multiplier() > 1.0, "a crit always lands harder");
    }

    #[test]
    fn endurance_deepens_its_own_pool_and_every_actor_has_the_one_stamina() {
        let disciplined = ActorAttributes::new(0, 0, 0, 10, 0, 0, 0, 0, 0);
        let mighty = ActorAttributes::new(-10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(disciplined.max_stamina(), plain.max_stamina());
        assert!(disciplined.max_endurance() > mighty.max_endurance(), "Discipline deepens it");
        assert!(mighty.max_endurance() > plain.max_endurance(), "and so does level");
        assert_eq!(disciplined.skill_endurance(), mighty.skill_endurance(), "a skill costs the same at a level, whatever the build");
        let skills = |attrs: &ActorAttributes| attrs.max_endurance() / attrs.skill_endurance();
        assert!((skills(&mighty) - skills(&plain)).abs() < 1e-3, "with no Discipline a pool holds as many skills at any level");
    }

    #[test]
    fn concentration_holds_what_its_blows_impose() {
        let resolute = ActorAttributes::new(0, 0, 0, 0, 0, 0, 10, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(plain.hold(), 1.0, "no Concentration, effects as they come");
        assert!(resolute.hold() > 1.0);
    }

    #[test]
    fn grace_widens_the_arc_to_every_way() {
        let graceful = ActorAttributes::new(10, 0, 0, 0, 0, 0, 0, 0, 0);
        let plain = ActorAttributes::default();
        assert_eq!(plain.arc(), crate::systems::targeting::STRIDE_ARC, "no Grace, the forward faces");
        assert_eq!(graceful.arc(), 180.0, "full commitment strikes every way");
    }

    #[test]
    fn patience_banks_what_came_due_behind_a_waiting_swing_up_to_its_tier() {
        let patient = ActorAttributes::new(0, 0, 0, 0, 0, 0, -10, 0, 0);
        let plain = ActorAttributes::default();
        let interval = std::time::Duration::from_secs(2);
        let secs = std::time::Duration::from_secs;
        assert_eq!(patient.banked(secs(1), interval), 0, "the due swing alone");
        assert_eq!(patient.banked(secs(3), interval), 1, "one more came due behind it");
        assert_eq!(patient.banked(secs(60), interval), 3, "no more than full commitment banks");
        assert_eq!(plain.banked(secs(60), interval), 0, "without Patience, missed swings are lost");
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
        // axis=-5, spectrum=0 → might=50, agility=0, total_budget=50
        // might commitment: 50/50 = 100% → T3
        let attrs = ActorAttributes::new(-5, 0, 0, 0, 0, 0, 0, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T3);
        assert_eq!(attrs.tier(Attribute::Agility), CommitmentTier::T0);
    }

    #[test]
    fn test_tier_of_balanced_build() {
        // Spread across pairs: each pair gets some investment
        // M/G: axis=0, spectrum=3 → might=18, agility=18 (3×6 balanced multiplier)
        // V/F: axis=0, spectrum=3 → vitality=18, discipline=18
        // I/P: axis=0, spectrum=3 → instinct=18, resolve=18
        // total_level = 9, max_possible = 90
        // each attr = 18/90 = 20% → T1 (exactly at threshold)
        let attrs = ActorAttributes::new(0, 3, 0, 0, 3, 0, 0, 3, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T1);
        assert_eq!(attrs.tier(Attribute::Agility), CommitmentTier::T1);
        assert_eq!(attrs.tier(Attribute::Vitality), CommitmentTier::T1);
        assert_eq!(attrs.tier(Attribute::Discipline), CommitmentTier::T1);
    }

    #[test]
    fn test_commitment_tier_budget_constraints() {
        // T3+T2 build:
        // axis=-6 → might = 6×16 = 96, axis=-3 → vitality = 3×16 = 48
        // total_level = 9, max_possible = 90
        // might: 96/90 = 106.7% → T3 ✓
        // vitality: 48/90 = 53.3% → T2 ✓
        let attrs = ActorAttributes::new(-6, 0, 0, -3, 0, 0, 0, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T3);
        assert_eq!(attrs.tier(Attribute::Vitality), CommitmentTier::T2);
    }

    #[test]
    fn test_commitment_tier_dual_t2() {
        // Dual T2 with axis×16 scaling:
        // axis=-3 on two pairs: total_level = 6, max_possible = 60
        // might = 3×16 = 48 → 48/60 = 80% → T3 (too high for single pair!)

        // Spread to three pairs for T2:
        // axis=-3 each → total_level = 9, max_possible = 90
        // might = 48 → 48/90 = 53.3% → T2 ✓
        // vitality = 48 → 53.3% → T2 ✓
        let attrs = ActorAttributes::new(-3, 0, 0, -3, 0, 0, -3, 0, 0);
        assert_eq!(attrs.tier(Attribute::Might), CommitmentTier::T2);
        assert_eq!(attrs.tier(Attribute::Vitality), CommitmentTier::T2);
        assert_eq!(attrs.tier(Attribute::Instinct), CommitmentTier::T2);
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

        let mut attrs = ActorAttributes::default();
        attrs.apply_respec([Pair::new(-6, 2, 5), Pair::new(0, 1, 1), Pair::new(1, 0, -3)]);
        assert_eq!(attrs.pairs(), [Pair::new(-6, 2, 2), Pair::new(0, 1, 0), Pair::new(1, 0, 0)], "each shift as far as its pair allows");
        assert_eq!(attrs.total_level(), level);
    }
}
