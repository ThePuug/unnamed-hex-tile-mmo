//! An NPC's movement channel: whether it closes on its target, gives
//! ground, circles it, or holds where it stands, facing it.
//!
//! Each move is a decision scored as a skill's is ([`super::utility`]),
//! from considerations its reach and its commitments bring, and holding
//! scores [`HOLD`], the threshold the others must beat. The move under way
//! scores [`MOMENTUM`] more, so two moves scoring alike do not trade
//! places every tick. How a move is walked is its pursuit's
//! ([`super::chase`]).

use bevy::prelude::*;

use super::{mind::Mind, utility::{score, Consideration, Curve}};

/// What holding scores unless a mind sets it: the threshold every other
/// move must beat.
pub const HOLD: f32 = 0.35;

/// What the move under way scores beside its own, unless a mind sets it
pub const MOMENTUM: f32 = 0.15;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &["pursuit_pays", "foe_nearing", "room_to_flee", "leash_tight", "bank_empty", "close_bank_full"];

/// A move an NPC makes.
#[derive(Clone, Component, Copy, Debug, Default, Eq, PartialEq)]
pub enum Move {
    /// It stands where it is and faces its target
    #[default]
    Hold,
    /// It walks to where it fights from
    Close,
    /// It runs on, keeping its target in the arc it strikes within, so it
    /// strikes as it goes
    Kite,
    /// It runs straight from its target
    Flee,
    /// It steps round its target, as far from it as it stands, toward its
    /// den: once its target stands outward of it, the way clear of it leads
    /// back in
    Circle,
}

/// What an NPC knows as it weighs its moves.
#[derive(Clone, Copy, Debug)]
pub struct Footing {
    /// It stands where it fights from: its assigned hex, or with its
    /// target in reach
    pub placed: bool,
    /// It fights from past a melee swing's reach
    pub ranged: bool,
    /// It has the stamina a swing across its line costs, so it can strike
    /// as it runs
    pub strikes_running: bool,
    /// Its target strikes it from where it stands, or faces it: closing on
    /// one running from it, that cannot strike it from there, gains nothing
    pub pursuit_pays: bool,
    /// Closing would carry it out past the edge of its leash, where it
    /// would let its target go and walk home
    pub at_leash: bool,
    /// Share of its leash it has left where it stands: 1 with none
    pub leash_room: f32,
    /// Its target stands nearer its den than it does, so the way clear of
    /// its target leads out toward its leash
    pub clear_outward: bool,
    /// Tiles to its target
    pub distance: i32,
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    /// Swings its Patience holds, and the most it holds: none while it is
    /// out of a fight and banks nothing
    pub banked: u32,
    pub patience: u32,
}

/// The move `footing` scores highest as `mind` shapes it, where it beats
/// holding; `under_way` is the move it is making.
pub fn choose(footing: &Footing, under_way: Move, mind: &Mind) -> Move {
    [Move::Close, Move::Kite, Move::Flee, Move::Circle]
        .into_iter()
        .map(|candidate| {
            let momentum = if candidate == under_way { mind.momentum } else { 0.0 };
            (candidate, weigh(footing, candidate, mind) + momentum)
        })
        .filter(|&(_, scored)| scored > mind.hold + if under_way == Move::Hold { mind.momentum } else { 0.0 })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(Move::Hold, |(candidate, _)| candidate)
}

/// What `candidate` scores for `footing`, as `mind` shapes it
pub fn weigh(footing: &Footing, candidate: Move, mind: &Mind) -> f32 {
    let considerations: &[Consideration<Footing>] = match candidate {
        Move::Hold => return mind.hold,
        Move::Close if footing.patience > 0 => &[NOT_PLACED, INSIDE_LEASH, PURSUIT_PAYS, BANK_FULL],
        Move::Close => &[NOT_PLACED, INSIDE_LEASH, PURSUIT_PAYS],
        Move::Kite => &[PLACED, RANGED, STRIKES_RUNNING],
        Move::Flee if footing.patience > 0 => &[FOE_NEARING, BANK_EMPTY, ROOM_TO_FLEE],
        Move::Flee => return 0.0,
        Move::Circle if footing.patience > 0 => &[CLEAR_OUTWARD, LEASH_TIGHT],
        Move::Circle => return 0.0,
    };
    score(1.0, considerations.iter().map(|consideration| mind.shape(consideration).answer(footing)))
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

const fn step(name: &'static str, read: fn(&Footing) -> f32) -> Consideration<Footing> {
    Consideration { name, read, bounds: (1.0, 1.0), curve: Curve::RISING }
}

const NOT_PLACED: Consideration<Footing> = step("not_placed", |footing| flag(!footing.placed));

const PURSUIT_PAYS: Consideration<Footing> = Consideration {
    name: "pursuit_pays",
    read: |footing| flag(footing.pursuit_pays),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.1),
};

const INSIDE_LEASH: Consideration<Footing> = step("inside_leash", |footing| flag(!footing.at_leash));

const PLACED: Consideration<Footing> = step("placed", |footing| flag(footing.placed));

const RANGED: Consideration<Footing> = step("ranged", |footing| flag(footing.ranged));

const STRIKES_RUNNING: Consideration<Footing> = step("strikes_running", |footing| flag(footing.strikes_running));

/// Its target closing on it, from half a leap past its reach to within it
const FOE_NEARING: Consideration<Footing> = Consideration {
    name: "foe_nearing",
    read: |footing| (footing.distance - footing.reach) as f32 / footing.leap.max(1) as f32,
    bounds: (0.5, 0.0),
    curve: Curve::RISING,
};

/// Room left on its leash to run through
const ROOM_TO_FLEE: Consideration<Footing> = Consideration {
    name: "room_to_flee",
    read: |footing| footing.leash_room,
    bounds: (0.1, 0.4),
    curve: Curve::RISING,
};

/// Its leash nearly run out
const LEASH_TIGHT: Consideration<Footing> = Consideration {
    name: "leash_tight",
    read: |footing| footing.leash_room,
    bounds: (0.5, 0.15),
    curve: Curve::RISING,
};

const CLEAR_OUTWARD: Consideration<Footing> = step("clear_outward", |footing| flag(footing.clear_outward));

/// Patience: the bank it fills standing out of reach, empty
const BANK_EMPTY: Consideration<Footing> = Consideration {
    name: "bank_empty",
    read: |footing| footing.banked as f32 / footing.patience.max(1) as f32,
    bounds: (0.0, 1.0),
    curve: Curve::FALLING,
};

/// Patience: the bank full, so it closes to spend it
const BANK_FULL: Consideration<Footing> = Consideration {
    name: "close_bank_full",
    read: |footing| footing.banked as f32 / footing.patience.max(1) as f32,
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.1),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn footing() -> Footing {
        Footing { placed: false, ranged: false, strikes_running: true, pursuit_pays: true, at_leash: false, leash_room: 1.0, clear_outward: false, distance: 8, reach: 2, leap: 9, banked: 0, patience: 0 }
    }

    #[test]
    fn it_closes_until_placed_and_then_holds() {
        assert_eq!(choose(&footing(), Move::Hold, &Mind::default()), Move::Close);
        assert_eq!(choose(&Footing { placed: true, distance: 2, ..footing() }, Move::Close, &Mind::default()), Move::Hold);
    }

    #[test]
    fn it_holds_rather_than_chase_one_running_that_cannot_strike_it() {
        assert_eq!(choose(&Footing { pursuit_pays: false, ..footing() }, Move::Close, &Mind::default()), Move::Hold);
    }

    #[test]
    fn at_its_leash_it_holds_rather_than_close_further_out() {
        assert_eq!(choose(&Footing { at_leash: true, ..footing() }, Move::Close, &Mind::default()), Move::Hold);
    }

    #[test]
    fn placed_with_its_target_in_reach_a_ranged_npc_kites_while_it_can_strike_running() {
        let placed = Footing { placed: true, ranged: true, distance: 15, reach: 20, ..footing() };
        assert_eq!(choose(&placed, Move::Hold, &Mind::default()), Move::Kite);
        assert_eq!(choose(&Footing { strikes_running: false, ..placed }, Move::Kite, &Mind::default()), Move::Hold, "spent, it stands and fights");
    }

    #[test]
    fn patience_keeps_away_while_its_bank_fills_and_closes_once_it_is_full() {
        let patient = Footing { patience: 3, ..footing() };
        assert_eq!(choose(&patient, Move::Hold, &Mind::default()), Move::Hold, "out of reach with its bank empty it waits");
        assert_eq!(choose(&Footing { distance: 3, ..patient }, Move::Hold, &Mind::default()), Move::Flee, "and runs as its target nears");
        assert_eq!(choose(&Footing { banked: 3, ..patient }, Move::Hold, &Mind::default()), Move::Close, "full, it closes");
    }

    #[test]
    fn near_its_leash_a_patient_npc_circles_inward_rather_than_flee_out() {
        let patient = Footing { patience: 3, distance: 3, ..footing() };
        let cornered = Footing { leash_room: 0.12, clear_outward: true, ..patient };
        assert_eq!(choose(&patient, Move::Hold, &Mind::default()), Move::Flee, "with room it flees");
        assert_eq!(choose(&cornered, Move::Flee, &Mind::default()), Move::Circle, "at the edge it circles round toward its den");
        assert_eq!(choose(&Footing { clear_outward: false, ..cornered }, Move::Hold, &Mind::default()), Move::Hold, "with its target outward already, it has nothing to circle for");
    }

    #[test]
    fn the_move_under_way_holds_against_one_scoring_alike() {
        let patient = Footing { patience: 3, distance: 5, ..footing() };
        let flee = weigh(&patient, Move::Flee, &Mind::default());
        assert!((flee - HOLD).abs() < MOMENTUM, "flee and hold score near alike here: {flee}");
        assert_eq!(choose(&patient, Move::Flee, &Mind::default()), Move::Flee);
        assert_eq!(choose(&patient, Move::Hold, &Mind::default()), Move::Hold);
    }
}
