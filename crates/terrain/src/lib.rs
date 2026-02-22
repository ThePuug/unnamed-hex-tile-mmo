// ──── Public API Types ────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlateId {
    pub cell_q: i32,
    pub cell_r: i32,
}

#[derive(Clone, Debug)]
pub struct TerrainEval {
    pub height: i32,
}

// ──── Terrain ────

pub struct Terrain {
    seed: u64,
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn get_height(&self, _q: i32, _r: i32) -> i32 {
        0
    }

    pub fn evaluate(&self, _q: i32, _r: i32) -> TerrainEval {
        TerrainEval { height: 0 }
    }
}

// ──── Tests ────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_height() {
        let t1 = Terrain::new(42);
        let t2 = Terrain::new(42);
        for q in -50..50 {
            for r in -50..50 {
                assert_eq!(t1.get_height(q, r), t2.get_height(q, r));
            }
        }
    }
}
