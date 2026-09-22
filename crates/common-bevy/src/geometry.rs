//! Flat-top hex placement in world space, shared by every lattice scale.

/// Tile center world position (x, z_world) for a flat-top hex of the given
/// radius: x = 1.5·q·radius, z = (√3/2·q + √3·r)·radius.
pub fn flat_top_tile_center(q: i32, r: i32, radius: f32) -> (f32, f32) {
    let x = (1.5 * q as f64 * radius as f64) as f32;
    let z = ((q as f64 * (3.0_f64).sqrt() / 2.0 + r as f64 * (3.0_f64).sqrt()) * radius as f64)
        as f32;
    (x, z)
}

/// Where slot `k` of tile `(q, r)` stands, in world xz: a share of the way
/// from the tile's centre toward the neighbour's, then the tree's own sway
/// along that way and across it.
pub fn slot_center(q: i32, r: i32, k: usize, sway: &common::Sway) -> (f32, f32) {
    let (cx, cz) = flat_top_tile_center(q, r, 1.0);
    let (dq, dr) = common::SLOTS[k];
    let (nx, nz) = flat_top_tile_center(q + dq, r + dr, 1.0);
    let (dx, dz) = (nx - cx, nz - cz);
    let spacing = dx.hypot(dz);
    let (ux, uz) = (dx / spacing, dz / spacing);
    let along = (common::SLOT_SHARE as f32 + sway.along as f32) * spacing;
    let across = sway.across as f32 * spacing;
    (cx + ux * along - uz * across, cz + uz * along + ux * across)
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    /// Every slot, swayed to its limit either way, stays inside its tile:
    /// nearer its own centre than any neighbour's.
    #[test]
    fn a_slot_stays_on_its_tile() {
        let extremes = [-common::SLOT_JITTER, common::SLOT_JITTER];
        for (q, r) in [(0, 0), (7, -3), (-100_000, 250_000)] {
            let (cx, cz) = flat_top_tile_center(q, r, 1.0);
            for k in 0..common::SLOTS.len() {
                for &along in &extremes {
                    for &across in &extremes {
                        let sway = common::Sway { along, across, yaw: 0.0, growth: 1.0, variation: 0 };
                        let (x, z) = slot_center(q, r, k, &sway);
                        let own = (x - cx).hypot(z - cz);
                        for (dq, dr) in [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)] {
                            let (nx, nz) = flat_top_tile_center(q + dq, r + dr, 1.0);
                            assert!((x - nx).hypot(z - nz) > own, "slot {k} of ({q}, {r}) lies nearer ({dq}, {dr})");
                        }
                    }
                }
            }
        }
    }
}
