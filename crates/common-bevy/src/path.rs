//! Finding a way over the ground a walker can take: A* over the map's
//! tiles, a step at a time to a neighbour no more than a level up or
//! down and not full of what stands on it. A cliff is no step, so a way
//! goes round it.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
};

use qrz::Qrz;

use crate::{resources::map::Map, systems::movement::is_full_cover};

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
/// tile, and the same map gives the same way.
pub fn find(map: &Map, from: Qrz, goal: Qrz, near: i32) -> Option<Vec<Qrz>> {
    let reached = |at: Qrz| at.flat_distance(&goal) <= near && (near == 0 || (at.q, at.r) != (goal.q, goal.r));
    let key = |at: Qrz| (at.q, at.r);
    // Each tile opened: how many steps it took, and the tile it was stepped from
    let mut came: HashMap<(i32, i32), (i32, Qrz)> = HashMap::from([(key(from), (0, from))]);
    let mut open = BinaryHeap::from([Reverse((from.flat_distance(&goal), 0, from.q, from.r, from.z))]);
    while let Some(Reverse((_, steps, q, r, z))) = open.pop() {
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
            open.push(Reverse((steps + 1 + next.flat_distance(&goal), steps + 1, next.q, next.r, next.z)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::entity_type::{decorator::Decorator, EntityType};

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
