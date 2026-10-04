//! An NPC's movement channel: which tile it steps to next, its own or one
//! of its neighbours, and for what.
//!
//! Every candidate tile is scored for each thing a step may be for, as a
//! skill's decision is ([`super::utility`]): to engage its target, from
//! where it strikes it soonest, and, with its Patience under way, to keep
//! away from it while its recovery runs down, out of its target's reach
//! but ready to strike. The best pair is taken where it beats holding, which
//! scores [`HOLD`]; the decision under way scores [`MOMENTUM`] more, so two
//! scoring alike do not trade places every tick. Every reading is the
//! candidate's own: the seconds from it to striking and to being struck,
//! the leash left there, how far round toward its target's back it
//! stands, and what a strike on the step there costs. How a step is walked
//! is its pursuit's ([`super::chase`]).

use bevy::prelude::*;
use qrz::Qrz;

use super::{mind::Mind, skills::{LEASH_LEFT_BOUNDS, LEASH_LEFT_CURVE}, utility::{score, Consideration, Curve}};

/// What holding scores unless a mind sets it: the threshold every step must
/// beat.
pub const HOLD: f32 = 0.35;

/// What the decision under way scores beside its own, unless a mind sets it
pub const MOMENTUM: f32 = 0.15;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &[
    "leash_left", "ready", "recovering", "detour", "time_to_strike", "time_to_be_struck",
    "behind", "strike_cost", "stride_kept",
];

/// What a step is for.
#[derive(Clone, Component, Copy, Debug, Default, Eq, PartialEq)]
pub enum Move {
    /// It stands where it is and faces its target
    #[default]
    Hold,
    /// It steps to where it strikes its target soonest
    Engage,
    /// It steps out of its target's reach while its recovery runs down,
    /// staying ready to strike: Patience's
    KeepAway,
}

/// What an NPC knows of itself as it weighs where to step.
#[derive(Clone, Copy, Debug)]
pub struct Footing {
    /// Its Grace lets it strike past its forward faces
    pub grace: bool,
    /// Share of its recovery it has left: 0 out of recovery
    pub recovering: f32,
    /// Its Patience tier, while it is engaged and its recovery runs faster
    /// waiting on a swing: none otherwise
    pub patience: u32,
}

/// One tile it may step to, as it would find it there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub tile: Qrz,
    /// Seconds from the tile until it could strike its target: to its
    /// assigned hex where it has one, else until its target stands in its
    /// reach, by the fastest way it can close
    pub time_to_strike: f32,
    /// Of that, how many seconds more than from the best of the candidates
    pub detour: f32,
    /// Seconds from the tile until its target could strike it there, at
    /// the pace it sees its target move, from the reach it has seen it
    /// strike from
    pub time_to_be_struck: f32,
    /// Share of its leash it would have left there: 1 with none
    pub room: f32,
    /// How far round toward its target's back the tile stands, off its
    /// target's heading: 0 straight before it, 1 straight behind. Past a
    /// third it stands outside its target's forward faces, where a target
    /// without Grace cannot strike it
    pub behind: f32,
    /// What a strike on the step there costs, of the endurance it has:
    /// nothing within its target's forward faces of its heading, more the
    /// further round its arc
    pub strike_cost: f32,
    /// A strike on the step there breaks its stride: across its line, with
    /// no Perfect Stride up
    pub breaks_stride: bool,
}

/// What it weighs a step by: itself and the candidate.
#[derive(Clone, Copy, Debug)]
struct Ground {
    footing: Footing,
    candidate: Candidate,
}

/// The step `candidates` offer that scores highest for `footing` as `mind`
/// shapes it, and what it is for, where it beats holding; `under_way` is
/// what its last step was for. The first candidate is the tile it stands
/// on, and of steps scoring alike it keeps to it.
pub fn choose(footing: &Footing, candidates: &[Candidate], under_way: Move, mind: &Mind) -> (Move, Option<Candidate>) {
    let threshold = mind.hold + if under_way == Move::Hold { mind.momentum } else { 0.0 };
    let mut best: Option<(Move, Candidate, f32)> = None;
    for &candidate in candidates {
        for decision in [Move::Engage, Move::KeepAway] {
            let momentum = if decision == under_way { mind.momentum } else { 0.0 };
            let scored = weigh(footing, &candidate, decision, mind) + momentum;
            if scored > threshold && best.is_none_or(|(.., top)| scored > top) {
                best = Some((decision, candidate, scored));
            }
        }
    }
    best.map_or((Move::Hold, None), |(decision, candidate, _)| (decision, Some(candidate)))
}

/// What a step to `candidate` scores for `decision`, for `footing`, as
/// `mind` shapes it
pub fn weigh(footing: &Footing, candidate: &Candidate, decision: Move, mind: &Mind) -> f32 {
    let considerations: &[Consideration<Ground>] = match decision {
        Move::Hold => return mind.hold,
        Move::Engage if footing.grace => &[DETOUR, LEASH_LEFT, READY, BEHIND, STRIKE_COST, STRIDE_KEPT],
        Move::Engage => &[DETOUR, LEASH_LEFT, READY],
        Move::KeepAway if footing.patience > 0 => &[TIME_TO_BE_STRUCK, TIME_TO_STRIKE, RECOVERING, LEASH_LEFT],
        Move::KeepAway => return 0.0,
    };
    let ground = Ground { footing: *footing, candidate: *candidate };
    score(1.0, considerations.iter().map(|consideration| mind.shape(consideration).answer(&ground)))
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// Seconds more the step costs it on its way to strike than the best one:
/// the best step answers whole however far it has to go
const DETOUR: Consideration<Ground> = Consideration {
    name: "detour",
    read: |ground| ground.candidate.detour,
    bounds: (0.0, 0.5),
    curve: Curve::FALLING,
};

/// The share of its leash it would have left there; `leash_left` as a Leap
/// reads it where it lands
const LEASH_LEFT: Consideration<Ground> = Consideration {
    name: "leash_left",
    read: |ground| ground.candidate.room,
    bounds: LEASH_LEFT_BOUNDS,
    curve: LEASH_LEFT_CURVE,
};

/// How far through its recovery it is: 1 out of it. Weighed only as far
/// as a mind lowers its floor, holding back from a fight it cannot act in
const READY: Consideration<Ground> = Consideration {
    name: "ready",
    read: |ground| 1.0 - ground.footing.recovering,
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(1.0),
};

/// Patience: recovery left, which runs faster out of its target's reach
const RECOVERING: Consideration<Ground> = Consideration {
    name: "recovering",
    read: |ground| ground.footing.recovering,
    bounds: (0.0, 1.0),
    curve: Curve::RISING,
};

/// Seconds from the tile until its target could strike it: the later, the
/// safer it recovers
const TIME_TO_BE_STRUCK: Consideration<Ground> = Consideration {
    name: "time_to_be_struck",
    read: |ground| ground.candidate.time_to_be_struck,
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(0.1),
};

/// Seconds from the tile until it could strike: kept short, it stays ready
/// to strike as its recovery runs out
const TIME_TO_STRIKE: Consideration<Ground> = Consideration {
    name: "time_to_strike",
    read: |ground| ground.candidate.time_to_strike,
    bounds: (0.0, 3.0),
    curve: Curve::FALLING.floored(0.1),
};

/// Grace: how far round toward its target's back the tile stands, every
/// step round worth more until it stands to its target's side, past its
/// forward faces
const BEHIND: Consideration<Ground> = Consideration {
    name: "behind",
    read: |ground| ground.candidate.behind,
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(0.2),
};

/// What a strike on the step costs: the dearer, against the endurance it has
/// left, the less the step pays
const STRIKE_COST: Consideration<Ground> = Consideration {
    name: "strike_cost",
    read: |ground| ground.candidate.strike_cost,
    bounds: (0.0, 0.8),
    curve: Curve::FALLING,
};

/// A strike on the step that breaks its stride slows it as it goes
const STRIDE_KEPT: Consideration<Ground> = Consideration {
    name: "stride_kept",
    read: |ground| flag(!ground.candidate.breaks_stride),
    bounds: (0.0, 1.0),
    curve: Curve::RISING.floored(0.6),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn footing() -> Footing {
        Footing { grace: false, recovering: 0.0, patience: 0 }
    }

    /// A tile `q` east of the origin, with its target standing further
    /// east: struck from in two tiles, struck at in two
    fn candidate(q: i32, target: i32) -> Candidate {
        let gap = (target - q).abs();
        Candidate {
            tile: Qrz { q, r: 0, z: 0 },
            time_to_strike: (gap - 2).max(0) as f32 * 0.25,
            detour: 0.0,
            time_to_be_struck: (gap - 2).max(0) as f32 * 0.25,
            room: 1.0,
            behind: 0.0,
            strike_cost: 0.0,
            breaks_stride: false,
        }
    }

    /// The tile it stands on and its two neighbours east and west, each's
    /// detour against the best of them
    fn around(q: i32, target: i32) -> Vec<Candidate> {
        let mut candidates = vec![candidate(q, target), candidate(q + 1, target), candidate(q - 1, target)];
        let best = candidates.iter().map(|candidate| candidate.time_to_strike).fold(f32::INFINITY, f32::min);
        for candidate in &mut candidates {
            candidate.detour = candidate.time_to_strike - best;
        }
        candidates
    }

    /// A mind that holds back while it recovers: it engages only ready to
    /// act
    fn keeping() -> Mind {
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.ready.floor", "0").unwrap();
        minds.mind(None)
    }

    #[test]
    fn it_steps_toward_where_it_strikes_soonest_and_stands_once_there() {
        let mind = Mind::default();
        let (decision, step) = choose(&footing(), &around(0, 8), Move::Hold, &mind);
        assert_eq!((decision, step.map(|step| step.tile.q)), (Move::Engage, Some(1)), "far off, it steps in");
        let (decision, step) = choose(&footing(), &around(6, 8), Move::Engage, &mind);
        assert_eq!((decision, step.map(|step| step.tile.q)), (Move::Engage, Some(6)), "in reach, it keeps to its tile");
    }

    #[test]
    fn it_holds_rather_than_step_out_to_its_leash_edge() {
        let mut candidates = around(0, 8);
        for candidate in &mut candidates {
            candidate.room = if candidate.tile.q > 0 { 0.0 } else { 1.0 };
        }
        let (_, step) = choose(&footing(), &candidates, Move::Hold, &Mind::default());
        assert_ne!(step.map(|step| step.tile.q), Some(1), "never onto a tile with no leash left");
    }

    #[test]
    fn a_mind_holding_back_holds_while_it_recovers() {
        let recovering = Footing { recovering: 1.0, ..footing() };
        assert_eq!(choose(&recovering, &around(0, 8), Move::Hold, &Mind::default()).0, Move::Engage);
        assert_eq!(choose(&recovering, &around(0, 8), Move::Hold, &keeping()).0, Move::Hold);
    }

    #[test]
    fn grace_steps_round_toward_its_targets_back_and_stands_once_past_its_faces() {
        let mind = Mind::default();
        let graceful = Footing { grace: true, ..footing() };
        let mut candidates = around(6, 8);
        candidates[1].behind = 0.17;
        let (_, step) = choose(&graceful, &candidates, Move::Engage, &mind);
        assert_eq!(step.map(|step| step.tile.q), Some(7), "faced, any step round is worth taking");
        candidates[1].behind = 0.5;
        candidates[0].behind = 0.5;
        let (_, step) = choose(&graceful, &candidates, Move::Engage, &mind);
        assert_eq!(step.map(|step| step.tile.q), Some(6), "past its faces already, it stands and strikes");
        let plain = choose(&footing(), &candidates, Move::Engage, &mind).1;
        assert_eq!(plain.map(|step| step.tile.q), Some(6), "with no Grace a tile past its target's faces is worth no step");
        candidates[1].strike_cost = 0.6;
        candidates[0].behind = 0.0;
        assert!(weigh(&graceful, &candidates[1], Move::Engage, &mind) < weigh(&graceful, &Candidate { strike_cost: 0.0, ..candidates[1] }, Move::Engage, &mind), "with each strike dearer it steps round less readily");
    }

    #[test]
    fn patience_keeps_away_out_of_reach_but_ready_and_engages_once_recovered() {
        let mind = keeping();
        let patient = Footing { patience: 3, recovering: 1.0, ..footing() };
        let (decision, step) = choose(&patient, &around(6, 8), Move::Hold, &mind);
        assert_eq!((decision, step.map(|step| step.tile.q)), (Move::KeepAway, Some(5)), "recovering in its target's reach, it steps out");
        let recovered = Footing { recovering: 0.0, ..patient };
        assert_eq!(choose(&recovered, &around(3, 8), Move::KeepAway, &mind).0, Move::Engage, "recovered, it engages");
        let far = choose(&patient, &around(0, 12), Move::Hold, &mind);
        assert_ne!(far.1.map(|step| step.tile.q), Some(-1), "and it gives no ground it need not, out of reach already");
    }

    #[test]
    fn without_patience_it_never_keeps_away() {
        let recovering = Footing { recovering: 1.0, ..footing() };
        assert_eq!(weigh(&recovering, &candidate(5, 8), Move::KeepAway, &Mind::default()), 0.0);
    }
}
