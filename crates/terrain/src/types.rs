// ──── Public API Types ────

#[derive(Clone, Debug)]
pub struct TerrainEval {
    pub height: i32,
    pub temperature: f64,
    pub flow: (f64, f64),
}
