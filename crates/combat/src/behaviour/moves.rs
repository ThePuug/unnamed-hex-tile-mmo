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

use super::{mind::Mind, skills::{LEASH_LEFT_BOUNDS, LEASH_LEFT_CURVE, STAMINA_LEFT_BOUNDS, STAMINA_LEFT_CURVE}, utility::{score, Consideration, Curve}};

/// What holding scores unless a mind sets it: the threshold every other
/// move must beat.
pub const HOLD: f32 = 0.35;

/// What the move under way scores beside its own, unless a mind sets it
pub const MOMENTUM: f32 = 0.15;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &[
    "leash_left", "stamina_left", "foe_nearing", "leash_tight", "stamina_spent",
    "foe_facing", "strike_cost", "stride_kept",
];

/// A move an NPC makes.
#[derive(Clone, Component, Copy, Debug, Default, Eq, PartialEq)]
pub enum Move {
    /// It stands where it is and faces its target
    #[default]
    Hold,
    /// It walks to where it fights from
    Close,
    /// It runs straight from its target
    Flee,
    /// It steps round its target, as far from it as it stands: with Grace
    /// toward its target's back, past its target's forward faces, where a
    /// target without Grace cannot strike it and one with Grace pays to,
    /// while its own strikes still land; with Patience toward its den, so once its
    /// target stands outward of it the way clear of it leads back in
    Circle,
}

/// What an NPC knows as it weighs its moves.
#[derive(Clone, Copy, Debug)]
pub struct Footing {
    /// It stands where it fights from: its assigned hex, or with its
    /// target in reach
    pub placed: bool,
    /// Its Grace lets it strike past its forward faces
    pub grace: bool,
    /// Its target has it within its forward faces, where its target strikes
    /// it without crossing its line
    pub foe_facing: bool,
    /// What a strike costs on the heading it would circle on, as a share of
    /// the stamina it has: nothing where its target stays within its
    /// forward faces, more the further round its arc the strike goes
    pub strike_cost: f32,
    /// A strike on that heading breaks its stride: across its line, with
    /// no Perfect Stride up
    pub breaks_stride: bool,
    /// Share of its leash it has left where it stands: 1 with none
    pub leash_room: f32,
    /// Share of its leash it would have left a step on toward its target
    pub room_closing: f32,
    /// Share of its leash it would have left a step on away from its target
    pub room_fleeing: f32,
    /// Its target stands nearer its den than it does, so the way clear of
    /// its target leads out toward its leash
    pub clear_outward: bool,
    /// Tiles to its target
    pub distance: i32,
    pub reach: i32,
    /// Tiles a Leap carries it
    pub leap: i32,
    /// Share of its stamina it has
    pub stamina: f32,
    /// Its Patience tier, while it is engaged and its stamina refills
    /// faster waiting on a swing: none otherwise
    pub patience: u32,
}

/// The move `footing` scores highest as `mind` shapes it, where it beats
/// holding; `under_way` is the move it is making.
pub fn choose(footing: &Footing, under_way: Move, mind: &Mind) -> Move {
    [Move::Close, Move::Flee, Move::Circle]
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
        Move::Close => &[NOT_PLACED, CLOSING_LEASH, STAMINA_LEFT],
        Move::Flee if footing.patience > 0 => &[FOE_NEARING, STAMINA_SPENT, FLEEING_LEASH],
        Move::Flee => return 0.0,
        Move::Circle if footing.patience > 0 => &[CLEAR_OUTWARD, LEASH_TIGHT],
        Move::Circle if footing.grace => &[IN_REACH, FOE_FACING, STRIKE_COST, STRIDE_KEPT],
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

/// The share of its leash it would have left a step on toward its target:
/// `leash_left` as a Leap reads it where it lands
const CLOSING_LEASH: Consideration<Footing> = Consideration {
    name: "leash_left",
    read: |footing| footing.room_closing,
    bounds: LEASH_LEFT_BOUNDS,
    curve: LEASH_LEFT_CURVE,
};

/// The share of its leash it would have left a step on away from its target
const FLEEING_LEASH: Consideration<Footing> = Consideration {
    name: "leash_left",
    read: |footing| footing.room_fleeing,
    bounds: LEASH_LEFT_BOUNDS,
    curve: LEASH_LEFT_CURVE,
};

/// The stamina it would have left, of its most: a move costs none, so what
/// it has. One setting with a skill's `stamina_left`
const STAMINA_LEFT: Consideration<Footing> = Consideration {
    name: "stamina_left",
    read: |footing| footing.stamina,
    bounds: STAMINA_LEFT_BOUNDS,
    curve: STAMINA_LEFT_CURVE,
};

const IN_REACH: Consideration<Footing> = step("in_reach", |footing| flag(footing.distance <= footing.reach));

/// Grace: its target faces it, so circling takes it past its target's
/// forward faces; behind it already, it has what circling gains
const FOE_FACING: Consideration<Footing> = Consideration {
    name: "foe_facing",
    read: |footing| flag(footing.foe_facing),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.2),
};

/// Its target closing on it, from half a leap past its reach to within it
const FOE_NEARING: Consideration<Footing> = Consideration {
    name: "foe_nearing",
    read: |footing| (footing.distance - footing.reach) as f32 / footing.leap.max(1) as f32,
    bounds: (0.5, 0.0),
    curve: Curve::RISING,
};

/// Its leash nearly run out
const LEASH_TIGHT: Consideration<Footing> = Consideration {
    name: "leash_tight",
    read: |footing| footing.leash_room,
    bounds: (0.5, 0.15),
    curve: Curve::RISING,
};

/// What each strike costs on the heading it would circle on: the dearer,
/// against the stamina it has left, the less circling pays
const STRIKE_COST: Consideration<Footing> = Consideration {
    name: "strike_cost",
    read: |footing| footing.strike_cost,
    bounds: (0.0, 0.8),
    curve: Curve::FALLING,
};

/// A strike on that heading that breaks its stride slows it as it goes
const STRIDE_KEPT: Consideration<Footing> = Consideration {
    name: "stride_kept",
    read: |footing| flag(!footing.breaks_stride),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.6),
};

const CLEAR_OUTWARD: Consideration<Footing> = step("clear_outward", |footing| flag(footing.clear_outward));

/// Patience: stamina spent, which it refills faster out of reach
const STAMINA_SPENT: Consideration<Footing> = Consideration {
    name: "stamina_spent",
    read: |footing| footing.stamina,
    bounds: (0.0, 1.0),
    curve: Curve::FALLING,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn footing() -> Footing {
        Footing { placed: false, grace: false, foe_facing: true, strike_cost: 0.05, breaks_stride: true, leash_room: 1.0, room_closing: 1.0, room_fleeing: 1.0, clear_outward: false, distance: 8, reach: 2, leap: 9, stamina: 0.0, patience: 0 }
    }

    /// A mind that keeps its stamina back: it closes only with stamina to
    /// spend
    fn keeping() -> Mind {
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.stamina_left.floor", "0").unwrap();
        minds.mind(None)
    }

    #[test]
    fn it_closes_until_placed_and_then_holds() {
        assert_eq!(choose(&footing(), Move::Hold, &Mind::default()), Move::Close);
        assert_eq!(choose(&Footing { placed: true, distance: 2, ..footing() }, Move::Close, &Mind::default()), Move::Hold);
    }

    #[test]
    fn at_its_leash_it_holds_rather_than_close_further_out() {
        assert_eq!(choose(&Footing { room_closing: 0.0, ..footing() }, Move::Close, &Mind::default()), Move::Hold);
    }

    #[test]
    fn grace_circles_a_target_in_reach_that_faces_it_and_stands_once_behind_it() {
        let mind = Mind::default();
        let graceful = Footing { placed: true, grace: true, distance: 1, ..footing() };
        assert_eq!(choose(&graceful, Move::Hold, &mind), Move::Circle, "faced, it steps round across its target's line");
        assert_eq!(choose(&Footing { foe_facing: false, ..graceful }, Move::Circle, &mind), Move::Hold, "behind it, it stands and strikes");
        assert_eq!(choose(&Footing { grace: false, ..graceful }, Move::Hold, &mind), Move::Hold, "with no Grace its strikes on the move would cross its own line");
        assert_eq!(choose(&Footing { distance: 4, ..graceful }, Move::Hold, &mind), Move::Hold, "out of reach it has nothing to circle for");
        let dear = Footing { strike_cost: 0.6, ..graceful };
        assert!(weigh(&dear, Move::Circle, &mind) < weigh(&graceful, Move::Circle, &mind), "with each strike dearer it circles less readily");
        assert!(weigh(&Footing { breaks_stride: false, ..dear }, Move::Circle, &mind) > weigh(&dear, Move::Circle, &mind), "and more in a Perfect Stride");
    }

    #[test]
    fn patience_keeps_away_while_its_stamina_refills_and_closes_once_it_is_back() {
        let (patient, mind) = (Footing { patience: 3, ..footing() }, keeping());
        assert_eq!(choose(&patient, Move::Hold, &mind), Move::Hold, "out of reach with its stamina spent it waits");
        assert_eq!(choose(&Footing { distance: 3, ..patient }, Move::Hold, &mind), Move::Flee, "and runs as its target nears");
        assert_eq!(choose(&Footing { stamina: 1.0, ..patient }, Move::Hold, &mind), Move::Close, "refilled, it closes");
    }

    #[test]
    fn a_mind_that_keeps_no_stamina_back_closes_spent() {
        assert_eq!(choose(&footing(), Move::Hold, &Mind::default()), Move::Close);
        assert_eq!(choose(&footing(), Move::Hold, &keeping()), Move::Hold);
    }

    #[test]
    fn near_its_leash_a_patient_npc_circles_inward_rather_than_flee_out() {
        let (patient, mind) = (Footing { patience: 3, distance: 3, ..footing() }, keeping());
        let cornered = Footing { leash_room: 0.12, room_fleeing: 0.05, clear_outward: true, ..patient };
        assert_eq!(choose(&patient, Move::Hold, &mind), Move::Flee, "with room it flees");
        assert_eq!(choose(&cornered, Move::Flee, &mind), Move::Circle, "at the edge it circles round toward its den");
        assert_eq!(choose(&Footing { clear_outward: false, ..cornered }, Move::Hold, &mind), Move::Hold, "with its target outward already, it has nothing to circle for");
    }

    #[test]
    fn the_move_under_way_holds_against_one_scoring_alike() {
        let (patient, mind) = (Footing { patience: 3, distance: 5, ..footing() }, keeping());
        let flee = weigh(&patient, Move::Flee, &mind);
        assert!((flee - HOLD).abs() < MOMENTUM, "flee and hold score near alike here: {flee}");
        assert_eq!(choose(&patient, Move::Flee, &mind), Move::Flee);
        assert_eq!(choose(&patient, Move::Hold, &mind), Move::Hold);
    }
}
