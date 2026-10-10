//! Hex tile coordinates on the plane, a tile the unit: neighbouring tiles
//! sit one unit apart, so the world's generators and the client's metrics
//! name a tile's place in the same numbers.

const SQRT_3: f64 = 1.7320508075688772;

/// Convert hex tile coordinates to world (cartesian) coordinates.
/// Hex q,r axes are 60° apart; this produces isotropic x,y.
pub fn hex_to_world(q: i32, r: i32) -> (f64, f64) {
    let qf = q as f64;
    let rf = r as f64;
    (qf + rf * 0.5, rf * SQRT_3 / 2.0)
}

/// Inverse of hex_to_world: convert world coordinates to nearest hex (q, r).
pub fn world_to_hex(wx: f64, wy: f64) -> (i32, i32) {
    let r = (wy * 2.0 / SQRT_3).round() as i32;
    let q = (wx - r as f64 * 0.5).round() as i32;
    (q, r)
}
