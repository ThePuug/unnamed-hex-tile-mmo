//! The tile grid's dimensions, shared by everything that stands a tile in
//! the world: the server, the client and the arena build their maps on
//! them, and the terrain surface and the dens measure by them.

/// World-space height per z-level: tan 34° over a tile's spacing of
/// `HEX_RADIUS * sqrt(3)`, so a step of one level per tile climbs at the
/// angle loose rock rests at, and walkable ground is one level per tile.
pub const RISE: f32 = 1.168_283;

/// World-space size of one hexagon — centre to vertex. Neighbouring tile
/// centres sit `HEX_RADIUS * sqrt(3)` apart, so together with [`RISE`] this
/// fixes the ratio between a z-level and a tile step, and with it the angle
/// any given gradient reads as on screen.
pub const HEX_RADIUS: f32 = 1.0;

/// A tile's ground: the level it stands at on the map, and where its
/// surface runs through, which rounds to that level. Whether a walker may
/// step, what a wall divides and what water covers are the level's; the
/// drawn ground and the feet on it follow the height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Ground {
    pub z: i32,
    /// The height above the level, in [`Ground::FINE`]ths of one: within
    /// half a level either way.
    pub fine: i8,
}

impl Ground {
    /// How many steps of `fine` make a level.
    pub const FINE: f64 = 256.0;

    /// Ground standing exactly at level `z`.
    pub const fn level(z: i32) -> Self {
        Ground { z, fine: 0 }
    }

    /// The ground whose surface runs through `height`, in levels: its level
    /// is the nearest, a half rounding up.
    pub fn through(height: f64) -> Self {
        let z = (height + 0.5).floor();
        Ground { z: z as i32, fine: ((height - z) * Self::FINE).round().clamp(-128.0, 127.0) as i8 }
    }

    /// The height its surface runs through, in levels above level `from`:
    /// exact however high the ground stands, where a height above zero
    /// would keep only a float's steps.
    pub fn above(self, from: i32) -> f32 {
        (self.z - from) as f32 + self.fine as f32 / Self::FINE as f32
    }

    /// The height its surface runs through, in levels.
    pub fn height(self) -> f64 {
        self.z as f64 + self.fine as f64 / Self::FINE
    }
}

impl From<i32> for Ground {
    fn from(z: i32) -> Self {
        Ground::level(z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A height's ground stands at its nearest level and runs through the
    /// height to within a step of `fine`, at any height.
    #[test]
    fn ground_keeps_its_level_and_its_height() {
        for base in [-40_000.0, -1.0, 0.0, 7.0, 300.0, 2_000_000.0] {
            for k in -8..=8 {
                let height = base + k as f64 / 16.0;
                let ground = Ground::through(height);
                assert_eq!(ground.z as f64, (height + 0.5).floor(), "{height}");
                assert!((ground.height() - height).abs() <= 1.0 / Ground::FINE, "{height} kept as {}", ground.height());
                assert!((ground.above(ground.z) as f64 - (ground.height() - ground.z as f64)).abs() < 1e-6);
            }
        }
        assert_eq!(Ground::from(5), Ground::through(5.0));
    }
}
