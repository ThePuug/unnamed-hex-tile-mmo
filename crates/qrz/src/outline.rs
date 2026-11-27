//! The tightest outline round a set of tiles: its corners, its length, and
//! the tiles it holds. An outline has no hollow, whatever it is drawn
//! round, so ground made from one is in one piece with the shortest edge
//! its tiles allow.
//!
//! Tiles are laid on the flat with a tile's width the unit, so a length
//! here is in tiles. Elevation plays no part.

/// A point on the flat.
pub type Point = (f64, f64);

/// How many points stand for the round drawn about a tile.
const ROUND: usize = 24;

/// Where the middle of tile `(q, r)` lies on the flat: neighbours are one
/// apart.
pub fn flat((q, r): (i32, i32)) -> Point {
    (q as f64 + r as f64 / 2.0, r as f64 * 3f64.sqrt() / 2.0)
}

/// How far apart two points lie.
pub fn apart(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The corners, in turn, of the tightest outline round `points`. Fewer
/// than three points that differ are their own outline.
pub fn outline(points: &[Point]) -> Vec<Point> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    sorted.dedup();
    if sorted.len() < 3 {
        return sorted;
    }
    let turn = |o: Point, a: Point, b: Point| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let half = |points: &mut dyn Iterator<Item = Point>| {
        let mut side: Vec<Point> = Vec::new();
        for p in points {
            while side.len() > 1 && turn(side[side.len() - 2], side[side.len() - 1], p) <= 0.0 {
                side.pop();
            }
            side.push(p);
        }
        side.pop();
        side
    };
    let mut corners = half(&mut sorted.iter().copied());
    corners.extend(half(&mut sorted.iter().rev().copied()));
    corners
}

/// How far round the tightest outline of `tiles` is: none for one tile,
/// there and back for tiles in a line.
pub fn girth(tiles: &[(i32, i32)]) -> f64 {
    around(&corners(tiles))
}

/// The corners of the tightest outline round `tiles`, in turn. An outline
/// round these and more is the outline round `tiles` and more, so what a
/// further tile adds to a large set is found from its corners alone.
pub fn corners(tiles: &[(i32, i32)]) -> Vec<Point> {
    outline(&tiles.iter().map(|&tile| flat(tile)).collect::<Vec<_>>())
}

/// How far round the tightest outline of `points` is.
pub fn around(points: &[Point]) -> f64 {
    let corners = outline(points);
    (0..corners.len()).map(|k| apart(corners[k], corners[(k + 1) % corners.len()])).sum()
}

/// Every tile whose middle lies inside the tightest outline round `tiles`
/// drawn `past` tiles out from each, in `q` then `r` order. None for no
/// tiles.
pub fn enclosed(tiles: &[(i32, i32)], past: f64) -> Vec<(i32, i32)> {
    let round = |&(x, y): &Point| {
        (0..ROUND).map(move |k| {
            let turn = k as f64 * std::f64::consts::TAU / ROUND as f64;
            (x + past * turn.cos(), y + past * turn.sin())
        })
    };
    // Only a corner tile's round reaches the outline of them all
    let corners = outline(&corners(tiles).iter().flat_map(round).collect::<Vec<_>>());
    if corners.len() < 3 {
        return tiles.to_vec();
    }
    let inside = |(x, y): Point| (0..corners.len()).all(|k| {
        let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0) >= -1e-9
    });
    let (low, high) = corners.iter().fold(((f64::MAX, f64::MAX), (f64::MIN, f64::MIN)), |(low, high), p| ((low.0.min(p.0), low.1.min(p.1)), (high.0.max(p.0), high.1.max(p.1))));
    let rise = 3f64.sqrt() / 2.0;
    let mut held = Vec::new();
    for r in (low.1 / rise).floor() as i32..=(high.1 / rise).ceil() as i32 {
        // An outline has no hollow, so a row's tiles inside it are one run: found from its two ends
        let mut first = (low.0 - r as f64 / 2.0).floor() as i32;
        let mut last = (high.0 - r as f64 / 2.0).ceil() as i32;
        while first <= last && !inside(flat((first, r))) {
            first += 1;
        }
        while last > first && !inside(flat((last, r))) {
            last -= 1;
        }
        held.extend((first..=last).map(|q| (q, r)));
    }
    held.sort();
    held
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex_distance;

    /// Neighbours lie one apart on the flat, whichever way.
    #[test]
    fn neighbours_are_one_apart() {
        for d in crate::DIRECTIONS {
            assert!((apart(flat((0, 0)), flat((d.q, d.r))) - 1.0).abs() < 1e-12);
        }
    }

    /// An outline keeps only the corners: a tile inside the others' and
    /// one on an edge between two of them are no corner of it.
    #[test]
    fn an_outline_is_its_corners() {
        let square = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
        let with = [&square[..], &[(2.0, 2.0), (2.0, 0.0)]].concat();
        assert_eq!(outline(&with).len(), 4);
        assert_eq!(outline(&square[..2]).len(), 2);
    }

    /// Tiles in a line are there and back round; a tile inside an outline
    /// adds nothing to it, and one outside lengthens it.
    #[test]
    fn girth_follows_the_outermost() {
        assert_eq!(girth(&[(0, 0)]), 0.0);
        assert!((girth(&[(0, 0), (3, 0)]) - 6.0).abs() < 1e-9);
        let ring = [(3, 0), (-3, 0), (0, 3), (0, -3), (3, -3), (-3, 3)];
        let round = girth(&ring);
        assert_eq!(girth(&[&ring[..], &[(0, 0), (1, 1)]].concat()), round);
        assert!(girth(&[&ring[..], &[(6, 0)]].concat()) > round);
    }

    /// What an outline holds has no hollow: it holds the tiles it is
    /// drawn round, every tile between two it holds, and nothing far
    /// past them. Drawn round one tile it is that tile's round.
    #[test]
    fn what_an_outline_holds_has_no_hollow() {
        let ends = [(0, 0), (12, 0), (0, 12)];
        let held = enclosed(&ends, 3.0);
        for tile in ends {
            assert!(held.contains(&tile));
        }
        for (i, &a) in held.iter().enumerate() {
            for &b in &held[i + 1..] {
                // The tile nearest the middle of any two held is held
                let (ax, ay) = flat(a);
                let (bx, by) = flat(b);
                let mid = ((ax + bx) / 2.0, (ay + by) / 2.0);
                let near = held.iter().map(|&t| apart(flat(t), mid)).fold(f64::MAX, f64::min);
                assert!(near <= 0.6, "the ground between {a:?} and {b:?} is held");
            }
        }
        assert!(held.iter().all(|&tile| ends.iter().any(|&end| hex_distance(tile, end) <= 12 + 4)));
        let one = enclosed(&[(5, -2)], 3.0);
        assert!(one.iter().all(|&tile| hex_distance(tile, (5, -2)) <= 3) && one.len() > 19);
        assert!(enclosed(&[], 3.0).is_empty());
    }
}
