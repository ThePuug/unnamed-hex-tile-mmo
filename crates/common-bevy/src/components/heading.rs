//! A compass bearing in fixed steps: the direction an entity travels and,
//! after it stops, the direction it faces.

use bevy::prelude::*;
use qrz::{Convert, Qrz};
use serde::{Deserialize, Serialize};

use crate::resources::map::Map;

/// Bearings a heading can take, evenly spaced around the compass. A multiple
/// of six, so every tile-to-tile bearing of the hex grid is a heading.
pub const HEADING_SLOTS: u8 = 24;

/// Degrees between adjacent headings.
pub const SLOT_DEGREES: f32 = 360.0 / HEADING_SLOTS as f32;

/// The six tile-to-tile directions on the flat-top grid, clockwise from
/// north.
const HEX_DIRS: [Qrz; 6] = [
    Qrz { q: 0, r: -1, z: 0 },
    Qrz { q: 1, r: -1, z: 0 },
    Qrz { q: 1, r: 0, z: 0 },
    Qrz { q: 0, r: 1, z: 0 },
    Qrz { q: -1, r: 1, z: 0 },
    Qrz { q: -1, r: 0, z: 0 },
];

/// A compass bearing clockwise from north in steps of [`SLOT_DEGREES`].
/// Movement travels along it; combat reads it as the facing. The six
/// tile-to-tile bearings of the flat-top grid are every fourth slot from
/// north.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Heading(u8);

impl Heading {
    pub const NORTH: Heading = Heading(0);

    pub fn from_slot(slot: u8) -> Self {
        Heading(slot % HEADING_SLOTS)
    }

    pub fn slot(self) -> u8 {
        self.0
    }

    pub fn degrees(self) -> f32 {
        self.0 as f32 * SLOT_DEGREES
    }

    /// Alias of [`Heading::degrees`] for the targeting cone.
    pub fn to_angle(self) -> f32 {
        self.degrees()
    }

    /// The nearest heading to a bearing in degrees clockwise from north.
    pub fn from_degrees(degrees: f32) -> Self {
        let slot = (degrees.rem_euclid(360.0) / SLOT_DEGREES).round() as u8;
        Heading(slot % HEADING_SLOTS)
    }

    /// This heading turned clockwise by `slots` (counter-clockwise if negative).
    pub fn turned(self, slots: i32) -> Self {
        Heading((self.0 as i32 + slots).rem_euclid(HEADING_SLOTS as i32) as u8)
    }

    pub fn reversed(self) -> Self {
        self.turned(HEADING_SLOTS as i32 / 2)
    }

    /// Unit direction in the ground plane as (x, z): north is -z, east is +x.
    pub fn to_world_dir(self) -> Vec2 {
        let (sin, cos) = self.degrees().to_radians().sin_cos();
        Vec2::new(sin, -cos)
    }

    /// The heading nearest a ground-plane direction given as (x, z), or None
    /// for a zero vector.
    pub fn from_world_dir(xz: Vec2) -> Option<Self> {
        if xz.length_squared() < 1e-12 {
            return None;
        }
        Some(Self::from_degrees(xz.x.atan2(-xz.y).to_degrees()))
    }

    /// The heading from one world point toward another.
    pub fn toward(from: Vec3, to: Vec3) -> Option<Self> {
        Self::from_world_dir((to - from).xz())
    }

    /// The heading from one tile's centre toward another's.
    pub fn between(map: &Map, from: Qrz, to: Qrz) -> Option<Self> {
        let (from, to): (Vec3, Vec3) = (map.convert(from), map.convert(to));
        Self::toward(from, to)
    }

    /// The heading of a hex offset on the flat-top grid, north for zero.
    pub fn from_hex(offset: Qrz) -> Self {
        let x = 1.5 * offset.q as f32;
        let z = 3f32.sqrt() * (offset.q as f32 / 2.0 + offset.r as f32);
        Self::from_world_dir(Vec2::new(x, z)).unwrap_or(Self::NORTH)
    }

    /// The tile-to-tile direction nearest this heading on the flat-top grid.
    pub fn hex_dir(self) -> Qrz {
        HEX_DIRS[self.hex_index()]
    }

    /// The three tile-to-tile directions of the front half: the one nearest
    /// this heading and the one either side of it.
    pub fn front_dirs(self) -> [Qrz; 3] {
        let i = self.hex_index();
        [HEX_DIRS[(i + 5) % 6], HEX_DIRS[i], HEX_DIRS[(i + 1) % 6]]
    }

    fn hex_index(self) -> usize {
        let per = HEADING_SLOTS / 6;
        ((self.0 + per / 2) / per) as usize % 6
    }
}

impl From<Heading> for Quat {
    /// A bearing runs clockwise seen from above; a Y rotation runs
    /// counter-clockwise, so the angle is negated.
    fn from(heading: Heading) -> Self {
        Quat::from_rotation_y(-heading.degrees().to_radians())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_bearings_are_headings() {
        let hex = [
            (Qrz { q: 0, r: -1, z: 0 }, 0.0),
            (Qrz { q: 1, r: -1, z: 0 }, 60.0),
            (Qrz { q: 1, r: 0, z: 0 }, 120.0),
            (Qrz { q: 0, r: 1, z: 0 }, 180.0),
            (Qrz { q: -1, r: 1, z: 0 }, 240.0),
            (Qrz { q: -1, r: 0, z: 0 }, 300.0),
        ];
        for (offset, degrees) in hex {
            let heading = Heading::from_hex(offset);
            assert_eq!(heading.degrees(), degrees, "{offset:?}");
            assert_eq!(heading.hex_dir(), offset, "{offset:?} round trips");
            assert_eq!(Heading::from_world_dir(heading.to_world_dir()), Some(heading));
        }
    }

    #[test]
    fn slots_wrap_and_turn() {
        assert_eq!(Heading::from_slot(HEADING_SLOTS), Heading::NORTH);
        assert_eq!(Heading::NORTH.turned(-1).slot(), HEADING_SLOTS - 1);
        assert_eq!(Heading::NORTH.reversed().degrees(), 180.0);
        assert_eq!(Heading::from_degrees(359.0), Heading::NORTH);
        assert_eq!(Heading::from_degrees(-90.0).degrees(), 270.0);
    }

    /// The front half is the faced direction and its two neighbours: never
    /// a direction behind, and the same three however the heading leans
    /// within its sixth.
    #[test]
    fn the_front_half_is_three_directions() {
        for slot in 0..HEADING_SLOTS {
            let heading = Heading::from_slot(slot);
            let front = heading.front_dirs();
            assert_eq!(front[1], heading.hex_dir());
            assert!(!front.contains(&heading.reversed().hex_dir()));
            assert!(front.iter().all(|d| HEX_DIRS.contains(d)));
        }
        assert_eq!(
            Heading::NORTH.front_dirs(),
            [Qrz { q: -1, r: 0, z: 0 }, Qrz { q: 0, r: -1, z: 0 }, Qrz { q: 1, r: -1, z: 0 }]
        );
    }

    #[test]
    fn world_dir_rounds_to_nearest_slot() {
        let a = Heading::from_slot(1).to_world_dir();
        let b = Heading::from_slot(2).to_world_dir();
        let nearer_a = a.lerp(b, 0.4);
        assert_eq!(Heading::from_world_dir(nearer_a), Some(Heading::from_slot(1)));
        assert_eq!(Heading::from_world_dir(Vec2::ZERO), None);
    }

    #[test]
    fn rotation_follows_the_compass() {
        let north: Quat = Heading::NORTH.into();
        assert!(north.abs_diff_eq(Quat::IDENTITY, 1e-6));
        let east: Quat = Heading::from_degrees(90.0).into();
        let forward = east * Vec3::NEG_Z;
        assert!(forward.abs_diff_eq(Vec3::X, 1e-5), "east faces +x, got {forward}");
    }
}
