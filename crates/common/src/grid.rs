//! The tile grid's dimensions, shared by everything that stands a tile in
//! the world: the server, the client and the arena build their maps on
//! them, and the terrain surface and the dens measure by them.

/// World-space height per z-level.
pub const RISE: f32 = 0.8;

/// World-space size of one hexagon — centre to vertex. Neighbouring tile
/// centres sit `HEX_RADIUS * sqrt(3)` apart, so together with [`RISE`] this
/// fixes the ratio between a z-level and a tile step, and with it the angle
/// any given gradient reads as on screen.
pub const HEX_RADIUS: f32 = 1.0;
