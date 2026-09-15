//! Flat-top hex placement in world space, shared by every lattice scale.

/// Tile center world position (x, z_world) for a flat-top hex of the given
/// radius: x = 1.5·q·radius, z = (√3/2·q + √3·r)·radius.
pub fn flat_top_tile_center(q: i32, r: i32, radius: f32) -> (f32, f32) {
    let x = (1.5 * q as f64 * radius as f64) as f32;
    let z = ((q as f64 * (3.0_f64).sqrt() / 2.0 + r as f64 * (3.0_f64).sqrt()) * radius as f64)
        as f32;
    (x, z)
}
