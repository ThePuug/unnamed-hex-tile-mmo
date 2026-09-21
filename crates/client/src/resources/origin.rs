//! The floating origin the world is drawn about. Positions are exact as a
//! tile and an offset, but a world vector tens of thousands of units out
//! keeps only millimetres, and an actor whose joints move by less than
//! that shakes on screen. So every rendered `Transform` is world minus the
//! origin, a tile near the player, computed from the tile difference so
//! nothing large is ever subtracted from anything large. The physics, the
//! map and the wire stay in world coordinates. The origin lies on the sea's
//! plane, so a rendered height is a world height: the terrain shader reads
//! its elevation and the sea its level from the position as rendered.

use bevy::prelude::*;
use qrz::{Convert, Qrz};

use common_bevy::components::position::{Position, VisualPosition};
use common_bevy::resources::map::Map;

/// Tiles the player may stray from the origin before it follows them.
/// Well inside the distance at which a float step beneath the player
/// would show, and far enough that a shift is rare.
pub const REBASE_TILES: i32 = 256;

/// The tile the world is drawn about, at `z = 0`, and its world vector.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct RenderOrigin {
    tile: Qrz,
    world: Vec3,
}

impl Default for RenderOrigin {
    fn default() -> Self {
        Self { tile: Qrz::default(), world: Vec3::ZERO }
    }
}

impl RenderOrigin {
    pub fn tile(&self) -> Qrz {
        self.tile
    }

    /// The origin's world vector: what a rendered position is short of its
    /// world position.
    pub fn world_vec(&self) -> Vec3 {
        self.world
    }

    /// Where `position` is drawn: its tile taken relative to the origin's
    /// before either becomes a vector, so the result is exact however far
    /// out the world is.
    pub fn render(&self, map: &Map, position: &Position) -> Vec3 {
        self.render_tile(map, position.tile) + position.offset
    }

    /// Where the centre of `tile` is drawn.
    pub fn render_tile(&self, map: &Map, tile: Qrz) -> Vec3 {
        map.convert(tile - self.tile)
    }

    /// Where a world vector is drawn. A subtraction of two large vectors,
    /// exact only to the world's own precision: for what already lives as
    /// a vector, never for a position that has a tile.
    pub fn render_world(&self, world: Vec3) -> Vec3 {
        world - self.world
    }

    /// The world vector of a rendered one, for the map.
    pub fn world(&self, render: Vec3) -> Vec3 {
        render + self.world
    }

    /// Moves the origin to the column of `tile`. Returns the vector every
    /// rendered transform and visual must move by, exact as a tile
    /// difference.
    pub fn rebase(&mut self, map: &Map, tile: Qrz) -> Vec3 {
        let tile = Qrz { z: 0, ..tile };
        let shift: Vec3 = map.convert(self.tile - tile);
        self.tile = tile;
        self.world = map.convert(tile);
        shift
    }
}

/// Keeps the origin near the player: when the player's tile is more than
/// `REBASE_TILES` from it, the origin moves to their tile and every rendered
/// root transform and every visual shifts by the difference in one pass,
/// before anything reads them this frame. UI roots and the overlay camera
/// are not of the world and stay.
pub fn rebase_origin(
    mut origin: ResMut<RenderOrigin>,
    map: Res<Map>,
    player: Query<&Position, With<common_bevy::components::behaviour::PlayerControlled>>,
    mut roots: Query<&mut Transform, (Without<ChildOf>, Without<Node>, Without<Camera2d>)>,
    mut visuals: Query<&mut VisualPosition>,
) {
    let Ok(position) = player.single() else { return };
    if position.tile.flat_distance(&origin.tile()) <= REBASE_TILES {
        return;
    }
    let shift = origin.rebase(&map, position.tile);
    for mut transform in &mut roots {
        transform.translation += shift;
    }
    for mut visual in &mut visuals {
        visual.shift(shift);
    }
    info!("Render origin moved to {:?}", origin.tile());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::new(qrz::Map::new(1.0, 0.8, qrz::HexOrientation::FlatTop))
    }

    /// Far from the world's origin a rendered position keeps the offset's
    /// precision: two positions a hair apart render a hair apart, where the
    /// world vectors of both would round to the same float step.
    #[test]
    fn a_rendered_position_keeps_its_precision_far_out() {
        let map = map();
        let tile = Qrz { q: -58244, r: 5068, z: 507 };
        let mut origin = RenderOrigin::default();
        origin.rebase(&map, Qrz { q: tile.q + 3, r: tile.r - 2, z: tile.z });
        let a = Position::new(tile, Vec3::new(0.25, 0.8, -0.4));
        let b = Position { offset: a.offset + Vec3::new(0.0006, 0.0, 0.0), ..a };
        let (ra, rb) = (origin.render(&map, &a), origin.render(&map, &b));
        assert!((rb.x - ra.x - 0.0006).abs() < 1e-6, "rendered {} apart", rb.x - ra.x);
        assert_eq!(a.to_world(&map).x, b.to_world(&map).x, "the world vectors cannot tell them apart");
        assert!(ra.xz().length() < 20.0, "drawn near the origin: {ra}");
        assert_eq!(ra.y, a.to_world(&map).y, "a rendered height is a world height");
    }

    /// A rebase moves the origin and reports the shift that keeps every
    /// rendered position where it was: render before equals render after
    /// plus the shift, and the origin's world vector is the new tile's.
    #[test]
    fn a_rebase_shifts_everything_by_the_tile_difference() {
        let map = map();
        let mut origin = RenderOrigin::default();
        origin.rebase(&map, Qrz { q: 100, r: 40, z: 3 });
        let position = Position::new(Qrz { q: 130, r: 35, z: 5 }, Vec3::new(-0.3, 0.1, 0.7));
        let before = origin.render(&map, &position);
        let shift = origin.rebase(&map, Qrz { q: 128, r: 36, z: 5 });
        let after = origin.render(&map, &position);
        assert!((before + shift - after).length() < 1e-5, "{before} + {shift} != {after}");
        let world: Vec3 = map.convert(Qrz { q: 128, r: 36, z: 0 });
        assert_eq!(origin.world_vec(), world, "the origin lies on the sea's plane");
        assert!((origin.world(after) - position.to_world(&map)).length() < 1e-2);
    }
}
