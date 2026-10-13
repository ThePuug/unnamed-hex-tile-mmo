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
