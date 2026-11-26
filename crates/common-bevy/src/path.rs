//! Finding a way over the ground a walker can take: A* over the map's
//! tiles, a step at a time to a neighbour no more than a level up or
//! down and not full of what stands on it. A cliff is no step, so a way
//! goes round it.
//!
//! A way is tiles, and tiles turn by sixths of a circle, so one walked a
//! tile at a time staggers across open ground. A walker instead heads for
//! the furthest tile of its way it can walk straight to, a corner of the
//! way ([`corners`]) found once with it, and turns only at those.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
};

use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use qrz::{Convert, Qrz};

use crate::{resources::map::Map, systems::movement::is_full_cover};

/// How many tiles of its way a walker looks along for one to walk
/// straight to: far enough to cross open ground without a turn, near
/// enough that a heading, which is one of a few bearings, has not carried
/// it a tile off the line looked along.
pub const LOOK: usize = 8;

/// Whether a walker on the floor tile `from` walks straight to the floor
/// tile `to`: each tile the line between their centres crosses is a step
/// from the one before, by the rule of [`find`].
fn straight(map: &Map, from: Qrz, to: Qrz) -> bool {
    let end: Vec3 = map.convert(Qrz { q: to.q - from.q, r: to.r - from.r, z: 0 });
    let dir = end.xz().normalize_or_zero();
    // The line in the frame of `from`: its tiles as differences from it
    let (mut pos, mut over, mut at) = (Vec2::ZERO, Qrz { q: 0, r: 0, z: 0 }, from);
    for _ in 0..=2 * from.flat_distance(&to) {
        if (at.q, at.r) == (to.q, to.r) {
            return true;
        }
        let (far, next) = map.exit(pos, dir, over);
        let Some((floor, _)) = map.get_by_qr(from.q + next.q, from.r + next.r) else { return false };
        if !far.is_finite() || (floor.z - at.z).abs() > 1 || is_full_cover(map, floor.q, floor.r) {
            return false;
        }
        (pos, over, at) = (pos + dir * far, next, floor);
    }
    false
}

/// The corners of `way`, the tiles stepped onto after the floor tile
/// `from`: each the furthest tile of the way, no more than [`LOOK`] on,
/// that a walker walks straight to from the corner before it, `from`
/// before the first; where it walks straight to none, the next tile. The
/// way's last tile is the last corner. A walker heading for each in turn
/// walks the way and turns only at them.
pub fn corners(map: &Map, from: Qrz, way: &[Qrz]) -> Vec<Qrz> {
    let mut corners = Vec::new();
    let (mut at, mut done) = (from, 0);
    while done < way.len() {
        let reach = (done + LOOK).min(way.len());
        let next = (done + 1..reach).rev().find(|&k| straight(map, at, way[k])).unwrap_or(done);
        corners.push(way[next]);
        (at, done) = (way[next], next + 1);
    }
    corners
}

/// The way [`find`] gives, as a walker takes it: its [`corners`], found
/// once with the way.
pub fn walk(map: &Map, from: Qrz, goal: Qrz, near: i32) -> Option<Vec<Qrz>> {
    find(map, from, goal, near).map(|way| corners(map, from, &way))
}

/// The most tiles a search opens before it gives up: a way round a cliff
/// a camp's width off is found well inside it, and a goal walled off
/// costs no more than this.
pub const SEARCH: usize = 4000;

/// The way from the floor tile `from` to the nearest floor tile within
/// `near` tiles of `goal`, as the tiles stepped onto after `from`, in
/// order: empty where `from` is there already. None where the map holds
/// no such way within [`SEARCH`] tiles. With `near` at none the way ends
/// on `goal`; at one it ends beside it, wherever `goal` can be stood on or
/// not. No step of it climbs or drops more than a level or enters a full
/// tile, and the same map gives the same way. Of the ways as short it is
/// one that keeps by the straight line from `from` to `goal`: many tie on
/// steps, and a walker crosses its way in straight lines, so one that
/// strays from the line is walked as bends.
pub fn find(map: &Map, from: Qrz, goal: Qrz, near: i32) -> Option<Vec<Qrz>> {
    let reached = |at: Qrz| at.flat_distance(&goal) <= near && (near == 0 || (at.q, at.r) != (goal.q, goal.r));
    let key = |at: Qrz| (at.q, at.r);
    // How far a tile lies off the line from `from` to `goal`, as the area
    // the three span: whole numbers, so ties fall the same way everywhere
    let line = ((goal.q - from.q) as i64, (goal.r - from.r) as i64);
    let off = |at: Qrz| ((at.q - from.q) as i64 * line.1 - (at.r - from.r) as i64 * line.0).abs();
    // Each tile opened: how many steps it took, and the tile it was stepped from
    let mut came: HashMap<(i32, i32), (i32, Qrz)> = HashMap::from([(key(from), (0, from))]);
    let mut open = BinaryHeap::from([Reverse((from.flat_distance(&goal), 0i64, 0, from.q, from.r, from.z))]);
    while let Some(Reverse((_, _, steps, q, r, z))) = open.pop() {
        let at = Qrz { q, r, z };
        if came.get(&key(at)).is_some_and(|&(best, _)| best < steps) {
            continue;
        }
        if reached(at) {
            let mut way = Vec::new();
            let mut back = at;
            while key(back) != key(from) {
                way.push(back);
                back = came[&key(back)].1;
            }
            way.reverse();
            return Some(way);
        }
        if came.len() >= SEARCH {
            continue;
        }
        for (next, _) in map.neighbors(at) {
            if is_full_cover(map, next.q, next.r) || came.get(&key(next)).is_some_and(|&(best, _)| best <= steps + 1) {
                continue;
            }
            came.insert(key(next), (steps + 1, at));
            open.push(Reverse((steps + 1 + next.flat_distance(&goal), off(next), steps + 1, next.q, next.r, next.z)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::entity_type::{decorator::Decorator, EntityType};

    /// Over open ground a way's corners are few, each a straight walk
    /// from the one before and no further than a walker looks, the last
    /// the way's end; a full tile or a cliff on a line makes a corner
    /// before it, and with nothing straight ahead the next tile is one.
    #[test]
    fn a_ways_corners_are_where_a_walker_turns() {
        let from = Qrz { q: 0, r: 0, z: 0 };
        let open = ground(12, |_, _| (0, common::Cover::NONE));
        let goal = Qrz { q: 9, r: -4, z: 0 };
        let way = find(&open, from, goal, 0).expect("open ground");
        let turns = walk(&open, from, goal, 0).unwrap();
        assert_eq!(turns, corners(&open, from, &way));
        assert!(turns.len() <= way.len().div_ceil(LOOK) + 1 && turns.last() == way.last(), "{turns:?} of {way:?}");
        let mut at = from;
        for &corner in &turns {
            assert!(way.contains(&corner) && straight(&open, at, corner) && at.flat_distance(&corner) <= LOOK as i32);
            at = corner;
        }
        assert!(corners(&open, from, &[]).is_empty());

        // East along a row the line crosses each tile of the row in turn
        let row: Vec<Qrz> = (1..=6).map(|q| Qrz { q, r: 0, z: 0 }).collect();
        assert_eq!(corners(&open, from, &row), vec![row[5]]);
        let full = (0..common::TILE_SLOTS as usize).fold(common::Cover::NONE, |c, k| c.with_boulder(k));
        let filled = ground(12, |q, r| (0, if (q, r) == (3, 0) { full } else { common::Cover::NONE }));
        let cliff = ground(12, |q, r| (if (q, r) == (3, 0) { 5 } else { 0 }, common::Cover::NONE));
        for barred in [filled, cliff] {
            assert_eq!(corners(&barred, from, &row)[..2], [row[1], row[2]], "one before what bars the line, then the next tile");
        }
    }

    /// Flat ground `radius` tiles round the origin, each tile's height
    /// and cover as `lies` says.
    fn ground(radius: i32, lies: impl Fn(i32, i32) -> (i32, common::Cover)) -> Map {
        let map = Map::new(qrz::Map::new(1.0, 0.8));
        for q in -radius..=radius {
            for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
                let (z, cover) = lies(q, r);
                map.insert(Qrz { q, r, z }, EntityType::Decorator(Decorator { cover, is_solid: true }));
            }
        }
        map
    }

    fn floor(map: &Map, q: i32, r: i32) -> Qrz {
        map.get_by_qr(q, r).expect("the ground holds it").0
    }

    /// Over open ground a way is as short as the tiles between, each step
    /// to a neighbour, and the same every time.
    #[test]
    fn a_way_over_open_ground_is_the_shortest() {
        let map = ground(8, |_, _| (0, common::Cover::NONE));
        let (from, goal) = (floor(&map, -5, 0), floor(&map, 5, -2));
        let way = find(&map, from, goal, 0).expect("open ground");
        assert_eq!(way.len() as i32, from.flat_distance(&goal));
        assert_eq!(way.last(), Some(&goal));
        let mut at = from;
        for &step in &way {
            assert_eq!(at.flat_distance(&step), 1);
            at = step;
        }
        assert_eq!(find(&map, from, goal, 0), Some(way));
        assert_eq!(find(&map, from, from, 0), Some(Vec::new()));
    }

    /// Over open ground a way keeps by the straight line to its goal,
    /// whichever way that runs, so its corners are walked without a bend.
    #[test]
    fn a_way_over_open_ground_keeps_by_the_line() {
        let map = ground(24, |_, _| (0, common::Cover::NONE));
        let from = floor(&map, 0, 0);
        let at = |tile: Qrz| -> Vec2 { map.convert(Qrz { q: tile.q - from.q, r: tile.r - from.r, z: 0 }).xz() };
        let tile = at(Qrz { q: 1, r: 0, z: 0 }).length();
        for (q, r) in [(12, -6), (-12, 6), (17, -5), (5, 14), (-9, -11), (20, -19), (3, -21), (-16, 2)] {
            let goal = floor(&map, q, r);
            let line = at(goal).normalize();
            let way = find(&map, from, goal, 0).expect("open ground");
            assert_eq!(way.len() as i32, from.flat_distance(&goal));
            for step in &way {
                assert!(at(*step).perp_dot(line).abs() < tile, "{step:?} strays from the line to ({q}, {r}): {way:?}");
            }
            // Each long stretch between corners runs along the line; a
            // short last one may meet the goal from a tile to its side
            let mut last = Vec2::ZERO;
            for corner in walk(&map, from, goal, 0).unwrap() {
                let leg = at(corner) - last;
                assert!(leg.length() < tile * LOOK as f32 / 2.0 || leg.normalize().dot(line) > 0.96, "the walk to ({q}, {r}) bends at {corner:?}");
                last = at(corner);
            }
        }
    }

    /// A cliff across the straight way is gone round: no step climbs it,
    /// and the way is longer for it.
    #[test]
    fn a_way_goes_round_a_cliff() {
        // A wall of high ground on q = 0, open only at its far end
        let cliff = |q: i32, r: i32| q == 0 && r < 6;
        let map = ground(8, |q, r| (if cliff(q, r) { 5 } else { 0 }, common::Cover::NONE));
        let (from, goal) = (floor(&map, -3, 0), floor(&map, 3, 0));
        let way = find(&map, from, goal, 0).expect("round the end");
        assert!(way.len() as i32 > from.flat_distance(&goal));
        assert!(way.iter().all(|step| !cliff(step.q, step.r)), "no step is on the cliff");
        let mut at = from;
        for &step in &way {
            assert!((step.z - at.z).abs() <= 1);
            at = step;
        }
    }

    /// A full tile is not entered, a goal that is one is come beside, and
    /// a goal walled off has no way.
    #[test]
    fn a_way_ends_beside_what_cannot_be_stood_on() {
        let full = (0..common::TILE_SLOTS as usize).fold(common::Cover::NONE, |c, k| c.with_boulder(k));
        let map = ground(6, |q, r| (0, if (q, r) == (2, 0) { full } else { common::Cover::NONE }));
        let (from, goal) = (floor(&map, -2, 0), floor(&map, 2, 0));
        assert_eq!(find(&map, from, goal, 0), None);
        let way = find(&map, from, goal, 1).expect("beside it");
        assert_eq!(way.last().map(|last| last.flat_distance(&goal)), Some(1));
        assert!(way.iter().all(|step| (step.q, step.r) != (2, 0)));

        let ring = |q: i32, r: i32| qrz::hex_distance((q, r), (0, 0)) == 3;
        let walled = ground(6, |q, r| (0, if ring(q, r) { full } else { common::Cover::NONE }));
        assert_eq!(find(&walled, floor(&walled, 5, 0), floor(&walled, 0, 0), 1), None);
    }
}
