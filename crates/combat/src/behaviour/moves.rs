//! An NPC's movement channel: which tile it steps to next, its own or one
//! of its neighbours, and for what.
//!
//! Every candidate tile is scored for each thing a step may be for, as a
//! skill's decision is ([`super::utility`]): to engage its target, from
//! where it strikes it soonest. The best is taken where it beats holding,
//! which scores [`HOLD`]; the decision under way scores [`MOMENTUM`] more,
//! so two scoring alike do not trade places every tick. Every reading is
//! the candidate's own: the seconds from it to striking and how far round
//! toward its target's back it stands. Its leash is no reading: a step may
//! carry it past, which is how its pursuit lets go ([`super::chase`]), whose
//! also is how a step is walked.

use bevy::prelude::*;
use qrz::Qrz;

use super::{mind::Mind, utility::{score, Consideration, Curve}};

/// What holding scores unless a mind sets it: the threshold every step must
/// beat.
pub const HOLD: f32 = 0.35;

/// What the decision under way scores beside its own, unless a mind sets it
pub const MOMENTUM: f32 = 0.15;

/// The considerations a mind may tune ([`super::mind`]): those with a
/// curve to shape, where a condition only holds or fails.
pub const TUNABLE: &[&str] = &[
    "detour", "behind",
];

/// What a step is for.
#[derive(Clone, Component, Copy, Debug, Default, Eq, PartialEq)]
pub enum Move {
    /// It stands where it is and faces its target
    #[default]
    Hold,
    /// It steps to where it strikes its target soonest
    Engage,
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
    /// How far round toward its target's back the tile stands, off its
    /// target's heading: 0 straight before it, 1 straight behind. Past a
    /// third it stands outside its target's forward faces. 1 on every
    /// tile for one with no strike ready that lands only from there: it
    /// has nothing to step round for
    pub behind: f32,
}

/// What it weighs a step by: the candidate.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    candidate: Candidate,
}

/// The step `candidates` offer that scores highest as `mind` shapes it,
/// and what it is for, where it beats holding; `under_way` is what its
/// last step was for. The first candidate is the tile it stands on, and of
/// steps scoring alike it keeps to it.
pub fn choose(candidates: &[Candidate], under_way: Move, mind: &Mind) -> (Move, Option<Candidate>) {
    let threshold = mind.hold + if under_way == Move::Hold { mind.momentum } else { 0.0 };
    let momentum = if under_way == Move::Engage { mind.momentum } else { 0.0 };
    let mut best: Option<(Candidate, f32)> = None;
    for &candidate in candidates {
        let scored = weigh(&candidate, mind) + momentum;
        if scored > threshold && best.is_none_or(|(_, top)| scored > top) {
            best = Some((candidate, scored));
        }
    }
    best.map_or((Move::Hold, None), |(candidate, _)| (Move::Engage, Some(candidate)))
}

/// What a step to `candidate` scores to engage, as `mind` shapes it
fn weigh(candidate: &Candidate, mind: &Mind) -> f32 {
    let ground = Ground { candidate: *candidate };
    score(1.0, ENGAGE.iter().map(|consideration| mind.shape(consideration).answer(&ground)))
}

/// What a step to engage is weighed by
pub const ENGAGE: &[Consideration<Ground>] = &[DETOUR, BEHIND];

/// Seconds more the step costs it on its way to strike than the best one:
/// the best step answers whole however far it has to go
const DETOUR: Consideration<Ground> = Consideration {
    name: "detour",
    read: |ground| ground.candidate.detour,
    bounds: (0.0, 1.0),
    curve: Curve::FALLING,
};

/// How far round toward its target's back the tile stands, every step
/// round worth more until it stands to its target's side, past its forward
/// faces, where a target cannot strike back without the arc to. Weighed
/// only as far as a mind lowers its floor
const BEHIND: Consideration<Ground> = Consideration {
    name: "behind",
    read: |ground| ground.candidate.behind,
    bounds: (0.0, 0.5),
    curve: Curve::RISING.floored(1.0),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile `q` east of the origin, with its target standing further
    /// east: struck from in two tiles
    fn candidate(q: i32, target: i32) -> Candidate {
        let gap = (target - q).abs();
        Candidate {
            tile: Qrz { q, r: 0, z: 0 },
            time_to_strike: (gap - 2).max(0) as f32 * 0.25,
            detour: 0.0,
            behind: 0.0,
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

    #[test]
    fn it_steps_toward_where_it_strikes_soonest_and_stands_once_there() {
        let mind = Mind::default();
        let (decision, step) = choose(&around(0, 8), Move::Hold, &mind);
        assert_eq!((decision, step.map(|step| step.tile.q)), (Move::Engage, Some(1)), "far off, it steps in");
        let (decision, step) = choose(&around(6, 8), Move::Engage, &mind);
        assert_eq!((decision, step.map(|step| step.tile.q)), (Move::Engage, Some(6)), "in reach, it keeps to its tile");
    }

    #[test]
    fn a_mind_weighing_its_targets_back_steps_round_and_stands_once_past_its_faces() {
        let mut minds = crate::behaviour::mind::Minds::default();
        minds.set("all.behind.floor", "0.2").unwrap();
        let mind = minds.mind(None);
        let mut candidates = around(6, 8);
        candidates[1].behind = 0.17;
        let (_, step) = choose(&candidates, Move::Engage, &mind);
        assert_eq!(step.map(|step| step.tile.q), Some(7), "faced, any step round is worth taking");
        candidates[1].behind = 0.5;
        candidates[0].behind = 0.5;
        let (_, step) = choose(&candidates, Move::Engage, &mind);
        assert_eq!(step.map(|step| step.tile.q), Some(6), "past its faces already, it stands and strikes");
        candidates[0].behind = 0.0;
        let plain = choose(&candidates, Move::Engage, &Mind::default()).1;
        assert_eq!(plain.map(|step| step.tile.q), Some(6), "a mind that does not weigh it keeps to its tile");
    }

}
