use bevy::prelude::*;
use std::ops::Deref;

/// Bevy Resource wrapper around the terrain generation library.
#[derive(Resource)]
pub struct Terrain(pub terrain::Terrain);

impl Default for Terrain {
    fn default() -> Self {
        Self(terrain::Terrain::default())
    }
}

impl Terrain {
    #[allow(dead_code)]
    pub fn new(seed: u64) -> Self {
        Self(terrain::Terrain::new(seed))
    }

    pub fn get(&self, q: i32, r: i32) -> i32 {
        self.0.get_height(q, r)
    }
}

impl Deref for Terrain {
    type Target = terrain::Terrain;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Critical alignment test: server wrapper must produce identical heights
    /// to the terrain crate. If this fails, the viewer is lying.
    #[test]
    fn server_terrain_matches_library() {
        let server = Terrain::default();
        let library = terrain::Terrain::default();

        let coords: Vec<(i32, i32)> = vec![
            (0, 0), (100, -50), (-5000, 3000), (2500, 2500),
            (10000, -8000), (-3333, 7777), (1, 1), (-1, -1),
        ];

        for (q, r) in coords {
            assert_eq!(
                server.get(q, r),
                library.get_height(q, r),
                "Server and library heights differ at ({q}, {r})"
            );
        }
    }
}
