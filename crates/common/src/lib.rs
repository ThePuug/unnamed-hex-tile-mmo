pub mod plate_tags;
pub use plate_tags::{PlateTag, Tagged, MAX_PLATE_TAGS};
pub use tinyvec::ArrayVec;

/// Topographic color ramp: (elevation, R, G, B) with sea level at 0.
/// Shared between game renderer and terrain-viewer so they never drift apart.
pub const ELEVATION_RAMP: &[(f64, u8, u8, u8)] = &[
    (-200.0, 10, 20, 80),    // deep ocean
    (-50.0, 30, 60, 160),    // ocean
    (-10.0, 60, 130, 200),   // shallow ocean
    (0.0, 80, 180, 180),     // sea level / coastal
    (20.0, 80, 160, 80),     // lowland green
    (100.0, 120, 170, 60),   // mid elevation
    (200.0, 180, 170, 50),   // yellow
    (400.0, 170, 120, 50),   // brown
    (700.0, 200, 190, 180),  // light brown / gray
    (1000.0, 255, 255, 255), // white peaks
];

/// Interpolate the elevation color ramp. Returns (R, G, B) in 0-255.
pub fn elevation_color_rgb(height: i32) -> (u8, u8, u8) {
    let h = height as f64;
    if h <= ELEVATION_RAMP[0].0 {
        return (ELEVATION_RAMP[0].1, ELEVATION_RAMP[0].2, ELEVATION_RAMP[0].3);
    }
    let last = ELEVATION_RAMP[ELEVATION_RAMP.len() - 1];
    if h >= last.0 {
        return (last.1, last.2, last.3);
    }
    for i in 0..ELEVATION_RAMP.len() - 1 {
        let (e0, r0, g0, b0) = ELEVATION_RAMP[i];
        let (e1, r1, g1, b1) = ELEVATION_RAMP[i + 1];
        if h >= e0 && h < e1 {
            let t = (h - e0) / (e1 - e0);
            return (
                (r0 as f64 + (r1 as f64 - r0 as f64) * t) as u8,
                (g0 as f64 + (g1 as f64 - g0 as f64) * t) as u8,
                (b0 as f64 + (b1 as f64 - b0 as f64) * t) as u8,
            );
        }
    }
    (128, 128, 128)
}
