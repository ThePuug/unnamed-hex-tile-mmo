//! Hex tile slope blending — shared between mesh generation and decimation.
//!
//! Each outer vertex of a hex tile gets a Y adjustment based on elevation
//! differences with neighboring tiles. Two neighbor edges touch each vertex;
//! each contributes ±rise/2. When both contribute, max-absolute-value wins.

/// Which two outer vertices lie on the edge facing each direction.
/// Index by DIRECTIONS index: dir d → vertices ((4-d)%6, (5-d)%6).
pub const DIRECTION_TO_VERTICES: [(usize, usize); 6] = [
    (4, 5), // Dir 0 (W):  W(4), NW(5)
    (3, 4), // Dir 1 (SW): SW(3), W(4)
    (2, 3), // Dir 2 (SE): SE(2), SW(3)
    (1, 2), // Dir 3 (E):  E(1), SE(2)
    (0, 1), // Dir 4 (NE): NE(0), E(1)
    (5, 0), // Dir 5 (NW): NW(5), NE(0)
];

/// Compute slope Y-adjustments for each outer vertex of a hex tile.
///
/// Each vertex is on two edges. For each neighboring tile with a different z,
/// the shared edge vertices get ±rise/2 adjustment. When a vertex gets multiple
/// adjustments (from its two edges), the one with largest absolute value wins.
///
/// Returns 6 adjustments to add to the base vertex Y (= tile_z * rise + rise).
pub fn slope_adjustments(
    tile_z: i32,
    rise: f32,
    neighbor_z: impl Fn(usize) -> Option<i32>,
) -> [f32; 6] {
    let mut vertex_adjustments: [Vec<f32>; 6] = Default::default();

    for dir_idx in 0..6 {
        if let Some(nz) = neighbor_z(dir_idx) {
            let elevation_diff = nz - tile_z;
            let adjustment = if elevation_diff > 0 {
                rise * 0.5
            } else if elevation_diff < 0 {
                rise * -0.5
            } else {
                0.0
            };
            if adjustment != 0.0 {
                let (v1, v2) = DIRECTION_TO_VERTICES[dir_idx];
                vertex_adjustments[v1].push(adjustment);
                vertex_adjustments[v2].push(adjustment);
            }
        }
    }

    let mut result = [0.0f32; 6];
    for (i, adjustments) in vertex_adjustments.iter().enumerate() {
        if let Some(&max_adj) = adjustments
            .iter()
            .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
        {
            result[i] = max_adj;
        }
    }
    result
}
